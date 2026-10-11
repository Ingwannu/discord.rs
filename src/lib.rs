//! discord.rs - Standalone Discord bot framework with Components V2, Gateway, and HTTP client
//!
//! # Features
//! - `gateway` - Gateway WebSocket client, BotClient, EventHandler
//! - `interactions` - HTTP Interactions Endpoint with Ed25519 verification

// The code rules in Cargo.toml's `[lints]` table hold for library code
// without exception; unit tests may unwrap, panic, cast, and discard, where
// a failure fails the test rather than a user's bot.
#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::as_conversions,
        clippy::let_underscore_must_use,
        clippy::clone_on_ref_ptr,
        reason = "unit tests: a failed expectation fails the test"
    )
)]

/// Bit flag helpers for Discord gateway intents, permissions, and message flags.
pub mod bitfield;
pub mod builders;
/// Gateway cache storage, policies, and typed cache manager helpers.
pub mod cache;
/// Collection helpers for Discord model maps and cached entity sets.
pub mod collection;
#[cfg(feature = "collectors")]
/// Event collectors for waiting on messages, components, modals, and interactions.
pub mod collector;
/// Application command builders and command option models.
pub mod command;
/// Discord API constants used by gateway, REST, and interaction helpers.
pub mod constants;
/// Error types returned by Discord REST, Gateway, voice, and parser operations.
pub mod error;
/// Typed Discord Gateway event payloads and dispatch decoding.
pub mod event;
// Module docs live in the file's `//!` header only: with `///` docs here as
// well, rustdoc resolves the header's intra-doc links from this scope.
pub mod formatters;
#[cfg(feature = "interactions")]
/// High-level typed interaction application framework.
pub mod framework;
/// Convenience helpers for interaction responses and webhook followups.
pub mod helpers;
#[cfg(feature = "interactions")]
mod hex;
/// Typed Discord REST client, request bodies, route helpers, and rate-limit handling.
pub mod http;
/// Shared manager traits for cached Discord model access.
pub mod manager;
/// Typed Discord API models and request payload shapes.
pub mod model;
/// discord.js-style convenience methods on model entities
/// (`message.reply(...)`, `member.kick(...)`, `guild.create_channel(...)`).
pub mod model_ext;
/// OAuth2 authorization URL and token exchange helpers.
pub mod oauth2;
/// Parsers for Discord interaction and modal payloads.
pub mod parsers;
// Docs in the file's `//!` header only (see `formatters`).
pub mod permissions;
/// Commonly used discord.rs types re-exported for application code.
pub mod prelude;
/// Interaction response and message response builders.
pub mod response;
#[cfg(feature = "sharding")]
/// Gateway sharding configuration, runtime coordination, and IPC helpers.
pub mod sharding;
#[cfg(any(feature = "voice", feature = "sharding"))]
mod task;
/// Legacy compatibility types and shared builder aliases.
pub mod types;
#[cfg(feature = "voice")]
/// Voice manager state, gateway commands, DAVE frame types, and playback controls.
pub mod voice;
#[cfg(feature = "voice")]
/// Discord voice runtime transport, RTP, Opus, and encryption helpers.
pub mod voice_runtime;
/// Typed parser for Discord application Webhook Events payloads.
pub mod webhook_events;
#[cfg(any(feature = "gateway", feature = "sharding"))]
/// Gateway websocket URL and connection configuration helpers.
pub mod ws;

#[cfg(feature = "gateway")]
/// Gateway bot runtime, client, context, event handler, and shard supervision.
pub mod gateway;

#[cfg(feature = "interactions")]
/// Signed Discord interactions endpoint helpers and typed handlers.
pub mod interactions;

