-- Follow without an account (BP26100205). Canon: work/_arc/leaflet/canon-canvas/20261002_canon_follow_without_account_leaflet_v1_0_0.md
--
-- A follower is a verified channel address plus a consent record, owned by the publisher. There is no follower
-- account: no password, no profile, no login. The signed manage link in each message is the only key.
--
-- Written beside `subscribers` (001), not over it. The email-only `subscribers` table keeps working until a later
-- slice backfills each confirmed subscriber into follower + follow_channel + consent and retires it.

-- follower: one person as the publisher knows them. Channels hang off it; two channels merge into one follower
-- only when the same person confirms both inside one manage session, never by guesswork (canon section 3).
CREATE TABLE follower (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  publication_id  UUID NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  -- set when this record was folded into another by a confirmed merge; the row is kept so old links still resolve
  merged_into     UUID REFERENCES follower(id),
  -- topics the follower chose, NULL = everything (the default). Stored as a text array of topic slugs.
  topics          TEXT[],
  frequency       VARCHAR(20) NOT NULL DEFAULT 'every_issue',   -- every_issue | monthly_digest
  paused_until    TIMESTAMPTZ,
  -- bumped to invalidate every outstanding manage link for this follower (rotation)
  token_version   INT NOT NULL DEFAULT 1,
  -- where they came from: issue_page | widget | short_link | qr | share_card | import
  source          VARCHAR(50),
  referred_by     UUID REFERENCES follower(id),
  CHECK (frequency IN ('every_issue', 'monthly_digest'))
);

-- follow_channel: one confirmed way to reach a follower. The address is stored so the issue can be delivered;
-- address_hash is HMAC(pepper, kind || address) so "is this address already following" never needs a plaintext scan
-- and a following-again on a known address merges instead of duplicating (UNIQUE below).
CREATE TABLE follow_channel (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  follower_id     UUID NOT NULL REFERENCES follower(id) ON DELETE CASCADE,
  publication_id  UUID NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
  kind            VARCHAR(20) NOT NULL,                         -- email | push | rss | whatsapp | telegram
  address         TEXT,                                          -- email, E.164 phone, or the push subscription JSON; NULL for rss
  address_hash    VARCHAR(64),                                   -- hex HMAC-SHA256; NULL for rss
  status          VARCHAR(20) NOT NULL DEFAULT 'pending',        -- pending | confirmed | paused | unsubscribed
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  confirmed_at    TIMESTAMPTZ,
  unsubscribed_at TIMESTAMPTZ,
  CHECK (kind IN ('email', 'push', 'rss', 'whatsapp', 'telegram')),
  CHECK (status IN ('pending', 'confirmed', 'paused', 'unsubscribed')),
  -- one row per (publication, kind, address): following again resolves to the existing row
  UNIQUE (publication_id, kind, address_hash)
);

-- consent: the Kenya Data Protection Act 2019 record. One row per grant, never updated: a later change is a new row.
-- `wording` is the exact text shown to the reader when they agreed.
CREATE TABLE consent (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  channel_id      UUID NOT NULL REFERENCES follow_channel(id) ON DELETE CASCADE,
  wording         TEXT NOT NULL CHECK (length(wording) > 0),
  topics          TEXT[],
  frequency       VARCHAR(20) NOT NULL,
  source          VARCHAR(50),
  ip_hash         VARCHAR(64),                                   -- hex HMAC of the client IP, never the IP
  granted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- manage_token: an audit and revocation record of each signed link that was sent. Verification is stateless
-- (signature, purpose, expiry, token_version); this table lets a single leaked link be revoked by its hash.
CREATE TABLE manage_token (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  follower_id     UUID NOT NULL REFERENCES follower(id) ON DELETE CASCADE,
  purpose         VARCHAR(20) NOT NULL,                          -- confirm | manage
  token_hash      VARCHAR(64) NOT NULL UNIQUE,                   -- hex SHA-256 of the token string
  version         INT NOT NULL,
  expires_at      TIMESTAMPTZ NOT NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  revoked_at      TIMESTAMPTZ,
  CHECK (purpose IN ('confirm', 'manage'))
);

CREATE INDEX idx_follower_publication ON follower(publication_id) WHERE merged_into IS NULL;
CREATE INDEX idx_follow_channel_follower ON follow_channel(follower_id);
CREATE INDEX idx_follow_channel_delivery ON follow_channel(publication_id, kind) WHERE status = 'confirmed';
CREATE INDEX idx_consent_channel ON consent(channel_id);
