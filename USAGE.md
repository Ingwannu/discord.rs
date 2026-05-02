# discord.rs Usage

`discord.rs` is a standalone Rust Discord framework with typed models, typed gateway events, command builders, Components V2 builders, REST helpers, cache managers, collectors, sharding control, an HTTP application framework, and voice runtime foundations.

Brand name: discord.rs. The crates.io package name and Rust import path remain `discordrs`.

## 1. Installation

Pick features based on the runtime surface you want to ship.

```toml
[dependencies]
# Core default: models, builders, parsers, helpers, REST client, cache storage
discordrs = "2.0.0"

# Gateway runtime
discordrs = { version = "2.0.0", features = ["gateway"] }

# HTTP interactions endpoint
discordrs = { version = "2.0.0", features = ["interactions"] }

# Minimal core without cache storage
discordrs = { version = "2.0.0", default-features = false }

# Gateway runtime with collectors
discordrs = { version = "2.0.0", features = ["gateway", "collectors"] }

# Gateway runtime with shard supervisor and shard status APIs
discordrs = { version = "2.0.0", features = ["gateway", "sharding"] }

# Voice manager plus voice gateway/UDP runtime
discordrs = { version = "2.0.0", features = ["voice"] }

# PCM source/mixer plus Opus encoder playback
discordrs = { version = "2.0.0", features = ["voice", "voice-encode"] }

# DAVE/MLS receive and outbound media hooks
discordrs = { version = "2.0.0", features = ["voice", "dave"] }

# Gateway runtime with voice helpers
discordrs = { version = "2.0.0", features = ["gateway", "voice"] }

# Gateway runtime with zstd-stream transport compression
discordrs = { version = "2.0.0", features = ["gateway", "zstd-stream"] }
```

If you want the common runtime helpers in one import, prefer:

```rust
use discordrs::prelude::*;
```

## 1.5 Migration Notes

The public API was tightened to make the typed surface the default:

- `RestClient` no longer exposes the old raw convenience methods such as `send_message`, `edit_message`, `create_dm_channel`, `create_interaction_response`, and `bulk_overwrite_global_commands`.
- Builder implementation submodules are private. Import from `discordrs::builders::{...}` or use the crate root re-exports.
- `ApplicationCommand` no longer implements `DiscordModel`; use `id_opt()` and `created_at()` directly on the command value.
- `1.2.2` fixes Gateway compression handling so default connections no longer request payload compression without a decoder, and explicit `zlib-stream` connections decode compressed `HELLO` frames before Identify.

Common replacements:

| Old path | New path |
|----------|----------|
| `RestClient::send_message(...)` | `send_message(...)` helper or `RestClient::create_message(...)` |
| `RestClient::edit_message(...)` | `RestClient::update_message(...)` |
| `RestClient::create_dm_channel(...)` | `RestClient::create_dm_channel_typed(...)` |
| `RestClient::create_interaction_response(...)` | `RestClient::create_interaction_response_typed(...)` or typed helper functions |
| `RestClient::bulk_overwrite_global_commands(...)` | `RestClient::bulk_overwrite_global_commands_typed(...)` |
| `discordrs::builders::modal::*` | `discordrs::builders::{...}` or crate root re-exports |
| generic `DiscordModel` access for `ApplicationCommand` | `ApplicationCommand::id_opt()` / `ApplicationCommand::created_at()` |

## 2. Start a Typed Gateway Bot

`Client` is the primary runtime entry point. `BotClient` remains as a compatibility alias.

Prefer `EventHandler::handle_event(...)` when you want typed `Event` dispatch from a single match point. The legacy per-event convenience callbacks remain available for compatibility, and `ready`, `message_create`, and `interaction_create` now also receive typed payloads.

```rust
use async_trait::async_trait;
use discordrs::{gateway_intents, Client, Context, Event, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                println!("READY: {}", ready.data.user.username);
                println!("Shard: {:?}", ctx.shard_pair());
            }
            Event::MessageCreate(message) => {
                println!("MESSAGE_CREATE: {}", message.message.content);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    Client::builder(
        &token,
        gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES,
    )
    .event_handler(Handler)
    .start()
    .await?;

    Ok(())
}
```

## 3. Create a `Context` Outside the Runtime

If you have test code or helper code that used to build `Context` manually, use `Context::new(http, data)`.

```rust
use std::sync::Arc;

use discordrs::{Context, DiscordHttpClient, TypeMap};
use tokio::sync::RwLock;

let http = Arc::new(DiscordHttpClient::new("token", 0));
let data = Arc::new(RwLock::new(TypeMap::new()));

let ctx = Context::new(http, data);
assert_eq!(ctx.shard_pair(), (0, 1));
```

`Context::new(...)` gives you a default standalone context:

- fresh `CacheHandle`
- shard pair `(0, 1)`
- empty gateway command map
- default `VoiceManager` when `voice` is enabled
- default `CollectorHub` when `collectors` is enabled

## 4. Register Typed Commands

Use command builders instead of passing raw JSON command bodies.

```rust
use discordrs::{
    option_type, CommandOptionBuilder, PermissionsBitField, SlashCommandBuilder,
};

let command = SlashCommandBuilder::new("ticket", "Create a support ticket")
    .string_option("topic", "Ticket topic", true)
    .option(
        CommandOptionBuilder::new(option_type::BOOLEAN, "private", "Create as private ticket")
            .required(false),
    )
    .default_member_permissions(PermissionsBitField(0))
    .build();
```

