//! Moderation resources: guild bans and auto-moderation rules.

use serde::{Deserialize, Serialize};

use super::{
    AutoModerationActionType, AutoModerationEventType, AutoModerationRuleId,
    AutoModerationTriggerType, ChannelId, GuildId, KeywordPresetType, RoleId, User, UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanResponse`.
pub struct BulkGuildBanResponse {
    #[serde(default)]
    pub banned_users: Vec<UserId>,
    #[serde(default)]
    pub failed_users: Vec<UserId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `AutoModerationRule`.
pub struct AutoModerationRule {
    pub id: AutoModerationRuleId,
    pub guild_id: GuildId,
    pub name: String,
    pub creator_id: UserId,
    pub event_type: AutoModerationEventType,
    pub trigger_type: AutoModerationTriggerType,
    #[serde(default)]
    pub trigger_metadata: AutoModerationTriggerMetadata,
    #[serde(default)]
    pub actions: Vec<AutoModerationAction>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub exempt_roles: Vec<RoleId>,
    #[serde(default)]
    pub exempt_channels: Vec<ChannelId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationTriggerMetadata`.
pub struct AutoModerationTriggerMetadata {
    #[serde(default)]
    pub keyword_filter: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub presets: Vec<KeywordPresetType>,
    #[serde(default)]
    pub allow_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_total_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_raid_protection_enabled: Option<bool>,
}

/// An auto moderation action. No `Default`: `type` is required and has no
/// default.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoModerationAction {
    #[serde(rename = "type")]
    pub kind: AutoModerationActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AutoModerationActionMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationActionMetadata`.
pub struct AutoModerationActionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
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
    pub before: Option<UserId>,
    pub after: Option<UserId>,
}
