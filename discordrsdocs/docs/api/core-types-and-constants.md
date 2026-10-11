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

## New in 3.0 (unreleased)

- `ChannelFlags` / `channel_flags::{PINNED, REQUIRE_TAG, HIDE_MEDIA_DOWNLOAD_OPTIONS, CHANNEL_OBFUSCATED, IS_SPOILER_CHANNEL}` and `Channel::channel_flags()` / `Channel::is_obfuscated()`. From 2026-11-16 Discord sends channels a bot cannot view with placeholder metadata; only `id`, `type`, `position`, and `parent_id` are reliable on them, and the flag (not the name) is the supported way to detect them.
- `GatewayCapabilities` / `gateway_capabilities::CHANNEL_OBFUSCATION` for the IDENTIFY `capabilities` field (Discord's temporary opt-in to test obfuscation early), set through `GatewayConnectionConfig::capabilities(...)` or `ClientBuilder::gateway_capabilities(...)`.
- `Embed::components`, `Embed::flags`, `Embed::KIND_COMPONENTS`, and `Embed::is_component_embed()` for link previews rendered from a website's Component Embed.
- Application Identity Profile models: `ApplicationIdentity`, `ApplicationIdentities`, `ApplicationIdentityProfile`, `ApplicationIdentityProfileData`, `PrimaryProfileData`, `DynamicProfileField` (unknown field types are kept verbatim), `ProfileMedia`, `UpdateApplicationIdentityProfile` (with `validate()` for the documented limits), and `ApplicationIdentityProviderId`.
- `CurrentUserGuildsQuery::shard`, `CreateChannelInvite::target_user_ids`, and `InviteTargetUserIds`.

### Permission computation (`discordrs::permissions`)

Effective permissions following Discord's documented algorithm (owner, `ADMINISTRATOR`, overwrite order, timeouts, implicit `VIEW_CHANNEL` / `SEND_MESSAGES` / `CONNECT` rules, threads using the parent's overwrites). discord.js `channel.permissionsFor(member)` stops at the overwrite calculation; that value is kept in `ComputedPermissions::raw`. Details: [Permissions and Formatters](#/docs/api/permissions-and-formatters).

- `permissions_for(guild, member, channel, parent, now)` and `guild_permissions(guild, member, now)` over models; `Channel::permissions_for(...)` and `Member::guild_permissions(...)` use the current time.
- `cached_permissions_for(...)` / `cached_guild_permissions(...)` read a `CacheHandle` and report exactly what is missing.
- `MemberPermissionInput` + `ChannelPermissionInput` (with `ChannelClass`): the pure core over explicit data.
- Results: `ChannelPermissions::{Known, Obfuscated}` and `ComputedPermissions` (`permissions`, `raw`, `source: PermissionSource`, `timeout_applied`, `has(...)`, `missing(...)`). Obfuscated channels are never guessed; owners and administrators are answered without data that cannot change their answer.
- `PermissionDataError` names the missing or contradictory object (`MissingRole`, `MissingParentChannel`, `MissingPermissionOverwrites`, `GuildMismatch`, `InvalidTimestamp`, ...).
- Owners and administrators receive `Permissions::all()`; permission bit names come from `bitfield::permissions`.

### Formatters (`discordrs::formatters`)

`@discordjs/formatters` parity. Formatters take and return `impl Display`; text transforms return `Cow<str>`.

- Mentions and links: `user_mention`, `channel_mention`, `role_mention`, `linked_role_mention`, `game_profile_mention`, `slash_command_mention(...)` (`SlashCommandMention`), `custom_emoji`, `animated_emoji`, `GuildNavigation`, `hyperlink`, `hyperlink_with_title`, `hide_link_embed`, `channel_link`, `message_link` (and `dm_` variants).
- Timestamps: `timestamp(secs).style(TimestampStyle::RelativeTime)`, `Timestamp::from(SystemTime)`; `TimestampStyle` has all nine documented styles (`t T d D f F s S R`).
- Markdown: `bold`, `italic`, `underline`, `strikethrough`, `spoiler`, `inline_code`, `code_block`, `code_block_with_language`, `quote`, `block_quote`, `heading` (`HeadingLevel`), `subtext`, `unordered_list`, `ordered_list`, and nested variants with `ListItem`.
- `escape_markdown(text, EscapeMarkdownOptions)` plus the individual escapers, matching discord.js 0.6.2 on a committed golden corpus (four documented discord.js bugs fixed); `clean_code_block_content`, which breaks every backtick run of three or more.
- `clean_content(text, &resolver)` with a `MentionResolver` (trait or closure; `MentionKind`), matching discord.js's command-name character class exactly (Unicode 17.0 table); `Message::clean_content(...)` and `Member::mention()` delegate to it.

## New in 2.2.0

- `MessageComponent` — `Message.components` (and related read-side payloads, including modal and snapshot components) moved from raw `serde_json::Value` to this tolerant typed structure. It is deliberately one *wide* struct covering the whole component tree: `kind` carries the raw component `type`, every known field is optional, and decoding never fails when Discord ships new component kinds. Associated constants (`MessageComponent::BUTTON`, `STRING_SELECT`, `SECTION`, `CONTAINER`, `LABEL`, ...) name the type codes, and `iter()` walks the component and every nested child (`components`, label `component`, section `accessory`) depth-first.
- `MessageSelectOption`, `MessageSelectDefaultValue`, `UnfurledMediaItem`, and `MessageMediaGalleryItem` — typed pieces of received components.
- `ResolvedData` — interaction `resolved` data as six lookup maps keyed by `Snowflake` (`users`, `members`, `roles`, `channels`, `messages`, `attachments`), with by-ID accessors (`user(id)`, `member(id)`, `role(id)`, `channel(id)`, `message(id)`, `attachment(id)`) and `is_empty()`.
- `MessageInteractionMetadata` — typed `Message::interaction_metadata` (triggering interaction id/type, user, authorizing integration owners, original response message id, target user/message, interacted message id, and nested `triggering_interaction_metadata` for modals). Prefer it over the deprecated `MessageInteraction`.
- `InteractionReplyData` and `AutocompleteChoice` in `discordrs::response` — the payload types behind the `2.2.0` `InteractionResponder` methods; `InteractionReplyData` converts from `&str`, `String`, `MessageBuilder`, and `CreateMessage`.
- `CollectorEndReason` and `CollectorStopHandle` in `discordrs::collector`; `RateLimitInfo` and `RestClientBuilder` in the HTTP layer; `AudioPlayerState`, `AudioPlayerEvent`, `TrackEndReason`, and `NoSubscriberBehavior` in `discordrs::voice::player`.

## New in 2.1

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
- gateway intents, including the `2.1` poll intents `gateway_intents::GUILD_MESSAGE_POLLS` (`1 << 24`) and `gateway_intents::DIRECT_MESSAGE_POLLS` (`1 << 25`) — both part of `NON_PRIVILEGED` — plus the `GUILD_EXPRESSIONS` alias for bit 3

## Advice

- Reference constants instead of hardcoded numeric magic values.
- Keep custom IDs + style values centralized in your app code.
- Prefer typed models for IDs and permission bitfields before dropping to raw JSON.