pub use cache::{
    CacheConfig, CacheHandle, ChannelManager, GuildManager, MemberManager, MessageManager,
    RoleManager, UserManager,
};
pub use collection::Collection;
pub use command::{
    CommandDefinition, CommandOptionBuilder, MessageCommandBuilder,
    PrimaryEntryPointCommandBuilder, SlashCommandBuilder, UserCommandBuilder,
};
pub use error::{DiscordError, HttpError};
pub use event::{
    decode_event, AuditLogEntryEvent, AutoModerationEvent, ChannelEvent, ChannelInfoChannel,
    ChannelInfoEvent, EntitlementEvent, Event, GuildDeleteEvent, GuildDeletePayload, GuildEvent,
    GuildMembersChunkEvent, GuildMembersChunkPayload, GuildScheduledEventUserEvent,
    GuildStickersUpdateEvent, InteractionEvent, MemberEvent, MemberRemoveEvent,
    MemberRemovePayload, MessageDeleteEvent, MessageDeletePayload, MessageEvent,
    ReactionRemoveEmojiEvent, ReadyEvent, ReadyPayload, ResumedEvent, RoleDeleteEvent,
    RoleDeletePayload, RoleEvent, ScheduledEvent, SoundboardSoundDeleteEvent, SoundboardSoundEvent,
    SoundboardSoundsEvent, StageInstanceEvent, ThreadEvent, ThreadListSyncEvent,
    ThreadMemberUpdateEvent, ThreadMembersUpdateEvent, UserUpdateEvent, VoiceChannelEffectEvent,
    VoiceChannelStartTimeUpdateEvent, VoiceChannelStatusUpdateEvent, VoiceServerEvent,
    VoiceStateEvent,
};
pub use formatters::{
    EscapeMarkdownOptions, GuildNavigation, HeadingLevel, ListItem, MentionKind, MentionResolver,
    MentionTarget, SlashCommandMention, Timestamp, TimestampStyle,
};
#[cfg(feature = "interactions")]
pub use framework::{
    AppContext, AppFramework, AppFrameworkBuilder, AppGuard, AppRouteHandler, FrameworkFuture,
    RouteKey,
};
pub use manager::CachedManager;
pub use model::{
    Activity, ActivityAssets, ActivityButton, ActivityInstance, ActivityLocation, ActivityParty,
    ActivitySecrets, ActivityTimestamps, AddGroupDmRecipient, AddGuildMember, AddLobbyMember,
    AllowedMentions, Application, ApplicationCommand, ApplicationCommandOption,
    ApplicationCommandOptionChoice, ApplicationCommandPermission, ApplicationIdentities,
    ApplicationIdentity, ApplicationIdentityProfile, ApplicationIdentityProfileData,
    ApplicationIdentityProviderId, ApplicationInstallParams, ApplicationIntegrationTypeConfig,
    ApplicationRoleConnectionMetadata, ArchivedThreadsQuery, Attachment, AuditLog, AuditLogEntry,
    AuditLogQuery, AuthorizationInformation, AuthorizingIntegrationOwners, AutoModerationAction,
    AutoModerationActionMetadata, AutoModerationRule, AutoModerationTriggerMetadata,
    AutocompleteInteraction, Ban, BeginGuildPruneRequest, BulkGuildBanRequest,
    BulkGuildBanResponse, Channel, ChannelMention, ChannelPins, ChannelPinsQuery,
    ChatInputCommandInteraction, ClientStatus, CommandInteractionData, CommandInteractionOption,
    CommandPermissionTarget, CommandPermissionsId, ComponentInteraction, CreateChannelInvite,
    CreateDmChannel, CreateGroupDmChannel, CreateGuildBan, CreateGuildChannel, CreateGuildRole,
    CreateGuildSticker, CreateLobby, CreateLobbyMessage, CreateMessage, CreatePoll,
    CreateStageInstance, CreateTestEntitlement, CreateWebhook, CurrentUserGuild,
    CurrentUserGuildsQuery, DefaultReaction, DiscordModel, DynamicProfileField,
    EditApplicationCommandPermissions, EditChannelPermission, Embed, Entitlement, EntitlementOwner,
    EntitlementQuery, FollowedChannel, ForumTag, Gateway, GatewayBot, GetGuildQuery,
    GetOrCreateLobby, Guild, GuildApplicationCommandPermissions, GuildBansQuery,
    GuildIncidentsData, GuildMembersQuery, GuildOnboarding, GuildPreview, GuildPruneCount,
    GuildPruneResult, GuildScheduledEvent, GuildScheduledEventEntityMetadata,
    GuildScheduledEventRecurrenceRule, GuildScheduledEventRecurrenceRuleNWeekday,
    GuildScheduledEventUser, GuildTemplate, GuildWidget, GuildWidgetChannel, GuildWidgetImageStyle,
    GuildWidgetMember, GuildWidgetSettings, Integration, IntegrationAccount,
    IntegrationApplication, Interaction, InteractionCallbackResponse, InteractionContextData,
    Invite, InviteTargetUserIds, InviteTargetUsersJobStatus, JoinedArchivedThreadsQuery,
    LinkLobbyChannel, Lobby, LobbyInvite, LobbyMember, LobbyMemberUpdate, LobbyMessage,
    LobbyMessageMember, Member, MentionableId, Message, MessageActivity, MessageCall,
    MessageComponent, MessageComponentIter, MessageContextMenuInteraction, MessageInteraction,
    MessageInteractionMetadata, MessageMediaGalleryItem, MessagePin, MessageReference,
    MessageSelectOption, MessageSnapshot, MessageSnapshotMessage, ModalSubmitInteraction,
    ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUser, ModifyCurrentUserVoiceState,
    ModifyGuild, ModifyGuildChannelPosition, ModifyGuildIncidentActions, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance,
    ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken, OverwriteTarget,
    PartialApplication, PermissionOverwrite, PermissionsBitField, Poll, PollAnswer,
    PollAnswerCount, PollAnswerVoters, PollMedia, PollResults, PresenceStatus, PrimaryProfileData,
    ProfileMedia, ReactionCountDetails, RequestChannelInfo, RequestGuildMembers, ResolvedData,
    Role, RoleColors, RoleSubscriptionData, RoleTags, SearchGuildMembersQuery,
    SearchGuildMessagesQuery, SearchGuildMessagesResponse, SetVoiceChannelStatus,
    SharedClientTheme, Sku, SoundboardSound, SoundboardSoundList, StageInstance, Sticker,
    StickerItem, StickerPack, StickerPackList, Subscription, SubscriptionQuery, ThreadListResponse,
    ThreadMember, ThreadMemberQuery, UnfurledMediaItem, UpdateApplicationIdentityProfile,
    UpdatePresence, UpdateUserApplicationRoleConnection, User, UserApplicationRoleConnection,
    UserCollectibles, UserConnection, UserContextMenuInteraction, UserNameplate, UserPrimaryGuild,
    VanityUrl, VoiceRegion, VoiceServerUpdate, VoiceState, Webhook, WebhookExecuteQuery,
    WebhookMessageQuery, WelcomeScreen, WelcomeScreenChannel,
};
// Phase 3 primitives (typed IDs, open discriminants, patch fields). Since
// series 2 step B every ID has its per-resource alias and since series 3
// every discriminant field uses its newtype (canonical module:
// `model::discriminant`); series 4 migrates the patch fields.
// `WebhookPayloadType` comes through `webhook_events` below.
pub use model::{
    ActivityLaunchId, ActivityType, ApplicationCommandHandlerType, ApplicationId,
    ApplicationIntegrationType, AttachmentId, AuditLogActionType, AuditLogEntryId,
    AutoModerationActionType, AutoModerationEventType, AutoModerationRuleId,
    AutoModerationTriggerType, ButtonStyle, ChannelId, ChannelType, CommandId, CommandOptionType,
    CommandPermissionType, CommandType, CommandVersionId, ComponentId, ComponentType,
    ConnectionVisibility, DefaultMessageNotificationLevel, EmojiId, EntitlementId,
    EntitlementOwnerType, EntitlementType, ExplicitContentFilterLevel, ForumLayout, ForumSortOrder,
    ForumTagId, GenericId, GuildId, GuildInstallOwner, GuildNsfwLevel, Id, IdMarker, IntegrationId,
    InteractionContextType, InteractionId, InteractionResponseType, InteractionType, InvalidId,
    InvalidIdKind, InviteTargetType, InviteTargetUsersJobStatusType, InviteType, KeywordPresetType,
    LobbyId, MessageActivityType, MessageId, MessageReferenceType, MessageType, MfaLevel, Nullable,
    NullablePatch, OnboardingOptionId, OnboardingPromptId, OverwriteType, Patch, PollAnswerId,
    PollLayoutType, PremiumTier, PromotionId, ReactionType, RoleConnectionMetadataType, RoleId,
    RoleSubscriptionListingId, ScheduledEventEntityType, ScheduledEventId,
    ScheduledEventPrivacyLevel, ScheduledEventStatus, SeparatorSpacing, SkuAccessType, SkuId,
    SkuType, SoundboardSoundId, StageInstanceId, StageInstancePrivacyLevel, StickerFormatType,
    StickerId, StickerPackId, StickerType, SubscriptionId, SubscriptionStatus, TeamId,
    TextInputStyle, UploadId, UserId, UserPremiumType, VerificationLevel, VideoQualityMode,
    WebhookId, WebhookType, WidgetUserId,
};
// The deprecated 2.x names, kept so `discordrs::Snowflake` and
// `discordrs::MessageSelectDefaultValue` users get a deprecation note
// pointing at the new types instead of "not found".
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use model::{MessageSelectDefaultValue, Snowflake};
pub use oauth2::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken, OAuth2Scope,
    OAuth2TokenResponse,
};
pub use permissions::{
    ChannelClass, ChannelPermissionInput, ChannelPermissions, ComputedPermissions,
    MemberPermissionInput, PermissionDataError, PermissionSource,
};
pub use response::{
    AutocompleteChoice, InteractionReplyData, InteractionResponder, InteractionResponseBuilder,
    InteractionResponsePhase, InteractionResponseState, MessageBuilder,
};
#[cfg(feature = "sharding")]
pub use sharding::{
    ShardConfig, ShardInfo, ShardIpcMessage, ShardRuntimeState, ShardRuntimeStatus,
    ShardSupervisorEvent, ShardingManager,
};
pub use types::{ButtonConfig, Emoji, MediaGalleryItem, MediaInfo, SelectOption};
pub use webhook_events::{
    parse_webhook_event_payload, ApplicationAuthorizedWebhookEvent,
    ApplicationDeauthorizedWebhookEvent, WebhookDeletedMessage, WebhookEvent, WebhookEventBody,
    WebhookEventPayload, WebhookPayloadType, WebhookSocialMessage,
};
/// Backward-compatible alias. Prefer `DiscordError`.
#[deprecated(since = "0.4.0", note = "Use DiscordError instead")]
pub type Error = DiscordError;
pub use bitfield::{
    BitField, BitFieldFlags, ChannelFlagBits, ChannelFlags, GatewayCapabilities,
    GatewayCapabilityBits, IntentFlags, Intents, MessageFlagBits, MessageFlags, PermissionFlags,
    Permissions,
};
#[cfg(feature = "voice")]
pub use voice::{
    AudioPlayer, AudioTrack, VoiceConnectionConfig, VoiceConnectionState, VoiceConnectionStatus,
    VoiceEncryptionMode, VoiceEvent, VoiceGatewayCommand, VoiceGatewayHello, VoiceGatewayOpcode,
    VoiceGatewayReady, VoiceManager, VoiceSelectProtocolCommand, VoiceSpeakingCommand,
    VoiceSpeakingFlags, VoiceSpeakingState, VoiceTransportProtocol, VoiceTransportState,
    VoiceUdpDiscoveryPacket,
};
#[cfg(feature = "voice")]
pub use voice_runtime::{
    connect as connect_voice_runtime, VoiceDaveFrame, VoiceDaveFrameDecryptor, VoiceDaveState,
    VoiceDaveUnencryptedRange, VoiceDecodedPacket, VoiceOpusDecoder, VoiceOpusFrame,
    VoiceOutboundPacket, VoiceOutboundRtpState, VoiceRawUdpPacket, VoiceReceivedPacket,
    VoiceRtpHeader, VoiceRuntimeConfig, VoiceRuntimeHandle, VoiceRuntimeState,
    VoiceSessionDescription, VoiceSpeakingUpdate,
};
#[cfg(all(feature = "voice", feature = "voice-encode"))]
pub use voice_runtime::{AudioMixer, AudioSource, PcmFrame, VoiceOpusEncoder};
#[cfg(all(feature = "voice", feature = "dave"))]
pub use voice_runtime::{VoiceDaveFrameEncryptor, VoiceDaveyDecryptor, VoiceDaveySession};
#[cfg(any(feature = "gateway", feature = "sharding"))]
pub use ws::{
    GatewayCompression, GatewayConnectionConfig, GatewayEncoding, IdentifyGate, IdentifyOutcome,
    IdentifyPermit, IdentifyPermitFuture,
};

