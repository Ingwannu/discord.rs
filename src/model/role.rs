//! Guild roles and role-management request bodies.

use serde::{Deserialize, Serialize};

use super::{GenericId, PermissionsBitField};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleTags`.
pub struct RoleTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscriber: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `RoleColors`.
pub struct RoleColors {
    pub primary_color: u64,
    pub secondary_color: Option<u64>,
    pub tertiary_color: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Role`.
pub struct Role {
    pub id: GenericId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<RoleTags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `ModifyGuildRolePosition`.
pub struct ModifyGuildRolePosition {
    pub id: GenericId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
}

/// Request body for creating a guild role.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
}

/// Request body for modifying a guild role.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as `icon` or `unicode_emoji`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Option<PermissionsBitField>>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<Option<RoleColors>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<Option<bool>>,
}
