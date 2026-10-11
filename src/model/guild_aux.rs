//! Auxiliary guild resources: vanity URL, prune, integrations, widget,
//! welcome screen, templates, and onboarding.

use serde::{Deserialize, Serialize};

use super::{GenericId, User, WidgetUserId};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VanityUrl`.
pub struct VanityUrl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPruneCount`.
pub struct GuildPruneCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruned: Option<u64>,
}

/// Type alias for `GuildPruneResult`.
pub type GuildPruneResult = GuildPruneCount;

/// Request body for beginning a guild prune.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BeginGuildPruneRequest {
    /// Number of inactive days to prune, between 1 and 30.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u64>,
    /// Whether Discord should compute and return the prune count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_prune_count: Option<bool>,
    /// Role IDs to include in the prune.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_roles: Option<Vec<GenericId>>,
    /// Deprecated Discord field for audit-log prune reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationAccount`.
pub struct IntegrationAccount {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `IntegrationApplication`.
pub struct IntegrationApplication {
    pub id: GenericId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Integration`.
pub struct Integration {
    pub id: GenericId,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syncing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_emoticons: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_behavior: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_grace_period: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    pub account: IntegrationAccount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriber_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<IntegrationApplication>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetSettings`.
pub struct GuildWidgetSettings {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<GenericId>,
}

/// Request body for modifying guild widget settings.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWidgetSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Option<GenericId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidgetChannel`.
pub struct GuildWidgetChannel {
    pub id: GenericId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidgetMember`.
pub struct GuildWidgetMember {
    /// Anonymized by Discord (often `"0"`); not a user ID. See
    /// [`WidgetUserId`].
    pub id: WidgetUserId,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidget`.
pub struct GuildWidget {
    pub id: GenericId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instant_invite: Option<String>,
    #[serde(default)]
    pub channels: Vec<GuildWidgetChannel>,
    #[serde(default)]
    pub members: Vec<GuildWidgetMember>,
    #[serde(default)]
    pub presence_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GuildWidgetImageStyle`.
pub enum GuildWidgetImageStyle {
    /// Discord API enum variant `Shield`.
    Shield,
    /// Discord API enum variant `Banner1`.
    Banner1,
    /// Discord API enum variant `Banner2`.
    Banner2,
    /// Discord API enum variant `Banner3`.
    Banner3,
    /// Discord API enum variant `Banner4`.
    Banner4,
}

impl GuildWidgetImageStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Banner1 => "banner1",
            Self::Banner2 => "banner2",
            Self::Banner3 => "banner3",
            Self::Banner4 => "banner4",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `WelcomeScreenChannel`.
pub struct WelcomeScreenChannel {
    pub channel_id: GenericId,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `WelcomeScreen`.
pub struct WelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub welcome_channels: Vec<WelcomeScreenChannel>,
}

/// Request body for modifying a guild welcome screen.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_channels: Option<Option<Vec<WelcomeScreenChannel>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildTemplate`.
pub struct GuildTemplate {
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub usage_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<User>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_guild_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialized_source_guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dirty: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildOnboarding`.
pub struct GuildOnboarding {
    pub guild_id: GenericId,
    #[serde(default)]
    pub prompts: Vec<serde_json::Value>,
    #[serde(default)]
    pub default_channel_ids: Vec<GenericId>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u8>,
}

/// Request body for modifying guild onboarding.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ModifyGuildOnboarding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_channel_ids: Option<Vec<GenericId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u8>,
}
