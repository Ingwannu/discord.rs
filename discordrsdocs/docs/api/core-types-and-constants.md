# Models and Constants

## Typed Models

The typed surface is centered on:

- `Snowflake`
- `PermissionsBitField`
- `User`
- `Guild`
- `Channel`
- `Member`
- `Role`
- `Message`
- typed interaction variants

These types are designed to replace mixed `String` / `u64` IDs and raw `Value` routing in new code.

## New in 2.1.0

- `Guild` models the GUILD_CREATE collections (`channels`, `threads`, `members`, `voice_states`, `presences`, `emojis`, `stickers`, `stage_instances`, `soundboard_sounds`, `guild_scheduled_events`, `joined_at`, `large`) plus `Guild::without_create_collections()`.
- `MessageReferenceType` (`DEFAULT` / `FORWARD`) and the `MessageReference::reply(...)` / `MessageReference::forward(...)` constructors.
- Guild lifecycle request bodies: `CreateGuild`, `CreateGuildFromTemplate`, and `GuildMfaLevel` (in `discordrs::model`).
- `InteractionCallbackResult`, `InteractionCallbackInteraction`, and `InteractionCallbackResource` for `with_response=true` interaction callbacks.
- Scheduled-event query types `GuildScheduledEventsQuery` and `GuildScheduledEventUsersQuery`.
- Typed event-field completion: `InviteEvent` (inviter, uses, max_uses, max_age, temporary, created_at, expires_at, target type/user), `ThreadEvent::newly_created`, `ThreadListSyncEvent` (`channel_ids`, `members`), `ThreadMemberUpdateEvent` (`user_id`, `join_timestamp`, `flags`), typed `ThreadMember` in `ThreadMembersUpdateEvent`, and typed `AutoModerationTriggerMetadata` / `AutoModerationAction` in auto-moderation events.
- Error classification helpers: `HttpError::is_timeout()` / `is_connect()` / `is_body()` / `is_retryable()` and `DiscordError::is_retryable_transport()`; gateway protocol failures surface as `DiscordError::Gateway`.

## Legacy Utility Types

`src/types.rs` still contains:

- `Error`
- `ButtonConfig`
- `Emoji`
- `SelectOption`
- `MediaGalleryItem`
- `MediaInfo`

These stay relevant for builders and compatibility helpers.

## Constants

Constant groups include:

- component type codes
- button styles
- text input styles
- separator spacing values
- gateway intents, including the `2.1.0` poll intents `gateway_intents::GUILD_MESSAGE_POLLS` (`1 << 24`) and `gateway_intents::DIRECT_MESSAGE_POLLS` (`1 << 25`) — both part of `NON_PRIVILEGED` — plus the `GUILD_EXPRESSIONS` alias for bit 3

## Advice

- Reference constants instead of hardcoded numeric magic values.
- Keep custom IDs + style values centralized in your app code.
- Prefer typed models for IDs and permission bitfields before dropping to raw JSON.
