//! The RSS 2.0 feed at `/<publication>/feed.xml`. Pure rendering: the route that loads the issues is a later slice.
//! No tracking pixel and no per-reader URL: every reader gets the same bytes.

use crate::follow::address::hex_encode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    /// A stable id for the item; the issue's permalink is the usual choice.
    pub guid: String,
    /// Unix seconds.
    pub published_at: i64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub title: String,
    pub link: String,
    /// The feed's own URL, for the `atom:link rel="self"` that validators ask for.
    pub self_url: String,
    pub description: String,
    pub items: Vec<FeedItem>,
}

/// Escape the five XML specials, and drop characters XML 1.0 cannot carry at all.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 allows tab, LF, CR and everything from U+0020 up except the surrogates and U+FFFE/U+FFFF.
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// RFC 822 date, as RSS wants: `Sat, 03 Oct 2026 08:30:00 +0000`.
pub fn rfc822(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let rem = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let weekday = DAYS[((days + 4).rem_euclid(7)) as usize];
    format!(
        "{weekday}, {d:02} {} {y:04} {:02}:{:02}:{:02} +0000",
        MONTHS[(m - 1) as usize],
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Render the feed. Items are written in the order given (newest first is the caller's job).
pub fn render(feed: &Feed) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n<channel>\n");
    out.push_str(&format!("<title>{}</title>\n", escape(&feed.title)));
    out.push_str(&format!("<link>{}</link>\n", escape(&feed.link)));
    out.push_str(&format!("<description>{}</description>\n", escape(&feed.description)));
    out.push_str(&format!("<atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>\n", escape(&feed.self_url)));
    if let Some(newest) = feed.items.iter().map(|i| i.published_at).max() {
        out.push_str(&format!("<lastBuildDate>{}</lastBuildDate>\n", rfc822(newest)));
    }
    for item in &feed.items {
        out.push_str("<item>\n");
        out.push_str(&format!("<title>{}</title>\n", escape(&item.title)));
        out.push_str(&format!("<link>{}</link>\n", escape(&item.link)));
        out.push_str(&format!("<guid isPermaLink=\"false\">{}</guid>\n", escape(&item.guid)));
        out.push_str(&format!("<pubDate>{}</pubDate>\n", rfc822(item.published_at)));
        out.push_str(&format!("<description>{}</description>\n", escape(&item.summary)));
        out.push_str("</item>\n");
    }
    out.push_str("</channel>\n</rss>\n");
    out
}

/// A strong ETag for the rendered feed so readers and proxies can send `If-None-Match`: hex of the first 8 bytes of
/// a SHA-256 would do, but this module stays dependency-light, so it is the FNV-1a hash of the body in hex. It is a
/// cache validator, not a security value.
pub fn etag(body: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in body.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("\"{}\"", hex_encode(&h.to_be_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> Feed {
        Feed {
            title: "Judgment & Systems".into(),
            link: "https://sconl.io/judgment".into(),
            self_url: "https://sconl.io/judgment/feed.xml".into(),
            description: "Notes on <deciding> well".into(),
            items: vec![
                FeedItem {
                    title: "Before you decide".into(),
                    link: "https://sconl.io/judgment/before-you-decide".into(),
                    guid: "https://sconl.io/judgment/before-you-decide".into(),
                    published_at: 1_791_016_200,
                    summary: "One \"quoted\" line & another".into(),
                },
                FeedItem {
                    title: "Older".into(),
                    link: "https://sconl.io/judgment/older".into(),
                    guid: "older".into(),
                    published_at: 1_790_000_000,
                    summary: String::new(),
                },
            ],
        }
    }

    #[test]
    fn rfc822_matches_a_known_date() {
        assert_eq!(rfc822(1_791_016_200), "Sat, 03 Oct 2026 08:30:00 +0000");
        assert_eq!(rfc822(0), "Thu, 01 Jan 1970 00:00:00 +0000");
    }

    #[test]
    fn text_is_escaped_everywhere_it_appears() {
        let xml = render(&feed());
        assert!(xml.contains("<title>Judgment &amp; Systems</title>"));
        assert!(xml.contains("Notes on &lt;deciding&gt; well"));
        assert!(xml.contains("One &quot;quoted&quot; line &amp; another"));
        assert!(!xml.contains("<deciding>"));
    }

    #[test]
    fn the_feed_is_well_formed_rss_with_a_self_link_and_one_item_per_issue() {
        let xml = render(&feed());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">"));
        assert!(xml.contains("<atom:link href=\"https://sconl.io/judgment/feed.xml\" rel=\"self\" type=\"application/rss+xml\"/>"));
        assert_eq!(xml.matches("<item>").count(), 2);
        assert_eq!(xml.matches("</item>").count(), 2);
        assert!(xml.contains("<lastBuildDate>Sat, 03 Oct 2026 08:30:00 +0000</lastBuildDate>"));
        assert!(xml.trim_end().ends_with("</rss>"));
    }

    #[test]
    fn nothing_in_the_feed_tracks_the_reader() {
        let xml = render(&feed());
        assert!(!xml.contains("<img"));
        assert!(!xml.contains("pixel"));
        // The XML declaration on line 1 has a `?`; no other line may, so no link carries a per-reader query string.
        assert!(xml.lines().skip(1).all(|l| !l.contains('?')), "no per-reader query strings in any link");
    }

    #[test]
    fn characters_xml_cannot_carry_are_dropped() {
        assert_eq!(escape("a\u{0}b\u{8}c\td"), "abc\td");
    }

    #[test]
    fn the_etag_follows_the_body() {
        let a = etag("one");
        assert_eq!(a, etag("one"));
        assert_ne!(a, etag("two"));
        assert!(a.starts_with('"') && a.ends_with('"'));
    }
}
