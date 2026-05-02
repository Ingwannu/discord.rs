# HTTP and Helpers API

## `RestClient`

`RestClient` is the primary Discord REST v10 surface. It keeps shared route/global rate-limit state and also keeps `DiscordHttpClient` as a compatibility alias.

Common operations include:

- typed message create/update/get
- typed guild/channel/member/role lookups
- typed application command overwrite
- typed Auto Moderation rule reads and writes through `get_auto_moderation_rules_typed`, `create_auto_moderation_rule_typed`, and `modify_auto_moderation_rule_typed`
- typed guild administration helpers for bulk bans, guild count fetches, guild ban pagination, member pagination, single-role lookup, audit logs, guild preview, prune count/result, vanity URL, incident actions, and voice regions
- typed guild channel-position updates, OAuth2 guild-member joins, role member-count reads, and public widget image downloads
- typed Stage Instance create/modify request bodies
- typed emoji helpers for guild and application emoji reads/writes
- typed scheduled-event recurrence/entity metadata, plus typed create/modify request helpers
- typed current-application reads/edits and application role-connection metadata helpers
- typed application Activity Instance lookup helper
- typed Gateway URL, OAuth2 current bot application, and OAuth2 current authorization metadata helpers
- typed application command permission reads, plus OAuth2 Bearer-token permission edits
- typed current-user guild list pagination/counts, current-member modify, OAuth2 user connection and application role connection helpers, webhook-by-token lookup, invite target-user CSV helpers, guild-specific voice region helpers, and voice-state REST reads/writes
- typed lobby create/read/update/delete, member, channel-linking, and moderation metadata helpers
- typed channel invite, voice-status, and permission overwrite helpers
- typed guild message search and current channel-pin listing helpers
- legacy channel pin/unpin route helpers for Discord's `/channels/{channel.id}/pins/{message.id}` aliases
- typed sticker-pack lookup and guild sticker write helpers
- typed Webhook Resource creation/modification request bodies, query-aware execution/message routes, and Slack/GitHub-compatible execution helpers
- interaction responses and follow-up webhook helpers
- Create Group DM plus Group DM recipient add/remove helpers for OAuth2 `gdm.join` flows

Raw `serde_json::Value` methods remain available for routes where Discord adds fields before discord.rs has a typed model. Prefer the typed methods first, then drop to raw JSON only for newly released or experimental API fields.

`1.2.1` hardens the REST layer:

- invite codes are validated before `get_invite`, `get_invite_with_options`, and `delete_invite` build bot-authorized paths
- generated query strings are percent-encoded
- request body serialization failures return `DiscordError::Json` instead of panicking
- repeated HTTP 429 responses are retried up to a bounded limit before `DiscordError::RateLimit`

## Application Resource Helpers

Current application management is typed through `Application`, `ModifyCurrentApplication`, `ApplicationInstallParams`, and `ApplicationIntegrationTypeConfig`:

```rust
use discordrs::{ApplicationInstallParams, ModifyCurrentApplication, PermissionsBitField};

let application = rest.get_current_application().await?;
println!("editing {}", application.name);

let updated = rest
    .edit_current_application_from_request(&ModifyCurrentApplication {
        description: Some("Support utilities".to_string()),
        install_params: Some(ApplicationInstallParams {
            scopes: vec!["bot".to_string(), "applications.commands".to_string()],
            permissions: PermissionsBitField(2048),
        }),
        tags: Some(vec!["utility".to_string()]),
        ..ModifyCurrentApplication::default()
    })
    .await?;
```

The generic `edit_current_application(...)` helper remains available for fields Discord adds before discord.rs grows a typed model.

## Webhook Resource Helpers

Webhook management has typed request bodies for creation and modification:

```rust
use discordrs::{CreateWebhook, ModifyWebhook, ModifyWebhookWithToken};

let webhook = rest
    .create_webhook_from_request(
        channel_id,
        &CreateWebhook {
            name: "deployments".to_string(),
            avatar: None,
        },
    )
    .await?;
let webhook_id = webhook.id.clone().expect("webhook id");
let webhook_token = webhook.token.as_deref().expect("webhook token");

rest.modify_webhook_from_request(
    webhook_id.clone(),
    &ModifyWebhook {
        name: Some("ops".to_string()),
        avatar: Some(None),
        channel_id: Some(archive_channel_id),
    },
)
.await?;

rest.modify_webhook_with_token_from_request(
    webhook_id,
    webhook_token,
    &ModifyWebhookWithToken {
        name: Some("public".to_string()),
        avatar: None,
    },
)
.await?;
```

