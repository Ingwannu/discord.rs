# Models and Constants

## Typed Models

The typed surface is centered on:

- the typed IDs (`GuildId`, `ChannelId`, `UserId`, ...) and `GenericId` for IDs of unknown kind
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
- `clean_content(text, &resolver)` with a `MentionResolver` (trait with typed `user(UserId)`/`role(RoleId)`/`channel(ChannelId)`, or a closure over `MentionTarget`), matching discord.js's command-name character class exactly (Unicode 17.0 table); `Message::clean_content(...)` and `Member::mention()` delegate to it.

## Typed IDs, Discriminants, and Patch Fields (3.0, unreleased)

Every ID in models, events, REST and cache methods, collectors, permissions, formatters, and helpers has its per-resource type (`GuildId`, `ChannelId`, `UserId`, ...); the 2.x `String`-backed `Snowflake` is gone. `GenericId` remains only where Discord leaves the kind open: `AuditLogEntry::target_id` (depends on the action), `GuildScheduledEvent::entity_id` (undocumented kind), `StickerPack::banner_asset_id` (a CDN asset), and the shared `CommandInteractionData::target_id`, which `UserContextMenuInteraction::target_id()` and `MessageContextMenuInteraction::target_id()` return as a `UserId` / `MessageId`. Raw `u8` codes and `Option<Option<T>>` remain until series 3 (discriminants) and phase 4 (patch fields). The design is in `docs/design/3.0-types-events-requests.md` in the repository.

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

Endpoints take the concrete type by value (`rest.get_guild(guild_id)`, `rest.get_channel_message(channel_id, message_id)`), and caches are keyed by typed tuples such as `(GuildId, UserId)` for members and presences and `(ChannelId, MessageId)` for messages. `GuildId::everyone_role()` gives the `@everyone` role's `RoleId` (it shares the guild's ID) without a hand-written `cast`.

### Polymorphic IDs

Some wire fields hold an ID whose kind depends on a sibling field. Each has an enum that reads and writes exactly the 2.x JSON:

| Type | Wire | Variants |
|---|---|---|
| `OverwriteTarget` (`PermissionOverwrite::target`) | `{"id", "type": 0 \| 1}` | `Role(RoleId)`, `Member(UserId)`, `Unknown { kind: OverwriteType, id }` |
| `CommandPermissionTarget` (`ApplicationCommandPermission::target`) | `{"id", "type": 1 \| 2 \| 3}` | `Role`, `User`, `Channel`, `Everyone(GuildId)` (role `id` = guild ID), `AllChannels(GuildId)` (channel `id` = guild ID − 1), `Unknown` |
| `CommandPermissionsId` (`GuildApplicationCommandPermissions::id`) | `"id"` next to `"application_id"` | `Command(CommandId)`, `Application(ApplicationId)` (application-wide: `id` = application ID) |
| `SelectDefaultValue` | `{"id", "type": "user" \| "role" \| "channel"}` | `User`, `Role`, `Channel`, `Unknown { kind: String, id }` |
| `EntitlementOwner` (`CreateTestEntitlement::owner`) | `{"owner_id", "owner_type": 1 \| 2}` | `Guild(GuildId)`, `User(UserId)` |
| `MentionableId` | a bare snowflake | `User`, `Role` (from `ResolvedData::mentionable(id)`) |

- `Everyone` and `AllChannels` cannot be recognised from a lone permission, so a standalone `ApplicationCommandPermission` decodes them as `Role` / `Channel`. `GuildApplicationCommandPermissions` and the `APPLICATION_COMMAND_PERMISSIONS_UPDATE` event resolve them against their `guild_id` (`CommandPermissionTarget::in_guild`), and both spellings serialize to the same JSON. `AllChannels` of guild `1` would be ID `0`, so it is a serialization error rather than a bad request.
- Overwrite endpoints take a target: `edit_channel_permissions_typed(channel_id, OverwriteTarget::Role(role_id), &body)`; build the body with `EditChannelPermission::for_target(target)`, because a body whose `type` contradicts the target is rejected before sending.
- `SelectDefaultValue` is shared by received components (`MessageComponent::default_values`) and `SelectMenuBuilder` (`default_user(UserId)`, `default_role(RoleId)`, `default_channel(ChannelId)`, `default_mentionable(MentionableId)`). Component `id(...)` setters take a `ComponentId`.
- `MentionResolver` methods take `UserId` / `RoleId` / `ChannelId`; a closure resolver takes a `MentionTarget` (`User`, `Role`, `Channel`).

