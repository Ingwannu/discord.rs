//! Moderation resources: guild bans and auto-moderation rules.

use serde::{Deserialize, Serialize};

use super::{Snowflake, User};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanRequest`.
pub struct BulkGuildBanRequest {
    pub user_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

/// Request body for banning a single guild member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildBan {
    /// Deprecated Discord field for message deletion duration in days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_days: Option<u64>,
    /// Message deletion duration in seconds, between 0 and 604800.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanResponse`.
pub struct BulkGuildBanResponse {
    #[serde(default)]
    pub banned_users: Vec<Snowflake>,
    #[serde(default)]
    pub failed_users: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationRule`.
pub struct AutoModerationRule {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub creator_id: Snowflake,
    pub event_type: u8,
    pub trigger_type: u8,
    #[serde(default)]
    pub trigger_metadata: AutoModerationTriggerMetadata,
    #[serde(default)]
    pub actions: Vec<AutoModerationAction>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub exempt_roles: Vec<Snowflake>,
    #[serde(default)]
    pub exempt_channels: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationTriggerMetadata`.
pub struct AutoModerationTriggerMetadata {
    #[serde(default)]
    pub keyword_filter: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub presets: Vec<u8>,
    #[serde(default)]
    pub allow_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_total_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_raid_protection_enabled: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationAction`.
pub struct AutoModerationAction {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AutoModerationActionMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationActionMetadata`.
pub struct AutoModerationActionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Ban`.
pub struct Ban {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildBansQuery`.
pub struct GuildBansQuery {
    pub limit: Option<u64>,
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
}
