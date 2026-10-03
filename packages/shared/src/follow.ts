import { z } from "zod";

/**
 * Follow without an account (BP26100205). Mirrors migrations/005_follow_without_account.sql and
 * app/api/src/follow/model.rs. Canon: work/_arc/leaflet/canon-canvas/20261002_canon_follow_without_account_leaflet_v1_0_0.md
 *
 * A follower is a verified channel address plus a consent record. There is no follower account: nothing here carries
 * a password, a profile or a login.
 */
export const ChannelKindSchema = z.enum(["email", "push", "rss", "whatsapp", "telegram"]);
export const ChannelStatusSchema = z.enum(["pending", "confirmed", "paused", "unsubscribed"]);
export const FrequencySchema = z.enum(["every_issue", "monthly_digest"]);

/** The front doors a follow can come through, stored as the consent record's `source`. */
export const FollowSourceSchema = z.enum(["issue_page", "widget", "short_link", "qr", "share_card", "import"]);

/** A topic is a lowercase slug. An empty or missing list means everything. */
export const TopicSchema = z.string().trim().toLowerCase().min(1).max(60);

/** The body of the follow endpoint for the email channel. The wording is what the reader was shown. */
export const FollowEmailRequestSchema = z.object({
  publication: z.string().max(30),
  email: z.string().trim().email().max(320),
  topics: z.array(TopicSchema).max(20).default([]),
  frequency: FrequencySchema.default("every_issue"),
  source: FollowSourceSchema.default("issue_page"),
  consentWording: z.string().trim().min(1).max(1000),
});

/** What the manage page may change, from a signed link. Every field is optional. */
export const ManageUpdateSchema = z.object({
  topics: z.array(TopicSchema).max(20).optional(),
  frequency: FrequencySchema.optional(),
  pauseUntil: z.string().datetime().nullable().optional(),
  unsubscribe: z.boolean().optional(),
});

export type ChannelKind = z.infer<typeof ChannelKindSchema>;
export type ChannelStatus = z.infer<typeof ChannelStatusSchema>;
export type Frequency = z.infer<typeof FrequencySchema>;
export type FollowSource = z.infer<typeof FollowSourceSchema>;
export type FollowEmailRequest = z.infer<typeof FollowEmailRequestSchema>;
export type ManageUpdate = z.infer<typeof ManageUpdateSchema>;
