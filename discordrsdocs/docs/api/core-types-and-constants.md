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

## Typed IDs, Discriminants, and Patch Fields (3.0, unreleased)

These primitives are available now, but existing models, REST methods, events, and builders still use `Snowflake`, raw `u8` codes, and `Option<Option<T>>`. Consumers migrate in phase 3 series 2–3 (IDs and discriminants) and phase 4 (patch fields). The design is in `docs/design/3.0-types-events-requests.md` in the repository.

### `Id<M>` typed IDs

- `Id<M>` is a nonzero `u64` snowflake tagged with a marker type, so a `ChannelId` cannot be passed where a `GuildId` is required. Use the aliases: `GuildId`, `ChannelId` (threads too), `UserId`, `RoleId`, `MessageId`, `ApplicationId`, `CommandId`, `CommandVersionId`, `InteractionId`, `WebhookId`, `EmojiId`, `AttachmentId`, `StickerId`, `StickerPackId`, `SoundboardSoundId`, `ScheduledEventId`, `StageInstanceId`, `IntegrationId`, `AuditLogEntryId`, `AutoModerationRuleId`, `ForumTagId`, `SkuId`, `EntitlementId`, `SubscriptionId`, `PromotionId`, `RoleSubscriptionListingId`, `LobbyId`, `ActivityLaunchId`, `TeamId`, `OnboardingPromptId`, `OnboardingOptionId`, and `GenericId` (kind not statically known). The markers live in `discordrs::model::marker`, and `IdMarker` is sealed. There is no `MemberId`, `PresenceId`, or `ThreadId`: members and presences are keyed by `(GuildId, UserId)`, and threads are channels.
- An ID is `Copy` and 8 bytes, and `Option<GuildId>` is also 8 bytes.
- Construction is explicit and checked: `GuildId::try_new(u64)`, `"123".parse::<GuildId>()`, `TryFrom<u64>`, and `TryFrom<&str>` return `InvalidId` (with `kind()`: `Zero`, `Empty`, `InvalidDigit`, `Overflow`); only `From<NonZeroU64>` is infallible. There is no `From<u64>`, no `Default`, and no conversion between markers except `erase()` (to `GenericId`) and `cast::<N>()`. `cast` only asserts that the same snowflake names another kind of resource (for example a guild's `@everyone` role); it checks nothing.
- `get()`, `into_nonzero()`, `timestamp()` (Unix milliseconds), `Display` (canonical decimal), and `Debug` (`GuildId(123)`). Equality, ordering, and hashing use only the number, so ordering is numeric (`9 < 10`).
- Wire format: serialized as a decimal string, which also works as a JSON object key (`HashMap<GuildId, _>`). Deserialized from a decimal string or an unsigned integer. Zero, negatives, floats, values above `u64::MAX`, whitespace, signs, and other non-digit characters are rejected. Leading zeros are accepted and dropped (`"007"` reads as 7 and writes back as `"7"`).

```rust
use discordrs::{GuildId, InvalidId};

// Validate untrusted input (a slash command string option, a config file)
// once at the boundary; rejects "0", " 1", "+1", "1.0", and so on.
fn parse_guild(raw: &str) -> Result<GuildId, InvalidId> {
    raw.parse()
}

fn takes_guild(guild_id: GuildId) {
    // format!("{guild_id:?}") == "GuildId(81384788765712384)"
    // Passing a ChannelId to this function is a compile error.
}
```

### Values that are not snowflakes

These look like IDs on the wire but are not nonzero snowflakes, so they have their own types instead of `Id<M>`:

- `WidgetUserId(String)`: the anonymized widget member `id` (Discord's examples show `"0"`). It is opaque and kept exactly as received.
- `GuildInstallOwner`: the `GUILD_INSTALL` value of `authorizing_integration_owners`. It is `BotDm` for `"0"` (the interaction came from a DM with the bot) or `Guild(GuildId)`.
- `UploadId(u32)`: the `files[n]` index of a new upload in the same request. `0` is allowed, and it is separate from a stored `AttachmentId`.
- `ComponentId(u32)`: the numeric component `id`. Discord treats `0` as "not set" and assigns one.
- `PollAnswerId(u32)`: a poll `answer_id`.

### Open discriminants

Integer enumerations are transparent newtypes with named constants: `MessageType::REPLY`, `ComponentType::CONTAINER`, `ButtonStyle::PREMIUM`, and so on. Each type provides `new(raw)`, `get()`, and `name()`. `Debug` prints the constant name, or `Unknown(n)` for values this version does not know. Serialization uses the bare integer (`#[serde(transparent)]`). There is no `Default`. Unknown values round-trip unchanged, and a value too wide for the storage type fails to decode instead of being truncated. All types use `u8` storage except `AuditLogActionType`, which uses `u16`.

Available from the crate root: `OverwriteType`, `VideoQualityMode`, `ForumSortOrder`, `ForumLayout`, `ComponentType`, `ButtonStyle`, `TextInputStyle`, `SeparatorSpacing`, `MessageType`, `MessageActivityType`, `ReactionType`, `PollLayoutType`, `InteractionType`, `InteractionResponseType`, `CommandType`, `CommandOptionType`, `CommandPermissionType`, `RoleConnectionMetadataType`, `VerificationLevel`, `DefaultMessageNotificationLevel`, `ExplicitContentFilterLevel`, `MfaLevel`, `PremiumTier`, `GuildNsfwLevel`, `ScheduledEventStatus`, `ScheduledEventEntityType`, `ScheduledEventPrivacyLevel`, `StageInstancePrivacyLevel`, `AutoModerationEventType`, `AutoModerationTriggerType`, `AutoModerationActionType`, `KeywordPresetType`, `AuditLogActionType`, `InviteType`, `InviteTargetType`, `InviteTargetUsersJobStatusType`, `WebhookType`, `StickerType`, `StickerFormatType`, `SkuType`, `SkuAccessType` (no documented values, so no names), `EntitlementType`, `EntitlementOwnerType`, `SubscriptionStatus`, `UserPremiumType`, and `ConnectionVisibility`.

Seven names are still taken by their 2.x versions. Until series 3 swaps them in, the new versions are reachable only as `discordrs::model::discriminant::{ChannelType, MessageReferenceType, ApplicationIntegrationType, InteractionContextType, ApplicationCommandHandlerType, ActivityType, WebhookPayloadType}`. The new `ChannelType` includes the thread types and provides `is_thread()`, `is_voice_based()`, and `is_text_based()` (the discord.js `isTextBased()` set). These helpers classify the channel type only: they do not mean you can send messages there, because permissions, archived or locked threads, and obfuscated channels can still forbid it.

### `Patch<T>`, `Nullable<T>`, `NullablePatch<T>`

- `Patch<T>` is `Missing` (field omitted) or `Value(T)`. `Nullable<T>` is `Null` or `Value(T)`. `NullablePatch<T>` = `Patch<Nullable<T>>` covers fields that can be omitted, cleared with `null`, or set.
- Constructors: `Patch::missing()`, `Patch::value(v)`, and, for nullable patches only, `Patch::null()`. Writing `Patch::<bool>::null()` is a compile error. `Default` is `Missing` and does not require `T: Default`.
- Declare struct fields as `#[serde(default, skip_serializing_if = "Patch::is_missing")]`. Both parts matter. Serializing a `Missing` that reaches the serializer is an error, never a silent `null`. Without `default`, serde reads an absent `NullablePatch` field as `null`, which would clear data.
- Values are written untagged. A present key is read through `T`, so `null` on a `Patch<bool>` field fails to decode, while `false`, `0`, `""`, and `[]` are ordinary values.
- Option conversions: `Patch::from(Option<T>)` maps `None` to `Missing`. `Nullable::from(Option<T>)` maps `None` to `Null`, and `Option::from(Nullable<T>)` goes back.

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
