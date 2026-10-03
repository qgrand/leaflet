//! The follow model: channels, their state machine, topics, frequency and the consent record.

use std::collections::BTreeSet;

/// The ways a reader can follow, in the order the canon lists them. RSS is pull-only: it has no address and
/// nothing is ever delivered to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChannelKind {
    Email,
    Push,
    Rss,
    Whatsapp,
    Telegram,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Email => "email",
            ChannelKind::Push => "push",
            ChannelKind::Rss => "rss",
            ChannelKind::Whatsapp => "whatsapp",
            ChannelKind::Telegram => "telegram",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "email" => ChannelKind::Email,
            "push" => ChannelKind::Push,
            "rss" => ChannelKind::Rss,
            "whatsapp" => ChannelKind::Whatsapp,
            "telegram" => ChannelKind::Telegram,
            _ => return None,
        })
    }

    /// Whether anything is ever pushed to this channel. RSS readers fetch the feed themselves.
    pub fn is_delivered(self) -> bool {
        self != ChannelKind::Rss
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    EveryIssue,
    MonthlyDigest,
}

impl Frequency {
    pub fn as_str(self) -> &'static str {
        match self {
            Frequency::EveryIssue => "every_issue",
            Frequency::MonthlyDigest => "monthly_digest",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "every_issue" => Some(Frequency::EveryIssue),
            "monthly_digest" => Some(Frequency::MonthlyDigest),
            _ => None,
        }
    }
}

/// Where a channel stands. `Pending` has proven nothing yet: nothing is sent to a pending channel except the
/// confirmation itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStatus {
    Pending,
    Confirmed,
    Paused,
    Unsubscribed,
}

impl ChannelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelStatus::Pending => "pending",
            ChannelStatus::Confirmed => "confirmed",
            ChannelStatus::Paused => "paused",
            ChannelStatus::Unsubscribed => "unsubscribed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => ChannelStatus::Pending,
            "confirmed" => ChannelStatus::Confirmed,
            "paused" => ChannelStatus::Paused,
            "unsubscribed" => ChannelStatus::Unsubscribed,
            _ => return None,
        })
    }
}

/// What can happen to a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEvent {
    /// The reader tapped the confirm link, allowed notifications, or sent the opt-in message.
    Confirm,
    Pause,
    Resume,
    /// One tap, from any state, always honoured.
    Unsubscribe,
    /// The same address followed again.
    Refollow,
}

/// What a state change asks the caller to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Send the confirmation (again): a re-follow of a pending or previously unsubscribed address.
    SendConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub status: ChannelStatus,
    pub effect: Effect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: ChannelStatus,
    pub event: ChannelEvent,
}

/// The channel state machine. Pure: the caller persists the result.
pub struct ChannelState;

impl ChannelState {
    pub fn apply(from: ChannelStatus, event: ChannelEvent) -> Result<Transition, InvalidTransition> {
        use ChannelEvent as E;
        use ChannelStatus as S;
        let ok = |status, effect| -> Result<Transition, InvalidTransition> { Ok(Transition { status, effect }) };
        match (from, event) {
            // Confirming proves the channel. Confirming an already-confirmed one is a harmless double tap.
            (S::Pending, E::Confirm) => ok(S::Confirmed, Effect::None),
            (S::Confirmed, E::Confirm) => ok(S::Confirmed, Effect::None),
            // A paused or unsubscribed channel cannot be confirmed by an old link: that is a new decision.
            (S::Paused, E::Confirm) | (S::Unsubscribed, E::Confirm) => Err(InvalidTransition { from, event }),

            (S::Confirmed, E::Pause) => ok(S::Paused, Effect::None),
            (S::Paused, E::Pause) => ok(S::Paused, Effect::None),
            (S::Pending, E::Pause) | (S::Unsubscribed, E::Pause) => Err(InvalidTransition { from, event }),

            (S::Paused, E::Resume) => ok(S::Confirmed, Effect::None),
            (S::Confirmed, E::Resume) => ok(S::Confirmed, Effect::None),
            (S::Pending, E::Resume) | (S::Unsubscribed, E::Resume) => Err(InvalidTransition { from, event }),

            // Unsubscribe is always honoured, and idempotent.
            (_, E::Unsubscribe) => ok(S::Unsubscribed, Effect::None),

            // Following again never creates a second row. A live channel stays as it is; a pending one is nudged
            // with a fresh confirmation; an unsubscribed one starts over and must be confirmed again.
            (S::Confirmed, E::Refollow) => ok(S::Confirmed, Effect::None),
            (S::Paused, E::Refollow) => ok(S::Paused, Effect::None),
            (S::Pending, E::Refollow) => ok(S::Pending, Effect::SendConfirmation),
            (S::Unsubscribed, E::Refollow) => ok(S::Pending, Effect::SendConfirmation),
        }
    }
}

