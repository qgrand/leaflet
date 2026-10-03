//! Addresses: normalising an email so the same person is recognised, hashing an address so a re-follow merges, and
//! masking a phone number for the dashboard.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    Empty,
    TooLong,
    NotAnEmail,
}

/// Trim and lowercase an email, and refuse what cannot be one. This is a gate against typos and abuse, not a
/// validator of deliverability: the double opt-in is the real proof. Plus-addressing is kept as typed (`a+b@x.com`
/// and `a@x.com` stay different followers), because a reader may use it on purpose.
pub fn normalise_email(raw: &str) -> Result<String, AddressError> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return Err(AddressError::Empty);
    }
    if s.len() > 320 {
        return Err(AddressError::TooLong);
    }
    if s.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == ';' || c == '<' || c == '>') {
        return Err(AddressError::NotAnEmail);
    }
    let mut parts = s.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(AddressError::NotAnEmail);
    };
    if local.is_empty() || local.len() > 64 {
        return Err(AddressError::NotAnEmail);
    }
    if domain.starts_with('.') || domain.ends_with('.') || domain.contains("..") || !domain.contains('.') {
        return Err(AddressError::NotAnEmail);
    }
    if domain.split('.').any(|label| label.is_empty() || label.starts_with('-') || label.ends_with('-')) {
        return Err(AddressError::NotAnEmail);
    }
    Ok(s)
}

/// `hex(HMAC-SHA256(pepper, kind 0x00 address))`. The pepper is a server secret read from a mounted file, never an
/// env var (CLAUDE.md 29 rule 3); it is passed in so this stays pure. The kind is part of the input so the same
/// string on two channels hashes differently.
pub fn address_hash(pepper: &[u8], kind: &str, normalised_address: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(pepper).expect("HMAC accepts a key of any length");
    mac.update(kind.as_bytes());
    mac.update(&[0]);
    mac.update(normalised_address.as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

/// Keep a `+` and the last three digits; every other digit becomes `*`. The publisher's dashboard shows this and
/// never the full number (canon section 6).
pub fn mask_phone(phone: &str) -> String {
    let digits = phone.chars().filter(char::is_ascii_digit).count();
    let keep_from = digits.saturating_sub(3);
    let mut seen = 0usize;
    phone
        .chars()
        .map(|c| {
            if c.is_ascii_digit() {
                seen += 1;
                if seen > keep_from {
                    c
                } else {
                    '*'
                }
            } else {
                c
            }
        })
        .collect()
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub(crate) fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    s.as_bytes().chunks(2).map(|p| Some(nibble(p[0])? << 4 | nibble(p[1])?)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_trimmed_and_lowercased() {
        assert_eq!(normalise_email("  Reader@Example.COM ").unwrap(), "reader@example.com");
    }

    #[test]
    fn plus_addressing_is_kept_as_typed() {
        assert_ne!(normalise_email("a+news@x.com").unwrap(), normalise_email("a@x.com").unwrap());
    }

    #[test]
    fn things_that_are_not_an_email_are_refused() {
        for bad in ["", "   ", "nope", "a@b", "a@@b.com", "@b.com", "a@.com", "a@b..com", "a b@c.com", "a@b.com,c@d.com", "a@-b.com", "<a@b.com>"] {
            assert!(normalise_email(bad).is_err(), "{bad:?}");
        }
        assert_eq!(normalise_email(""), Err(AddressError::Empty));
        let long = format!("{}@x.com", "a".repeat(70));
        assert_eq!(normalise_email(&long), Err(AddressError::NotAnEmail));
        let huge = format!("a@{}.com", "b".repeat(330));
        assert_eq!(normalise_email(&huge), Err(AddressError::TooLong));
    }

    #[test]
    fn the_hash_is_stable_and_separates_channels_and_peppers() {
        let a = address_hash(b"pepper-one", "email", "reader@example.com");
        assert_eq!(a, address_hash(b"pepper-one", "email", "reader@example.com"));
        assert_eq!(a.len(), 64);
        assert_ne!(a, address_hash(b"pepper-one", "whatsapp", "reader@example.com"));
        assert_ne!(a, address_hash(b"pepper-two", "email", "reader@example.com"));
        assert_ne!(a, address_hash(b"pepper-one", "email", "other@example.com"));
    }

    #[test]
    fn phone_numbers_keep_a_plus_and_the_last_three_digits() {
        assert_eq!(mask_phone("+254712345678"), "+*********678");
        assert_eq!(mask_phone("0712 345 678"), "**** *** 678");
        assert_eq!(mask_phone("12"), "12");
        assert_eq!(mask_phone(""), "");
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 15, 16, 254, 255];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes.to_vec());
        assert_eq!(hex_decode("abc"), None);
        assert_eq!(hex_decode("zz"), None);
    }
}
