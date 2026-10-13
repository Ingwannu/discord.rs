//! Guild members and member-management request bodies.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    AvatarDecorationData, DisplayNameStyles, Permissions, RoleId, User, UserCollectibles, UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Member`.
pub struct Member {
    /// Shared with the cache's user store (see `CacheHandle::user_arc`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "super::shared::option_arc"
    )]
    pub user: Option<Arc<User>>,
    #[serde(default)]
    pub roles: Vec<RoleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communication_disabled_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Styles applied to the member's display (or nick) name; a preview
    /// field boxed like [`User::display_name_styles`] (unreleased
    /// discord-api-docs PR #8178, may change).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name_styles: Option<Box<DisplayNameStyles>>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildMembersQuery`.
pub struct GuildMembersQuery {
    pub limit: Option<u64>,
    pub after: Option<UserId>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SearchGuildMembersQuery`.
pub struct SearchGuildMembersQuery {
    pub query: String,
    pub limit: Option<u64>,
}