With a REST client:

```rust
use discordrs::{DiscordHttpClient, SlashCommandBuilder};

async fn register(http: &DiscordHttpClient) -> Result<(), discordrs::DiscordError> {
    let command = SlashCommandBuilder::new("hello", "Reply with hello").build();
    http.create_global_command(&command).await?;
    Ok(())
}
```

## 4.5 OAuth2 Backend Helpers

Use `OAuth2Client` for application backend OAuth2 flows. It is separate from `RestClient` because OAuth2 token endpoints use form-encoded application credentials, not bot-token `Authorization`.

```rust
use discordrs::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2Scope,
};

async fn oauth_flow() -> Result<(), discordrs::DiscordError> {
    let oauth = OAuth2Client::new("client_id", "client_secret");
    let authorize_url = oauth.authorization_url(
        OAuth2AuthorizationRequest::code(
            "https://app.example/callback",
            [OAuth2Scope::identify(), OAuth2Scope::guilds()],
        )
        .state("csrf-token")
        .prompt("consent"),
    )?;

    println!("Open: {authorize_url}");

    let token = oauth
        .exchange_code(OAuth2CodeExchange::new(
            "returned-code",
            "https://app.example/callback",
        ))
        .await?;
    println!("OAuth token type: {}", token.token_type);
    Ok(())
}
```

## 4.6 Guild REST Helpers

Guild channel reordering and OAuth2 guild joins have typed request bodies:

```rust
use discordrs::{
    AddGroupDmRecipient, AddGuildMember, AuditLogQuery, BeginGuildPruneRequest, CreateGuildBan,
    CreateGuildChannel, CreateGuildRole, CreateStageInstance, DiscordHttpClient, GetGuildQuery,
    GuildBansQuery, GuildWidgetImageStyle, ModifyCurrentMember, ModifyGuild,
    ModifyGuildChannelPosition, ModifyGuildMember, ModifyGuildOnboarding, ModifyGuildRole,
    ModifyGuildRolePosition, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings,
    ModifyStageInstance, RoleColors, SearchGuildMembersQuery, Snowflake, WelcomeScreenChannel,
};

async fn guild_admin(http: &DiscordHttpClient) -> Result<(), discordrs::DiscordError> {
    let guild_id = Snowflake::from("123");
    let channel_id = Snowflake::from("456");
    let user_id = Snowflake::from("789");

    let guild = http
        .get_guild_with_query(
            guild_id.clone(),
            &GetGuildQuery {
                with_counts: Some(true),
            },
        )
        .await?;
    println!("guild: {}", guild.name);

    let bans = http
        .get_guild_bans_with_query(
            guild_id.clone(),
            &GuildBansQuery {
                limit: Some(100),
                after: Some(Snowflake::from("100")),
                ..Default::default()
            },
        )
        .await?;
    println!("bans: {}", bans.len());

    let matching_members = http
        .search_guild_members_with_query(
            guild_id.clone(),
            &SearchGuildMembersQuery {
                query: "alice & bob".to_string(),
                limit: Some(5),
            },
        )
        .await?;
    println!("matching members: {}", matching_members.len());

    http.modify_guild(
        guild_id.clone(),
        &ModifyGuild {
            name: Some("renamed".to_string()),
            description: Some(None),
            ..Default::default()
        },
    )
    .await?;

    http.create_guild_channel_from_request(
        guild_id.clone(),
        &CreateGuildChannel {
            name: "rules".to_string(),
            kind: Some(0),
            parent_id: Some(None),
            ..Default::default()
        },
    )
    .await?;

    http.modify_guild_channel_positions(
        guild_id.clone(),
        &[ModifyGuildChannelPosition {
            id: channel_id,
            position: Some(Some(2)),
            lock_permissions: None,
            parent_id: None,
        }],
    )
    .await?;

    http.modify_guild_role_positions_typed(
        guild_id.clone(),
        &[ModifyGuildRolePosition {
            id: Snowflake::from("321"),
            position: Some(Some(1)),
        }],
    )
    .await?;

    http.create_guild_ban_typed(
        guild_id.clone(),
        user_id.clone(),
        &CreateGuildBan {
            delete_message_seconds: Some(60),
            ..Default::default()
        },
    )
    .await?;

    let role = http
        .create_guild_role(
            guild_id.clone(),
            &CreateGuildRole {
                name: Some("Gradient".to_string()),
                colors: Some(RoleColors {
                    primary_color: 11127295,
                    secondary_color: Some(16759788),
                    tertiary_color: Some(16761760),
                }),
                ..Default::default()
            },
        )
        .await?;

    http.modify_guild_role(
        guild_id.clone(),
        role.id.clone(),
        &ModifyGuildRole {
            mentionable: Some(Some(true)),
            unicode_emoji: Some(None),
            ..Default::default()
        },
    )
    .await?;

    http.modify_guild_member_typed(
        guild_id.clone(),
        user_id.clone(),
        &ModifyGuildMember {
            nick: Some(None),
            channel_id: Some(None),
            ..Default::default()
        },
    )
    .await?;

    let current_member = http
        .modify_current_member(
            guild_id.clone(),
            &ModifyCurrentMember {
                nick: Some(Some("bot".to_string())),
                avatar: Some(None),
                ..Default::default()
            },
        )
        .await?;
    println!("current member: {:?}", current_member.user);

    http.modify_guild_widget(
        guild_id.clone(),
        &ModifyGuildWidgetSettings {
            enabled: Some(true),
            channel_id: Some(None),
        },
    )
    .await?;

    http.modify_guild_welcome_screen_config(
        guild_id.clone(),
        &ModifyGuildWelcomeScreen {
            enabled: Some(Some(true)),
            welcome_channels: Some(Some(vec![WelcomeScreenChannel {
                channel_id: Snowflake::from("456"),
                description: "Start here".to_string(),
                emoji_id: None,
                emoji_name: Some("wave".to_string()),
            }])),
            description: Some(None),
        },
    )
    .await?;

    http.modify_guild_onboarding_config(
        guild_id.clone(),
        &ModifyGuildOnboarding {
            default_channel_ids: Some(vec![Snowflake::from("456")]),
            enabled: Some(true),
            mode: Some(0),
            ..Default::default()
        },
    )
    .await?;

    let prune = http
        .begin_guild_prune_with_request(
            guild_id.clone(),
            &BeginGuildPruneRequest {
                days: Some(7),
                compute_prune_count: Some(false),
                include_roles: Some(vec![Snowflake::from("321")]),
                ..Default::default()
            },
        )
        .await?;
    println!("pruned: {:?}", prune.pruned);

    let stage = http
        .create_stage_instance_from_request(&CreateStageInstance {
            channel_id: channel_id.clone(),
            topic: "Town hall".to_string(),
            privacy_level: Some(2),
            send_start_notification: Some(true),
            guild_scheduled_event_id: None,
        })
        .await?;

    http.modify_stage_instance_from_request(
        stage.channel_id,
        &ModifyStageInstance {
            privacy_level: Some(2),
        },
    )
    .await?;

    let member = http
        .add_guild_member(
            guild_id.clone(),
            user_id.clone(),
            &AddGuildMember {
                access_token: "oauth2-guilds.join-token".to_string(),
                ..Default::default()
            },
        )
        .await?;

    if member.is_none() {
        println!("User was already in the guild");
    }

    let role_counts = http.get_guild_role_member_counts(guild_id.clone()).await?;
    println!("{role_counts:?}");

    let audit_log = http
        .get_guild_audit_log_typed(
            guild_id.clone(),
            &AuditLogQuery {
                after: Some(Snowflake::from("100")),
                limit: Some(25),
                ..Default::default()
            },
        )
        .await?;
    println!("audit entries: {}", audit_log.audit_log_entries.len());

    let widget_png = http
        .get_guild_widget_image(guild_id, Some(GuildWidgetImageStyle::Banner2))
        .await?;
    println!("widget bytes: {}", widget_png.len());

    http.add_group_dm_recipient(
        Snowflake::from("555"),
        user_id,
        &AddGroupDmRecipient {
            access_token: "oauth2-gdm.join-token".to_string(),
            nick: "friend".to_string(),
        },
    )
    .await?;

    Ok(())
}
```

