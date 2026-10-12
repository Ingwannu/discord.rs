# discord.rs

<a href="https://coveralls.io/github/Ingwannu/discordrs?branch=main"><img src="https://coveralls.io/repos/github/Ingwannu/discordrs/badge.svg?branch=main&t=20260411" alt="Coveralls"></a>

discord.rs is a standalone Discord bot framework for Rust with typed models, typed gateway events, Components V2, collectors, cache managers, and an HTTP client.

Brand name: discord.rs. The crates.io package name and Rust import path remain `discordrs`.

## Features

- Typed `Client` runtime with `Event` enum dispatch and compatibility `BotClient` alias
- Typed `RestClient` with shared route/global rate-limit state and compatibility `DiscordHttpClient` alias; the client is `Clone`, and the transport serializes requests per Discord bucket scoped by major parameter, honors `Retry-After` and global-scope rate-limit headers, retries 429s and connection failures, replays 5xx/timeout failures only for requests that are safe to repeat (opt in for POST/PATCH with `retry_non_idempotent_requests`), does not follow redirects, and keeps webhook/interaction tokens out of REST logs and errors
- `RestClient::with_reason(...)` scoped clones that send `X-Audit-Log-Reason` on every mutating request so bans, kicks, and edits show up with a reason in the guild audit log
- Guild lifecycle routes: `create_guild(...)`, `delete_guild(...)`, `create_guild_from_template(...)`, and `modify_guild_mfa_level(...)` with typed `CreateGuild`, `CreateGuildFromTemplate`, and `GuildMfaLevel` bodies
- `create_interaction_response_with_result(...)` for `with_response=true` interaction callbacks returning the typed `InteractionCallbackResult` resource
- Message forwarding helpers: `MessageReferenceType`, `MessageReference::reply(...)`/`forward(...)`, and one-call `RestClient::forward_message(...)`
- Poll gateway intents `GUILD_MESSAGE_POLLS` and `DIRECT_MESSAGE_POLLS` (both in `NON_PRIVILEGED`), plus a `GUILD_EXPRESSIONS` alias for bit 3
- Initial presence inside IDENTIFY via `ClientBuilder::presence(...)` and serial or concurrent handler scheduling via `ClientBuilder::event_dispatch(EventDispatchMode::...)`
- `Context::fetch_members(...)` gateway member fetching that awaits the correlated `GUILD_MEMBERS_CHUNK` payloads and fills the cache — the discord.js `guild.members.fetch()` equivalent
- discord.js-style interaction response API (`discordrs::response::InteractionResponder`): `interaction.reply(...)`, `reply_ephemeral`, `defer`, `defer_update`, `update_message`, `edit_reply`, `fetch_reply`, `delete_reply`, `follow_up`, `show_modal(ModalBuilder)`, and `respond_autocomplete`, with an atomic shared acknowledgement state that fails double replies locally
- Entity convenience methods (`discordrs::model_ext`): `message.reply/edit/delete/react/pin/crosspost/forward_to/start_thread/link`, `member.kick/ban/timeout/add_role`, `guild.create_channel/fetch_member/ban/icon_url`, `channel.send/mention/is_text_based`, `role.mention`, `user.create_dm/dm/tag/display_avatar_url`, plus CDN URL helpers
- Multi-process sharding (`discordrs::sharding::process`): `ProcessShardManager` with `spawn`/`spawn_auto`, exponential-backoff auto-respawn, prefixed and size-bounded IPC frames over child stdio, every IDENTIFY (including re-IDENTIFYs) granted by the parent per `shard_id % max_concurrency` bucket with the daily session-start budget enforced, bounded shutdown, `broadcast(...)` across all children (the `broadcastEval` equivalent), and one-binary parent/child via `ShardChildProcess::from_env()`
- Audio playback pipeline (`discordrs::voice::player`): `AudioInput` (FFmpeg, raw PCM, files), `AudioResource` with 20ms framing and live volume, `AudioPlayer` state machine with `TrackStart`/`TrackEnd` events, and drop-safe subscriptions with `NoSubscriberBehavior`
- Collector controls: `stop()`/`stop_with_reason(...)`, cloneable `CollectorStopHandle`, `end_reason()`, `idle(...)` timeouts, `reset_timer()`, and uniform `filter(...)` on component and modal collectors
- Builder runtime validation: `validate()` and `try_build()` on command, component, embed, modal, container, and media builders, enforcing Discord's documented limits with errors that name the field, the limit, and the actual value
- Configurable REST client via `RestClient::builder()` (API base/version, timeouts, user agent, proxy, custom reqwest client, rate-limit callback, default allowed mentions) plus `ClientBuilder::default_allowed_mentions(...)` and `ClientBuilder::cache_backend(...)`
- Typed read-side components: `Message.components` as `Vec<MessageComponent>` with a depth-first `iter()`, typed `ResolvedData` lookup maps, and typed `MessageInteractionMetadata`
- `prelude::*` re-exports for common runtime, builder, helper, and response types
- Cache-backed manager reads for guilds, channels, members, roles, presences, and messages, with bounded defaults, cheap `Arc` read APIs for hot member/message/presence paths, and explicit `CacheConfig` overrides
- GUILD_CREATE cache population for channels, threads, members, voice states, presences, emojis, stickers, and stage instances (plus GUILD_MEMBERS_CHUNK and THREAD_* events), with O(log n) LRU order tracking, incremental per-guild/per-channel cap counters, and throttled TTL sweeps
- Collectors for messages, interactions, components, and modals behind the `collectors` feature
- Current with Discord's API as of 2026-10-09 (234 / 234 documented REST routes): channel obfuscation flags and IDENTIFY capabilities ahead of the 2026-11-16 deadline, the large-bot-sharding `shard` param, invite target-user routes, `file_types` filters, Component Embed link previews (reading them on messages, and generating the website-side document with `discordrs::request::ComponentEmbed`), Game Stats Widget (Application Identity Profile) routes, and the newer Social SDK lobby routes
- Gateway WebSocket client with connect, heartbeat, identify, resume, reconnect, terminal close-code handling (4004/4010–4014 stop the shard with an error instead of reconnecting; 4007/4009 start a fresh session), and fixed compressed binary frame decoding for explicit `zlib-stream` connections; IDENTIFY is paced to Discord's 1-per-5s-per-shard limit and short-lived sessions reconnect with escalating backoff
- Shard supervisor and shard messenger control paths for queued shard boot, reconnect, shutdown, presence, and voice state updates; `spawn_shards(...)` uses the account's real `max_concurrency` from `/gateway/bot` for identify wave sizing
- Voice manager plus voice runtime support for websocket hello/identify, UDP discovery, select-protocol, speaking updates, raw UDP receive, AES-GCM/XChaCha RTP-size Opus packet decrypt, pure-Rust Opus PCM decode, and Opus-frame RTP send helpers
- Live-validated `dave` feature with DAVE opcode state tracking, `davey`/OpenMLS-backed MLS lifecycle helpers, receive decryptor hooks, and outbound media encryption hooks
- Optional OAuth2 backend helpers for authorization URLs, authorization-code exchange, refresh-token exchange, and token revocation via `OAuth2Client::revoke_token(...)`
- Typed Discord coverage for all official REST route shapes audited on 2026-05-02, plus Webhook Events, lobbies, guild incident actions, audit logs, guild count fetches, guild modifications, guild channel creation and reordering, guild ban pagination, single-member ban bodies, guild member profile fields and search/list pagination, current-user guild pagination/counts, guild/member/current-member edits, guild role create/update/reordering bodies, guild widget/welcome/onboarding writes, guild prune count/result including the current JSON-body begin route, guild-member join, role member-count, public widget, Stage Instance writes, sticker pack fetches, typed guild sticker writes, voice-state REST reads/writes, current-application and OAuth2 metadata reads, Create Group DM and Group DM recipient routes, channel invite/target-user and permission routes, voice-channel status updates, guild message search, current and legacy channel-pin routes, forwarded message snapshots, shared client themes, Gateway rate-limit, reaction metadata, and presence metadata events, Activity instances, polls, subscriptions, entitlements, soundboard, threads, forum channel fields, invites, integrations, Auto Moderation, guild preview/vanity, voice regions, OAuth2 user connections, application command permissions, and bulk bans
- Application framework routing for slash commands, components, and modals behind the `interactions` feature
- Components V2 builders (`Container`, `TextDisplay`, `Section`, `MediaGallery`, `Button`, `SelectMenu` with auto-populated defaults, and more)
- Typed command builders for slash, user, and message commands
- Modal builders with `RadioGroup`, `CheckboxGroup`, `Checkbox`, and `FileUpload`
- V2 modal submission parser with preserved `FileUpload`, `RadioGroup`, `CheckboxGroup`, and other V2 component types
- Typed interaction decoding with chat-input, context-menu, autocomplete, component, and modal submit variants
- Interaction routing helpers: `parse_interaction`, `parse_raw_interaction`, `parse_interaction_context`, and `try_interactions_endpoint`
- Feature-gated runtime layers: `gateway`, `interactions`, `collectors`, `sharding`, and `voice`; the in-memory `cache` storage is enabled by default and can still be disabled with `default-features = false`

