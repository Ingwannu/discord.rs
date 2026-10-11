//! Guild endpoints: fetch/modify a guild, guild preview, incident actions, and
//! the deprecated guild-creation/ownership routes.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    GetGuildQuery, Guild, GuildIncidentsData, GuildPreview, ModifyGuild,
    ModifyGuildIncidentActions, Snowflake,
};

use super::paths::{self, get_guild_query};
use super::RestClient;

impl RestClient {
    pub async fn get_guild(&self, guild_id: impl Into<Snowflake>) -> Result<Guild, DiscordError> {
        self.get_guild_with_query(guild_id, &GetGuildQuery::default())
            .await
    }

    pub async fn get_guild_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &GetGuildQuery,
    ) -> Result<Guild, DiscordError> {
        let query = get_guild_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    pub async fn update_guild_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Guild, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuild,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    pub async fn get_guild_preview(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::GET,
            &format!("/guilds/{}/preview", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_preview_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<GuildPreview, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/preview", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_incident_actions(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildIncidentActions,
    ) -> Result<GuildIncidentsData, DiscordError> {
        self.request_typed(
            Method::PUT,
            &format!("/guilds/{}/incident-actions", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Creates a guild via `POST /guilds`.
    ///
    /// Discord stopped allowing applications to create guilds on 2025-07-15
    /// and removed the route from its docs; kept only so 3.0 migrations get
    /// a deprecation warning instead of a missing method.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn create_guild(
        &self,
        body: &crate::model::CreateGuild,
    ) -> Result<Guild, DiscordError> {
        self.request_typed(Method::POST, "/guilds", Some(body))
            .await
    }

    /// Deletes a guild via `DELETE /guilds/{id}`. Requires guild ownership,
    /// which bots lost when Discord transferred bot-owned guilds to users in
    /// 2025.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn delete_guild(&self, guild_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Creates a guild from a guild template via
    /// `POST /guilds/templates/{code}`, which shares the removed
    /// guild-creation capability.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn create_guild_from_template(
        &self,
        template_code: &str,
        body: &crate::model::CreateGuildFromTemplate,
    ) -> Result<Guild, DiscordError> {
        let template_code = template_code.trim();
        paths::validate_template_code(template_code)?;
        self.request_typed(
            Method::POST,
            &format!("/guilds/templates/{template_code}"),
            Some(body),
        )
        .await
    }

    /// Sets the guild's required MFA level for moderation actions via
    /// `POST /guilds/{id}/mfa` and returns the updated level. Owner-only,
    /// so unusable by bots since bot-owned guilds were transferred in 2025.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn modify_guild_mfa_level(
        &self,
        guild_id: impl Into<Snowflake>,
        level: u64,
    ) -> Result<crate::model::GuildMfaLevel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/mfa", guild_id.into()),
            Some(&crate::model::GuildMfaLevel { level }),
        )
        .await
    }
}
