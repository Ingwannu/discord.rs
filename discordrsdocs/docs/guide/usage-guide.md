# discord.rs Usage

`discord.rs` is a standalone Discord bot framework for Rust with a typed Gateway runtime, typed REST surface, Components V2 builders, cache managers, collectors, and an HTTP application framework.

Brand name: discord.rs. The crates.io package name and Rust import path remain `discordrs`.

## 1. Pick a runtime mode

```toml
[dependencies]
# Core default with cache storage
discordrs = "2.0.1"

# Typed gateway runtime
discordrs = { version = "2.0.1", features = ["gateway"] }

# Minimal core without cache storage
discordrs = { version = "2.0.1", default-features = false }

# Typed gateway runtime with collectors
discordrs = { version = "2.0.1", features = ["gateway", "collectors"] }

# HTTP interactions endpoint
discordrs = { version = "2.0.1", features = ["interactions"] }

# Voice receive and Opus decode
discordrs = { version = "2.0.1", features = ["voice"] }

# PCM source/mixer plus Opus encoder playback
discordrs = { version = "2.0.1", features = ["voice", "voice-encode"] }

# DAVE/MLS receive and outbound media hook
discordrs = { version = "2.0.1", features = ["voice", "dave"] }
```

## 2. Start a typed Gateway client

```rust
use async_trait::async_trait;
use discordrs::{gateway_intents, Client, Context, Event, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, _ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => println!("READY: {}", ready.data.user.username),
            Event::MessageCreate(message) => println!("MESSAGE_CREATE: {}", message.message.id),
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;
    Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
        .event_handler(Handler)
        .start()
        .await?;
    Ok(())
}
```

## 3. Register typed commands

```rust
use discordrs::{option_type, CommandOptionBuilder, SlashCommandBuilder};

let command = SlashCommandBuilder::new("ticket", "Create a support ticket")
    .option(
        CommandOptionBuilder::new(option_type::STRING, "topic", "Ticket topic")
            .required(true)
            .autocomplete(true),
    )
    .build();
```

## 4. Use typed REST plus helpers

- Use `RestClient` for direct REST access and manager-backed lookups.
- Use helper functions when you are building Components V2 or interaction responses.
- Keep payload building inside the fluent builders instead of hand-written JSON whenever possible.
- Use `get_guild_application_command_permissions(...)`, `get_application_command_permissions(...)`, and `edit_application_command_permissions(...)` for command permission flows. Permission writes require an OAuth2 Bearer token with Discord's `applications.commands.permissions.update` scope.
- Use `get_current_user_connections(...)` and the current-user application role connection helpers for OAuth2 linked-role flows.
- Use `GetGuildQuery` with `get_guild_with_query(...)` for approximate guild counts, `ModifyGuildChannelPosition` with `modify_guild_channel_positions(...)` for guild channel reordering, `CreateStageInstance` and `ModifyStageInstance` for Stage Instance writes, `AddGuildMember` with `add_guild_member(...)` for OAuth2 `guilds.join` member adds, `AuditLogQuery` with `get_guild_audit_log_typed(...)` for Audit Log reads, `get_guild_role_member_counts(...)` for role membership counts, `get_guild_widget_image(...)` for public PNG widgets, and `AddGroupDmRecipient` for `gdm.join` group DM recipient flows.
- Prefer typed legacy replacements: `list_public_archived_threads(...)` instead of `get_public_archived_threads(...)`, and `get_guild_audit_log_typed(...)` instead of `get_guild_audit_log(...)`.
- `discordrs::Error` and `discordrs::BoxError` remain compatibility aliases during the 2.x line. New code should use `DiscordError`; the aliases are candidates for removal in the next major release.

## 4.5 Route HTTP interactions

```rust
use discordrs::{AppFramework, InteractionResponse};

let app = AppFramework::builder()
    .command("hello", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "Hello from discord.rs"
        }))
    })
    .component("ticket:close", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "Ticket closed",
            "flags": 64
        }))
    })
    .build();
```

Pass the framework to `try_typed_interactions_endpoint(...)` when you want route-based slash command, component, and modal handling.

## 5. Turn on cache or collectors when the bot needs them

- `cache`: enables the in-memory cache storage, `CacheBackend` extension trait, and gateway manager reads; this feature is included by default in `2.0.1`
- `collectors`: enables async collectors for messages, interactions, components, and modals

Hot member, message, and presence cache lookups have `Arc` variants such as `member_arc(...)`, `message_arc(...)`, `presence_arc(...)`, and manager `cached_arc(...)` helpers. Use them when repeated cache reads should avoid deep cloning larger cached payloads. The existing owned-return methods remain available for compatibility.

## 6. Use typed Discord coverage before raw JSON

