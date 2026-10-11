//! Guild ban endpoints. Auto-moderation rules live in `auto_moderation.rs`.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{Ban, BulkGuildBanRequest, BulkGuildBanResponse, GuildBansQuery, Snowflake};

use super::paths::guild_bans_query;
use super::RestClient;

impl RestClient {
    pub async fn get_guild_bans(
        &self,
        guild_id: impl Into<Snowflake>,
        limit: Option<u64>,
        before: Option<Snowflake>,
    ) -> Result<Vec<Ban>, DiscordError> {
        self.get_guild_bans_with_query(
            guild_id,
            &GuildBansQuery {
                limit,
                before,
                ..GuildBansQuery::default()
            },
        )
        .await
    }

    pub async fn get_guild_bans_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &GuildBansQuery,
    ) -> Result<Vec<Ban>, DiscordError> {
        let query = guild_bans_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/bans{}", guild_id.into(), query),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<Ban, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.create_guild_ban_typed(guild_id, user_id, body).await
    }

    pub async fn create_guild_ban_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PUT,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn bulk_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &BulkGuildBanRequest,
    ) -> Result<BulkGuildBanResponse, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/bulk-ban", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn remove_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }
}
