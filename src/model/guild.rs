//! Guild objects, guild queries, and guild-level request bodies.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{
    Channel, Member, Presence, Role, Snowflake, SoundboardSound, StageInstance, Sticker, VoiceState,
};

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GetGuildQuery`.
pub struct GetGuildQuery {
    pub with_counts: Option<bool>,
}

/// Request body for `POST /guilds` (create guild; bot must be in fewer
/// than 10 guilds).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuild {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Role>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<Channel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
}

/// Request body for creating a guild from a guild template
/// (`POST /guilds/templates/{code}`).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuildFromTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Request body and response shape for `POST /guilds/{id}/mfa`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildMfaLevel {
    pub level: u64,
}

/// Request body for modifying a guild.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as image data, linked channel IDs, or
/// description.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Option<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Guild`.
pub struct Guild {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_presences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanity_url_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscription_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incidents_data: Option<GuildIncidentsData>,
    // GUILD_CREATE-only fields. Discord sends these once per session on the
    // initial guild payload; REST guild fetches leave them empty. The
    // runtime distributes them into the per-entity caches and strips them
    // from the cached Guild copy to avoid double storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Member>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voice_states: Vec<VoiceState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presences: Vec<Presence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emojis: Vec<crate::types::Emoji>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stickers: Vec<Sticker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_instances: Vec<StageInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soundboard_sounds: Vec<SoundboardSound>,
    /// Scheduled events active in the guild, as raw objects; the typed
    /// [`crate::event::ScheduledEvent`] shape lives in the event layer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guild_scheduled_events: Vec<serde_json::Value>,
}

impl Guild {
    /// Returns a copy without the bulk GUILD_CREATE collections
    /// (channels, threads, members, voice states, presences, emojis,
    /// stickers, stage instances, soundboard sounds, scheduled events).
    /// The cache stores this stripped form once the collections have been
    /// distributed into their per-entity stores.
    pub fn without_create_collections(&self) -> Guild {
        Guild {
            channels: Vec::new(),
            threads: Vec::new(),
            members: Vec::new(),
            voice_states: Vec::new(),
            presences: Vec::new(),
            emojis: Vec::new(),
            stickers: Vec::new(),
            stage_instances: Vec::new(),
            soundboard_sounds: Vec::new(),
            guild_scheduled_events: Vec::new(),
            ..self.clone()
        }
    }
}

/// Active safety incident actions configured for a guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildIncidentsData {
    /// Timestamp when invite creation will be re-enabled, or `null` when invite blocking is disabled.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages will be re-enabled, or `null` when DM blocking is disabled.
    pub dms_disabled_until: Option<String>,
    /// Timestamp when Discord detected DM spam, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_spam_detected_at: Option<String>,
    /// Timestamp when Discord detected a raid, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_detected_at: Option<String>,
}

/// Request body for modifying guild incident actions.
///
/// Supplying `None` serializes `null`, which disables that incident action.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildIncidentActions {
    /// Timestamp when invites should be re-enabled, or `null` to disable invite blocking.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages should be re-enabled, or `null` to disable DM blocking.
    pub dms_disabled_until: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CurrentUserGuild`.
pub struct CurrentUserGuild {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default)]
    pub owner: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CurrentUserGuildsQuery`.
pub struct CurrentUserGuildsQuery {
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub with_counts: Option<bool>,
    /// Returns only the guilds stored in this shard (`0` to
    /// `max_concurrency - 1` from Get Gateway Bot). Required since
    /// 2026-09-15 for apps using large bot sharding, which otherwise get a
    /// 400; this is not the gateway shard id.
    pub shard: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPreview`.
pub struct GuildPreview {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<String>,
    #[serde(default)]
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub features: Vec<String>,
    pub approximate_member_count: u64,
    pub approximate_presence_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
}