## 5. Send Messages with Typed Helpers

If you want a typed message body instead of hand-written JSON, use `MessageBuilder` and `send_message`.

```rust
use discordrs::{send_message, ActionRowBuilder, ButtonBuilder, MessageBuilder, button_style};

async fn send_panel(
    http: &discordrs::DiscordHttpClient,
    channel_id: u64,
) -> Result<(), discordrs::DiscordError> {
    let message = MessageBuilder::new()
        .content("Support panel")
        .components(vec![
            ActionRowBuilder::new()
                .add_button(
                    ButtonBuilder::new()
                        .custom_id("ticket_open")
                        .label("Open Ticket")
                        .style(button_style::PRIMARY),
                )
                .build(),
        ]);

    send_message(http, channel_id, message.build()).await?;
    Ok(())
}
```

For Components V2 containers, the existing builder path still works:

```rust
use discordrs::{
    button_style, create_container, send_container_message, ButtonConfig, DiscordHttpClient,
};

async fn send_support_panel(
    http: &DiscordHttpClient,
    channel_id: u64,
) -> Result<(), discordrs::DiscordError> {
    let buttons = vec![
        ButtonConfig::new("ticket_open", "Open Ticket").style(button_style::PRIMARY),
        ButtonConfig::new("ticket_status", "Check Status").style(button_style::SECONDARY),
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

Auto-populated select menus preserve Discord's current `default_values`, component `id`, and modal `required` fields:

```rust
use discordrs::{ActionRowBuilder, SelectDefaultValue, SelectMenuBuilder};

let row = ActionRowBuilder::new().add_select_menu(
    SelectMenuBuilder::mentionable("notify_targets")
        .id(12)
        .default_value(SelectDefaultValue::role("701"))
        .min_values(0)
        .max_values(2)
        .required(false),
);
```

## 5.4 Sticker Resource Helpers

Sticker packs and guild sticker writes are typed:

```rust
use discordrs::{CreateGuildSticker, DiscordHttpClient, FileAttachment, ModifyGuildSticker};

