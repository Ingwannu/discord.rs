//! Auto moderation request bodies (3.0 phase 5 B).
//!
//! Bans keep their 2.x bodies in [`crate::model`] (`CreateGuildBan`,
//! `BulkGuildBanRequest`) until checkpoint 5C.

use serde::Serialize;

use super::fields::request_setters;
use crate::model::{
    AutoModerationAction, AutoModerationEventType, AutoModerationTriggerMetadata,
    AutoModerationTriggerType, ChannelId, Patch, RoleId,
};

/// Body of Create Auto Moderation Rule
/// (`POST /guilds/{id}/auto-moderation/rules`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateAutoModerationRule {
    pub name: String,
    pub event_type: AutoModerationEventType,
    pub trigger_type: AutoModerationTriggerType,
    /// Required for the keyword, keyword-preset, and mention-spam triggers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_metadata: Option<AutoModerationTriggerMetadata>,
    pub actions: Vec<AutoModerationAction>,
    /// Discord's default is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Up to 20 roles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exempt_roles: Option<Vec<RoleId>>,
    /// Up to 50 channels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exempt_channels: Option<Vec<ChannelId>>,
}

impl CreateAutoModerationRule {
    /// A rule with the fields Discord requires.
    pub fn new(
        name: impl Into<String>,
        event_type: AutoModerationEventType,
        trigger_type: AutoModerationTriggerType,
        actions: Vec<AutoModerationAction>,
    ) -> Self {
        Self {
            name: name.into(),
            event_type,
            trigger_type,
            trigger_metadata: None,
            actions,
            enabled: None,
            exempt_roles: None,
            exempt_channels: None,
        }
    }

    request_setters! {
        opt trigger_metadata: AutoModerationTriggerMetadata;
        opt enabled: bool;
        opt exempt_roles: Vec<RoleId>;
        opt exempt_channels: Vec<ChannelId>;
    }
}

/// Body of Modify Auto Moderation Rule
/// (`PATCH /guilds/{id}/auto-moderation/rules/{id}`). Every field is
/// optional; the trigger type cannot change.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyAutoModerationRule {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub event_type: Patch<AutoModerationEventType>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub trigger_metadata: Patch<AutoModerationTriggerMetadata>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub actions: Patch<Vec<AutoModerationAction>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub enabled: Patch<bool>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub exempt_roles: Patch<Vec<RoleId>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub exempt_channels: Patch<Vec<ChannelId>>,
}

impl ModifyAutoModerationRule {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        patch event_type: AutoModerationEventType;
        patch trigger_metadata: AutoModerationTriggerMetadata;
        patch actions: Vec<AutoModerationAction>;
        patch enabled: bool;
        patch exempt_roles: Vec<RoleId>;
        patch exempt_channels: Vec<ChannelId>;
    }
}