pub use builders::{
    create_container, create_default_buttons, ActionRowBuilder, ButtonBuilder, CheckboxBuilder,
    CheckboxGroupBuilder, ComponentsV2Message, ContainerBuilder, EmbedBuilder, FileBuilder,
    FileUploadBuilder, LabelBuilder, MediaGalleryBuilder, ModalBuilder, RadioGroupBuilder,
    SectionBuilder, SelectDefaultValue, SelectMenuBuilder, SeparatorBuilder, TextDisplayBuilder,
    TextInputBuilder, ThumbnailBuilder,
};

pub use parsers::{
    parse_interaction, parse_interaction_context, parse_modal_submission, parse_raw_interaction,
    InteractionContext, RawInteraction, V2ModalComponent, V2ModalSubmission,
};

pub use constants::{gateway_intents, MESSAGE_FLAG_IS_COMPONENTS_V2};

pub use http::{DiscordHttpClient, FileAttachment, FileUpload, RestClient};

pub use helpers::{
    defer_and_followup_container, edit_message_with_container, followup_with_container,
    launch_activity, respond_component_with_components_v2, respond_component_with_container,
    respond_modal_with_container, respond_to_interaction, respond_with_components_v2,
    respond_with_container, respond_with_message, respond_with_modal, send_components_v2,
    send_container_message, send_message, send_to_channel, update_component_with_container,
};
// Deprecated in 3.0 in favour of `InteractionResponder`; re-exported so
// existing imports keep compiling (with a deprecation warning at the call site).
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use helpers::{
    defer_interaction, defer_update_interaction, delete_followup_response,
    delete_original_response, edit_followup_response, edit_original_response, followup_message,
    get_original_response, respond_with_autocomplete_choices, respond_with_modal_typed,
    update_interaction_message,
};

#[cfg(all(feature = "gateway", feature = "cache"))]
pub use gateway::CacheBackendHealth;
#[cfg(all(feature = "gateway", feature = "sharding"))]
pub use gateway::ShardSupervisor;
#[cfg(feature = "gateway")]
pub use gateway::{
    BotClient, BotClientBuilder, Client, ClientBuilder, Context, EventDispatchMode, EventHandler,
    InteractionOverloadReason, OverloadedInteraction, ShardMessenger, TypeMap,
};

#[cfg(feature = "interactions")]
pub use interactions::{
    try_interactions_endpoint, try_typed_interactions_endpoint, verify_discord_signature,
    InteractionHandler, InteractionResponse, TypedInteractionHandler,
};
// Deprecated in 3.0: they panic on an invalid public key.
#[cfg(feature = "interactions")]
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use interactions::{interactions_endpoint, typed_interactions_endpoint};

#[cfg(feature = "collectors")]
pub use collector::{
    CollectorHub, ComponentCollector, EmojiKey, EventCursor, EventSubscription,
    InteractionCollector, MessageCollector, ModalCollector, ReactionCollector,
};