## Install

```toml
[dependencies]
discordrs = "2.2.0"
```

```toml
[dependencies]
# Gateway bot client
discordrs = { version = "2.2.0", features = ["gateway"] }

# HTTP Interactions Endpoint
discordrs = { version = "2.2.0", features = ["interactions"] }

# Gateway runtime with default cache storage
discordrs = { version = "2.2.0", features = ["gateway"] }

# Minimal core without cache storage
discordrs = { version = "2.2.0", default-features = false }

# Gateway runtime with collectors
discordrs = { version = "2.2.0", features = ["gateway", "collectors"] }

# Sharding foundations
discordrs = { version = "2.2.0", features = ["gateway", "sharding"] }

# Voice foundations
discordrs = { version = "2.2.0", features = ["voice"] }

# PCM -> Opus voice encode/playback helpers
discordrs = { version = "2.2.0", features = ["voice", "voice-encode"] }

# DAVE receive/outbound media integration
discordrs = { version = "2.2.0", features = ["voice", "dave"] }

# Gateway runtime with zstd-stream transport compression
discordrs = { version = "2.2.0", features = ["gateway", "zstd-stream"] }

# Both runtime modes
discordrs = { version = "2.2.0", features = ["gateway", "interactions"] }
```

## Quick Taste: 2.2.0 Ergonomics

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, EventKind, Interaction};

