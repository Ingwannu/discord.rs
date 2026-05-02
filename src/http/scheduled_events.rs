use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{GuildScheduledEvent, GuildScheduledEventUser, Snowflake};

use super::RestClient;

impl RestClient {
    pub async fn get_guild_scheduled_events(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_guild_scheduled_event_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn get_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_scheduled_event_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_scheduled_event_users(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<Vec<GuildScheduledEventUser>, DiscordError> {
        let path = match limit {
            Some(l) => format!(
                "/guilds/{}/scheduled-events/{}/users?limit={}",
                guild_id.into(),
                event_id.into(),
                l
            ),
            None => format!(
                "/guilds/{}/scheduled-events/{}/users",
                guild_id.into(),
                event_id.into()
            ),
        };
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }
}
