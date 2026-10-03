//! Export: the publisher can take the whole audience out at any time (leaflet v2.1.0 principle 5), as CSV or JSON,
//! with each follower's consent wording and time. Pure rendering over loaded rows.

use serde::Serialize;

use super::address::mask_phone;
use super::model::ChannelKind;

/// One channel of one follower, as the export query will load it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportRow {
    pub follower_id: String,
    pub channel: String,
    /// Email, E.164 phone, or empty for RSS. A push subscription's endpoint is never exported (it is a bearer URL).
    pub address: String,
    pub status: String,
    /// Empty means everything.
    pub topics: String,
    pub frequency: String,
    pub source: String,
    pub consent_wording: String,
    /// RFC 3339 UTC.
    pub consented_at: String,
}

/// How much of a phone number the export may show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneVisibility {
    /// The publisher's owner role: full numbers (canon section 6).
    Full,
    /// Every other view, including the dashboard: last three digits only.
    Masked,
}

/// Apply the address rules for who is looking.
pub fn prepare(rows: &[ExportRow], phones: PhoneVisibility) -> Vec<ExportRow> {
    rows.iter()
        .map(|r| {
            let mut r = r.clone();
            if r.channel == ChannelKind::Whatsapp.as_str() && phones == PhoneVisibility::Masked {
                r.address = mask_phone(&r.address);
            }
            if r.channel == ChannelKind::Push.as_str() {
                r.address.clear();
            }
            r
        })
        .collect()
}

const HEADER: [&str; 9] = [
    "follower_id",
    "channel",
    "address",
    "status",
    "topics",
    "frequency",
    "source",
    "consent_wording",
    "consented_at",
];

/// One CSV field, RFC 4180 quoting, plus the spreadsheet formula-injection guard: a field that a spreadsheet would
/// run as a formula (starts with `=`, `@`, a tab or a CR, or a `+`/`-` that is not a plain number such as a phone)
/// gets a leading apostrophe so it opens as text.
fn csv_field(raw: &str) -> String {
    let mut s = raw.to_string();
    let risky = match s.chars().next() {
        Some('=') | Some('@') | Some('\t') | Some('\r') => true,
        Some('+') | Some('-') => !s[1..].chars().all(|c| c.is_ascii_digit() || c == ' '),
        _ => false,
    };
    if risky {
        s.insert(0, '\'');
    }
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

pub fn to_csv(rows: &[ExportRow]) -> String {
    let mut out = HEADER.join(",");
    out.push_str("\r\n");
    for r in rows {
        let fields = [
            &r.follower_id,
            &r.channel,
            &r.address,
            &r.status,
            &r.topics,
            &r.frequency,
            &r.source,
            &r.consent_wording,
            &r.consented_at,
        ];
        out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn to_json(rows: &[ExportRow]) -> String {
    serde_json::to_string_pretty(rows).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(channel: &str, address: &str) -> ExportRow {
        ExportRow {
            follower_id: "f1".into(),
            channel: channel.into(),
            address: address.into(),
            status: "confirmed".into(),
            topics: "judgment;build notes".into(),
            frequency: "every_issue".into(),
            source: "issue_page".into(),
            consent_wording: "Email me each new issue, \"no tracking\".".into(),
            consented_at: "2026-10-03T08:30:00Z".into(),
        }
    }

    #[test]
    fn the_owner_sees_full_numbers_everyone_else_the_last_three_digits() {
        let rows = vec![row("whatsapp", "+254712345678"), row("email", "a@b.com")];
        let owner = prepare(&rows, PhoneVisibility::Full);
        assert_eq!(owner[0].address, "+254712345678");
        let masked = prepare(&rows, PhoneVisibility::Masked);
        assert_eq!(masked[0].address, "+*********678");
        assert_eq!(masked[1].address, "a@b.com", "emails are not masked");
    }

    #[test]
    fn a_push_subscription_endpoint_never_leaves() {
        let rows = vec![row("push", "https://push.example/endpoint/secret-token")];
        assert_eq!(prepare(&rows, PhoneVisibility::Full)[0].address, "");
    }

    #[test]
    fn csv_quotes_commas_and_quotes_and_carries_the_consent_wording() {
        let csv = to_csv(&[row("email", "a@b.com")]);
        let mut lines = csv.split("\r\n");
        assert_eq!(lines.next().unwrap(), "follower_id,channel,address,status,topics,frequency,source,consent_wording,consented_at");
        assert_eq!(
            lines.next().unwrap(),
            "f1,email,a@b.com,confirmed,judgment;build notes,every_issue,issue_page,\"Email me each new issue, \"\"no tracking\"\".\",2026-10-03T08:30:00Z"
        );
    }

    #[test]
    fn formulas_are_defused_but_phone_numbers_survive() {
        assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_field("@cmd"), "'@cmd");
        assert_eq!(csv_field("+254712345678"), "+254712345678");
        assert_eq!(csv_field("-5"), "-5");
        assert_eq!(csv_field("+1+1"), "'+1+1");
        assert_eq!(csv_field("plain"), "plain");
    }

    #[test]
    fn json_carries_every_column() {
        let json = to_json(&[row("email", "a@b.com")]);
        for key in ["follower_id", "channel", "address", "status", "topics", "frequency", "source", "consent_wording", "consented_at"] {
            assert!(json.contains(&format!("\"{key}\"")), "{key}");
        }
        assert_eq!(to_json(&[]), "[]");
    }
}
