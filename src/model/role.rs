//! Guild roles and role-management request bodies.

use serde::{Deserialize, Serialize};

use super::{IntegrationId, Permissions, RoleId, UserId};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleTags`.
pub struct RoleTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_id: Option<IntegrationId>,
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
    pub id: RoleId,
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
    pub permissions: Option<Permissions>,
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
