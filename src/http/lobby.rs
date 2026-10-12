use std::collections::HashMap;

use crate::error::DiscordError;
use crate::model::{
    AddLobbyMember, CreateLobby, CreateLobbyMessage, GetOrCreateLobby, LinkLobbyChannel, Lobby,
    LobbyId, LobbyInvite, LobbyMember, LobbyMemberUpdate, LobbyMessage, MessageId, ModifyLobby,
    UserId,
};
use crate::types::invalid_data_error;

use super::{json_body, validate_authorization_token, RestClient, RouteId, NO_BODY};

/// Upper bound of `limit` on Get Lobby Messages, which accepts 1-200
/// (Discord docs: Lobby › Get Lobby Messages).
const MAX_LOBBY_MESSAGES_LIMIT: u8 = 200;

impl RestClient {
    /// Creates a Discord lobby owned by the configured application.
    pub async fn create_lobby(&self, body: &CreateLobby) -> Result<Lobby, DiscordError> {
        self.send(RouteId::CreateLobby, "/lobbies", json_body(body))
            .await
    }

    /// Returns one lobby by ID.
    pub async fn get_lobby(&self, lobby_id: LobbyId) -> Result<Lobby, DiscordError> {
        self.send(
            RouteId::GetLobby,
            &format!("/lobbies/{}", lobby_id),
            NO_BODY,
        )
        .await
    }

    /// Modifies one lobby.
    pub async fn modify_lobby(
        &self,
        lobby_id: LobbyId,
        body: &ModifyLobby,
    ) -> Result<Lobby, DiscordError> {
        self.send(
            RouteId::ModifyLobby,
            &format!("/lobbies/{}", lobby_id),
            json_body(body),
        )
        .await
    }

    /// Deletes one lobby.
    pub async fn delete_lobby(&self, lobby_id: LobbyId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteLobby,
            &format!("/lobbies/{}", lobby_id),
            NO_BODY,
        )
        .await
    }

    /// Adds or updates one lobby member.
    pub async fn add_lobby_member(
        &self,
        lobby_id: LobbyId,
        user_id: UserId,
        body: &AddLobbyMember,
    ) -> Result<LobbyMember, DiscordError> {
        self.send(
            RouteId::AddLobbyMember,
            &format!("/lobbies/{}/members/{}", lobby_id, user_id),
            json_body(body),
        )
        .await
    }

    /// Adds, updates, or removes lobby members in bulk.
    pub async fn bulk_update_lobby_members(
        &self,
        lobby_id: LobbyId,
        body: &[LobbyMemberUpdate],
    ) -> Result<Vec<LobbyMember>, DiscordError> {
        self.send(
            RouteId::BulkUpdateLobbyMembers,
            &format!("/lobbies/{}/members/bulk", lobby_id),
            json_body(body),
        )
        .await
    }

    /// Removes one lobby member.
    pub async fn remove_lobby_member(
        &self,
        lobby_id: LobbyId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::RemoveLobbyMember,
            &format!("/lobbies/{}/members/{}", lobby_id, user_id),
            NO_BODY,
        )
        .await
    }

    /// Removes the current OAuth2 user from a lobby.
    pub async fn leave_lobby(
        &self,
        bearer_token: &str,
        lobby_id: LobbyId,
    ) -> Result<(), DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer_unit(
            RouteId::LeaveLobby,
            bearer_token,
            &format!("/lobbies/{}/members/@me", lobby_id),
            NO_BODY,
        )
        .await
        .map(|_| ())
    }

    /// Links or unlinks a channel for the current OAuth2 user in a lobby.
    pub async fn link_lobby_channel(
        &self,
        bearer_token: &str,
        lobby_id: LobbyId,
        body: &LinkLobbyChannel,
    ) -> Result<Lobby, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::LinkChannelToLobby,
            bearer_token,
            &format!("/lobbies/{}/channel-linking", lobby_id),
            json_body(body),
        )
        .await
    }

    /// Updates app-scoped moderation metadata for lobby messages.
    pub async fn update_lobby_message_moderation_metadata(
        &self,
        lobby_id: LobbyId,
        message_id: MessageId,
        metadata: &HashMap<String, String>,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::UpdateLobbyMessageModerationMetadata,
            &format!(
                "/lobbies/{}/messages/{}/moderation-metadata",
                lobby_id, message_id
            ),
            json_body(metadata),
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
        self.send_bearer(
            RouteId::CreateOrJoinLobby,
            bearer_token,
            "/lobbies",
            json_body(body),
        )
        .await
    }

    /// Sends a message to a lobby as the current OAuth2 user. If the lobby
    /// has a linked channel, Discord also forwards the message there.
    pub async fn send_lobby_message(
        &self,
        bearer_token: &str,
        lobby_id: LobbyId,
        body: &CreateLobbyMessage,
    ) -> Result<LobbyMessage, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::SendLobbyMessage,
            bearer_token,
            &format!("/lobbies/{}/messages", lobby_id),
            json_body(body),
        )
        .await
    }

    /// Returns the most recent lobby messages (`limit` 1..=200, Discord
    /// defaults to 50) for a lobby the current OAuth2 user is a member of.
    pub async fn get_lobby_messages(
        &self,
        bearer_token: &str,
        lobby_id: LobbyId,
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
        self.send_bearer(
            RouteId::GetLobbyMessages,
            bearer_token,
            &format!("/lobbies/{}/messages{query}", lobby_id),
            NO_BODY,
        )
        .await
    }

    /// Creates a single-use, one-hour invite to the lobby's linked channel
    /// for the current OAuth2 user (Bearer token).
    pub async fn create_lobby_channel_invite_for_self(
        &self,
        bearer_token: &str,
        lobby_id: LobbyId,
    ) -> Result<LobbyInvite, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.send_bearer(
            RouteId::CreateLobbyChannelInviteForSelf,
            bearer_token,
            &format!("/lobbies/{}/members/@me/invites", lobby_id),
            NO_BODY,
        )
        .await
    }

    /// Creates a single-use, one-hour invite to the lobby's linked channel
    /// for `user_id`, on behalf of the application (Bot token).
    pub async fn create_lobby_channel_invite_for_user(
        &self,
        lobby_id: LobbyId,
        user_id: UserId,
    ) -> Result<LobbyInvite, DiscordError> {
        self.send(
            RouteId::CreateLobbyChannelInviteForUser,
            &format!("/lobbies/{}/members/{}/invites", lobby_id, user_id),
            NO_BODY,
        )
        .await
    }
}