async fn sticker_admin(http: &DiscordHttpClient) -> Result<(), discordrs::DiscordError> {
    let pack = http.get_sticker_pack(123456789012345678).await?;
    println!("pack: {}", pack.name);

    let sticker = http
        .create_guild_sticker_from_request(
            123456789012345678,
            &CreateGuildSticker {
                name: "wave".to_string(),
                description: "Waves hello".to_string(),
                tags: "wave,hello".to_string(),
            },
            FileAttachment::new("wave.png", Vec::<u8>::new()),
        )
        .await?;

    http.modify_guild_sticker_from_request(
        123456789012345678,
        sticker.id,
        &ModifyGuildSticker {
            name: Some("wave2".to_string()),
            description: Some(None),
            tags: Some("wave".to_string()),
        },
    )
    .await?;

    Ok(())
}
```

## 5.5 Application Resource Helpers

Current application reads and edits are typed through `Application`, `ModifyCurrentApplication`, `ApplicationInstallParams`, and `ApplicationIntegrationTypeConfig`. The existing generic `edit_current_application(...)` helper is still available for newly released Discord fields.

```rust
use discordrs::{ApplicationInstallParams, DiscordHttpClient, ModifyCurrentApplication, PermissionsBitField};

async fn update_application(http: &DiscordHttpClient) -> Result<(), discordrs::DiscordError> {
    let app = http.get_current_application().await?;
    println!("editing {}", app.name);

    http.edit_current_application_from_request(&ModifyCurrentApplication {
        description: Some("Support utilities".to_string()),
        install_params: Some(ApplicationInstallParams {
            scopes: vec!["bot".to_string(), "applications.commands".to_string()],
            permissions: PermissionsBitField(2048),
        }),
        tags: Some(vec!["utility".to_string()]),
        ..ModifyCurrentApplication::default()
    })
    .await?;

    Ok(())
}
```

## 5.6 Search Messages and List Current Pins

Discord's Message Resource search and current-pin routes are exposed as typed `RestClient` methods:

```rust
use discordrs::{ChannelPinsQuery, DiscordHttpClient, SearchGuildMessagesQuery};

async fn inspect_messages(
    http: &DiscordHttpClient,
    guild_id: u64,
    channel_id: u64,
) -> Result<(), discordrs::DiscordError> {
    let search = http
        .search_guild_messages(
            guild_id,
            &SearchGuildMessagesQuery {
                content: Some("release notes".to_string()),
                limit: Some(25),
                include_nsfw: Some(false),
                ..Default::default()
            },
        )
        .await?;

    println!("matches: {}", search.total_results);

    let pins = http
        .get_channel_pins(
            channel_id,
            &ChannelPinsQuery {
                limit: Some(50),
                ..Default::default()
            },
        )
        .await?;

    println!("current pins: {}", pins.items.len());

    http.pin_message(channel_id, 123456789012345678).await?;
    http.unpin_message(channel_id, 123456789012345678).await?;
    Ok(())
}
```

Incoming `Message` values also keep current Message Resource fields such as forwarded `MessageSnapshot` data, `MessageCall` metadata, `SharedClientTheme`, role subscription data, mention-role IDs, and `referenced_message`.

Channel Resource gaps are typed too:

```rust
use discordrs::{
    CreateChannelInvite, EditChannelPermission, FileAttachment, PermissionsBitField,
    SetVoiceChannelStatus,
};

async fn channel_admin(
    http: &discordrs::DiscordHttpClient,
    channel_id: u64,
    role_id: u64,
) -> Result<(), discordrs::DiscordError> {
    let invites = http.get_channel_invites(channel_id).await?;
    println!("active invites: {}", invites.len());

    http.create_channel_invite_typed(
        channel_id,
        &CreateChannelInvite {
            max_age: Some(600),
            max_uses: Some(1),
            unique: Some(true),
            ..Default::default()
        },
    )
    .await?;

    http.create_channel_invite_with_target_users_file(
        channel_id,
        &CreateChannelInvite {
            role_ids: Some(vec![role_id.into()]),
            ..Default::default()
        },
        &FileAttachment::new("target_users.csv", b"user_id\n777\n"),
    )
    .await?;

    let target_users_csv = http.get_invite_target_users_csv("abc123").await?;
    println!("target user csv bytes: {}", target_users_csv.len());
    let job = http.get_invite_target_users_job_status("abc123").await?;
    println!("target user job status: {}", job.status);

    http.set_voice_channel_status(
        channel_id,
        &SetVoiceChannelStatus {
            status: Some("Office hours".to_string()),
        },
    )
    .await?;

    http.edit_channel_permissions_typed(
        channel_id,
        role_id,
        &EditChannelPermission {
            kind: 0,
            allow: Some(PermissionsBitField(1024)),
            deny: None,
        },
    )
    .await?;

    Ok(())
}
```

## 6. Reply to Gateway Interactions Without Raw JSON

`Context` now exposes direct gateway control helpers, and the helpers module exposes typed response helpers.

```rust
use async_trait::async_trait;
use discordrs::{
    defer_interaction, followup_message, gateway_intents, Client, Context, Event, EventHandler,
    MessageBuilder,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        if let Event::InteractionCreate(interaction) = event {
            let interaction = interaction.interaction;
            let interaction_ctx = interaction.context().clone();
            let response = MessageBuilder::new().content("Working...").build();

            let _ = defer_interaction(&ctx.http, &interaction_ctx, true).await;
            let _ = followup_message(&ctx.http, &interaction_ctx, response, true).await;
        }
    }
}
```

Other typed helper entry points:

- `respond_to_interaction(...)`
- `respond_with_message(...)`
- `update_interaction_message(...)`
- `respond_with_modal_typed(...)`

## 7. Build a Typed Interactions Endpoint

If you run an outgoing-interactions HTTP server instead of the gateway runtime, prefer the typed endpoint helpers.

```rust
use async_trait::async_trait;
use axum::Router;
use discordrs::{
    Interaction, InteractionContextData, InteractionResponse, TypedInteractionHandler,
    try_typed_interactions_endpoint,
};

