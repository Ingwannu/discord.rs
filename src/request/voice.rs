//! Voice state and stage instance request bodies.
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::model::{ChannelId, ScheduledEventId, StageInstancePrivacyLevel};
use serde::Serialize;

/// Request body for modifying the current user's guild voice state.
///
/// The outer `Option` controls omission. Use `Some(None)` to clear
/// `request_to_speak_timestamp`.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_to_speak_timestamp: Option<Option<String>>,
}

/// Request body for modifying another user's guild voice state.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `SetVoiceChannelStatus`.
pub struct SetVoiceChannelStatus {
    pub status: Option<String>,
}

/// Request body for creating a stage instance.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyStageInstance {
    /// The new topic (1-120 characters). Added in 3.0: the 2.x body could
    /// not change the topic, which Discord's Modify Stage Instance accepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_level: Option<StageInstancePrivacyLevel>,
}
