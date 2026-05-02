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
- collector types behind the `collectors` feature
- `connect_voice_runtime(...)`, `VoiceOpusDecoder`, and live-validated DAVE hooks behind `voice` / `dave`
- typed REST/event coverage for all 223 official Discord REST route shapes audited on 2026-05-02, plus Webhook Events, lobbies, guild incident actions, Activity instances, Gateway rate-limit, reaction metadata, presence metadata dispatches, polls, subscriptions, entitlements, soundboard, thread details, forum fields, invite target-user flows, integrations, OAuth2 metadata/user connections, application command permissions, and channel-info dispatches
- `AppFramework` routing for HTTP interactions

## Feature Flags

```toml
[dependencies]
# core only
discordrs = "2.0.2"

# typed gateway runtime
discordrs = { version = "2.0.2", features = ["gateway"] }

# typed gateway runtime with cache storage or collectors
discordrs = { version = "2.0.2", features = ["gateway", "cache"] }
discordrs = { version = "2.0.2", features = ["gateway", "collectors"] }

# interactions endpoint
discordrs = { version = "2.0.2", features = ["interactions"] }

# voice receive, Opus decode, and DAVE hook
discordrs = { version = "2.0.2", features = ["voice"] }
discordrs = { version = "2.0.2", features = ["voice", "dave"] }
```

## Runtime Extensions

- [Sharding](#/docs/api/sharding)
- [Voice](#/docs/api/voice)

## Language

Use the floating `LANG` button (bottom-right) to switch language.

