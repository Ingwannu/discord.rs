//! Guild role request bodies.
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::bitfield::Permissions;
use crate::model::{RoleColors, RoleId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
/// Typed Discord API object for `ModifyGuildRolePosition`.
pub struct ModifyGuildRolePosition {
    pub id: RoleId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
}

impl ModifyGuildRolePosition {
    /// Entry for role `id` that changes nothing until `position` is set.
    pub fn new(id: RoleId) -> Self {
        Self { id, position: None }
    }
}

/// Request body for creating a guild role.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct CreateGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Option<Permissions>>,
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
