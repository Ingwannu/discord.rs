# Lobby API

Discord's Lobby resource is exposed through typed REST helpers for matchmaking and Social SDK integrations.

## Models

- `Lobby`: lobby ID, owning application, metadata, members, and optional linked channel
- `LobbyMember`: user ID, metadata, and lobby member flags
- `CreateLobby`: metadata, initial members, and idle timeout
- `ModifyLobby`: replacement metadata, replacement members, and idle timeout
- `AddLobbyMember`: metadata and flags for one member upsert
- `LobbyMemberUpdate`: bulk upsert/remove entry
- `LinkLobbyChannel`: optional channel ID for linking or unlinking
- `GetOrCreateLobby` (3.0): lobby `secret`, idle timeout, and lobby/member metadata for `PUT /lobbies`
- `CreateLobbyMessage` / `LobbyMessage` / `LobbyMessageMember` (3.0): send body and the returned lobby message (author, `additional_name`, metadata, moderation metadata, flags)
- `LobbyInvite` (3.0): single-use, one-hour invite code for the lobby's linked channel

## Bot-Authorized Routes

- `create_lobby(body)`
- `get_lobby(lobby_id)`
- `modify_lobby(lobby_id, body)`
- `delete_lobby(lobby_id)`
- `add_lobby_member(lobby_id, user_id, body)`
- `bulk_update_lobby_members(lobby_id, members)` for `POST /lobbies/{lobby.id}/members/bulk`
- `remove_lobby_member(lobby_id, user_id)`
- `update_lobby_message_moderation_metadata(lobby_id, message_id, metadata)`
- `create_lobby_channel_invite_for_user(lobby_id, user_id)` (3.0)

## Bearer-Authorized Routes

Discord requires user OAuth2 Bearer authorization for routes that represent the current lobby user:

- `leave_lobby(bearer_token, lobby_id)`
- `link_lobby_channel(bearer_token, lobby_id, body)`
- `get_or_create_lobby(bearer_token, body)` (3.0): creates the lobby for `secret` or joins the existing one
- `send_lobby_message(bearer_token, lobby_id, body)` and `get_lobby_messages(bearer_token, lobby_id, limit)` (3.0, `limit` 1–200)
- `create_lobby_channel_invite_for_self(bearer_token, lobby_id)` (3.0)

```rust
use discordrs::{CreateLobby, GenericId, LobbyMember};

let lobby = rest
    .create_lobby(&CreateLobby {
        // `LobbyMember` has no `Default` (its ID is required); `new` fills
        // the optional fields with `None`.
        members: Some(vec![LobbyMember::new("123456789012345678".parse::<GenericId>()?)]),
        idle_timeout_seconds: Some(300),
        ..CreateLobby::default()
    })
    .await?;
```