```rust
use discordrs::model::{OverwriteTarget, PermissionOverwrite};

let overwrite: PermissionOverwrite =
    serde_json::from_str(r#"{"id": "80351110224678912", "type": 1, "deny": "2048"}"#)?;
match overwrite.target {
    OverwriteTarget::Role(role_id) => println!("role {role_id}"),
    OverwriteTarget::Member(user_id) => println!("member {user_id}"),
    OverwriteTarget::Unknown { kind, id } => println!("type {kind:?} for {id}"),
}
// Writes back {"id":"80351110224678912","type":1,"deny":"2048"}.
```

### Generic code over models

`DiscordModel` has an associated `Marker`, so `User::id()` is a `UserId`, `Guild::id()` a `GuildId`, and so on. `CachedManager<T>` takes `Id<T::Marker>` (`CachedManager<Guild>::get(GuildId)`) and is the same async trait with and without the `gateway` feature.

### Values that are not snowflakes

These look like IDs on the wire but are not nonzero snowflakes, so they have their own types instead of `Id<M>`:

- `WidgetUserId(String)`: the anonymized widget member `id` (Discord's examples show `"0"`). It is opaque and kept exactly as received.
- `GuildInstallOwner`: the `GUILD_INSTALL` value of `authorizing_integration_owners`. It is `BotDm` for `"0"` (the interaction came from a DM with the bot) or `Guild(GuildId)`.
- `UploadId(u32)`: the `files[n]` index of a new upload in the same request. `0` is allowed, and it is separate from a stored `AttachmentId`. (No typed request field carries one yet; it arrives with the typed attachment metadata of phase 5.)
- `ComponentId(u32)`: the numeric component `id` (`MessageComponent::id`). Discord treats `0` as "not set" and assigns one.
- `PollAnswerId(u32)`: a poll `answer_id` (`PollAnswer::answer_id`, `PollAnswerCount::id`, `PollVoteEvent::answer_id`, and the `answer_id` of `get_poll_answer_voters`).
- `AuthorizingIntegrationOwners`: an interaction's (and `MessageInteractionMetadata`'s) `authorizing_integration_owners`, with `guild_install: Option<GuildInstallOwner>` (key `"0"`), `user_install: Option<UserId>` (key `"1"`), and `other` for keys added later, written back unchanged.
- `CreateGuildPlaceholderId(u64)`: the request-local role and channel numbers of the deprecated create-guild body (`CreateGuild` with `CreateGuildPlaceholderRole` / `CreateGuildPlaceholderChannel` / `CreateGuildPlaceholderOverwrite`). `0` is allowed.
- Plain strings, not IDs: voice region IDs, integration account IDs, connection account IDs, invite codes, Activity instance and location IDs, `custom_id`, and webhook/follow-up message IDs in `*_webhook_message*` / `*_followup_message*` calls (Discord also accepts `@original` there).

### Migrating from 2.x `Snowflake`

- **Type.** `Snowflake` (a `String` plus a parsed `u64`) is replaced by typed IDs: 8 bytes, `Copy`, never zero. `discordrs::Snowflake` survives only as a deprecated alias of `GenericId`, so old imports still compile with a deprecation warning that names the replacement; write the typed alias (`GuildId`, `UserId`, ...) instead.
- **No implicit conversions.** `From<u64>`, `From<&str>`, `From<String>`, `Snowflake::new`, and the infallible `FromStr` are gone, so `rest.get_channel("123")` and `get_channel(123u64)` no longer compile, and neither does passing a `UserId` where a `RoleId` is expected. Endpoints and helpers take the concrete type by value. Validate untrusted text or config once, at the boundary:

```rust
use discordrs::{ChannelId, DiscordError, GuildId};

fn ids(raw: &str) -> Result<(GuildId, ChannelId), DiscordError> {
    let guild_id: GuildId = raw.parse()?; // `?` maps InvalidId into DiscordError
    let channel_id = ChannelId::try_from(81384788765712384_u64)?;
    Ok((guild_id, channel_id))
}
```

