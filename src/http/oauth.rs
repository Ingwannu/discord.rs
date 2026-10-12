use super::paths::{configured_application_id, current_user_guilds_query};
use super::{json_body, validate_authorization_token, RestClient, RouteId, NO_BODY};
use crate::error::DiscordError;
use crate::model::{
    Application, AuthorizationInformation, CurrentUserGuild, CurrentUserGuildsQuery, GuildId,
    Member, ModifyCurrentUser, UpdateUserApplicationRoleConnection, User,
    UserApplicationRoleConnection, UserConnection, UserId,
};

impl RestClient {
    pub async fn get_current_user(&self) -> Result<User, DiscordError> {
        self.send(RouteId::GetCurrentUser, "/users/@me", NO_BODY)
            .await
    }

    /// Modifies the current bot user.
    pub async fn modify_current_user(
        &self,
        body: &ModifyCurrentUser,
    ) -> Result<User, DiscordError> {
        self.send(RouteId::ModifyCurrentUser, "/users/@me", json_body(body))
            .await
    }

    pub async fn get_user(&self, user_id: UserId) -> Result<User, DiscordError> {
        self.send(RouteId::GetUser, &format!("/users/{}", user_id), NO_BODY)
            .await
    }

    /// The guilds the bot (or OAuth2 user) is in, up to 200.
    pub async fn get_current_user_guilds(&self) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        self.get_current_user_guilds_with_query(&CurrentUserGuildsQuery::default())
            .await
    }

    /// [`Self::get_current_user_guilds`] with pagination and counts.
    pub async fn get_current_user_guilds_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        let query = current_user_guilds_query(query);
        self.send(
            RouteId::GetCurrentUserGuilds,
            &format!("/users/@me/guilds{query}"),
            NO_BODY,
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `get_current_user_guilds`")]
    pub async fn get_current_user_guilds_typed(
        &self,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        self.get_current_user_guilds().await
    }

    #[deprecated(
        since = "3.0.0",
        note = "renamed to `get_current_user_guilds_with_query`"
    )]
    pub async fn get_current_user_guilds_typed_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        self.get_current_user_guilds_with_query(query).await
    }

    /// Returns the current OAuth2 user's guild member object for one guild.
    pub async fn get_current_user_guild_member(
        &self,
        bearer_token: &str,
        guild_id: GuildId,
    ) -> Result<Member, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::GetCurrentUserGuildMember,
            bearer_token,
            &format!("/users/@me/guilds/{}/member", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn leave_guild(&self, guild_id: GuildId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::LeaveGuild,
            &format!("/users/@me/guilds/{}", guild_id),
            NO_BODY,
        )
        .await
    }

    /// Returns OAuth2 connections for the current user.
    pub async fn get_current_user_connections(
        &self,
        bearer_token: &str,
    ) -> Result<Vec<UserConnection>, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::GetCurrentUserConnections,
            bearer_token,
            "/users/@me/connections",
            NO_BODY,
        )
        .await
    }

    /// Returns the current user's application role connection for this client application.
    pub async fn get_current_user_application_role_connection(
        &self,
        bearer_token: &str,
    ) -> Result<UserApplicationRoleConnection, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.send_bearer(
            RouteId::GetCurrentUserApplicationRoleConnection,
            bearer_token,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            NO_BODY,
        )
        .await
    }

    /// Updates the current user's application role connection for this client application.
    pub async fn update_current_user_application_role_connection(
        &self,
        bearer_token: &str,
        body: &UpdateUserApplicationRoleConnection,
    ) -> Result<UserApplicationRoleConnection, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.send_bearer(
            RouteId::UpdateCurrentUserApplicationRoleConnection,
            bearer_token,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            json_body(body),
        )
        .await
    }

    /// Deletes the current user's application role connection for this
    /// client application (Bearer token with `role_connections.write`).
    pub async fn delete_current_user_application_role_connection(
        &self,
        bearer_token: &str,
    ) -> Result<(), DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.send_bearer_unit(
            RouteId::DeleteCurrentUserApplicationRoleConnection,
            bearer_token,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            NO_BODY,
        )
        .await
        .map(|_| ())
    }

    /// Returns the bot application's OAuth2 application object.
    pub async fn get_current_bot_application_information(
        &self,
    ) -> Result<Application, DiscordError> {
        self.send(
            RouteId::GetCurrentBotApplicationInformation,
            "/oauth2/applications/@me",
            NO_BODY,
        )
        .await
    }

    /// Returns OAuth2 authorization information for a bearer token.
    pub async fn get_current_authorization_information(
        &self,
        bearer_token: &str,
    ) -> Result<AuthorizationInformation, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::GetCurrentAuthorizationInformation,
            bearer_token,
            "/oauth2/@me",
            NO_BODY,
        )
        .await
    }
}
