# discord.rs Documentation

A practical docs site for building typed Discord bots with the `discordrs` crate.

> discord.rs now centers on `Client`, `RestClient`, typed models/events/interactions, command builders, Components V2 helpers, and optional cache/collector layers.

Brand name: discord.rs. The crates.io package name and Rust import path remain `discordrs`.

## Start Here

- [Getting Started](#/docs/guide/getting-started)
- [Architecture](#/docs/guide/architecture)
- [Usage Guide](#/docs/guide/usage-guide)
- [Commands API](#/docs/api/commands)
- [Cache and Collectors](#/docs/api/cache-and-collectors)

## Main Runtime Surfaces

- `Client`: typed Gateway runtime with `Event` dispatch through `EventHandler::handle_event(...)`
- `RestClient`: low-level REST surface with shared rate-limit state
- `parse_interaction(...)`: typed interaction decoding
- `SlashCommandBuilder` / `UserCommandBuilder` / `MessageCommandBuilder`
- `CacheHandle` plus manager types, with bounded in-memory cache storage enabled by default
- collector types behind the `collectors` feature, with stop handles, idle timeouts, and end reasons
- `ProcessShardManager` multi-process sharding behind `sharding` (the discord.js `ShardingManager` equivalent)
- `connect_voice_runtime(...)`, `VoiceOpusDecoder`, the `AudioPlayer` playback pipeline, and live-validated DAVE hooks behind `voice` / `voice-encode` / `dave`
- typed REST/event coverage for all 223 official Discord REST route shapes audited on 2026-05-02, plus Webhook Events, lobbies, guild incident actions, Activity instances, Gateway rate-limit, reaction metadata, presence metadata dispatches, polls, subscriptions, entitlements, soundboard, thread details, forum fields, invite target-user flows, integrations, OAuth2 metadata/user connections, application command permissions, and channel-info dispatches
- `AppFramework` routing for HTTP interactions
- `2.1` additions: audit-log reasons via `RestClient::with_reason(...)`, guild create/delete/template/MFA routes, `with_response=true` interaction callbacks, message forwarding helpers, poll intents, initial IDENTIFY presence, `Context::fetch_members(...)` chunk collection, concurrent event dispatch mode, GUILD_CREATE cache population, and a retry-hardened REST transport
- `2.2.0` additions: the discord.js-style ergonomics layer — `interaction.reply(...)` and friends via `discordrs::response::InteractionResponder`, entity convenience methods in `discordrs::model_ext` (`message.reply`, `member.timeout`, `guild.create_channel`, `channel.send`, `user.dm`, ...), multi-process sharding with `ProcessShardManager` and `broadcast(...)`, the `AudioPlayer`/`AudioResource` playback pipeline, collector stop handles/idle timeouts/end reasons, builder `validate()`/`try_build()`, `RestClient::builder()` configuration, default allowed mentions, pluggable `CacheBackend` forwarding with a configurable sweep interval, and typed `Message.components`/`ResolvedData`/`MessageInteractionMetadata`

## Feature Flags

```toml
[dependencies]
# core only
discordrs = "2.2.0"

# typed gateway runtime
discordrs = { version = "2.2.0", features = ["gateway"] }

# typed gateway runtime with cache storage or collectors
discordrs = { version = "2.2.0", features = ["gateway", "cache"] }
discordrs = { version = "2.2.0", features = ["gateway", "collectors"] }

# interactions endpoint
discordrs = { version = "2.2.0", features = ["interactions"] }

# voice receive, Opus decode, and DAVE hook
discordrs = { version = "2.2.0", features = ["voice"] }
discordrs = { version = "2.2.0", features = ["voice", "dave"] }
```

## Runtime Extensions

- [Sharding](#/docs/api/sharding)
- [Voice](#/docs/api/voice)

## Language

Use the floating `LANG` button (bottom-right) to switch language.

