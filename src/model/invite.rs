//! Invites and invite target-user requests.

use serde::{Deserialize, Serialize};

use super::{GenericId, Role, User};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Invite`.
pub struct Invite {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
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
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_ids: Option<Vec<GenericId>>,
    /// Users allowed to see and accept the invite (max 1000). Mutually
    /// exclusive with a `target_users_file` CSV upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_ids: Option<Vec<GenericId>>,
}

/// JSON body for the invite target-user bulk-add and bulk-delete routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InviteTargetUserIds {
    /// Users to add or remove; Discord accepts at most 1000 per request.
    pub user_ids: Vec<GenericId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `InviteTargetUsersJobStatus`.
pub struct InviteTargetUsersJobStatus {
    pub status: u8,
    pub total_users: u64,
    pub processed_users: u64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}
