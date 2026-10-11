//! Voice states, voice regions, voice channel status, and stage instances.

use serde::{Deserialize, Serialize};

use super::{
    ChannelId, GuildId, Member, ScheduledEventId, StageInstanceId, StageInstancePrivacyLevel,
    UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceState`.
pub struct VoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub deaf: bool,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub self_deaf: bool,
    #[serde(default)]
    pub self_mute: bool,
    #[serde(default)]
    pub suppress: bool,
    #[serde(default)]
    pub self_stream: bool,
    #[serde(default)]
    pub self_video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_to_speak_timestamp: Option<String>,
}

/// Request body for modifying the current user's guild voice state.
///
/// The outer `Option` controls omission. Use `Some(None)` to clear
/// `request_to_speak_timestamp`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_to_speak_timestamp: Option<Option<String>>,
}

/// Request body for modifying another user's guild voice state.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `VoiceServerUpdate`.
pub struct VoiceServerUpdate {
    pub guild_id: GuildId,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceRegion`.
pub struct VoiceRegion {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub optimal: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub custom: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `SetVoiceChannelStatus`.
pub struct SetVoiceChannelStatus {
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `StageInstance`.
pub struct StageInstance {
    pub id: StageInstanceId,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub topic: String,
    pub privacy_level: StageInstancePrivacyLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event_id: Option<ScheduledEventId>,
}

/// Request body for creating a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateStageInstance {
    pub channel_id: ChannelId,
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_level: Option<StageInstancePrivacyLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_start_notification: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event_id: Option<ScheduledEventId>,
}

impl CreateStageInstance {
    /// Starts a stage in `channel_id` with `topic`; optional fields unset.
    pub fn new(channel_id: ChannelId, topic: impl Into<String>) -> Self {
        Self {
            channel_id,
            topic: topic.into(),
            privacy_level: None,
            send_start_notification: None,
            guild_scheduled_event_id: None,
        }
    }
}

/// Request body for modifying a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyStageInstance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_level: Option<StageInstancePrivacyLevel>,
}
