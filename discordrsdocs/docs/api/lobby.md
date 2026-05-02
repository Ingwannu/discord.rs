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

## Bot-Authorized Routes

- `create_lobby(body)`
- `get_lobby(lobby_id)`
- `modify_lobby(lobby_id, body)`
- `delete_lobby(lobby_id)`
- `add_lobby_member(lobby_id, user_id, body)`
- `bulk_update_lobby_members(lobby_id, members)` for `POST /lobbies/{lobby.id}/members/bulk`
- `remove_lobby_member(lobby_id, user_id)`
- `update_lobby_message_moderation_metadata(lobby_id, message_id, metadata)`

## Bearer-Authorized Routes

Discord requires user OAuth2 Bearer authorization for routes that represent the current lobby user:

- `leave_lobby(bearer_token, lobby_id)`
- `link_lobby_channel(bearer_token, lobby_id, body)`

```rust
use discordrs::{CreateLobby, LobbyMember, Snowflake};

let lobby = rest
    .create_lobby(&CreateLobby {
        members: Some(vec![LobbyMember {
            id: Snowflake::from("123456789012345678"),
            ..LobbyMember::default()
        }]),
        idle_timeout_seconds: Some(300),
        ..CreateLobby::default()
    })
    .await?;
```
