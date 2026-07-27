# Gateway API

Gateway runtime is provided behind the `gateway` feature.

## Primary Types

- `Client`: high-level typed runtime surface for Gateway bots
- `ClientBuilder`: runtime configuration + startup
- `Event`: typed Gateway event enum
- `GatewayClient`: raw websocket lifecycle management (identify, heartbeat, resume, reconnect)
- `Context`: shared runtime handles (`http`, cache, typemap, shard info)
- `EventHandler`: async trait with `handle_event(ctx, event)`
- `EventDispatchMode`: serial (default) or concurrent handler scheduling
- `RequestChannelInfo`: typed opcode 43 request for ephemeral voice-channel metadata

## Setup

```toml
[dependencies]
discordrs = { version = "2.2.0", features = ["gateway"] }
```

## Boot Pattern

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

## Intents

`gateway_intents` exposes named constants for every documented intent bit, including the poll intents added in `2.1`:

- `gateway_intents::GUILD_MESSAGE_POLLS` (`1 << 24`) and `gateway_intents::DIRECT_MESSAGE_POLLS` (`1 << 25`) enable the `MESSAGE_POLL_VOTE_*` events; both are included in `gateway_intents::NON_PRIVILEGED`.
- `gateway_intents::GUILD_EXPRESSIONS` is the current Discord name for bit 3 (the older `GUILD_EMOJIS_AND_STICKERS` constant remains).

## Initial Presence and Dispatch Mode

`ClientBuilder::presence(...)` delivers an initial presence inside IDENTIFY, so the bot connects with the desired status and activity instead of updating it after READY. `ClientBuilder::event_dispatch(...)` chooses how handler calls are scheduled:

```rust
use discordrs::{EventDispatchMode, UpdatePresence};

Client::builder(&token, intents)
    .event_handler(handler)
    .presence(UpdatePresence::online_with_activity("Handling tickets"))
    .event_dispatch(EventDispatchMode::Concurrent)
    .start()
    .await?;
```

- `EventDispatchMode::Serial` (default): events on a shard are handled one at a time, in gateway order.
- `EventDispatchMode::Concurrent`: each handler call runs in its own task. Cache and collector updates still happen in gateway order before dispatch, but one slow handler no longer stalls the shard. A warning is logged when the dispatch backlog exceeds 5,000 events.

## Default Allowed Mentions and Cache Backend (2.2.0)

`ClientBuilder` gains two configuration hooks in `2.2.0`:

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{AllowedMentions, Client};

fn configure(token: &str, intents: u64, backend: Arc<dyn CacheBackend>) {
    let _builder = Client::builder(token, intents)
        // Injected into outgoing message payloads whenever the payload does
        // not set allowed_mentions itself — discord.js ClientOptions#allowedMentions.
        .default_allowed_mentions(AllowedMentions::default())
        // Forwards member/message/presence cache writes to an external store
        // (Redis, Valkey, ...) from a spawned task per event, so a slow
        // backend cannot stall the gateway.
        .cache_backend(backend);
}
```

The same allowed-mentions default exists on the REST side via `RestClient::builder().default_allowed_mentions(...)`, and `Context::default_allowed_mentions()` exposes the configured value inside handlers.

## Respond to Interactions from Gateway Events (2.2.0)

Import `discordrs::response::InteractionResponder` and reply to `Event::InteractionCreate` payloads directly, discord.js-style:

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, Interaction};

async fn on_event(ctx: Context, event: Event) -> Result<(), discordrs::DiscordError> {
    if let Event::InteractionCreate(event) = event {
        if let Interaction::ChatInputCommand(command) = event.interaction {
            command.reply(&ctx.http, "hi").await?;
        }
    }
    Ok(())
}
```

See [HTTP and Helpers](http-and-helpers.md) for the full responder surface (`reply_ephemeral`, `defer`, `edit_reply`, `follow_up`, `show_modal`, `respond_autocomplete`, ...).

## Fetch Guild Members over the Gateway

`Context::fetch_members(...)` requests guild members over the gateway and awaits the correlated `GUILD_MEMBERS_CHUNK` payloads — the discord.js `guild.members.fetch()` equivalent. Fetched members and presences also fill the cache.

```rust
// All members (requires the GUILD_MEMBERS privileged intent).
let members = ctx.fetch_members(guild_id, None, None).await?;

// Username prefix search with a limit.
let admins = ctx.fetch_members(guild_id, Some("admin".to_string()), Some(10)).await?;
```

`Context::fetch_members_with_timeout(...)` overrides the default 60-second overall deadline, and the lower-level `ctx.request_guild_members(...)` remains available for raw chunk handling.

## Event Surface

- Prefer `handle_event` for new code.
- `Event` currently exposes typed variants for `READY`, message events, interaction events, guild/channel/member/role cache flows, `CHANNEL_INFO`, `RATE_LIMITED`, and `Unknown`.
- Reaction dispatches preserve Discord's current metadata fields, including `member`, `message_author_id`, `burst`, `burst_colors`, and `reaction_type`.
- Presence dispatches expose the documented partial user ID, activities, and `ClientStatus` platform status metadata.
- Legacy `ready`, `message_create`, `interaction_create`, and `raw_event` hooks still exist for migration.

## Gateway Control Helpers

`Context` and `ShardMessenger` expose typed control paths for common outbound Gateway commands:

- presence updates
- guild member requests
- soundboard sound requests
- voice state updates
- channel info requests through `request_channel_info(RequestChannelInfo::voice_metadata(guild_id))`

`RequestChannelInfo` serializes to Discord Gateway opcode 43 and currently exposes the documented `status` and `voice_start_time` fields. Incoming `CHANNEL_INFO` dispatches decode to `Event::ChannelInfo`.

## Operational Notes

- Keep handler methods non-blocking.
- Push heavy work to background tasks.
- Use `Context.rest()` or the cache-aware managers from `Context`.
- `BotClient` still exists as a compatibility alias, but the docs prefer `Client`.
- `1.2.2` fixes Gateway compression negotiation so default connections do not request payload compression without a decoder, and explicit `zlib-stream` connections decode compressed `HELLO` frames before Identify.
- `2.1` paces IDENTIFY to Discord's 1-per-5s-per-shard limit, and short-lived sessions (< 30s) reconnect with escalating backoff instead of immediately, preventing tight reconnect/re-IDENTIFY loops on repeated INVALID_SESSION.
- `2.1` surfaces gateway protocol failures (missing Hello, decode errors, terminal close codes, command-queue overflow) as `DiscordError::Gateway` instead of `DiscordError::Model`.

