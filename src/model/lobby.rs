//! Lobby resource: lobbies, lobby members, and lobby messages.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{ApplicationId, Channel, ChannelId, LobbyId, MessageId, User, UserId};

/// Lobby member object used by Discord's Lobby resource.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LobbyMember {
    /// User ID of the lobby member.
    pub id: UserId,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

impl LobbyMember {
    /// Member `id` with no metadata or flags.
    pub fn new(id: UserId) -> Self {
        Self {
            id,
            metadata: None,
            flags: None,
        }
    }
}

/// Lobby object used by Discord matchmaking APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lobby {
    /// Lobby ID.
    pub id: LobbyId,
    /// Application that owns the lobby.
    pub application_id: ApplicationId,
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Current lobby members.
    #[serde(default)]
    pub members: Vec<LobbyMember>,
    /// Text channel linked to this lobby, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_channel: Option<Channel>,
}

/// Request body for creating a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateLobby {
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Initial lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for modifying a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyLobby {
    /// Replacement app-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Replacement lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for adding or updating one lobby member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AddLobbyMember {
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Request body for Get or Create Lobby (`PUT /lobbies`, Bearer token).
///
/// Creates a lobby identified by `secret`, or joins the calling user to the
/// existing lobby with that secret.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GetOrCreateLobby {
    /// Lobby secret (max 250 characters).
    pub secret: String,
    /// Seconds before an idle lobby shuts down (5..=604800).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
    /// Lobby metadata; overwrites existing metadata (max total length 1000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lobby_metadata: Option<HashMap<String, String>>,
    /// Metadata for the calling user's lobby member (max total length 1000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_metadata: Option<HashMap<String, String>>,
}

/// Request body for Send Lobby Message (Bearer token).
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateLobbyMessage {
    /// Message content; must be non-empty.
    pub content: String,
    /// Key/value pairs delivered to active Social SDK clients; not persisted
    /// on a linked channel's copy of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Message flags; only flags the Social SDK may create are accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Author details captured on a lobby message.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyMessageMember {
    /// The author's lobby display name when the message was sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_name: Option<String>,
}

/// Message sent in a lobby.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LobbyMessage {
    pub id: MessageId,
    /// Message type.
    #[serde(rename = "type", default)]
    pub kind: u8,
    #[serde(default)]
    pub content: String,
    pub lobby_id: LobbyId,
    /// Equal to `lobby_id`; present for compatibility with message APIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lobby_member: Option<LobbyMessageMember>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation_metadata: Option<HashMap<String, String>>,
    #[serde(default)]
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
}

/// Single-use, one-hour guild invite to a lobby's linked channel.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyInvite {
    pub code: String,
}

/// Request body entry for bulk lobby member updates.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LobbyMemberUpdate {
    /// User ID to add, update, or remove.
    pub id: UserId,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Removes this member instead of upserting when true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_member: Option<bool>,
}

impl LobbyMemberUpdate {
    /// Update for member `id` that changes nothing until a field is set.
    pub fn new(id: UserId) -> Self {
        Self {
            id,
            metadata: None,
            flags: None,
            remove_member: None,
        }
    }
}

/// Request body for linking or unlinking a channel from a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LinkLobbyChannel {
    /// Channel to link. Omit this field to unlink any currently linked channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
}
