# Gateway API

Gateway runtime is provided behind the `gateway` feature.

## Primary Types

- `Client`: high-level typed runtime surface for Gateway bots
- `ClientBuilder`: runtime configuration + startup
- `Event`: typed Gateway event enum
- `GatewayClient`: raw websocket lifecycle management (identify, heartbeat, resume, reconnect)
- `Context`: shared runtime handles (`http`, cache, typemap, shard info)
- `EventHandler`: async trait with `handle_event(ctx, event)`
- `RequestChannelInfo`: typed opcode 43 request for ephemeral voice-channel metadata

## Setup

```toml
[dependencies]
discordrs = { version = "2.0.1", features = ["gateway"] }
```

## Boot Pattern

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

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