Webhook execution supports Discord's `wait`, `thread_id`, and `with_components` query parameters. Slack-compatible and GitHub-compatible execution helpers use Discord's documented compatibility endpoints:

```rust
use discordrs::WebhookExecuteQuery;

let query = WebhookExecuteQuery {
    wait: Some(false),
    thread_id: Some(thread_id),
    with_components: Some(true),
};

rest.execute_webhook_with_query(webhook_id, token, &query, &serde_json::json!({
    "content": "deployment finished"
}))
.await?;

rest.execute_slack_compatible_webhook(
    webhook_id,
    token,
    &WebhookExecuteQuery {
        wait: Some(true),
        thread_id: Some(thread_id),
        with_components: None,
    },
    &serde_json::json!({ "text": "deployment finished" }),
)
.await?;
```

Token-authenticated webhook routes validate token path segments before sending a request.

Webhook message helpers expose Discord's `thread_id` and `with_components` query options when they apply:

```rust
use discordrs::WebhookMessageQuery;

let message_query = WebhookMessageQuery {
    thread_id: Some(thread_id),
    with_components: Some(true),
};

let message = rest
    .get_webhook_message_with_query(webhook_id, token, message_id, &message_query)
    .await?;

rest.edit_webhook_message_with_query(webhook_id, token, message.id.as_str(), &message_query, &body)
    .await?;

rest.delete_webhook_message_with_query(webhook_id, token, message.id.as_str(), &message_query)
    .await?;
```

## Application Command Permissions

Discord exposes application command permission writes as an OAuth2 flow. Reads use the configured bot token:

- `get_guild_application_command_permissions(guild_id)`
- `get_application_command_permissions(guild_id, command_id)`

Writes require an access token with Discord's `applications.commands.permissions.update` scope:

```rust
use discordrs::{ApplicationCommandPermission, EditApplicationCommandPermissions};

let body = EditApplicationCommandPermissions::new([
    ApplicationCommandPermission::role(role_id, true),
]);

let updated = rest
    .edit_application_command_permissions(oauth_access_token, guild_id, command_id, &body)
    .await?;
```

`edit_application_command_permissions(...)` sends `Authorization: Bearer ...` for this route instead of `Authorization: Bot ...`.

## OAuth2 Current User Helpers

Discord's user connection and linked-role endpoints require user OAuth2 scopes. These helpers take an explicit Bearer token so bot-token REST calls and user-token REST calls do not get mixed accidentally:

- `get_current_user_connections(bearer_token)`
- `get_current_user_guilds_typed_with_query(query)`
- `get_current_user_guild_member(bearer_token, guild_id)`
- `get_current_user_application_role_connection(bearer_token)`
- `update_current_user_application_role_connection(bearer_token, body)`

`ModifyCurrentUser` is separate and uses normal bot authorization through `modify_current_user(...)`.

Current-user guild lists are available with Discord's documented pagination and count flags through `CurrentUserGuildsQuery`:

```rust
use discordrs::CurrentUserGuildsQuery;

let guilds = rest
    .get_current_user_guilds_typed_with_query(&CurrentUserGuildsQuery {
        after: Some(last_seen_guild_id),
        limit: Some(50),
        with_counts: Some(true),
        ..Default::default()
    })
    .await?;
```

## Guild Administration Helpers

Guild fetches support Discord's optional approximate member and presence counts through `GetGuildQuery`:

```rust
use discordrs::GetGuildQuery;

let guild = rest
    .get_guild_with_query(
        guild_id,
        &GetGuildQuery {
            with_counts: Some(true),
        },
    )
    .await?;
```

Guild ban lists support Discord's `before` and `after` pagination through `GuildBansQuery`:

```rust
use discordrs::GuildBansQuery;

let bans = rest
    .get_guild_bans_with_query(
        guild_id,
        &GuildBansQuery {
            limit: Some(100),
            after: Some(last_seen_user_id),
            ..Default::default()
        },
    )
    .await?;
```

Channel position updates are typed through `ModifyGuildChannelPosition`:

```rust
use discordrs::ModifyGuildChannelPosition;

rest
    .modify_guild_channel_positions(
        guild_id,
        &[ModifyGuildChannelPosition {
            id: channel_id,
            position: Some(Some(3)),
            lock_permissions: Some(Some(true)),
            parent_id: Some(None),
        }],
    )
    .await?;
```

Use `Some(None)` for nullable Discord fields such as `parent_id`; use `None` to omit the field.

Guild role reordering is typed through `ModifyGuildRolePosition`:

