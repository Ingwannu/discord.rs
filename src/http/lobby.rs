use std::collections::HashMap;

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    AddLobbyMember, CreateLobby, CreateLobbyMessage, GenericId, GetOrCreateLobby, LinkLobbyChannel,
    Lobby, LobbyInvite, LobbyMember, LobbyMemberUpdate, LobbyMessage, ModifyLobby,
};
use crate::types::invalid_data_error;

use super::{validate_authorization_token, RequestAuthorization, RestClient};

/// Upper bound of `limit` on Get Lobby Messages, which accepts 1-200
/// (Discord docs: Lobby › Get Lobby Messages).
const MAX_LOBBY_MESSAGES_LIMIT: u8 = 200;

impl RestClient {
    /// Creates a Discord lobby owned by the configured application.
    pub async fn create_lobby(&self, body: &CreateLobby) -> Result<Lobby, DiscordError> {
        self.request_typed(Method::POST, "/lobbies", Some(body))
            .await
    }

    /// Returns one lobby by ID.
    pub async fn get_lobby(&self, lobby_id: GenericId) -> Result<Lobby, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/lobbies/{}", lobby_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Modifies one lobby.
    pub async fn modify_lobby(
        &self,
        lobby_id: GenericId,
        body: &ModifyLobby,
    ) -> Result<Lobby, DiscordError> {
        self.request_typed(Method::PATCH, &format!("/lobbies/{}", lobby_id), Some(body))
            .await
    }

    /// Deletes one lobby.
    pub async fn delete_lobby(&self, lobby_id: GenericId) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/lobbies/{}", lobby_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Adds or updates one lobby member.
    pub async fn add_lobby_member(
        &self,
        lobby_id: GenericId,
        user_id: GenericId,
        body: &AddLobbyMember,
    ) -> Result<LobbyMember, DiscordError> {
        self.request_typed(
            Method::PUT,
            &format!("/lobbies/{}/members/{}", lobby_id, user_id),
            Some(body),
        )
        .await
    }

    /// Adds, updates, or removes lobby members in bulk.
    pub async fn bulk_update_lobby_members(
        &self,
        lobby_id: GenericId,
        body: &[LobbyMemberUpdate],
    ) -> Result<Vec<LobbyMember>, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/lobbies/{}/members/bulk", lobby_id),
            Some(body),
        )
        .await
    }

    /// Removes one lobby member.
    pub async fn remove_lobby_member(
        &self,
        lobby_id: GenericId,
        user_id: GenericId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/lobbies/{}/members/{}", lobby_id, user_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Removes the current OAuth2 user from a lobby.
    pub async fn leave_lobby(
        &self,
        bearer_token: &str,
        lobby_id: GenericId,
    ) -> Result<(), DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_with_headers_authorized(
            RequestAuthorization::Bearer(bearer_token),
            Method::DELETE,
            &format!("/lobbies/{}/members/@me", lobby_id),
            None,
        )
        .await
        .map(|_| ())
    }

    /// Links or unlinks a channel for the current OAuth2 user in a lobby.
    pub async fn link_lobby_channel(
        &self,
        bearer_token: &str,
        lobby_id: GenericId,
        body: &LinkLobbyChannel,
    ) -> Result<Lobby, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PATCH,
            &format!("/lobbies/{}/channel-linking", lobby_id),
            Some(body),
        )
        .await
    }

    /// Updates app-scoped moderation metadata for lobby messages.
    pub async fn update_lobby_message_moderation_metadata(
        &self,
        lobby_id: GenericId,
        message_id: GenericId,
        metadata: &HashMap<String, String>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/lobbies/{}/messages/{}/moderation-metadata",
                lobby_id, message_id
            ),
            Some(metadata),
        )
        .await
    }

    /// Creates the lobby identified by `body.secret`, or joins the current
    /// OAuth2 user to the existing lobby with that secret (`PUT /lobbies`,
    /// Bearer token with `sdk.social_layer`).
    pub async fn get_or_create_lobby(
        &self,
        bearer_token: &str,
        body: &GetOrCreateLobby,
    ) -> Result<Lobby, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            "/lobbies",
            Some(body),
        )
        .await
    }

    /// Sends a message to a lobby as the current OAuth2 user. If the lobby
    /// has a linked channel, Discord also forwards the message there.
    pub async fn send_lobby_message(
        &self,
        bearer_token: &str,
        lobby_id: GenericId,
        body: &CreateLobbyMessage,
    ) -> Result<LobbyMessage, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::POST,
            &format!("/lobbies/{}/messages", lobby_id),
            Some(body),
        )
        .await
    }

    /// Returns the most recent lobby messages (`limit` 1..=200, Discord
    /// defaults to 50) for a lobby the current OAuth2 user is a member of.
    pub async fn get_lobby_messages(
        &self,
        bearer_token: &str,
        lobby_id: GenericId,
        limit: Option<u8>,
    ) -> Result<Vec<LobbyMessage>, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let query = match limit {
            Some(limit @ 1..=MAX_LOBBY_MESSAGES_LIMIT) => format!("?limit={limit}"),
            Some(limit) => {
                return Err(invalid_data_error(format!(
                    "lobby message limit must be between 1 and {MAX_LOBBY_MESSAGES_LIMIT}, got \
                     {limit}"
                )))
            }
            None => String::new(),
        };
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            &format!("/lobbies/{}/messages{query}", lobby_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Creates a single-use, one-hour invite to the lobby's linked channel
    /// for the current OAuth2 user (Bearer token).
    pub async fn create_lobby_channel_invite_for_self(
        &self,
        bearer_token: &str,
        lobby_id: GenericId,
    ) -> Result<LobbyInvite, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::POST,
            &format!("/lobbies/{}/members/@me/invites", lobby_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Creates a single-use, one-hour invite to the lobby's linked channel
    /// for `user_id`, on behalf of the application (Bot token).
    pub async fn create_lobby_channel_invite_for_user(
        &self,
        lobby_id: GenericId,
        user_id: GenericId,
    ) -> Result<LobbyInvite, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/lobbies/{}/members/{}/invites", lobby_id, user_id),
            Option::<&Value>::None,
        )
        .await
    }
}