#[derive(Clone)]
struct Handler;

#[async_trait]
impl TypedInteractionHandler for Handler {
    async fn handle_typed(
        &self,
        _ctx: InteractionContextData,
        interaction: Interaction,
    ) -> InteractionResponse {
        match interaction {
            Interaction::ChatInputCommand(command)
                if command.data.name.as_deref() == Some("hello") =>
            {
                InteractionResponse::ChannelMessage(serde_json::json!({
                    "content": "Hello from typed endpoint"
                }))
            }
            _ => InteractionResponse::DeferredMessage,
        }
    }
}

fn build_router(public_key: &str) -> Router {
    try_typed_interactions_endpoint(public_key, Handler)
        .expect("invalid Discord public key")
}
```

Use `try_interactions_endpoint(...)` instead when you intentionally want the raw interaction surface.

Typed slash/autocomplete input now keeps real user-entered option data. `interaction.data.options` uses `CommandInteractionOption`, which preserves nested options plus `value` and `focused` for autocomplete flows.

## 7.5 Route Interactions with `AppFramework`

When the `interactions` feature is enabled, `AppFramework` provides a small high-level routing layer on top of `TypedInteractionHandler`. It routes slash/user/message commands by command name, components by `custom_id`, and modal submissions by `custom_id`.

```rust
use discordrs::{AppFramework, InteractionResponse, RouteKey};

let app = AppFramework::builder()
    .command("hello", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "Hello from AppFramework"
        }))
    })
    .component("ticket:close", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "Ticket closed",
            "flags": 64
        }))
    })
    .cooldown(RouteKey::Command("hello".to_string()), std::time::Duration::from_secs(5))
    .build();
```

Use this for ordinary command/component/modal dispatch. Keep a custom `TypedInteractionHandler` when a request needs unusual routing, streaming state, or raw payload handling.

## 8. Use Cache-Aware Managers

On the gateway runtime, `Context` exposes manager shortcuts in all builds:

- `ctx.guilds()`
- `ctx.channels()`
- `ctx.members()`
- `ctx.messages()`
- `ctx.roles()`

These managers keep the REST handle and cache handle together. The `cache` feature is enabled by default, so normal installs store gateway cache data in memory before falling back to HTTP. The default cache policy is bounded; if you compile with `default-features = false`, the same types still exist but cached reads stay empty. Use `CacheHandle::is_enabled()` when shared code needs to detect that mode.

```rust
async fn inspect_cache(ctx: &discordrs::Context) {
    let guilds = ctx.guilds().list_cached().await;
    println!("Cached guilds: {}", guilds.len());
}
```

For long-running bots, tune the bounded defaults to match your guild size and lookup patterns:

```rust
use std::time::Duration;
use discordrs::{gateway_intents, CacheConfig, Client};

let client = Client::builder("bot-token", gateway_intents::GUILD_MESSAGES)
    .cache_config(
        CacheConfig::default()
            .max_messages_per_channel(100)
            .max_total_messages(10_000)
            .message_ttl(Duration::from_secs(60 * 60))
            .presence_ttl(Duration::from_secs(10 * 60))
            .max_presences(50_000)
            .max_members_per_guild(25_000),
    );
```

Use `CacheConfig::unbounded()` only when retaining all cached gateway data is an intentional operator decision.

## 8.5 REST Safety Notes

`RestClient` validates token-like path segments before authenticated routes are built. This includes invite codes passed to `get_invite`, `get_invite_with_options`, and `delete_invite`, so user-provided invite text cannot inject `/`, `\`, `?`, `#`, or control characters into bot-authorized REST paths.

Application command permission edits are an OAuth2 Bearer-token flow in Discord, not a bot-token flow. Use `get_guild_application_command_permissions(...)` and `get_application_command_permissions(...)` for bot-authenticated reads, then use `edit_application_command_permissions(bearer_token, ...)` with an access token that has Discord's `applications.commands.permissions.update` scope.

```rust
use discordrs::{ApplicationCommandPermission, EditApplicationCommandPermissions};

let edit = EditApplicationCommandPermissions::new([
    ApplicationCommandPermission::role("123456789012345678", true),
]);

let permissions = rest
    .edit_application_command_permissions(oauth_access_token, guild_id, command_id, &edit)
    .await?;
```

Current-user connection and linked-role endpoints are also OAuth2 Bearer-token flows:

```rust
let connections = rest
    .get_current_user_connections(oauth_access_token)
    .await?;

let role_connection = rest
    .get_current_user_application_role_connection(oauth_access_token)
    .await?;
```

Webhook Resource management supports typed request bodies and query-aware execution:

```rust
use discordrs::{CreateWebhook, ModifyWebhook, WebhookExecuteQuery, WebhookMessageQuery};

let webhook = rest
    .create_webhook_from_request(
        channel_id,
        &CreateWebhook {
            name: "deployments".to_string(),
            avatar: None,
        },
    )
    .await?;

let webhook_id = webhook.id.expect("webhook id");
let webhook_token = webhook.token.expect("webhook token");

rest.modify_webhook_from_request(
    webhook_id.clone(),
    &ModifyWebhook {
        name: Some("ops".to_string()),
        avatar: Some(None),
        channel_id: None,
    },
)
.await?;

rest.execute_webhook_with_query(
    webhook_id.clone(),
    &webhook_token,
    &WebhookExecuteQuery {
        wait: Some(false),
        thread_id: Some(thread_id),
        with_components: Some(true),
    },
    &serde_json::json!({ "content": "deployment finished" }),
)
.await?;

let message_query = WebhookMessageQuery {
    thread_id: Some(thread_id),
    with_components: Some(true),
};

let message = rest
    .get_webhook_message_with_query(webhook_id.clone(), &webhook_token, message_id, &message_query)
    .await?;

rest.edit_webhook_message_with_query(
    webhook_id,
    &webhook_token,
    message.id.as_str(),
    &message_query,
    &discordrs::CreateMessage {
        content: Some("deployment edited".to_string()),
        ..Default::default()
    },
)
.await?;
```

