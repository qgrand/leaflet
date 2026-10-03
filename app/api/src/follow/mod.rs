//! Follow without an account (BP26100205). Canon:
//! `work/_arc/leaflet/canon-canvas/20261002_canon_follow_without_account_leaflet_v1_0_0.md`.
//!
//! **Slice 1: the pure core.** Everything here is a function of its inputs, with no database, no HTTP and no clock
//! of its own (callers pass `now`), so it can be proved by unit tests before any wiring exists. It was written on
//! Windows and has not been compiled (CLAUDE.md 23); the Linux compile row runs the tests.
//!
//! | Module | What it owns |
//! |---|---|
//! | `model` | channel kinds, the channel state machine, topics, frequency, the consent record |
//! | `address` | email normalisation, the address hash used to merge a re-follow, phone masking |
//! | `token` | signed, single-purpose, rotating manage and confirm links (stateless to verify) |
//! | `delivery` | which confirmed channels receive a given issue, and which get the monthly digest |
//! | `rss` | the RSS 2.0 feed for a publication |
//! | `export` | CSV and JSON export, with the formula-injection guard and phone masking |
//!
//! **Not here yet (later slices, each its own row):** the routes and the follow page, the double opt-in email send,
//! Web Push with VAPID keys from a mounted secret (CLAUDE.md 29 rule 3), the digest job, the publisher dashboard,
//! the embeddable widget, and the backfill that retires the email-only `subscribers` table. `migrations/005` is the
//! schema they sit on.
//!
//! **Principles kept in code, not only in prose:**
//! - No follower account, ever: nothing here creates a credential, only a signed link.
//! - Following again on a known address merges: `address::address_hash` is the key the UNIQUE index uses, and
//!   `model::ChannelState::apply(Refollow)` never creates a second row from a live one.
//! - Consent is a record of the exact wording shown (`model::ConsentRecord::new` refuses empty wording).
//! - No tracking pixel: nothing in `delivery` or `rss` emits one.

pub mod address;
pub mod delivery;
pub mod export;
pub mod model;
pub mod rss;
pub mod token;
