//! Who receives what. Pure selection over already-loaded recipients; the database query and the sending are the
//! next slices.

use super::model::{ChannelKind, ChannelStatus, Frequency, Topics};

/// One confirmed-or-not channel with the follower's preferences, as the delivery query will load it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    pub follower: uuid::Uuid,
    pub channel_id: uuid::Uuid,
    pub kind: ChannelKind,
    pub status: ChannelStatus,
    pub frequency: Frequency,
    pub topics: Topics,
    /// Unix seconds; the follower asked for a pause until then.
    pub paused_until: Option<i64>,
}

impl Recipient {
    /// Whether anything may be sent to this channel at `now`. Only a confirmed, un-paused, delivered channel can
    /// receive: a pending address has proven nothing, a paused one asked for quiet, RSS pulls for itself.
    fn can_receive(&self, now: i64) -> bool {
        self.status == ChannelStatus::Confirmed
            && self.kind.is_delivered()
            && self.paused_until.map_or(true, |until| now >= until)
    }
}

/// Channels that get a new issue right now: every-issue followers whose topics match.
pub fn for_issue<'a>(recipients: &'a [Recipient], issue_topics: &[String], now: i64) -> Vec<&'a Recipient> {
    recipients
        .iter()
        .filter(|r| r.can_receive(now) && r.frequency == Frequency::EveryIssue && r.topics.wants(issue_topics))
        .collect()
}

/// Channels that get the monthly roll-up: digest followers. The caller decides which issues are in it, and a
/// follower's topic filter is applied to those issues when the digest is assembled, so a digest follower is
/// returned here whatever their topics.
pub fn for_digest<'a>(recipients: &'a [Recipient], now: i64) -> Vec<&'a Recipient> {
    recipients
        .iter()
        .filter(|r| r.can_receive(now) && r.frequency == Frequency::MonthlyDigest)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn r(kind: ChannelKind, status: ChannelStatus, frequency: Frequency, topics: Topics, paused_until: Option<i64>) -> Recipient {
        Recipient { follower: Uuid::new_v4(), channel_id: Uuid::new_v4(), kind, status, frequency, topics, paused_until }
    }

    fn live(kind: ChannelKind) -> Recipient {
        r(kind, ChannelStatus::Confirmed, Frequency::EveryIssue, Topics::everything(), None)
    }

    const NOW: i64 = 1_791_016_200;

    #[test]
    fn only_confirmed_channels_receive() {
        let all = vec![
            live(ChannelKind::Email),
            r(ChannelKind::Email, ChannelStatus::Pending, Frequency::EveryIssue, Topics::everything(), None),
            r(ChannelKind::Email, ChannelStatus::Paused, Frequency::EveryIssue, Topics::everything(), None),
            r(ChannelKind::Email, ChannelStatus::Unsubscribed, Frequency::EveryIssue, Topics::everything(), None),
        ];
        assert_eq!(for_issue(&all, &[], NOW).len(), 1);
    }

    #[test]
    fn rss_is_never_delivered_to() {
        let all = vec![live(ChannelKind::Rss), live(ChannelKind::Push)];
        let got = for_issue(&all, &[], NOW);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, ChannelKind::Push);
    }

    #[test]
    fn a_pause_holds_until_its_date() {
        let all = vec![r(ChannelKind::Email, ChannelStatus::Confirmed, Frequency::EveryIssue, Topics::everything(), Some(NOW + 100))];
        assert!(for_issue(&all, &[], NOW).is_empty());
        assert_eq!(for_issue(&all, &[], NOW + 100).len(), 1);
    }

    #[test]
    fn topics_filter_and_untagged_issues_reach_everyone() {
        let judgment = r(ChannelKind::Email, ChannelStatus::Confirmed, Frequency::EveryIssue, Topics::only(["judgment"]), None);
        let everything = live(ChannelKind::Email);
        let all = vec![judgment, everything];
        assert_eq!(for_issue(&all, &["judgment".to_string()], NOW).len(), 2);
        assert_eq!(for_issue(&all, &["build notes".to_string()], NOW).len(), 1);
        assert_eq!(for_issue(&all, &[], NOW).len(), 2);
    }

    #[test]
    fn digest_followers_get_the_roll_up_and_not_each_issue() {
        let digest = r(ChannelKind::Email, ChannelStatus::Confirmed, Frequency::MonthlyDigest, Topics::everything(), None);
        let every = live(ChannelKind::Email);
        let all = vec![digest.clone(), every];
        assert_eq!(for_issue(&all, &[], NOW).len(), 1);
        let d = for_digest(&all, NOW);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].channel_id, digest.channel_id);
    }
}
