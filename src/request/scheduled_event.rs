//! Guild scheduled event request bodies (3.0 phase 5 B).

use serde::Serialize;

use super::fields::request_setters;
use crate::model::{
    ChannelId, GuildScheduledEventEntityMetadata, GuildScheduledEventRecurrenceRule, NullablePatch,
    Patch, ScheduledEventEntityType, ScheduledEventPrivacyLevel, ScheduledEventStatus,
};

/// Body of Create Guild Scheduled Event
/// (`POST /guilds/{id}/scheduled-events`).
///
/// Stage and voice events need `channel_id`; external events need
/// `entity_metadata.location` and `scheduled_end_time` instead.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateGuildScheduledEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_metadata: Option<GuildScheduledEventEntityMetadata>,
    pub name: String,
    pub privacy_level: ScheduledEventPrivacyLevel,
    /// ISO 8601 timestamp.
    pub scheduled_start_time: String,
    /// ISO 8601 timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub entity_type: ScheduledEventEntityType,
    /// Cover image as image data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_rule: Option<GuildScheduledEventRecurrenceRule>,
}

impl CreateGuildScheduledEvent {
    /// An event with the fields every entity type requires.
    pub fn new(
        name: impl Into<String>,
        entity_type: ScheduledEventEntityType,
        privacy_level: ScheduledEventPrivacyLevel,
        scheduled_start_time: impl Into<String>,
    ) -> Self {
        Self {
            channel_id: None,
            entity_metadata: None,
            name: name.into(),
            privacy_level,
            scheduled_start_time: scheduled_start_time.into(),
            scheduled_end_time: None,
            description: None,
            entity_type,
            image: None,
            recurrence_rule: None,
        }
    }

    request_setters! {
        opt channel_id: ChannelId;
        opt entity_metadata: GuildScheduledEventEntityMetadata;
        opt scheduled_end_time: String;
        opt description: String;
        opt image: String;
        opt recurrence_rule: GuildScheduledEventRecurrenceRule;
    }
}

/// Body of Modify Guild Scheduled Event
/// (`PATCH /guilds/{id}/scheduled-events/{id}`). Changing the entity type to
/// external also needs `channel_id: null`, a location, and an end time.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyGuildScheduledEvent {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub channel_id: NullablePatch<ChannelId>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub entity_metadata: NullablePatch<GuildScheduledEventEntityMetadata>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub privacy_level: Patch<ScheduledEventPrivacyLevel>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub scheduled_start_time: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub scheduled_end_time: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub description: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub entity_type: Patch<ScheduledEventEntityType>,
    /// Start (`ACTIVE`), end (`COMPLETED`), or cancel (`CANCELED`) the event.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub status: Patch<ScheduledEventStatus>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub image: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub recurrence_rule: NullablePatch<GuildScheduledEventRecurrenceRule>,
}

impl ModifyGuildScheduledEvent {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        nullable channel_id / clear_channel_id: ChannelId;
        nullable entity_metadata / clear_entity_metadata: GuildScheduledEventEntityMetadata;
        patch name: String;
        patch privacy_level: ScheduledEventPrivacyLevel;
        patch scheduled_start_time: String;
        patch scheduled_end_time: String;
        nullable description / clear_description: String;
        patch entity_type: ScheduledEventEntityType;
        patch status: ScheduledEventStatus;
        patch image: String;
        nullable recurrence_rule / clear_recurrence_rule: GuildScheduledEventRecurrenceRule;
    }
}
