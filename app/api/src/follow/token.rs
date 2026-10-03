//! The signed link in every message: the only key a follower ever holds.
//!
//! `<purpose>.<follower uuid>.<version>.<expires unix>.<hex HMAC-SHA256>`
//!
//! - **Single-purpose.** The purpose is inside the signed text, so a confirm link cannot be replayed as a manage
//!   link and the other way round.
//! - **Rotating.** `version` is the follower's `token_version`; bumping it kills every outstanding link at once.
//! - **Stateless to verify**, so a lost database row never makes a valid link fail. `manage_token` records each
//!   link's hash so one leaked link can still be revoked on its own (the caller checks `token_hash` against it).
//! - **Costs nothing to lose.** Following again on the same address merges and mints a fresh link.
//!
//! The signing key is a server secret read from a mounted file (CLAUDE.md 29 rule 3). It is passed in, never read
//! here.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::address::{hex_decode, hex_encode};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Double opt-in: proves the address belongs to the reader.
    Confirm,
    /// Change channel or topics, pause, unsubscribe.
    Manage,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Purpose::Confirm => "confirm",
            Purpose::Manage => "manage",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "confirm" => Some(Purpose::Confirm),
            "manage" => Some(Purpose::Manage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    pub purpose: Purpose,
    pub follower: Uuid,
    pub version: i32,
    pub expires_at: i64,
}

/// Why a link was refused. The caller shows one generic message for all of them (a refusal that explains itself
/// helps whoever is guessing); the variants exist for logs and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    BadSignature,
    WrongPurpose,
    Expired,
    /// The follower's `token_version` moved on, so this link was rotated out.
    Rotated,
}

fn signing_text(purpose: Purpose, follower: Uuid, version: i32, expires_at: i64) -> String {
    format!("{}.{}.{}.{}", purpose.as_str(), follower, version, expires_at)
}

fn sign(key: &[u8], text: &str) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(text.as_bytes());
    mac
}

/// Mint a link token. `ttl_secs` is how long it lives: a confirm link is short (a day), a manage link long (a year).
pub fn mint(key: &[u8], purpose: Purpose, follower: Uuid, version: i32, now: i64, ttl_secs: i64) -> String {
    let expires_at = now + ttl_secs;
    let text = signing_text(purpose, follower, version, expires_at);
    let tag = hex_encode(&sign(key, &text).finalize().into_bytes());
    format!("{text}.{tag}")
}

/// Verify a link token. `current_version` is the follower's stored `token_version`. The signature is checked first,
/// in constant time, so nothing about the claims is trusted before it passes.
pub fn verify(key: &[u8], token: &str, expected: Purpose, now: i64, current_version: i32) -> Result<Claims, TokenError> {
    let parts: Vec<&str> = token.split('.').collect();
    let [purpose, follower, version, expires, tag] = parts.as_slice() else {
        return Err(TokenError::Malformed);
    };
    let purpose = Purpose::parse(purpose).ok_or(TokenError::Malformed)?;
    let follower = Uuid::parse_str(follower).map_err(|_| TokenError::Malformed)?;
    let version: i32 = version.parse().map_err(|_| TokenError::Malformed)?;
    let expires_at: i64 = expires.parse().map_err(|_| TokenError::Malformed)?;
    let tag = hex_decode(tag).ok_or(TokenError::Malformed)?;

    let text = signing_text(purpose, follower, version, expires_at);
    sign(key, &text).verify_slice(&tag).map_err(|_| TokenError::BadSignature)?;

    if purpose != expected {
        return Err(TokenError::WrongPurpose);
    }
    if now >= expires_at {
        return Err(TokenError::Expired);
    }
    if version != current_version {
        return Err(TokenError::Rotated);
    }
    Ok(Claims { purpose, follower, version, expires_at })
}

