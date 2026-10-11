//! Gateway REST responses and gateway command payloads.

use serde::{Deserialize, Serialize};

use super::{GuildId, UserId};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SessionStartLimit`.
pub struct SessionStartLimit {
    pub total: u32,
    pub remaining: u32,
    pub reset_after: u64,
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Gateway`.
pub struct Gateway {
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GatewayBot`.
pub struct GatewayBot {
    pub url: String,
    pub shards: u32,
    pub session_start_limit: SessionStartLimit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RequestGuildMembers`.
pub struct RequestGuildMembers {
    pub guild_id: GuildId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presences: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<UserId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Gateway request for ephemeral channel metadata.
///
/// Discord currently documents `status` and `voice_start_time` as requestable
/// fields for voice-channel metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestChannelInfo {
    /// Guild whose channel metadata should be requested.
    pub guild_id: GuildId,
    /// Metadata fields to request.
    pub fields: Vec<String>,
}

impl RequestChannelInfo {
    /// Channel status field name.
    pub const STATUS: &'static str = "status";
    /// Voice channel start-time field name.
    pub const VOICE_START_TIME: &'static str = "voice_start_time";

    /// Creates a channel-info request for explicit field names.
    pub fn new<I, S>(guild_id: GuildId, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            guild_id,
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }

    /// Creates a request for Discord's documented voice metadata fields.
    pub fn voice_metadata(guild_id: GuildId) -> Self {
        Self {
            guild_id,
            fields: vec![Self::STATUS.to_string(), Self::VOICE_START_TIME.to_string()],
        }
    }
}