async fn on_event(ctx: Context, event: &Event) -> Result<(), discordrs::DiscordError> {
    match event.kind() {
        // Reply to a slash command like discord.js: interaction.reply(...)
        EventKind::InteractionCreate(interaction) => {
            if let Interaction::ChatInputCommand(command) = &**interaction {
                command.reply(&ctx.http, "hi").await?;
            }
        }
        // Reply to a message like discord.js: message.reply(...)
        EventKind::MessageCreate(message) => {
            if message.content == "!ping" {
                message.reply(&ctx.http, "pong").await?;
            }
        }
        _ => {}
    }
    Ok(())
}
```

## API Cleanup

Recent API cleanup tightened the public surface:

- `RestClient` now exposes the typed REST methods as the supported public path. The old raw convenience methods such as `send_message`, `edit_message`, `create_dm_channel`, `create_interaction_response`, and `bulk_overwrite_global_commands` are no longer public.
- Builder implementation submodules are private. Import builders from `discordrs::builders::{...}` or use the crate root re-exports.
- `ApplicationCommand` no longer implements `DiscordModel` because its ID is optional until Discord assigns one. Use `ApplicationCommand::id_opt()` and `ApplicationCommand::created_at()` instead.
- 3.0 (unreleased): REST methods take concrete request types and return typed models. The 2.x `_typed` / `_from_request` / `_config` pairs collapse into one canonical method (`update_channel(&ModifyChannel)`, `modify_guild`, `create_guild_role`, `get_guild_emojis() -> Vec<Emoji>`, ...); the 1:1 renames remain as deprecated forwards for one release. Message endpoints take `discordrs::request::{CreateMessage, EditMessage, ExecuteWebhook, EditWebhookMessage}`. Routes without a typed method go through `RestClient::request(RawRequest)`. The full old → new table is "Migrating REST names from 2.x" in [HTTP and Helpers](discordrsdocs/docs/api/http-and-helpers.md).
- 3.0 (unreleased): integer discriminants are open newtypes (`ChannelType`, `ButtonStyle`, `CommandOptionType`, `InteractionResponseType`, ...) with named constants, the 2.x `u8` constant modules (`button_style`, `option_type`, ...) are gone, and permission sets are `Permissions` (`PermissionsBitField` is a deprecated alias). See "Migrating from 2.x discriminants" in [Models and Constants](discordrsdocs/docs/api/core-types-and-constants.md).

If you are upgrading existing code, the common replacements are:

| Old path | New path |
|----------|----------|
| `RestClient::send_message(...)` | `send_message(...)` helper or `RestClient::create_message(&request::CreateMessage)` |
| `RestClient::edit_message(...)` (1.x), `update_message(...)` (2.x) | `RestClient::edit_message(&request::EditMessage)` (3.0) |
| `RestClient::create_dm_channel(...)` (1.x), `create_dm_channel_typed(...)` (2.x) | `RestClient::create_dm_channel(&CreateDmChannel)` (3.0) |
| `RestClient::create_interaction_response(...)` | `RestClient::create_interaction_response_typed(...)` or typed helper functions |
| `RestClient::bulk_overwrite_global_commands(...)` (1.x), `bulk_overwrite_global_commands_typed(...)` (2.x) | `RestClient::bulk_overwrite_global_commands(&[CommandDefinition])` (3.0) |
| `RestClient::request(method, path, Option<&Value>)`, `request_typed`, `request_multipart` (2.x) | `RestClient::request::<T>(RawRequest)` (3.0) |
| `discordrs::builders::modal::*` | `discordrs::builders::{...}` or crate root re-exports |
| generic `DiscordModel` access for `ApplicationCommand` | `ApplicationCommand::id_opt()` / `ApplicationCommand::created_at()` |

## Documentation Site

A navigation-focused docs website (similar to the discord.js docs browsing style) is available in [`discordrsdocs/`](discordrsdocs/):

- Live URL: `http://discordrs.teamwicked.me/discordrsdocs/#/`
- Korean URL: `http://discordrs.teamwicked.me/discordrsdocs/#/ko/README`
- Entry: [`discordrsdocs/index.html`](discordrsdocs/index.html)
- Testing guide: [`discordrsdocs/docs/guide/testing-and-coverage.md`](discordrsdocs/docs/guide/testing-and-coverage.md)
- Local preview:
  ```bash
  python3 -m http.server 8080 --directory discordrsdocs
  ```

