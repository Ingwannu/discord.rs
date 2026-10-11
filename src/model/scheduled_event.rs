//! Guild scheduled events, their recurrence rules, and query types.

use serde::{Deserialize, Serialize};

use super::{
    ChannelId, GenericId, GuildId, Member, RecurrenceRuleFrequency, RecurrenceRuleMonth,
    RecurrenceRuleWeekday, ScheduledEventEntityType, ScheduledEventId, ScheduledEventPrivacyLevel,
    ScheduledEventStatus, User, UserId,
};

/// Query options for `GET /guilds/{id}/scheduled-events`.
#[derive(Clone, Debug, Default)]
pub struct GuildScheduledEventsQuery {
    pub with_user_count: Option<bool>,
}

/// Query options for `GET /guilds/{id}/scheduled-events/{id}/users`.
#[derive(Clone, Debug, Default)]
pub struct GuildScheduledEventUsersQuery {
    pub limit: Option<u64>,
    pub with_member: Option<bool>,
    pub before: Option<UserId>,
    pub after: Option<UserId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildScheduledEvent`.
pub struct GuildScheduledEvent {
    pub id: ScheduledEventId,
    pub guild_id: GuildId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<UserId>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scheduled_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_end_time: Option<String>,
    pub privacy_level: ScheduledEventPrivacyLevel,
    pub status: ScheduledEventStatus,
    pub entity_type: ScheduledEventEntityType,
    /// ID of an entity tied to the event. Discord documents it only as "an
    /// entity" and does not say which kind, so it stays a [`GenericId`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_metadata: Option<GuildScheduledEventEntityMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_rule: Option<GuildScheduledEventRecurrenceRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventEntityMetadata`.
pub struct GuildScheduledEventEntityMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// "The `n`th `day` of the month" entry of a recurrence rule. No
/// `Default`: both fields are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuildScheduledEventRecurrenceRuleNWeekday {
    /// The week to reoccur on, 1-5.
    pub n: i8,
    pub day: RecurrenceRuleWeekday,
}

/// How a scheduled event repeats. No `Default`: `start`, `frequency`, and
/// `interval` are required and `frequency` has no default.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuildScheduledEventRecurrenceRule {
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub frequency: RecurrenceRuleFrequency,
    pub interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_weekday: Option<Vec<RecurrenceRuleWeekday>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_n_weekday: Option<Vec<GuildScheduledEventRecurrenceRuleNWeekday>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<Vec<RecurrenceRuleMonth>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month_day: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_year_day: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildScheduledEventUser`.
pub struct GuildScheduledEventUser {
    pub guild_scheduled_event_id: ScheduledEventId,
    pub user: User,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
}
