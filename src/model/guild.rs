//! Guild objects, guild queries, and guild-level request bodies.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{
    ApplicationId, Channel, ChannelId, ChannelType, DefaultMessageNotificationLevel,
    ExplicitContentFilterLevel, GuildId, GuildNsfwLevel, Member, MfaLevel, OverwriteType,
    PermissionsBitField, PremiumTier, Presence, Role, SoundboardSound, StageInstance, Sticker,
    UserId, VerificationLevel, VoiceState,
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
    pub verification_level: Option<VerificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<DefaultMessageNotificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<ExplicitContentFilterLevel>,
    /// Roles to create; the first entry edits `@everyone`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<CreateGuildPlaceholderRole>>,
    /// Channels to create (categories before their children).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<CreateGuildPlaceholderChannel>>,
    /// Placeholder of an entry in [`Self::channels`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    /// Placeholder of an entry in [`Self::channels`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
}

/// Local reference inside one [`CreateGuild`] body.
///
/// The legacy create-guild route let a request name the roles and channels
/// it was creating with small integers (the `@everyone` entry was usually
/// `0`) so that channel `parent_id`, permission overwrites, and
/// `afk_channel_id`/`system_channel_id` could point at them before Discord
/// assigned real IDs. Those numbers are not snowflakes and mean nothing
/// outside the request, so they get this type instead of [`GenericId`](super::GenericId)
/// (docs/design/3.0-types-events-requests.md §1 exceptions). Serialized as a
/// JSON integer. Only used by the deprecated `RestClient::create_guild`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CreateGuildPlaceholderId(u64);

impl CreateGuildPlaceholderId {
    /// Wraps a placeholder number; `0` is allowed.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the placeholder number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Role entry of a [`CreateGuild`] body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderRole {
    /// Placeholder other entries use to refer to this role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
}

/// Channel entry of a [`CreateGuild`] body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderChannel {
    /// Placeholder other entries use to refer to this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CreateGuildPlaceholderId>,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChannelType>,
    /// Placeholder of a category channel listed earlier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_overwrites: Option<Vec<CreateGuildPlaceholderOverwrite>>,
}

/// Permission overwrite of a [`CreateGuildPlaceholderChannel`], targeting a
/// role placeholder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderOverwrite {
    /// Placeholder of a role in [`CreateGuild::roles`].
    pub id: CreateGuildPlaceholderId,
    /// Always [`OverwriteType::ROLE`] in practice: the legacy route only
    /// lets overwrites point at role placeholders.
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<PermissionsBitField>,
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
///
/// No `Default`: `level` is required and has no protocol default.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildMfaLevel {
    pub level: MfaLevel,
}

impl GuildMfaLevel {
    /// A body that sets the guild's moderation MFA requirement to `level`.
    pub const fn new(level: MfaLevel) -> Self {
        Self { level }
    }
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
    pub verification_level: Option<Option<VerificationLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<Option<DefaultMessageNotificationLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<Option<ExplicitContentFilterLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Option<ChannelId>>,
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
    pub system_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Option<ChannelId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Guild`.
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<UserId>,
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
    pub premium_tier: Option<PremiumTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_presences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanity_url_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<VerificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<DefaultMessageNotificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<ExplicitContentFilterLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_level: Option<MfaLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw_level: Option<GuildNsfwLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscription_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<ChannelId>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `CurrentUserGuild`.
pub struct CurrentUserGuild {
    pub id: GuildId,
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
    pub before: Option<GuildId>,
    pub after: Option<GuildId>,
    pub limit: Option<u64>,
    pub with_counts: Option<bool>,
    /// Returns only the guilds stored in this shard (`0` to
    /// `max_concurrency - 1` from Get Gateway Bot). Required since
    /// 2026-09-15 for apps using large bot sharding, which otherwise get a
    /// 400; this is not the gateway shard id.
    pub shard: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildPreview`.
pub struct GuildPreview {
    pub id: GuildId,
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
