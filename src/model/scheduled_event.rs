//! Guild scheduled events, their recurrence rules, and query types.

use serde::{Deserialize, Serialize};

use super::{Member, Snowflake, User};

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
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEvent`.
pub struct GuildScheduledEvent {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<Snowflake>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scheduled_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_end_time: Option<String>,
    pub privacy_level: u8,
    pub status: u8,
    pub entity_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<Snowflake>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRuleNWeekday`.
pub struct GuildScheduledEventRecurrenceRuleNWeekday {
    pub n: i8,
    pub day: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRule`.
pub struct GuildScheduledEventRecurrenceRule {
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub frequency: u8,
    pub interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_weekday: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_n_weekday: Option<Vec<GuildScheduledEventRecurrenceRuleNWeekday>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month_day: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_year_day: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventUser`.
pub struct GuildScheduledEventUser {
    pub guild_scheduled_event_id: Snowflake,
    pub user: User,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
}
