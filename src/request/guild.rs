//! Guild request bodies: templates (3.0 phase 5 B) and the 2.x guild bodies
//! that moved here from `model` in checkpoint 5C ([`CreateGuild`] and its
//! placeholders, [`CreateGuildFromTemplate`], [`ModifyGuild`],
//! [`ModifyGuildIncidentActions`], [`BeginGuildPruneRequest`], widget,
//! welcome screen, and onboarding).

use serde::Serialize;

use super::fields::request_setters;
use crate::bitfield::Permissions;
use crate::model::{
    ChannelId, ChannelType, DefaultMessageNotificationLevel, ExplicitContentFilterLevel,
    NullablePatch, OnboardingMode, OnboardingPrompt, OverwriteType, Patch, RoleId,
    VerificationLevel, WelcomeScreenChannel,
};

/// Body of Create Guild Template (`POST /guilds/{id}/templates`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateGuildTemplate {
    /// 1-100 characters.
    pub name: String,
    /// 0-120 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl CreateGuildTemplate {
    /// A template named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
        }
    }

    request_setters! {
        opt description: String;
    }
}

/// Body of Modify Guild Template (`PATCH /guilds/{id}/templates/{code}`).
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyGuildTemplate {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub description: NullablePatch<String>,
}

impl ModifyGuildTemplate {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        nullable description / clear_description: String;
    }
}

/// Request body for `POST /guilds` (create guild; bot must be in fewer
/// than 10 guilds).
#[derive(Clone, Debug, Serialize, Default)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
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
#[derive(Clone, Debug, Default, Serialize)]
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
    pub permissions: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
}

/// Channel entry of a [`CreateGuild`] body.
#[derive(Clone, Debug, Default, Serialize)]
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
#[derive(Clone, Debug, Serialize)]
pub struct CreateGuildPlaceholderOverwrite {
    /// Placeholder of a role in [`CreateGuild::roles`].
    pub id: CreateGuildPlaceholderId,
    /// Always [`OverwriteType::ROLE`] in practice: the legacy route only
    /// lets overwrites point at role placeholders.
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Permissions>,
}

/// Request body for creating a guild from a guild template
/// (`POST /guilds/templates/{code}`).
#[derive(Clone, Debug, Serialize, Default)]
pub struct CreateGuildFromTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Request body for modifying a guild.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as image data, linked channel IDs, or
/// description.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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

/// Request body for modifying guild incident actions.
///
/// Supplying `None` serializes `null`, which disables that incident action.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyGuildIncidentActions {
    /// Timestamp when invites should be re-enabled, or `null` to disable invite blocking.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages should be re-enabled, or `null` to disable DM blocking.
    pub dms_disabled_until: Option<String>,
}

/// Request body for beginning a guild prune.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct BeginGuildPruneRequest {
    /// Number of inactive days to prune, between 1 and 30.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u64>,
    /// Whether Discord should compute and return the prune count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_prune_count: Option<bool>,
    /// Role IDs to include in the prune.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_roles: Option<Vec<RoleId>>,
    /// Deprecated Discord field for audit-log prune reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request body for modifying guild widget settings.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWidgetSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Option<ChannelId>>,
}

/// Request body for modifying a guild welcome screen.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_channels: Option<Option<Vec<WelcomeScreenChannel>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
}

/// Request body for modifying guild onboarding.
#[derive(Clone, Debug, Serialize, Default, PartialEq)]
pub struct ModifyGuildOnboarding {
    /// Prompts shown during onboarding and in Customize Community.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<OnboardingPrompt>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_channel_ids: Option<Vec<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<OnboardingMode>,
}