```rust
use discordrs::ModifyGuildRolePosition;

let roles = rest
    .modify_guild_role_positions_typed(
        guild_id,
        &[ModifyGuildRolePosition {
            id: role_id,
            position: Some(Some(1)),
        }],
    )
    .await?;
```

Guild Resource write bodies are typed for common administration routes:

```rust
use discordrs::{
    BeginGuildPruneRequest, CreateGuildBan, CreateGuildChannel, CreateGuildRole,
    CreateStageInstance, ModifyCurrentMember, ModifyGuild, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings,
    ModifyStageInstance, RoleColors, WelcomeScreenChannel,
};

rest.modify_guild(
    guild_id,
    &ModifyGuild {
        name: Some("renamed".to_string()),
        description: Some(None),
        ..Default::default()
    },
)
.await?;

rest.create_guild_channel_from_request(
    guild_id,
    &CreateGuildChannel {
        name: "rules".to_string(),
        kind: Some(0),
        parent_id: Some(None),
        ..Default::default()
    },
)
.await?;

rest.create_guild_ban_typed(
    guild_id,
    user_id,
    &CreateGuildBan {
        delete_message_seconds: Some(60),
        ..Default::default()
    },
)
.await?;

let role = rest
    .create_guild_role(
        guild_id,
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

rest.modify_guild_role(
    guild_id,
    role.id,
    &ModifyGuildRole {
        mentionable: Some(Some(true)),
        unicode_emoji: Some(None),
        ..Default::default()
    },
)
.await?;

rest.modify_guild_member_typed(
    guild_id,
    user_id,
    &ModifyGuildMember {
        nick: Some(None),
        channel_id: Some(None),
        ..Default::default()
    },
)
.await?;

let current_member = rest
    .modify_current_member(
        guild_id,
        &ModifyCurrentMember {
            nick: Some(Some("bot".to_string())),
            avatar: Some(None),
            ..Default::default()
        },
    )
    .await?;

rest.modify_guild_widget(
    guild_id,
    &ModifyGuildWidgetSettings {
        enabled: Some(true),
        channel_id: Some(None),
    },
)
.await?;

rest.modify_guild_welcome_screen_config(
    guild_id,
    &ModifyGuildWelcomeScreen {
        enabled: Some(Some(true)),
        welcome_channels: Some(Some(vec![WelcomeScreenChannel {
            channel_id,
            description: "Start here".to_string(),
            emoji_id: None,
            emoji_name: Some("wave".to_string()),
        }])),
        description: Some(None),
    },
)
.await?;

rest.modify_guild_onboarding_config(
    guild_id,
    &ModifyGuildOnboarding {
        default_channel_ids: Some(vec![channel_id]),
        enabled: Some(true),
        mode: Some(0),
        ..Default::default()
    },
)
.await?;

let prune = rest
    .begin_guild_prune_with_request(
        guild_id,
        &BeginGuildPruneRequest {
            days: Some(7),
            compute_prune_count: Some(false),
            include_roles: Some(vec![role.id]),
            ..Default::default()
        },
    )
    .await?;

let stage = rest
    .create_stage_instance_from_request(&CreateStageInstance {
        channel_id,
        topic: "Town hall".to_string(),
        privacy_level: Some(2),
        send_start_notification: Some(true),
        guild_scheduled_event_id: None,
    })
    .await?;

rest.modify_stage_instance_from_request(
    stage.channel_id,
    &ModifyStageInstance {
        privacy_level: Some(2),
    },
)
.await?;
```

Adding an OAuth2-authorized user to a guild is typed through `AddGuildMember`:

```rust
use discordrs::AddGuildMember;

let joined = rest
    .add_guild_member(
        guild_id,
        user_id,
        &AddGuildMember {
            access_token: oauth_access_token.to_string(),
            ..Default::default()
        },
    )
    .await?;

if let Some(member) = joined {
    println!("created member: {:?}", member.user);
}
```

`add_guild_member(...)` returns `None` when Discord responds with `204 No Content` because the user is already in the guild.

Guild member payloads preserve current profile fields such as guild `banner`, `avatar_decoration_data`, and `collectibles`. Guild member listing supports Discord's `after` cursor through `GuildMembersQuery`, and guild member search supports shared percent-encoded query building through `SearchGuildMembersQuery`:

```rust
use discordrs::{GuildMembersQuery, SearchGuildMembersQuery};

let members = rest
    .get_guild_members_with_query(
        guild_id,
        &GuildMembersQuery {
            limit: Some(100),
            after: Some(last_seen_user_id),
        },
    )
    .await?;

let matches = rest
    .search_guild_members_with_query(
        guild_id,
        &SearchGuildMembersQuery {
            query: "alice & bob".to_string(),
            limit: Some(5),
        },
    )
    .await?;
```

