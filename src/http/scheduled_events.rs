//! Guild scheduled event endpoints.

use crate::error::DiscordError;
use crate::model::{
    GuildId, GuildScheduledEvent, GuildScheduledEventUser, GuildScheduledEventUsersQuery,
    GuildScheduledEventsQuery, ScheduledEventId,
};

use crate::request::{CreateGuildScheduledEvent, ModifyGuildScheduledEvent};

use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_guild_scheduled_events(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        self.send(
            RouteId::ListScheduledEventsForGuild,
            &format!("/guilds/{}/scheduled-events", guild_id),
            NO_BODY,
        )
        .await
    }

    /// Lists scheduled events with query options
    /// (`with_user_count=true` adds subscriber counts to each event).
    pub async fn get_guild_scheduled_events_with_query(
        &self,
        guild_id: GuildId,
        query: &GuildScheduledEventsQuery,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        let mut path = format!("/guilds/{}/scheduled-events", guild_id);
        if let Some(with_user_count) = query.with_user_count {
            path.push_str(&format!("?with_user_count={with_user_count}"));
        }
        self.send(RouteId::ListScheduledEventsForGuild, &path, NO_BODY)
            .await
    }

    pub async fn create_guild_scheduled_event(
        &self,
        guild_id: GuildId,
        body: &CreateGuildScheduledEvent,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.send(
            RouteId::CreateGuildScheduledEvent,
            &format!("/guilds/{}/scheduled-events", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn get_guild_scheduled_event(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.send(
            RouteId::GetGuildScheduledEvent,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_scheduled_event(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
        body: &ModifyGuildScheduledEvent,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.send(
            RouteId::ModifyGuildScheduledEvent,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_guild_scheduled_event(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildScheduledEvent,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_scheduled_event_users(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
        limit: Option<u64>,
    ) -> Result<Vec<GuildScheduledEventUser>, DiscordError> {
        self.get_guild_scheduled_event_users_with_query(
            guild_id,
            event_id,
            &GuildScheduledEventUsersQuery {
                limit,
                ..GuildScheduledEventUsersQuery::default()
            },
        )
        .await
    }

    /// Lists scheduled-event subscribers with the full set of query
    /// options: `limit`, `with_member`, and `before`/`after` pagination.
    pub async fn get_guild_scheduled_event_users_with_query(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
        query: &GuildScheduledEventUsersQuery,
    ) -> Result<Vec<GuildScheduledEventUser>, DiscordError> {
        let mut parameters = Vec::new();
        if let Some(limit) = query.limit {
            parameters.push(format!("limit={limit}"));
        }
        if let Some(with_member) = query.with_member {
            parameters.push(format!("with_member={with_member}"));
        }
        if let Some(before) = &query.before {
            parameters.push(format!("before={before}"));
        }
        if let Some(after) = &query.after {
            parameters.push(format!("after={after}"));
        }
        let mut path = format!("/guilds/{}/scheduled-events/{}/users", guild_id, event_id);
        if !parameters.is_empty() {
            path.push('?');
            path.push_str(&parameters.join("&"));
        }
        self.send(RouteId::GetGuildScheduledEventUsers, &path, NO_BODY)
            .await
    }
}
