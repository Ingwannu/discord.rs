use reqwest::Method;
use serde_json::Value;

use super::paths::{configured_application_id, current_user_guilds_query};
use super::{validate_authorization_token, RequestAuthorization, RestClient};
use crate::error::DiscordError;
use crate::model::{
    Application, AuthorizationInformation, CurrentUserGuild, CurrentUserGuildsQuery, Member,
    ModifyCurrentUser, Snowflake, UpdateUserApplicationRoleConnection, User,
    UserApplicationRoleConnection, UserConnection,
};

impl RestClient {
    pub async fn get_current_user(&self) -> Result<User, DiscordError> {
        self.request_typed(Method::GET, "/users/@me", Option::<&Value>::None)
            .await
    }

    /// Modifies the current bot user.
    pub async fn modify_current_user(
        &self,
        body: &ModifyCurrentUser,
    ) -> Result<User, DiscordError> {
        self.request_typed(Method::PATCH, "/users/@me", Some(body))
            .await
    }

    pub async fn get_user(&self, user_id: impl Into<Snowflake>) -> Result<User, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_current_user_guilds(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.get_current_user_guilds_with_query(&CurrentUserGuildsQuery::default())
            .await
    }

    pub async fn get_current_user_guilds_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        let query = current_user_guilds_query(query);
        self.request_typed(
            Method::GET,
            &format!("/users/@me/guilds{query}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_current_user_guilds_typed(
        &self,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        self.get_current_user_guilds_typed_with_query(&CurrentUserGuildsQuery::default())
            .await
    }

    pub async fn get_current_user_guilds_typed_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        let query = current_user_guilds_query(query);
        self.request_typed(
            Method::GET,
            &format!("/users/@me/guilds{query}"),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns the current OAuth2 user's guild member object for one guild.
    pub async fn get_current_user_guild_member(
        &self,
        bearer_token: &str,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Member, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            &format!("/users/@me/guilds/{}/member", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn leave_guild(&self, guild_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/users/@me/guilds/{}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns OAuth2 connections for the current user.
    pub async fn get_current_user_connections(
        &self,
        bearer_token: &str,
    ) -> Result<Vec<UserConnection>, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            "/users/@me/connections",
            Option::<&Value>::None,
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
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            Option::<&Value>::None,
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
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            Some(body),
        )
        .await
    }

    /// Returns the bot application's OAuth2 application object.
    pub async fn get_current_bot_application_information(
        &self,
    ) -> Result<Application, DiscordError> {
        self.request_typed(
            Method::GET,
            "/oauth2/applications/@me",
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns OAuth2 authorization information for a bearer token.
    pub async fn get_current_authorization_information(
        &self,
        bearer_token: &str,
    ) -> Result<AuthorizationInformation, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            "/oauth2/@me",
            Option::<&Value>::None,
        )
        .await
    }
}