Webhook Events use the same Ed25519 request-signature rules as interactions. After verifying the incoming HTTP body, parse the JSON body with `parse_webhook_event_payload(...)`:

```rust
use discordrs::{parse_webhook_event_payload, WebhookEvent, WebhookPayloadType};

let payload = parse_webhook_event_payload(body_json)?;

if payload.kind == WebhookPayloadType::PING {
    // Return HTTP 204 with an empty body.
}

if let Some(event) = payload.event {
    match event.event {
        WebhookEvent::ApplicationAuthorized(data) => {
            println!("{} authorized scopes {:?}", data.user.username, data.scopes);
        }
        WebhookEvent::EntitlementCreate(entitlement) => {
            println!("new entitlement {}", entitlement.id);
        }
        _ => {}
    }
}
```

Guild incident actions use the normal bot authorization path and return typed `GuildIncidentsData`:

```rust
use discordrs::ModifyGuildIncidentActions;

let incidents = rest
    .modify_guild_incident_actions(
        guild_id,
        &ModifyGuildIncidentActions {
            invites_disabled_until: Some("2026-05-01T12:00:00.000000+00:00".to_string()),
            dms_disabled_until: None,
        },
    )
    .await?;
```

Generated query strings are percent-encoded. Request body serialization failures return `DiscordError::Json` instead of panicking, and repeated HTTP 429 responses are retried up to a bounded limit before surfacing `DiscordError::RateLimit`.

## 9. Control the Active Shard from `Context`

When you are inside a gateway handler, `Context` can drive shard-local gateway actions directly.

```rust
async fn rotate_presence(ctx: &discordrs::Context) -> Result<(), discordrs::DiscordError> {
    ctx.update_presence("Handling tickets").await?;
    Ok(())
}
```

Available `Context` control methods:

- `shard_messenger().await`
- `update_presence(...).await`
- `reconnect_shard().await`
- `shutdown_shard().await`
- `update_voice_state(...).await`
- `join_voice(...).await`
- `leave_voice(...).await`

If you want the underlying shard-local sender, call `ctx.shard_messenger().await` and use `ShardMessenger` directly.

## 10. Spawn and Supervise Multiple Shards

With `gateway + sharding`, you have two entry points:

- `start_shards(count)`: spawn and wait until all shard tasks finish
- `spawn_shards(count)`: return a `ShardSupervisor` so you can inspect state and control shards yourself

```rust
use async_trait::async_trait;
use discordrs::{gateway_intents, Client, Context, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    let supervisor = Client::builder(&token, gateway_intents::GUILDS)
        .event_handler(Handler)
        .spawn_shards(4)
        .await?;

    for status in supervisor.statuses() {
        println!("Shard {} state: {:?}", status.info.id, status.state);
    }

    supervisor.shutdown_and_wait().await?;
    Ok(())
}
```

Current sharding behavior:

- initial shard boot is queued instead of identifying every shard at once
- queued shards report `ShardRuntimeState::Queued`
- later shards wait for the earlier shard boot window before being released
- shutdown can be awaited with `shutdown_and_wait()` or `wait_for_shutdown(timeout)`
- reconnect backoff is interruptible, so shutdown does not wait for a long sleep to finish

Useful supervisor APIs:

- `statuses()`
- `drain_events()`
- `send(shard_id, ShardIpcMessage)`
- `reconnect(shard_id)`
- `update_presence(shard_id, ...)`
- `join_voice(shard_id, ...)`
- `leave_voice(shard_id, ...)`
- `shutdown()`
- `shutdown_and_wait().await`
- `wait_for_shutdown(duration).await`

## 11. Voice Manager and Voice Runtime

There are two layers:

- `VoiceManager`: tracks gateway voice state/server updates and local queue state
- `VoiceRuntime`: performs voice websocket and UDP handshake work

From `Context`, the common gateway-driven flow is:

```rust
#[cfg(feature = "voice")]
async fn join_and_prepare_voice(ctx: &discordrs::Context) -> Result<(), discordrs::DiscordError> {
    ctx.join_voice("1", "2", false, false).await?;

    if let Some(config) = ctx.voice_runtime_config("1", "1234").await {
        println!("Voice endpoint: {}", config.websocket_url());
    }

    Ok(())
}
```

If you already have a full runtime config, connect directly:

