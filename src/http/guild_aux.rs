//! Auxiliary guild endpoints: integrations, prune, vanity URL, widget, welcome
//! screen, onboarding, and templates.

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    BeginGuildPruneRequest, GuildId, GuildOnboarding, GuildPruneCount, GuildPruneResult,
    GuildTemplate, GuildWidget, GuildWidgetImageStyle, GuildWidgetSettings, Integration,
    IntegrationId, ModifyGuildOnboarding, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings,
    RoleId, VanityUrl, WelcomeScreen,
};

use super::paths::{guild_prune_query, validate_token_path_segment};
use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_guild_integrations(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Integration>, DiscordError> {
        self.send(
            RouteId::GetGuildIntegrations,
            &format!("/guilds/{}/integrations", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn delete_guild_integration(
        &self,
        guild_id: GuildId,
        integration_id: IntegrationId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildIntegration,
            &format!("/guilds/{}/integrations/{}", guild_id, integration_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_prune_count(
        &self,
        guild_id: GuildId,
        days: Option<u64>,
        include_roles: &[RoleId],
    ) -> Result<serde_json::Value, DiscordError> {
        let query = guild_prune_query(days, None, include_roles);
        self.send(
            RouteId::GetGuildPruneCount,
            &format!("/guilds/{}/prune{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_prune_count_typed(
        &self,
        guild_id: GuildId,
        days: Option<u64>,
        include_roles: &[RoleId],
    ) -> Result<GuildPruneCount, DiscordError> {
        let query = guild_prune_query(days, None, include_roles);
        self.send(
            RouteId::GetGuildPruneCount,
            &format!("/guilds/{}/prune{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn begin_guild_prune(
        &self,
        guild_id: GuildId,
        days: Option<u64>,
        compute_prune_count: Option<bool>,
        include_roles: &[RoleId],
    ) -> Result<serde_json::Value, DiscordError> {
        let query = guild_prune_query(days, compute_prune_count, include_roles);
        self.send(
            RouteId::BeginGuildPrune,
            &format!("/guilds/{}/prune{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn begin_guild_prune_typed(
        &self,
        guild_id: GuildId,
        days: Option<u64>,
        compute_prune_count: Option<bool>,
        include_roles: &[RoleId],
    ) -> Result<GuildPruneResult, DiscordError> {
        let query = guild_prune_query(days, compute_prune_count, include_roles);
        self.send(
            RouteId::BeginGuildPrune,
            &format!("/guilds/{}/prune{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn begin_guild_prune_with_body<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<GuildPruneResult, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::BeginGuildPrune,
            &format!("/guilds/{}/prune", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn begin_guild_prune_with_request(
        &self,
        guild_id: GuildId,
        body: &BeginGuildPruneRequest,
    ) -> Result<GuildPruneResult, DiscordError> {
        self.begin_guild_prune_with_body(guild_id, body).await
    }

    pub async fn get_guild_vanity_url(&self, guild_id: GuildId) -> Result<VanityUrl, DiscordError> {
        self.send(
            RouteId::GetGuildVanityUrl,
            &format!("/guilds/{}/vanity-url", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_widget_settings(
        &self,
        guild_id: GuildId,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.send(
            RouteId::GetGuildWidgetSettings,
            &format!("/guilds/{}/widget", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_widget_settings(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    pub async fn modify_guild_widget_settings_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<GuildWidgetSettings, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyGuildWidget,
            &format!("/guilds/{}/widget", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_widget(
        &self,
        guild_id: GuildId,
        body: &ModifyGuildWidgetSettings,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    pub async fn get_guild_widget(
        &self,
        guild_id: GuildId,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::GetGuildWidget,
            &format!("/guilds/{}/widget.json", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_widget_typed(
        &self,
        guild_id: GuildId,
    ) -> Result<GuildWidget, DiscordError> {
        self.send(
            RouteId::GetGuildWidget,
            &format!("/guilds/{}/widget.json", guild_id),
            NO_BODY,
        )
        .await
    }

    /// Return the public PNG widget image for a guild.
    ///
    /// Discord documents this route as unauthenticated, so this helper omits
    /// the bot authorization header even when the client has a bot token.
    pub async fn get_guild_widget_image(
        &self,
        guild_id: GuildId,
        style: Option<GuildWidgetImageStyle>,
    ) -> Result<Vec<u8>, DiscordError> {
        let query = style
            .map(|style| format!("?style={}", style.as_str()))
            .unwrap_or_default();
        self.send_bytes(
            RouteId::GetGuildWidgetImage,
            &format!("/guilds/{}/widget.png{query}", guild_id),
        )
        .await
    }

    pub async fn get_guild_welcome_screen(
        &self,
        guild_id: GuildId,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.send(
            RouteId::GetGuildWelcomeScreen,
            &format!("/guilds/{}/welcome-screen", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_welcome_screen(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    pub async fn modify_guild_welcome_screen_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<WelcomeScreen, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyGuildWelcomeScreen,
            &format!("/guilds/{}/welcome-screen", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_welcome_screen_config(
        &self,
        guild_id: GuildId,
        body: &ModifyGuildWelcomeScreen,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    pub async fn get_guild_onboarding(
        &self,
        guild_id: GuildId,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.send(
            RouteId::GetGuildOnboarding,
            &format!("/guilds/{}/onboarding", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_onboarding(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    pub async fn modify_guild_onboarding_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<GuildOnboarding, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyGuildOnboarding,
            &format!("/guilds/{}/onboarding", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_onboarding_config(
        &self,
        guild_id: GuildId,
        body: &ModifyGuildOnboarding,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    pub async fn get_guild_templates(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<GuildTemplate>, DiscordError> {
        self.send(
            RouteId::GetGuildTemplates,
            &format!("/guilds/{}/templates", guild_id),
            NO_BODY,
        )
        .await
    }

    /// Fetches the `get_guild_template` resource by template code.
    pub async fn get_guild_template_by_code(
        &self,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.send(
            RouteId::GetGuildTemplate,
            &format!("/guilds/templates/{template_code}"),
            NO_BODY,
        )
        .await
    }

    pub async fn create_guild_template(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<GuildTemplate, DiscordError> {
        self.send(
            RouteId::CreateGuildTemplate,
            &format!("/guilds/{}/templates", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn sync_guild_template(
        &self,
        guild_id: GuildId,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.send(
            RouteId::SyncGuildTemplate,
            &format!("/guilds/{}/templates/{template_code}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_template(
        &self,
        guild_id: GuildId,
        template_code: &str,
        body: &Value,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.send(
            RouteId::ModifyGuildTemplate,
            &format!("/guilds/{}/templates/{template_code}", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_guild_template(
        &self,
        guild_id: GuildId,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.send(
            RouteId::DeleteGuildTemplate,
            &format!("/guilds/{}/templates/{template_code}", guild_id),
            NO_BODY,
        )
        .await
    }
}
