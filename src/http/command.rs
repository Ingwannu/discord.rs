//! Application command endpoints (global and guild) and command permissions.

use crate::command::CommandDefinition;
use crate::error::DiscordError;
use crate::model::{ApplicationCommand, CommandId, GuildApplicationCommandPermissions, GuildId};
use crate::request::EditApplicationCommandPermissions;

use crate::request::EditApplicationCommand;

use super::paths::{configured_application_id, global_commands_path};
use super::{json_body, validate_authorization_token, RestClient, RouteId, NO_BODY};

impl RestClient {
    /// Replaces every global command with `commands` (commands missing from
    /// the list are deleted).
    pub async fn bulk_overwrite_global_commands(
        &self,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send(
            RouteId::BulkOverwriteGlobalApplicationCommands,
            &path,
            json_body(commands),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `bulk_overwrite_global_commands`")]
    pub async fn bulk_overwrite_global_commands_typed(
        &self,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        self.bulk_overwrite_global_commands(commands).await
    }

    pub async fn create_global_command(
        &self,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send(
            RouteId::CreateGlobalApplicationCommand,
            &path,
            json_body(command),
        )
        .await
    }

    pub async fn get_global_commands(&self) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send(RouteId::GetGlobalApplicationCommands, &path, NO_BODY)
            .await
    }

    /// Replaces every command of `guild_id` with `commands`.
    pub async fn bulk_overwrite_guild_commands(
        &self,
        guild_id: GuildId,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::BulkOverwriteGuildApplicationCommands,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            json_body(commands),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `bulk_overwrite_guild_commands`")]
    pub async fn bulk_overwrite_guild_commands_typed(
        &self,
        guild_id: GuildId,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        self.bulk_overwrite_guild_commands(guild_id, commands).await
    }

    pub async fn get_global_command(
        &self,
        command_id: CommandId,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send(
            RouteId::GetGlobalApplicationCommand,
            &format!("{}/{}", path, command_id),
            NO_BODY,
        )
        .await
    }

    pub async fn edit_global_command(
        &self,
        command_id: CommandId,
        body: &EditApplicationCommand,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send(
            RouteId::EditGlobalApplicationCommand,
            &format!("{}/{}", path, command_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_global_command(&self, command_id: CommandId) -> Result<(), DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.send_unit(
            RouteId::DeleteGlobalApplicationCommand,
            &format!("{}/{}", path, command_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_commands(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetGuildApplicationCommands,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn create_guild_command(
        &self,
        guild_id: GuildId,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::CreateGuildApplicationCommand,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id
            ),
            json_body(command),
        )
        .await
    }

    pub async fn get_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetGuildApplicationCommand,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn edit_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
        body: &EditApplicationCommand,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::EditGuildApplicationCommand,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            json_body(body),
        )
        .await
    }

    pub async fn delete_guild_command(
        &self,
        guild_id: GuildId,
        command_id: CommandId,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send_unit(
            RouteId::DeleteGuildApplicationCommand,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id, command_id
            ),
            NO_BODY,
        )
        .await
    }

    /// Returns command permissions for every application command in one guild.
    pub async fn get_guild_application_command_permissions(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetGuildApplicationCommandPermissions,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id
            ),
            NO_BODY,
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
        self.send(
            RouteId::GetApplicationCommandPermissions,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id, command_id
            ),
            NO_BODY,
        )
        .await
    }

    /// Replaces all application command permissions in a guild using the
    /// legacy batch route.
    ///
    /// Discord disabled this endpoint with Permissions v2; it stays because
    /// the route is still in Discord's route table. Use
    /// [`Self::edit_application_command_permissions`] per command instead.
    #[deprecated(
        since = "3.0.0",
        note = "Discord disabled batch edits with Permissions v2; use `edit_application_command_permissions`"
    )]
    pub async fn batch_edit_application_command_permissions(
        &self,
        bearer_token: &str,
        guild_id: GuildId,
        permissions: &[GuildApplicationCommandPermissions],
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.send_bearer(
            RouteId::BatchEditApplicationCommandPermissions,
            bearer_token,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id
            ),
            json_body(permissions),
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
        self.send_bearer(
            RouteId::EditApplicationCommandPermissions,
            bearer_token,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id, command_id
            ),
            json_body(body),
        )
        .await
    }
}