/// Hex SHA-256 of the token string: what `manage_token.token_hash` stores, so a link can be revoked without the
/// database ever holding a usable link.
pub fn token_hash(token: &str) -> String {
    hex_encode(&Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"test-signing-key-not-a-real-secret";
    const NOW: i64 = 1_791_016_200;

    fn id() -> Uuid {
        Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap()
    }

    #[test]
    fn a_minted_link_verifies_and_carries_its_claims() {
        let t = mint(KEY, Purpose::Manage, id(), 3, NOW, 3600);
        let c = verify(KEY, &t, Purpose::Manage, NOW + 10, 3).unwrap();
        assert_eq!((c.purpose, c.follower, c.version, c.expires_at), (Purpose::Manage, id(), 3, NOW + 3600));
    }

    #[test]
    fn a_confirm_link_is_not_a_manage_link() {
        let t = mint(KEY, Purpose::Confirm, id(), 1, NOW, 3600);
        assert_eq!(verify(KEY, &t, Purpose::Manage, NOW, 1), Err(TokenError::WrongPurpose));
        assert!(verify(KEY, &t, Purpose::Confirm, NOW, 1).is_ok());
    }

    #[test]
    fn changing_any_claim_breaks_the_signature() {
        let t = mint(KEY, Purpose::Manage, id(), 1, NOW, 3600);
        let swapped_purpose = t.replacen("manage", "confirm", 1);
        assert_eq!(verify(KEY, &swapped_purpose, Purpose::Confirm, NOW, 1), Err(TokenError::BadSignature));
        let other = Uuid::parse_str("99999999-2222-3333-4444-555555555555").unwrap();
        let swapped_id = t.replacen(&id().to_string(), &other.to_string(), 1);
        assert_eq!(verify(KEY, &swapped_id, Purpose::Manage, NOW, 1), Err(TokenError::BadSignature));
        let longer = t.replacen(&format!(".{}.", NOW + 3600), &format!(".{}.", NOW + 999_999), 1);
        assert_eq!(verify(KEY, &longer, Purpose::Manage, NOW, 1), Err(TokenError::BadSignature));
    }

    #[test]
    fn a_different_key_does_not_verify() {
        let t = mint(KEY, Purpose::Manage, id(), 1, NOW, 3600);
        assert_eq!(verify(b"another-key", &t, Purpose::Manage, NOW, 1), Err(TokenError::BadSignature));
    }

    #[test]
    fn a_link_expires() {
        let t = mint(KEY, Purpose::Confirm, id(), 1, NOW, 100);
        assert!(verify(KEY, &t, Purpose::Confirm, NOW + 99, 1).is_ok());
        assert_eq!(verify(KEY, &t, Purpose::Confirm, NOW + 100, 1), Err(TokenError::Expired));
    }

    #[test]
    fn bumping_the_version_rotates_every_old_link_out() {
        let t = mint(KEY, Purpose::Manage, id(), 1, NOW, 3600);
        assert_eq!(verify(KEY, &t, Purpose::Manage, NOW, 2), Err(TokenError::Rotated));
        let fresh = mint(KEY, Purpose::Manage, id(), 2, NOW, 3600);
        assert!(verify(KEY, &fresh, Purpose::Manage, NOW, 2).is_ok());
    }

    #[test]
    fn junk_is_malformed_not_a_panic() {
        for bad in ["", "a.b.c", "manage.not-a-uuid.1.2.ab", "manage.11111111-2222-3333-4444-555555555555.x.2.ab", "manage.11111111-2222-3333-4444-555555555555.1.2.zz", "x.y.z.w.v.u"] {
            assert_eq!(verify(KEY, bad, Purpose::Manage, NOW, 1), Err(TokenError::Malformed), "{bad:?}");
        }
    }

    #[test]
    fn the_stored_hash_is_not_the_token_and_is_stable() {
        let t = mint(KEY, Purpose::Manage, id(), 1, NOW, 3600);
        let h = token_hash(&t);
        assert_eq!(h.len(), 64);
        assert_ne!(h, t);
        assert_eq!(h, token_hash(&t));
    }
}