## Quick Example

Handler traits are implemented with plain `async fn` (no `#[async_trait]`);
the futures must be `Send` because handlers run on spawned Tokio tasks.

```rust
use std::sync::Arc;

use discordrs::{gateway_intents, Client, Context, Event, EventHandler, EventKind};

struct Handler;

impl EventHandler for Handler {
    // Every event arrives once, shared with the cache and collectors.
    async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
        match event.kind() {
            EventKind::Ready(ready) => {
                println!("Bot ready! User: {}", ready.user.username);
            }
            EventKind::MessageCreate(message) => {
                println!("Message: {}", message.content);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let token = std::env::var("DISCORD_TOKEN").unwrap();
    Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
        .event_handler(Handler)
        .start()
        .await
        .unwrap();
}
```

## Typed APIs

```rust
use discordrs::{CommandOptionBuilder, CommandOptionType, Permissions, SlashCommandBuilder};

let command = SlashCommandBuilder::new("ticket", "Create a support ticket")
    .option(
        CommandOptionBuilder::new(CommandOptionType::STRING, "topic", "Ticket topic")
            .required(true)
            .autocomplete(true),
    )
    .default_member_permissions(Permissions::from_bits(0))
    .build();
```

## Components V2 Example

```rust
use discordrs::{
    create_container, send_container_message, ButtonConfig, ButtonStyle, DiscordHttpClient,
};

async fn send_support_panel(
    http: &DiscordHttpClient,
    channel_id: discordrs::ChannelId,
) -> Result<(), discordrs::DiscordError> {
    let buttons = vec![
        ButtonConfig::new("ticket_open", "Open Ticket").style(ButtonStyle::PRIMARY),
        ButtonConfig::new("ticket_status", "Check Status").style(ButtonStyle::SECONDARY),
    ];

    let container = create_container(
        "Support Panel",
        "Use the buttons below to manage support requests.",
        buttons,
        None,
    );

    send_container_message(http, channel_id, container).await?;
    Ok(())
}
```