- Polls: `CreatePoll`, `Poll`, `get_poll_answer_voters(...)`, `end_poll(...)`, `MESSAGE_POLL_VOTE_ADD`, `MESSAGE_POLL_VOTE_REMOVE`
- Monetization: `Sku`, `Entitlement`, `Subscription`, current SKU/entitlement response metadata, entitlement helpers, SKU subscription helpers, `ENTITLEMENT_*`, `SUBSCRIPTION_*`
- Soundboard: default/guild soundboard REST helpers plus `GUILD_SOUNDBOARD_*` and `SOUNDBOARD_SOUNDS`
- Stickers: `StickerPack`, `get_sticker_pack(...)`, `CreateGuildSticker`, `ModifyGuildSticker`, and typed guild sticker write wrappers
- Threads and forums: thread member/detail/archive helpers plus forum tags, applied tags, default reactions, and default thread slowmode fields
- Integrations and invites: integration list/delete, `INTEGRATION_*`, invite options, `INVITE_CREATE`, and `INVITE_DELETE`
- Webhook Events: `parse_webhook_event_payload(...)`, `WebhookEventPayload`, `WebhookPayloadType`, and `WebhookEvent`
- OAuth2 user data: `UserConnection`, `UserApplicationRoleConnection`, `CurrentUserGuildsQuery`, current-user guild pagination/count reads, and current-user application role connection REST helpers
- Application management: `Application`, `ModifyCurrentApplication`, `ApplicationInstallParams`, and typed current-application edit helpers
- Lobbies: `Lobby`, `LobbyMember`, lobby member upserts, channel linking, and moderation metadata helpers
- Guild safety: `GuildIncidentsData`, `ModifyGuildIncidentActions`, and `modify_guild_incident_actions(...)`
- Audit logs: `AuditLog`, `AuditLogQuery`, and `get_guild_audit_log_typed(...)`
- Guild administration: `GetGuildQuery`, `ModifyGuild`, `CreateGuildChannel`, `GuildBansQuery`, `GuildMembersQuery`, `SearchGuildMembersQuery`, `Member`, `CreateGuildBan`, `BeginGuildPruneRequest`, `CreateStageInstance`, `ModifyStageInstance`, `ModifyGuildMember`, `ModifyCurrentMember`, `ModifyCurrentUserVoiceState`, `ModifyUserVoiceState`, `ModifyGuildChannelPosition`, `ModifyGuildRolePosition`, `CreateGuildRole`, `ModifyGuildRole`, `ModifyGuildWidgetSettings`, `ModifyGuildWelcomeScreen`, `ModifyGuildOnboarding`, `AddGuildMember`, guild count fetches, guild writes, channel creation/reordering, guild ban pagination, single-member ban bodies, typed guild prune begin bodies, Stage Instance writes, voice-state REST reads/writes, guild member profile fields and search/list pagination, typed member/current-member edits, typed role create/update/reordering bodies, typed widget/welcome/onboarding writes, OAuth2 guild-member joins, role member-count reads, typed guild widgets, and public widget images
- Group DMs: `CreateGroupDmChannel`, `create_group_dm_channel_typed(...)`, `AddGroupDmRecipient`, `add_group_dm_recipient(...)`, and `remove_group_dm_recipient(...)`
- Channel administration: `CreateChannelInvite`, invite `role_ids`, target-user CSV helpers, `get_channel_invites(...)`, `create_channel_invite_typed(...)`, `SetVoiceChannelStatus`, and `EditChannelPermission`
- Message resource: `SearchGuildMessagesQuery`, `search_guild_messages(...)`, `ChannelPinsQuery`, and `get_channel_pins(...)`
- Message model extras: `MessageSnapshot`, `MessageCall`, `SharedClientTheme`, referenced messages, and mention-role IDs
- Activity sessions: `ActivityInstance`, `ActivityLocation`, and `get_application_activity_instance(...)`
- Webhooks: `CreateWebhook`, `ModifyWebhook`, `ModifyWebhookWithToken`, `WebhookExecuteQuery`, `WebhookMessageQuery`, query-aware execution/message helpers, and Slack/GitHub-compatible execution helpers
- Gateway channel metadata: `RequestChannelInfo::voice_metadata(...)`, `Context::request_channel_info(...)`, and typed `CHANNEL_INFO` events
- REST route coverage: the 2026-05-02 audit maps all 223 official Discord REST `<Route>` entries from `discord-api-docs` to route wrappers or tokenized helper paths. This is a route-shape guarantee; object-field drift and live-only behavior still need focused tests when Discord updates upstream docs.

## 7. Voice receive boundaries

```rust
use discordrs::{connect_voice_runtime, VoiceOpusDecoder, VoiceRuntimeConfig};

async fn receive_pcm() -> Result<(), discordrs::DiscordError> {
    let handle = connect_voice_runtime(VoiceRuntimeConfig::new(
        "guild_id",
        "bot_user_id",
        "voice_session_id",
        "voice_token",
        "wss://voice.discord.media/?v=8",
    ))
    .await?;

    let mut decoder = VoiceOpusDecoder::discord_default()?;
    let decoded = handle.recv_decoded_voice_packet(&mut decoder, 2048).await?;
    println!("{} PCM samples/channel", decoded.samples_per_channel);
    handle.close().await
}
```

Default `voice` covers raw UDP receive, RTP header parsing, RTP-size transport decrypt, Opus-frame send, and Opus PCM decode. Enable `voice-encode` for `PcmFrame`, `AudioSource`, `AudioMixer`, and `VoiceOpusEncoder`. Active DAVE sessions require `recv_voice_packet_with_dave(...)` or `recv_decoded_voice_packet_with_dave(...)` with a `VoiceDaveFrameDecryptor`; the `dave` feature exposes `VoiceDaveySession` and outbound DAVE media helpers. The ignored live MLS transition harness is the release gate for Discord interop evidence.

## 8. Keep old raw helpers only for migration

- `parse_raw_interaction(...)` still exists
- `BotClient` still exists as a compatibility alias
- `EventHandler::handle_event(...)` is the typed gateway entry point; legacy callbacks such as `ready`, `message_create`, and `interaction_create` remain available for compatibility and now accept typed payloads too
- New code should prefer `Client`, `Event`, `RestClient`, and `parse_interaction(...)`