```rust
use std::time::Duration;

use discordrs::{
    connect_voice_runtime, VoiceOpusDecoder, VoiceOpusFrame, VoiceRuntimeConfig, VoiceSpeakingFlags,
};

async fn connect_runtime() -> Result<(), discordrs::DiscordError> {
    let handle = connect_voice_runtime(VoiceRuntimeConfig::new(
        "1",
        "42",
        "session-id",
        "voice-token",
        "wss://voice.discord.media/?v=8",
    ))
    .await?;

    handle.set_speaking(VoiceSpeakingFlags::MICROPHONE, 0)?;
    handle
        .send_opus_frame(&[0xf8, 0xff, 0xfe], Duration::from_millis(20))
        .await?;
    handle
        .play_opus_frames([VoiceOpusFrame::new([0xf8, 0xff, 0xfe])])
        .await?;

    let mut decoder = VoiceOpusDecoder::discord_default()?;
    let packet = handle.recv_decoded_voice_packet(&mut decoder, 2048).await?;
    println!(
        "received {} samples/channel from SSRC {}",
        packet.samples_per_channel,
        packet.packet.rtp.ssrc
    );
    handle.close().await?;
    Ok(())
}
```

Voice-state REST routes are also typed:

```rust
use discordrs::{DiscordHttpClient, ModifyCurrentUserVoiceState, ModifyUserVoiceState, Snowflake};

async fn manage_voice_state(http: &DiscordHttpClient) -> Result<(), discordrs::DiscordError> {
    let guild_id = Snowflake::from("123");
    let channel_id = Snowflake::from("456");
    let user_id = Snowflake::from("789");

    let state = http.get_current_user_voice_state(guild_id.clone()).await?;
    println!("current voice session: {:?}", state.session_id);

    http.modify_current_user_voice_state_from_request(
        guild_id.clone(),
        &ModifyCurrentUserVoiceState {
            channel_id: Some(channel_id.clone()),
            suppress: Some(false),
            request_to_speak_timestamp: Some(None),
        },
    )
    .await?;

    http.modify_user_voice_state_from_request(
        guild_id,
        user_id,
        &ModifyUserVoiceState {
            channel_id: Some(channel_id),
            suppress: Some(true),
        },
    )
    .await?;

    Ok(())
}
```

The current runtime covers:

- voice websocket hello and identify
- ready payload handling
- UDP IP discovery
- select protocol
- session description wait
- speaking updates
- server speaking/SSRC-to-user tracking when the voice gateway sends that mapping
- Opus-frame RTP send with sequence/timestamp/nonce management
- simple paced Opus-frame playback through `play_opus_frames(...)`
- 48 kHz stereo 20 ms PCM encode/playback through `PcmFrame`, `AudioSource`, `AudioMixer`, and `VoiceOpusEncoder` when `voice-encode` is enabled
- raw UDP packet receive
- RTP header parsing with CSRC/extension-aware RTP-size header calculation
- transport decrypt for `aead_aes256_gcm_rtpsize` and `aead_xchacha20_poly1305_rtpsize`
- pure-Rust Opus decode to interleaved `i16` PCM through `VoiceOpusDecoder`
- DAVE opcode state tracking and a `VoiceDaveFrameDecryptor` hook
- experimental `VoiceDaveySession`, `VoiceDaveFrameEncryptor`, `send_opus_frame_with_dave(...)`, and DAVE MLS outbound command helpers when the `dave` feature is enabled
- graceful close

The default `voice` feature can send already-encoded Opus frames, returns transport-decrypted Opus frames, and can decode received frames to PCM.
`recv_voice_packet(...)` still rejects active DAVE sessions unless the caller uses
`recv_voice_packet_with_dave(...)` or `recv_decoded_voice_packet_with_dave(...)` with a DAVE decryptor.
The `dave` feature exposes a `davey`/OpenMLS-backed session wrapper and outbound media insertion point, but live Discord DAVE interoperability still depends on validating the full gateway MLS transition lifecycle for the target voice session.
Use `examples/live_dave_capture_bot.rs` to join a prepared voice channel and print the `DISCORDRS_LIVE_VOICE_*` environment block needed by `tests/live_voice_dave.rs`:

```powershell
$env:DISCORD_TOKEN="bot-token"
$env:DISCORDRS_CAPTURE_GUILD_ID="123456789012345678"
$env:DISCORDRS_CAPTURE_CHANNEL_ID="345678901234567890"
cargo run --all-features --example live_dave_capture_bot
```

## 12. Modal and Components V2 Helpers

V2 modal parsing still preserves Discord-specific component types such as `FileUpload`, `RadioGroup`, `CheckboxGroup`, and `Checkbox`.

```rust
use discordrs::{
    create_container, parse_interaction_context, parse_raw_interaction,
    respond_modal_with_container, DiscordHttpClient, RawInteraction, V2ModalSubmission,
};
use serde_json::Value;

fn summarize(submission: &V2ModalSubmission) -> String {
    let theme = submission.get_radio_value("theme").unwrap_or("Not selected");
    let channels = submission
        .get_select_values("notify_channels")
        .map(|v| v.join(", "))
        .unwrap_or_else(|| "None".to_string());
    let files = submission
        .get_file_values("attachments")
        .map(|v| v.join(", "))
        .unwrap_or_else(|| "No files".to_string());

    format!("Theme: {theme}, Notifications: {channels}, Files: {files}")
}

async fn handle_modal(http: &DiscordHttpClient, payload: &Value) -> Result<(), discordrs::DiscordError> {
    let ctx = parse_interaction_context(payload)?;

    if let RawInteraction::ModalSubmit(submission) = parse_raw_interaction(payload)? {
        let result = summarize(&submission);
        let container = create_container("Modal Processed", &result, vec![], None);
        respond_modal_with_container(http, &ctx.id, &ctx.token, container, true).await?;
    }

    Ok(())
}
```

## 13. Frequently Used APIs