## Modal Example with RadioGroup

```rust
use discordrs::{
    CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, ModalBuilder, RadioGroupBuilder,
    SelectOption,
};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    )
    .add_checkbox_group(
        "Notifications",
        Some("Choose any"),
        CheckboxGroupBuilder::new("notify_channels")
            .add_option(SelectOption::new("Email", "email"))
            .add_option(SelectOption::new("Push", "push"))
            .min_values(0)
            .max_values(2),
    )
    // A checkbox cannot be required; for a box the user must tick, use a
    // one-option CheckboxGroupBuilder with `.required(true)`.
    .add_checkbox(
        "Send me updates",
        None,
        CheckboxBuilder::new("updates").default_checked(true),
    )
    .add_file_upload(
        "Screenshot",
        Some("Attach one or more files"),
        FileUploadBuilder::new("attachments").min_values(1),
    );
```

## Interactions Endpoint Example

```rust
use axum::Router;
use discordrs::request::{EndpointReply, InteractionMessage, InteractionResponse, TextDisplay};
use discordrs::{InteractionContext, InteractionHandler, RawInteraction, try_interactions_endpoint};

#[derive(Clone)]
struct Handler;

impl InteractionHandler for Handler {
    async fn handle(
        &self,
        _ctx: InteractionContext,
        interaction: RawInteraction,
    ) -> EndpointReply {
        match interaction {
            RawInteraction::Command { name, .. } if name.as_deref() == Some("hello") => {
                // Answered inline: a Components V2 reply, encoded once.
                let message = InteractionMessage::new()
                    .components_v2(vec![TextDisplay::new("## Hello\nWorld").into()]);
                InteractionResponse::reply(message).into()
            }
            _ => InteractionResponse::defer(false).into(),
        }
    }
}

fn app(public_key: &str) -> Router {
    try_interactions_endpoint(public_key, Handler)
        .expect("invalid Discord public key")
}
```

## Feature Flags

| Feature | Description | Key deps |
|---------|-------------|----------|
| (default) | Builders, typed models, command builders, parsers, REST client, helpers, and in-memory cache storage | reqwest, serde_json, tokio |
| `gateway` | Gateway WebSocket, `Client`, typed `Event`, and `EventHandler::handle_event(...)` dispatch | tokio-tungstenite, flate2 |
| `zstd-stream` | Gateway zstd-stream transport compression | gateway, zstd |
| `interactions` | HTTP Interactions Endpoint with Ed25519 | axum 0.8, ed25519-dalek |
| `cache` | Enables the in-memory cache storage and `CacheBackend` extension trait used by gateway cache managers; included in default features | tokio |
| `collectors` | Async collectors for messages, interactions, components, and modals, with stop handles, idle timeouts, and end reasons | tokio |
| `sharding` | Sharding manager, reusable gateway config abstractions, and the multi-process `ProcessShardManager` | tokio |
| `voice` | Voice connection/player skeletons, the `AudioPlayer`/`AudioResource` playback pipeline, plus voice gateway/UDP receive, Opus-frame send, transport decrypt, and Opus PCM decode helpers | tokio, aes-gcm, chacha20poly1305, opus-decoder |
| `voice-encode` | PCM source/mixer and `opus-rs` encoder helpers for 48 kHz stereo 20 ms voice playback through the existing Opus frame path | voice, opus-rs |
| `dave` | DAVE/MLS receive and outbound media hooks backed by `davey`, with live Discord MLS transition validation coverage | voice, davey |