- **Copy, by value.** Drop `.clone()` on IDs, and pass them by value where 2.x took `&Snowflake` (cache getters such as `cache.member(guild_id, user_id)`, `CacheBackend` methods, `MentionResolver`, `MemberPermissionInput::new`). `DiscordModel::id()` returns the model's typed ID. Cache keys are typed numeric tuples, so lookups no longer hash strings.
- **Accessors.** `as_str()` is gone (an ID stores no text): use `Display`/`to_string()` for text and `get()` for the number. `as_u64()`/`to_u64()` become `get()`, `timestamp()` returns `u64` (no `Option`), and `is_valid()` has nothing left to check.
- **Stricter decoding.** Zero, empty, signed, or non-digit IDs are decode errors instead of being kept as strings. The documented exceptions above have their own types.
- **`Default` removed from persisted models with a required ID** (`Message`, `User`, `Guild`, `Channel`, `Role`, `Attachment`, `Sticker`, `Entitlement`, the interaction variants, and about sixty types in all): a default would need a sentinel ID. `Member` keeps `Default` because its `user` is optional. Request bodies whose ID is required gained a `new(...)` constructor instead: `CreateDmChannel::new`, `ModifyGuildChannelPosition::new`, `ModifyGuildRolePosition::new`, `CreateTestEntitlement::new(sku_id, EntitlementOwner::Guild(guild_id))`, `CreateStageInstance::new`, `LobbyMember::new`, `LobbyMemberUpdate::new`. Queries, patches, and bodies whose IDs are optional keep `Default`. In tests, build models from JSON (`serde_json::from_value(json!({"id": "1", ...}))`).
- **Polymorphic fields changed shape** (see the table above): `PermissionOverwrite { id, kind }` → `target: OverwriteTarget`; `ApplicationCommandPermission { id, kind }` → `target: CommandPermissionTarget` (the `ROLE`/`USER`/`CHANNEL` `u8` constants are gone; use `CommandPermissionType`), plus `everyone(guild_id, ..)` / `all_channels(guild_id, ..)` constructors; `GuildApplicationCommandPermissions::id` and the permissions update event's `id` → `CommandPermissionsId`; `CreateTestEntitlement { owner_id, owner_type }` → `owner: EntitlementOwner`; `MessageSelectDefaultValue` is a deprecated alias of the `SelectDefaultValue` enum, whose builders take typed IDs instead of strings. `edit_channel_permissions(_typed)` and `delete_channel_permission` take an `OverwriteTarget`.
- **Application ID.** `RestClient::new(token, application_id)`, `RestClient::builder(...)`, and `Client::rest(...)` take `Option<ApplicationId>` (2.x: `u64` with `0` for unset); `application_id()` returns `Option<ApplicationId>`, `set_application_id` and `ClientBuilder::application_id` take an `ApplicationId`.
- **Other signatures.** `MentionResolver` methods take typed IDs and closure resolvers take a `MentionTarget` (2.x: `Fn(MentionKind, Snowflake)`); `CachedManager<T>` requires `T: DiscordModel` and takes `Id<T::Marker>`, with no separate synchronous form in non-gateway builds; builder `id(...)` setters take a `ComponentId`; `EmojiKey::custom` takes an `EmojiId`.
- **Raw interaction context.** `parsers::InteractionContext` IDs are typed (`id: InteractionId`, `application_id: ApplicationId`, `guild_id: Option<GuildId>`, `channel_id: Option<ChannelId>`, `user_id: Option<UserId>`), `RawInteraction::{Command, Autocomplete}::id` is a `CommandId`, and the stateless helpers take `InteractionId` / `ChannelId` / `MessageId`.
- **Prelude.** `discordrs::prelude` exports `GuildId`, `ChannelId`, `UserId`, `RoleId`, `MessageId`, `ApplicationId`, and `InvalidId`; import the other aliases (and `GenericId`) from the crate root.

### Open discriminants

Integer enumerations are transparent newtypes with named constants: `MessageType::REPLY`, `ComponentType::CONTAINER`, `ButtonStyle::PREMIUM`, and so on. Each type provides `new(raw)`, `get()`, and `name()`. `Debug` prints the constant name, or `Unknown(n)` for values this version does not know. Serialization uses the bare integer (`#[serde(transparent)]`). There is no `Default`. Unknown values round-trip unchanged, and a value too wide for the storage type fails to decode instead of being truncated. All types use `u8` storage except `AuditLogActionType`, which uses `u16`.