Role member counts are exposed as `get_guild_role_member_counts(guild_id)`, returning a map of role IDs to member counts. Discord excludes the `@everyone` role from this response.

Guild widgets are available as typed JSON through `get_guild_widget_typed(guild_id)`. The public PNG widget image is available through `get_guild_widget_image(guild_id, style)`, where `style` is a `GuildWidgetImageStyle` value. This image route is documented by Discord as unauthenticated, so discord.rs intentionally omits the bot authorization header for it.

Audit logs are available as typed `AuditLog` payloads through `get_guild_audit_log_typed(guild_id, query)`. Use `AuditLogQuery` for `user_id`, `action_type`, `before`, `after`, and `limit` filters:

```rust
use discordrs::AuditLogQuery;

let audit_log = rest
    .get_guild_audit_log_typed(
        guild_id,
        &AuditLogQuery {
            after: Some(last_entry_id),
            limit: Some(50),
            ..Default::default()
        },
    )
    .await?;
```

## Channel Administration Helpers

Channel invite listing and creation are typed through `get_channel_invites(...)` and `create_channel_invite_typed(...)`:

```rust
use discordrs::{CreateChannelInvite, Snowflake};

let invites = rest.get_channel_invites(channel_id).await?;
let invite = rest
    .create_channel_invite_typed(
        channel_id,
        &CreateChannelInvite {
            max_age: Some(600),
            unique: Some(true),
            role_ids: Some(vec![Snowflake::from("701")]),
            ..Default::default()
        },
    )
    .await?;
```

Target-user invite allowlists use Discord's CSV upload flow:

- `create_channel_invite_with_target_users_file(channel_id, body, file)`
- `get_invite_target_users_csv(code)`
- `update_invite_target_users(code, file)`
- `get_invite_target_users_job_status(code)`

Use `FileAttachment::new("target_users.csv", b"user_id\n...")` for the `target_users_file` part. `InviteTargetUsersJobStatus` exposes Discord's async processing status counters and error text.

Voice-channel status and permission overwrite updates are typed through `SetVoiceChannelStatus` and `EditChannelPermission`:

```rust
use discordrs::{EditChannelPermission, PermissionsBitField, SetVoiceChannelStatus};

rest
    .set_voice_channel_status(
        channel_id,
        &SetVoiceChannelStatus {
            status: Some("Office hours".to_string()),
        },
    )
    .await?;

rest
    .edit_channel_permissions_typed(
        channel_id,
        role_id,
        &EditChannelPermission {
            kind: 0,
            allow: Some(PermissionsBitField(1024)),
            deny: None,
        },
    )
    .await?;
```

## Group DM Helpers

Group DM creation is typed through `CreateGroupDmChannel`, and recipient management is typed through `AddGroupDmRecipient`:

```rust
use std::collections::HashMap;

use discordrs::{AddGroupDmRecipient, CreateGroupDmChannel, Snowflake};

let group_dm = rest
    .create_group_dm_channel_typed(&CreateGroupDmChannel {
        access_tokens: vec![gdm_join_access_token.to_string()],
        nicks: HashMap::from([(Snowflake::from("42"), "friend".to_string())]),
    })
    .await?;

rest
    .add_group_dm_recipient(
        channel_id,
        user_id,
        &AddGroupDmRecipient {
            access_token: gdm_join_access_token.to_string(),
            nick: "friend".to_string(),
        },
    )
    .await?;

rest.remove_group_dm_recipient(channel_id, user_id).await?;
```

## Sticker Resource Helpers

Sticker packs and guild sticker writes are typed through `StickerPack`, `CreateGuildSticker`, and `ModifyGuildSticker`:

```rust
use discordrs::{CreateGuildSticker, FileAttachment, ModifyGuildSticker};

let pack = rest.get_sticker_pack(sticker_pack_id).await?;
println!("pack: {}", pack.name);

let sticker = rest
    .create_guild_sticker_from_request(
        guild_id,
        &CreateGuildSticker {
            name: "wave".to_string(),
            description: "Waves hello".to_string(),
            tags: "wave,hello".to_string(),
        },
        FileAttachment::new("wave.png", Vec::<u8>::new()),
    )
    .await?;

rest.modify_guild_sticker_from_request(
    guild_id,
    sticker.id,
    &ModifyGuildSticker {
        name: Some("wave2".to_string()),
        description: Some(None),
        tags: Some("wave".to_string()),
    },
)
.await?;
```