## Notes

- `discord.rs` started as a helper around serenity workflows, and `1.0.0` is the first stabilized standalone framework release with the typed runtime surface.
- Use `try_interactions_endpoint()` when you want invalid public keys to fail at startup instead of during requests.
- Use `discordrs::prelude::*` when you want the shortest path to the main runtime, command, helper, and response APIs.
- Use `DiscordHttpClient::create_followup_message_with_application_id()` when you already have `InteractionContext.application_id` and the client was not initialized with an application id.
- Prefer the typed `RestClient` methods such as `create_message`, `edit_message`, `create_interaction_response_typed`, and `bulk_overwrite_global_commands`.
- Prefer typed REST wrappers such as `bulk_guild_ban`, `get_guild_role`, `get_guild_audit_log`, `get_auto_moderation_rules`, `get_guild_preview`, `get_guild_vanity_url`, `get_voice_regions`, `get_guild_voice_regions`, `get_current_application`, and `get_application_role_connection_metadata_records` before falling back to raw `serde_json::Value` methods.
- `CacheHandle::new()` uses bounded cache defaults so normal gateway builds do not retain every gateway entity forever.
- Use `Client::builder(...).cache_config(...)` or `CacheHandle::with_config(...)` to tune cache limits, and use `CacheConfig::unbounded()` only when unbounded retention is intentional.
- Use `CacheHandle::is_enabled()` when code may be compiled with `default-features = false`; cache storage is enabled in the default feature set.
- Use `member_arc(...)`, `message_arc(...)`, `presence_arc(...)`, and the manager `cached_arc(...)` helpers when hot cache reads should avoid deep cloning large cached payloads.
- Implement `CacheBackend` when experimenting with an external member/message/presence store; `CacheHandle` implements the same trait for the default in-memory backend.
- Invite codes, webhook tokens, interaction tokens, and other token-like REST path segments are validated before authenticated paths are built.
- Generated REST query strings are percent-encoded, and repeated HTTP 429 responses are retried up to a bounded limit before returning `DiscordError::RateLimit`.
- Use `OAuth2Client` for OAuth2 authorization-code or refresh-token flows; it is intentionally separate from bot-token `RestClient` authorization.
- Token-authenticated `/interactions/...` and `/webhooks/...` requests intentionally omit bot `Authorization` headers, and token/path segments are validated before webhook/callback paths are built.
- Typed slash and autocomplete interaction payloads preserve option `value`, `focused`, and nested option input through `CommandInteractionOption`.
- Use `Client` for new gateway code. `BotClient` remains available as a compatibility alias.
- Use `EventHandler::handle_event(...)` when you want one entry point for every gateway event: it receives the shared `Arc<Event>`; match on `event.kind()`. The per-event hooks (`ready`, `message_create`, `interaction_create`, ...) receive the payload without a copy (`Arc<Message>` for `message_create`, references for most others). The raw JSON payload is kept only with `ClientBuilder::keep_raw_events(true)` (always for unknown or undecodable dispatches).
- Use `Context::new(...)` when tests or helper crates need a standalone context outside the live gateway runtime.
- Use `examples/live_dave_capture_bot.rs` to capture matching bot `VOICE_STATE_UPDATE` and `VOICE_SERVER_UPDATE` values before running the ignored live DAVE validation test.
- Prefer builder imports from `discordrs::builders::{...}` or the crate root re-exports. Deeper implementation submodules are private.
- Use `ApplicationCommand::id_opt()` until Discord has assigned an ID. Unsaved commands are no longer treated as generic `DiscordModel`s.
- The parser keeps V2 modal component types, including `FileUpload`, `RadioGroup`, and `CheckboxGroup`, so routing logic can keep full fidelity.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Developer

- ingwannu
- Contact: ingwannu@teamwicked.me, ingwannu@gmail.com
