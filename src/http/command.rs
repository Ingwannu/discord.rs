//! Application command endpoints (global and guild) and command permissions.

use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

use crate::command::CommandDefinition;
use crate::error::DiscordError;
use crate::model::{
    ApplicationCommand, CommandId, EditApplicationCommandPermissions,
    GuildApplicationCommandPermissions, GuildId,
};

use super::paths::{configured_application_id, global_commands_path};
use super::{validate_authorization_token, RequestAuthorization, RestClient};

impl RestClient {
    pub async fn bulk_overwrite_global_commands_typed(
        &self,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::PUT, &path, Some(commands)).await
    }

    pub async fn create_global_command(
        &self,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::POST, &path, Some(command)).await
    }

    pub async fn get_global_commands(&self) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn bulk_overwrite_guild_commands_typed(
        &self,
        guild_id: GuildId,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            Some(commands),
        )
        .await
    }

    pub async fn get_global_command(
        &self,
        command_id: CommandId,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("{}/{}", path, command_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_global_command(
        &self,
        command_id: CommandId,
        body: &Value,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!("{}/{}", path, command_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_global_command(&self, command_id: CommandId) -> Result<(), DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!("{}/{}", path, command_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_commands(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_command(
        &self,
        guild_id: GuildId,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            Some(command),
        )
        .await
    }

    pub async fn get_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
        body: &Value,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns command permissions for every application command in one guild.
    pub async fn get_guild_application_command_permissions(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns command permissions for one application command in one guild.
    pub async fn get_application_command_permissions(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
    ) -> Result<GuildApplicationCommandPermissions, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id, command_id
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Replaces all application command permissions in a guild using the legacy batch route.
    ///
    /// Discord marks this endpoint as disabled after Permissions v2, but it remains
    /// present in the official route table. Prefer [`Self::edit_application_command_permissions`]
    /// for active integrations.
    pub async fn batch_edit_application_command_permissions<B>(
        &self,
        bearer_token: &str,
        guild_id: GuildId,
        body: &B,
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError>
    where
        B: Serialize + ?Sized,
    {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id
            ),
            Some(body),
        )
        .await
    }

    /// Replaces command permissions using Discord's OAuth2 Bearer-token flow.
    ///
    /// Discord requires an access token with the
    /// `applications.commands.permissions.update` scope for this route. Bot
    /// tokens are not accepted by Discord for this write operation.
    pub async fn edit_application_command_permissions(
        &self,
        bearer_token: &str,
        guild_id: GuildId,
        command_id: CommandId,
        body: &EditApplicationCommandPermissions,
    ) -> Result<GuildApplicationCommandPermissions, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id, command_id
            ),
            Some(body),
        )
        .await
    }
}