Available from the crate root: `OverwriteType`, `VideoQualityMode`, `ForumSortOrder`, `ForumLayout`, `ComponentType`, `ButtonStyle`, `TextInputStyle`, `SeparatorSpacing`, `MessageType`, `MessageActivityType`, `ReactionType`, `PollLayoutType`, `InteractionType`, `InteractionResponseType`, `CommandType`, `CommandOptionType`, `CommandPermissionType`, `RoleConnectionMetadataType`, `VerificationLevel`, `DefaultMessageNotificationLevel`, `ExplicitContentFilterLevel`, `MfaLevel`, `PremiumTier`, `GuildNsfwLevel`, `ScheduledEventStatus`, `ScheduledEventEntityType`, `ScheduledEventPrivacyLevel`, `StageInstancePrivacyLevel`, `AutoModerationEventType`, `AutoModerationTriggerType`, `AutoModerationActionType`, `KeywordPresetType`, `AuditLogActionType`, `InviteType`, `InviteTargetType`, `InviteTargetUsersJobStatusType`, `WebhookType`, `StickerType`, `StickerFormatType`, `SkuType`, `SkuAccessType` (no documented values, so no names), `EntitlementType`, `EntitlementOwnerType`, `SubscriptionStatus`, `UserPremiumType`, and `ConnectionVisibility`.

Seven names are still taken by their 2.x versions. Until series 3 swaps them in, the new versions are reachable only as `discordrs::model::discriminant::{ChannelType, MessageReferenceType, ApplicationIntegrationType, InteractionContextType, ApplicationCommandHandlerType, ActivityType, WebhookPayloadType}`. The new `ChannelType` includes the thread types and provides `is_thread()`, `is_voice_based()`, and `is_text_based()` (the discord.js `isTextBased()` set). These helpers classify the channel type only: they do not mean you can send messages there, because permissions, archived or locked threads, and obfuscated channels can still forbid it.

### `Patch<T>`, `Nullable<T>`, `NullablePatch<T>`

- `Patch<T>` is `Missing` (field omitted) or `Value(T)`. `Nullable<T>` is `Null` or `Value(T)`. `NullablePatch<T>` = `Patch<Nullable<T>>` covers fields that can be omitted, cleared with `null`, or set.
- Constructors: `Patch::missing()`, `Patch::value(v)`, and, for nullable patches only, `Patch::null()`. Writing `Patch::<bool>::null()` is a compile error. `Default` is `Missing` and does not require `T: Default`.
- Declare patch fields as `#[serde(default, skip_serializing_if = "Patch::is_missing")]`. Forgetting either part fails loudly instead of changing data:
  - Without `default`, an absent key is a `missing field` error. It never becomes `null`, so a `NullablePatch` cannot silently turn "omitted" into "clear it".
  - Without `skip_serializing_if`, serializing a `Missing` is an error, never a silent `null`.
- A required field that may be `null` is a plain `Nullable<T>` with no attribute. `null` reads as `Null`, and an absent key is a `missing field` error (unlike `Option<T>`, which reads absence as `None`).
- Values are written untagged. A present key is read through `T`, so `null` on a `Patch<bool>` field fails to decode, while `false`, `0`, `""`, and `[]` are ordinary values.
- Option conversions: `Patch::from(Option<T>)` maps `None` to `Missing`. `Nullable::from(Option<T>)` maps `None` to `Null`, and `Option::from(Nullable<T>)` goes back.

## New in 2.2.0

- `MessageComponent` — `Message.components` (and related read-side payloads, including modal and snapshot components) moved from raw `serde_json::Value` to this tolerant typed structure. It is deliberately one *wide* struct covering the whole component tree: `kind` carries the raw component `type`, every known field is optional, and decoding never fails when Discord ships new component kinds. Associated constants (`MessageComponent::BUTTON`, `STRING_SELECT`, `SECTION`, `CONTAINER`, `LABEL`, ...) name the type codes, and `iter()` walks the component and every nested child (`components`, label `component`, section `accessory`) depth-first.
- `MessageSelectOption`, `MessageSelectDefaultValue` (in 3.0 the `SelectDefaultValue` enum), `UnfurledMediaItem`, and `MessageMediaGalleryItem` — typed pieces of received components.
- `ResolvedData` — interaction `resolved` data as six lookup maps keyed by ID (typed since 3.0: `UserId`, `RoleId`, `ChannelId`, `MessageId`, `AttachmentId`; `users`, `members`, `roles`, `channels`, `messages`, `attachments`), with by-ID accessors (`user(id)`, `member(id)`, `role(id)`, `channel(id)`, `message(id)`, `attachment(id)`) and `is_empty()`.
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