## Message Resource Helpers

Guild message search is typed through `SearchGuildMessagesQuery` and `search_guild_messages(...)`:

```rust
use discordrs::SearchGuildMessagesQuery;

let results = rest
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

for group in results.messages {
    for message in group {
        println!("{}: {}", message.id, message.content);
    }
}
```

Current channel pins are typed through `ChannelPinsQuery` and `get_channel_pins(...)`:

```rust
use discordrs::ChannelPinsQuery;

let pins = rest
    .get_channel_pins(
        channel_id,
        &ChannelPinsQuery {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await?;

println!("fetched {} pinned messages", pins.items.len());

rest.pin_message(channel_id, message_id).await?;
rest.unpin_message(channel_id, message_id).await?;
```

The `Message` model also preserves newer Message Resource payloads such as `message_snapshots`, `referenced_message`, `call`, `shared_client_theme`, `mention_roles`, and role subscription data. Use `MessageSnapshot`, `MessageCall`, and `SharedClientTheme` when you need typed access to those fields.

## Monetization Helpers

Monetization routes use typed `Sku`, `Subscription`, and `Entitlement` models:

- `get_skus(...)`
- `get_sku_subscriptions(...)`
- `get_sku_subscription(...)`
- `get_entitlements(...)`
- `get_entitlement(...)`
- `consume_entitlement(...)`
- `create_test_entitlement(...)`
- `delete_test_entitlement(...)`

`Sku` preserves current response metadata such as `dependent_sku_id`, `manifest_labels`, `access_type`, `features`, `release_date`, `premium`, and `show_age_gate`. `Entitlement` preserves Discord's promotion, gift-code, consumed, validity-window, guild, and subscription metadata.

## Lobby Helpers

The Lobby resource is typed through `Lobby`, `LobbyMember`, `CreateLobby`, `ModifyLobby`, `AddLobbyMember`, `LobbyMemberUpdate`, and `LinkLobbyChannel`.

Bot-authorized helpers:

- `create_lobby(...)`
- `get_lobby(...)`
- `modify_lobby(...)`
- `delete_lobby(...)`
- `add_lobby_member(...)`
- `bulk_update_lobby_members(...)`
- `remove_lobby_member(...)`
- `update_lobby_message_moderation_metadata(...)`

User Bearer-token helpers:

- `leave_lobby(bearer_token, lobby_id)`
- `link_lobby_channel(bearer_token, lobby_id, body)`

## Guild Incident Actions

Discord's guild safety incident-action route is typed through `GuildIncidentsData` and `ModifyGuildIncidentActions`.

```rust
use discordrs::ModifyGuildIncidentActions;

let data = rest
    .modify_guild_incident_actions(
        guild_id,
        &ModifyGuildIncidentActions {
            invites_disabled_until: Some("2026-05-01T12:00:00.000000+00:00".to_string()),
            dms_disabled_until: None,
        },
    )
    .await?;
```

Supplying `None` serializes `null`, which disables the matching incident action.

## Helper Functions

For Components V2 and interaction response workflows, use:

- `send_container_message(...)`
- `respond_with_container(...)`
- `respond_component_with_container(...)`
- `respond_modal_with_container(...)`
- `respond_with_modal(...)`

## Example

```rust
let c = create_container("Notice", "Done", vec![], None);
respond_with_container(http, &ctx.id, &ctx.token, c, true).await?;
```

## OAuth2 Backend Helpers

`OAuth2Client` is the application-backend OAuth2 surface. It builds authorization URLs and exchanges authorization codes or refresh tokens with form-encoded OAuth2 requests. It is intentionally separate from bot-token `RestClient` calls.

```rust
use discordrs::{OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2Scope};

let oauth = OAuth2Client::new("client_id", "client_secret");
let url = oauth.authorization_url(
    OAuth2AuthorizationRequest::code(
        "https://app.example/callback",
        [OAuth2Scope::identify(), OAuth2Scope::guilds()],
    )
    .state("csrf-token"),
)?;

let token = oauth
    .exchange_code(OAuth2CodeExchange::new("code", "https://app.example/callback"))
    .await?;
```

## Recommended Pattern

- Use `RestClient` or context managers for typed fetch/send flows.
- Use `OAuth2Client` for OAuth2 application backend flows.
- Use helper functions for common interaction acknowledgment paths.
- Keep Components V2 payload generation inside the builders.
- Fall back to low-level raw request helpers only when the typed surface does not yet cover the route.
