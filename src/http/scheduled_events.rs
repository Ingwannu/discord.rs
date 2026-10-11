use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    GenericId, GuildScheduledEvent, GuildScheduledEventUser, GuildScheduledEventUsersQuery,
    GuildScheduledEventsQuery,
};

use super::RestClient;

impl RestClient {
    pub async fn get_guild_scheduled_events(
        &self,
        guild_id: GenericId,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/scheduled-events", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Lists scheduled events with query options
    /// (`with_user_count=true` adds subscriber counts to each event).
    pub async fn get_guild_scheduled_events_with_query(
        &self,
        guild_id: GenericId,
        query: &GuildScheduledEventsQuery,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        let mut path = format!("/guilds/{}/scheduled-events", guild_id);
        if let Some(with_user_count) = query.with_user_count {
            path.push_str(&format!("?with_user_count={with_user_count}"));
        }
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn create_guild_scheduled_event(
        &self,
        guild_id: GenericId,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn create_guild_scheduled_event_typed<B>(
        &self,
        guild_id: GenericId,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn get_guild_scheduled_event(
        &self,
        guild_id: GenericId,
        event_id: GenericId,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_scheduled_event(
        &self,
        guild_id: GenericId,
        event_id: GenericId,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_scheduled_event_typed<B>(
        &self,
        guild_id: GenericId,
        event_id: GenericId,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_scheduled_event(
        &self,
        guild_id: GenericId,
        event_id: GenericId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/scheduled-events/{}", guild_id, event_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_scheduled_event_users(
        &self,
        guild_id: GenericId,
        event_id: GenericId,
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
        guild_id: GenericId,
        event_id: GenericId,
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
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }
}