- `Client::builder(token, intents)`
- `Context::new(http, data)`
- `Context::rest()`
- `RestClient::new(token, application_id)`
- `get_poll_answer_voters(...)`
- `end_poll(...)`
- `get_skus(...)`
- `get_sku_subscriptions(...)`
- `get_sku_subscription(...)`
- `get_entitlements(...)`
- `get_entitlement(...)`
- `get_application_activity_instance(...)`
- `get_guild_integrations(...)`
- `get_guild_audit_log_typed(...)`
- `get_guild_with_query(...)`
- `get_guild_bans_with_query(...)`
- `get_guild_members_with_query(...)`
- `search_guild_members_with_query(...)`
- `get_current_user_guilds_typed_with_query(...)`
- `get_current_user_connections(...)`
- `get_current_user_application_role_connection(...)`
- `update_current_user_application_role_connection(...)`
- `create_group_dm_channel_typed(...)`
- `parse_webhook_event_payload(...)`
- `modify_guild_incident_actions(...)`
- `create_lobby(...)`, `get_lobby(...)`, `modify_lobby(...)`, `delete_lobby(...)`
- `add_lobby_member(...)`, `bulk_update_lobby_members(...)`, `remove_lobby_member(...)`, `leave_lobby(...)`
- `link_lobby_channel(...)`, `update_lobby_message_moderation_metadata(...)`
- `list_thread_members(...)`
- `list_public_archived_threads(...)`
- `list_private_archived_threads(...)`
- `list_joined_private_archived_threads(...)`
- `get_active_guild_threads(...)`
- `SlashCommandBuilder`, `UserCommandBuilder`, `MessageCommandBuilder`
- `MessageBuilder`, `InteractionResponseBuilder`
- `send_message(...)`
- `respond_to_interaction(...)`
- `respond_with_message(...)`
- `followup_message(...)`
- `defer_interaction(...)`
- `update_interaction_message(...)`
- `parse_interaction(...)`
- `parse_raw_interaction(...)`
- `try_interactions_endpoint(...)`
- `try_typed_interactions_endpoint(...)`
- `AppFramework`, `AppFrameworkBuilder`, `RouteKey`
- `CacheHandle`, `GuildManager`, `ChannelManager`, `MemberManager`, `MessageManager`, `RoleManager`
- `ShardMessenger`
- `ShardSupervisor`
- `VoiceRuntimeConfig`
- `connect_voice_runtime(...)`
- `VoiceOpusDecoder`
- `VoiceOpusEncoder` with `voice,voice-encode`
- `VoiceDaveFrameDecryptor`
- `VoiceDaveFrameEncryptor` and `VoiceDaveySession` with `voice,dave`

## 14. Notes

- `Client` is the main gateway runtime surface. `BotClient` is kept as an alias for compatibility.
- `EventHandler::handle_event(...)` is the typed gateway entry point. Legacy callbacks such as `ready`, `message_create`, and `interaction_create` are still available for compatibility and now receive typed payloads.
- `RestClient` is the preferred REST-facing name. `DiscordHttpClient` remains available.
- Prefer the typed `RestClient` methods for new code.
- Token-authenticated `/interactions/...` and `/webhooks/...` requests intentionally omit bot `Authorization` headers, and webhook/callback path segments are validated before requests are built.
- `edit_application_command_permissions(...)` intentionally sends `Authorization: Bearer ...` because Discord requires OAuth2 authorization for that write route.
- Current-user connections and application role connection user endpoints also intentionally send `Authorization: Bearer ...` because Discord requires user OAuth2 scopes.
- `Context::new(...)` exists for tests and helper code that need a standalone context outside the live gateway runtime.
- Prefer builder imports from `discordrs::builders::{...}` or the crate root re-exports. Deeper implementation submodules are private.
- Use `ApplicationCommand::id_opt()` until Discord has assigned an ID. Unsaved commands are no longer treated as generic `DiscordModel`s.
- `spawn_shards(...)` is the right choice when you want status inspection, manual shutdown, or supervisor-driven shard control.
- `start_shards(...)` is the right choice when you only want the runtime to own the shard lifecycle and block until it exits.
- `voice` currently provides handshake, state plumbing, raw UDP receive, transport-decrypted Opus frames, Opus send, and PCM decode. `voice-encode` adds PCM-to-Opus playback. DAVE/MLS operation is exposed through the `dave` feature with live Discord MLS transition validation recorded for 2.0.0.
- Webhook Resource routes, Webhook Events, lobby, guild incident actions, guild channel-position/member join/role count/widget routes, Group DM recipients, channel invite/permission routes, voice-channel status updates, guild message search, current channel pins, current-application edits, forwarded message snapshots, shared client themes, Gateway rate-limit, reaction metadata, and presence metadata events, Activity instances, poll vote, subscription, integration, entitlement, soundboard, invite, thread, forum, OAuth2 user connection, application command permission, and channel-info data now have typed models/events or REST wrappers where Discord documents them.
- A 2026-05-02 audit mapped all 223 official Discord REST `<Route>` entries from `discord-api-docs` to route wrappers or tokenized helper paths. This is a REST route-shape claim; object-field drift and live-only behavior still require targeted tests when Discord changes the upstream docs.

## 15. Testing And Coverage

Coverage-specific workflow guidance lives in:

- [`discordrsdocs/docs/guide/testing-and-coverage.md`](discordrsdocs/docs/guide/testing-and-coverage.md)

Use that guide when you need repeatable local HTTP harnesses, websocket harnesses, or a fast order
for attacking low-coverage modules.