/// Which topics a follower wants. `None` is everything, which is the default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Topics(pub Option<BTreeSet<String>>);

impl Topics {
    pub fn everything() -> Self {
        Topics(None)
    }

    /// Normalise a chosen list: lowercase slugs, trimmed, no empties, no duplicates. An empty list means everything,
    /// because "follow nothing" is an unsubscribe, not a topic choice.
    pub fn only<I, S>(chosen: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let set: BTreeSet<String> = chosen
            .into_iter()
            .map(|t| t.as_ref().trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if set.is_empty() {
            Topics(None)
        } else {
            Topics(Some(set))
        }
    }

    /// Whether an issue tagged with `issue_topics` reaches this follower. An untagged issue is general and reaches
    /// everyone; a follower with no filter gets everything.
    pub fn wants(&self, issue_topics: &[String]) -> bool {
        match &self.0 {
            None => true,
            Some(chosen) => {
                if issue_topics.is_empty() {
                    return true;
                }
                issue_topics.iter().any(|t| chosen.contains(&t.trim().to_lowercase()))
            }
        }
    }
}

/// The Kenya Data Protection Act 2019 record of one grant. Immutable once written: a change is a new record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentRecord {
    pub channel: ChannelKind,
    /// The exact wording the reader saw when they agreed.
    pub wording: String,
    pub topics: Topics,
    pub frequency: Frequency,
    pub source: String,
    /// Hex HMAC of the client IP, never the IP itself.
    pub ip_hash: Option<String>,
    /// Unix seconds.
    pub granted_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentError {
    EmptyWording,
    EmptySource,
}

impl ConsentRecord {
    pub fn new(
        channel: ChannelKind,
        wording: &str,
        topics: Topics,
        frequency: Frequency,
        source: &str,
        ip_hash: Option<String>,
        granted_at: i64,
    ) -> Result<Self, ConsentError> {
        if wording.trim().is_empty() {
            return Err(ConsentError::EmptyWording);
        }
        if source.trim().is_empty() {
            return Err(ConsentError::EmptySource);
        }
        Ok(ConsentRecord {
            channel,
            wording: wording.to_string(),
            topics,
            frequency,
            source: source.trim().to_string(),
            ip_hash,
            granted_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChannelEvent as E;
    use ChannelStatus as S;

    fn next(from: S, e: E) -> S {
        ChannelState::apply(from, e).unwrap().status
    }

    #[test]
    fn a_new_channel_is_confirmed_once_and_a_double_tap_is_harmless() {
        assert_eq!(next(S::Pending, E::Confirm), S::Confirmed);
        assert_eq!(next(S::Confirmed, E::Confirm), S::Confirmed);
    }

    #[test]
    fn an_old_confirm_link_cannot_revive_a_paused_or_unsubscribed_channel() {
        assert!(ChannelState::apply(S::Paused, E::Confirm).is_err());
        assert!(ChannelState::apply(S::Unsubscribed, E::Confirm).is_err());
    }

    #[test]
    fn unsubscribe_is_honoured_from_every_state_and_is_idempotent() {
        for s in [S::Pending, S::Confirmed, S::Paused, S::Unsubscribed] {
            assert_eq!(next(s, E::Unsubscribe), S::Unsubscribed, "{s:?}");
        }
    }

    #[test]
    fn pause_and_resume_only_move_a_confirmed_channel() {
        assert_eq!(next(S::Confirmed, E::Pause), S::Paused);
        assert_eq!(next(S::Paused, E::Resume), S::Confirmed);
        assert!(ChannelState::apply(S::Pending, E::Pause).is_err());
        assert!(ChannelState::apply(S::Unsubscribed, E::Resume).is_err());
    }

    #[test]
    fn following_again_never_adds_a_second_live_channel() {
        let t = ChannelState::apply(S::Confirmed, E::Refollow).unwrap();
        assert_eq!((t.status, t.effect), (S::Confirmed, Effect::None));
        let t = ChannelState::apply(S::Paused, E::Refollow).unwrap();
        assert_eq!((t.status, t.effect), (S::Paused, Effect::None));
    }

    #[test]
    fn following_again_nudges_a_pending_channel_and_restarts_an_unsubscribed_one() {
        let t = ChannelState::apply(S::Pending, E::Refollow).unwrap();
        assert_eq!((t.status, t.effect), (S::Pending, Effect::SendConfirmation));
        let t = ChannelState::apply(S::Unsubscribed, E::Refollow).unwrap();
        assert_eq!((t.status, t.effect), (S::Pending, Effect::SendConfirmation));
    }

    #[test]
    fn names_round_trip() {
        for k in [ChannelKind::Email, ChannelKind::Push, ChannelKind::Rss, ChannelKind::Whatsapp, ChannelKind::Telegram] {
            assert_eq!(ChannelKind::parse(k.as_str()), Some(k));
        }
        for s in [S::Pending, S::Confirmed, S::Paused, S::Unsubscribed] {
            assert_eq!(ChannelStatus::parse(s.as_str()), Some(s));
        }
        for f in [Frequency::EveryIssue, Frequency::MonthlyDigest] {
            assert_eq!(Frequency::parse(f.as_str()), Some(f));
        }
        assert_eq!(ChannelKind::parse("sms"), None);
    }

    #[test]
    fn rss_is_never_delivered() {
        assert!(!ChannelKind::Rss.is_delivered());
        assert!(ChannelKind::Email.is_delivered());
        assert!(ChannelKind::Push.is_delivered());
    }

    #[test]
    fn topics_default_to_everything_and_an_empty_choice_is_everything_too() {
        assert!(Topics::everything().wants(&["judgment".into()]));
        assert_eq!(Topics::only(Vec::<String>::new()), Topics(None));
        assert_eq!(Topics::only(["  ", ""]), Topics(None));
    }

    #[test]
    fn a_topic_filter_matches_case_insensitively_and_untagged_issues_reach_everyone() {
        let t = Topics::only(["Judgment", "build notes", "judgment"]);
        assert_eq!(t.0.as_ref().unwrap().len(), 2, "deduplicated and lowercased");
        assert!(t.wants(&["JUDGMENT".into(), "other".into()]));
        assert!(!t.wants(&["adaptive operations".into()]));
        assert!(t.wants(&[]), "an untagged issue is general");
    }

    #[test]
    fn consent_needs_the_wording_and_a_source() {
        let ok = ConsentRecord::new(
            ChannelKind::Email,
            "Email me each new issue. Unsubscribe in one tap.",
            Topics::everything(),
            Frequency::EveryIssue,
            " issue_page ",
            None,
            1_791_016_200,
        )
        .unwrap();
        assert_eq!(ok.source, "issue_page");
        assert_eq!(
            ConsentRecord::new(ChannelKind::Email, "  ", Topics::everything(), Frequency::EveryIssue, "widget", None, 0),
            Err(ConsentError::EmptyWording)
        );
        assert_eq!(
            ConsentRecord::new(ChannelKind::Email, "x", Topics::everything(), Frequency::EveryIssue, "", None, 0),
            Err(ConsentError::EmptySource)
        );
    }
}
