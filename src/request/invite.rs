//! Invite request bodies.
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::model::{ApplicationId, InviteTargetType, RoleId, UserId};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateChannelInvite`.
pub struct CreateChannelInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<InviteTargetType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_ids: Option<Vec<RoleId>>,
    /// Users allowed to see and accept the invite (max 1000). Mutually
    /// exclusive with a `target_users_file` CSV upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_ids: Option<Vec<UserId>>,
}
