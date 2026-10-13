//! Lobby request bodies (Social SDK lobbies).
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::model::{ChannelId, LobbyMember};
use serde::Serialize;
use std::collections::HashMap;

/// Request body for creating a lobby.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
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

/// Request body for linking or unlinking a channel from a lobby.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct LinkLobbyChannel {
    /// Channel to link. Omit this field to unlink any currently linked channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
}
