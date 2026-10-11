//! Presences, activities, and presence updates.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{ApplicationId, UserId};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Presence`.
pub struct Presence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// Partial user object carried by gateway presence payloads
    /// (GUILD_CREATE `presences[]` and GUILD_MEMBERS_CHUNK `presences[]`
    /// identify the user this way instead of `user_id`). Only `id` is
    /// guaranteed by Discord.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<PresenceUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<Activity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_status: Option<ClientStatus>,
}

/// Partial user object inside gateway presence payloads; Discord only
/// guarantees `id` here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresenceUser {
    pub id: UserId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ClientStatus`.
pub struct ClientStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ActivityType`.
pub struct ActivityType(pub u8);

impl ActivityType {
    /// Public API item `PLAYING`.
    pub const PLAYING: Self = Self(0);
    /// Public API item `STREAMING`.
    pub const STREAMING: Self = Self(1);
    /// Public API item `LISTENING`.
    pub const LISTENING: Self = Self(2);
    /// Public API item `WATCHING`.
    pub const WATCHING: Self = Self(3);
    /// Public API item `CUSTOM`.
    pub const CUSTOM: Self = Self(4);
    /// Public API item `COMPETING`.
    pub const COMPETING: Self = Self(5);
}

impl Default for ActivityType {
    fn default() -> Self {
        Self::PLAYING
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityTimestamps`.
pub struct ActivityTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityParty`.
pub struct ActivityParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityAssets`.
pub struct ActivityAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivitySecrets`.
pub struct ActivitySecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_secret: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityButton`.
pub struct ActivityButton {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Activity`.
pub struct Activity {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: ActivityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<ActivityParty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<ActivityAssets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<ActivitySecrets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<ActivityButton>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UpdatePresence`.
pub struct UpdatePresence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default)]
    pub activities: Vec<Activity>,
    pub status: String,
    #[serde(default)]
    pub afk: bool,
}

impl UpdatePresence {
    /// Creates a `online_with_activity` value.
    pub fn online_with_activity(name: impl Into<String>) -> Self {
        Self {
            since: None,
            activities: vec![Activity {
                name: name.into(),
                kind: ActivityType::PLAYING,
                ..Activity::default()
            }],
            status: "online".to_string(),
            afk: false,
        }
    }
}
