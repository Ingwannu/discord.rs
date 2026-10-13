//! Lobby resource: lobbies, lobby members, and lobby messages.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{ApplicationId, Channel, ChannelId, LobbyId, MessageId, MessageType, User, UserId};

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
    /// Message type (required; 2.x read a missing `type` as `0`).
    #[serde(rename = "type")]
    pub kind: MessageType,
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
