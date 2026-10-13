//! Guild endpoints: fetch/modify a guild, guild preview, incident actions, and
//! the deprecated guild-creation/ownership routes.

use crate::error::DiscordError;
use crate::model::{GetGuildQuery, Guild, GuildId, GuildIncidentsData, GuildPreview};
use crate::request::{ModifyGuild, ModifyGuildIncidentActions};

use super::paths::{self, get_guild_query};
use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_guild(&self, guild_id: GuildId) -> Result<Guild, DiscordError> {
        self.get_guild_with_query(guild_id, &GetGuildQuery::default())
            .await
    }

    pub async fn get_guild_with_query(
        &self,
        guild_id: GuildId,
        query: &GetGuildQuery,
    ) -> Result<Guild, DiscordError> {
        let query = get_guild_query(query);
        self.send(
            RouteId::GetGuild,
            &format!("/guilds/{}{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild(
        &self,
        guild_id: GuildId,
        body: &ModifyGuild,
    ) -> Result<Guild, DiscordError> {
        self.send(
            RouteId::ModifyGuild,
            &format!("/guilds/{}", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn get_guild_preview(&self, guild_id: GuildId) -> Result<GuildPreview, DiscordError> {
        self.send(
            RouteId::GetGuildPreview,
            &format!("/guilds/{}/preview", guild_id),
            NO_BODY,
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `get_guild_preview`")]
    pub async fn get_guild_preview_typed(
        &self,
        guild_id: GuildId,
    ) -> Result<GuildPreview, DiscordError> {
        self.get_guild_preview(guild_id).await
    }

    pub async fn modify_guild_incident_actions(
        &self,
        guild_id: GuildId,
        body: &ModifyGuildIncidentActions,
    ) -> Result<GuildIncidentsData, DiscordError> {
        self.send(
            RouteId::ModifyGuildIncidentActions,
            &format!("/guilds/{}/incident-actions", guild_id),
            json_body(body),
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
        body: &crate::request::CreateGuild,
    ) -> Result<Guild, DiscordError> {
        self.send(RouteId::CreateGuild, "/guilds", json_body(body))
            .await
    }

    /// Deletes a guild via `DELETE /guilds/{id}`. Requires guild ownership,
    /// which bots lost when Discord transferred bot-owned guilds to users in
    /// 2025.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn delete_guild(&self, guild_id: GuildId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuild,
            &format!("/guilds/{}", guild_id),
            NO_BODY,
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
        body: &crate::request::CreateGuildFromTemplate,
    ) -> Result<Guild, DiscordError> {
        let template_code = template_code.trim();
        paths::validate_template_code(template_code)?;
        self.send(
            RouteId::CreateGuildFromTemplate,
            &format!("/guilds/templates/{template_code}"),
            json_body(body),
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
        guild_id: GuildId,
        level: crate::model::MfaLevel,
    ) -> Result<crate::model::GuildMfaLevel, DiscordError> {
        self.send(
            RouteId::ModifyGuildMfaLevel,
            &format!("/guilds/{}/mfa", guild_id),
            json_body(&crate::model::GuildMfaLevel::new(level)),
        )
        .await
    }
}
