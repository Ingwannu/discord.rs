//! discord.rs - Standalone Discord bot framework with Components V2, Gateway, and HTTP client
//!
//! # Features
//! - `gateway` - Gateway WebSocket client, BotClient, EventHandler
//! - `interactions` - HTTP Interactions Endpoint with Ed25519 verification

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
#[cfg(feature = "interactions")]
/// High-level typed interaction application framework.
pub mod framework;
/// Convenience helpers for interaction responses and webhook followups.
pub mod helpers;
/// Typed Discord REST client, request bodies, route helpers, and rate-limit handling.
pub mod http;
/// Shared manager traits for cached Discord model access.
pub mod manager;
/// Typed Discord API models and request payload shapes.
pub mod model;
/// OAuth2 authorization URL and token exchange helpers.
pub mod oauth2;
/// Parsers for Discord interaction and modal payloads.
pub mod parsers;
/// Commonly used discord.rs types re-exported for application code.
pub mod prelude;
/// Interaction response and message response builders.
pub mod response;
#[cfg(feature = "sharding")]
/// Gateway sharding configuration, runtime coordination, and IPC helpers.
pub mod sharding;
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
    command_type, option_type, CommandDefinition, CommandOptionBuilder, MessageCommandBuilder,
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
#[cfg(feature = "interactions")]
pub use framework::{
    AppContext, AppFramework, AppFrameworkBuilder, AppGuard, AppRouteHandler, FrameworkFuture,
    RouteKey,
};
pub use manager::CachedManager;
pub use model::{
    Activity, ActivityAssets, ActivityButton, ActivityInstance, ActivityLocation, ActivityParty,
    ActivitySecrets, ActivityTimestamps, ActivityType, AddGroupDmRecipient, AddGuildMember,
    AddLobbyMember, AllowedMentions, Application, ApplicationCommand,
    ApplicationCommandHandlerType, ApplicationCommandOption, ApplicationCommandOptionChoice,
    ApplicationCommandPermission, ApplicationInstallParams, ApplicationIntegrationType,
    ApplicationIntegrationTypeConfig, ApplicationRoleConnectionMetadata, ArchivedThreadsQuery,
    Attachment, AuditLog, AuditLogEntry, AuditLogQuery, AuthorizationInformation,
    AutoModerationAction, AutoModerationActionMetadata, AutoModerationRule,
    AutoModerationTriggerMetadata, AutocompleteInteraction, Ban, BeginGuildPruneRequest,
    BulkGuildBanRequest, BulkGuildBanResponse, Channel, ChannelMention, ChannelPins,
    ChannelPinsQuery, ChatInputCommandInteraction, ClientStatus, CommandInteractionData,
    CommandInteractionOption, ComponentInteraction, CreateChannelInvite, CreateDmChannel,
    CreateGroupDmChannel, CreateGuildBan, CreateGuildChannel, CreateGuildRole, CreateGuildSticker,
    CreateLobby, CreateMessage, CreatePoll, CreateStageInstance, CreateTestEntitlement,
    CreateWebhook, CurrentUserGuild, CurrentUserGuildsQuery, DefaultReaction, DiscordModel,
    EditApplicationCommandPermissions, EditChannelPermission, Embed, Entitlement, EntitlementQuery,
    FollowedChannel, ForumTag, Gateway, GatewayBot, GetGuildQuery, Guild,
    GuildApplicationCommandPermissions, GuildBansQuery, GuildIncidentsData, GuildMembersQuery,
    GuildOnboarding, GuildPreview, GuildPruneCount, GuildPruneResult, GuildScheduledEvent,
    GuildScheduledEventEntityMetadata, GuildScheduledEventRecurrenceRule,
    GuildScheduledEventRecurrenceRuleNWeekday, GuildScheduledEventUser, GuildTemplate, GuildWidget,
    GuildWidgetChannel, GuildWidgetImageStyle, GuildWidgetMember, GuildWidgetSettings, Integration,
    IntegrationAccount, IntegrationApplication, Interaction, InteractionCallbackResponse,
    InteractionContextData, InteractionContextType, Invite, InviteTargetUsersJobStatus,
    JoinedArchivedThreadsQuery, LinkLobbyChannel, Lobby, LobbyMember, LobbyMemberUpdate, Member,
    Message, MessageActivity, MessageCall, MessageContextMenuInteraction, MessagePin,
    MessageReference, MessageSnapshot, MessageSnapshotMessage, ModalSubmitInteraction,
    ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUser, ModifyCurrentUserVoiceState,
    ModifyGuild, ModifyGuildChannelPosition, ModifyGuildIncidentActions, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance,
    ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken, PermissionOverwrite,
    PermissionsBitField, Poll, PollAnswer, PollAnswerCount, PollAnswerVoters, PollMedia,
    PollResults, ReactionCountDetails, RequestChannelInfo, RequestGuildMembers, Role, RoleColors,
    RoleSubscriptionData, RoleTags, SearchGuildMembersQuery, SearchGuildMessagesQuery,
    SearchGuildMessagesResponse, SetVoiceChannelStatus, SharedClientTheme, Sku, Snowflake,
    SoundboardSound, SoundboardSoundList, StageInstance, Sticker, StickerItem, StickerPack,
    StickerPackList, Subscription, SubscriptionQuery, ThreadListResponse, ThreadMember,
    ThreadMemberQuery, UpdatePresence, UpdateUserApplicationRoleConnection, User,
    UserApplicationRoleConnection, UserCollectibles, UserConnection, UserContextMenuInteraction,
    UserNameplate, UserPrimaryGuild, VanityUrl, VoiceRegion, VoiceServerUpdate, VoiceState,
    Webhook, WebhookExecuteQuery, WebhookMessageQuery, WelcomeScreen, WelcomeScreenChannel,
};
pub use oauth2::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken, OAuth2Scope,
    OAuth2TokenResponse,
};
pub use response::{InteractionResponseBuilder, MessageBuilder};
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
    BitField, BitFieldFlags, IntentFlags, Intents, MessageFlagBits, MessageFlags, PermissionFlags,
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
pub use ws::{GatewayCompression, GatewayConnectionConfig, GatewayEncoding};

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

pub use constants::{
    button_style, component_type, gateway_intents, separator_spacing, text_input_style,
    MESSAGE_FLAG_IS_COMPONENTS_V2,
};

pub use http::{DiscordHttpClient, FileAttachment, FileUpload, RestClient};

pub use helpers::{
    defer_and_followup_container, defer_interaction, defer_update_interaction,
    delete_followup_response, delete_original_response, edit_followup_response,
    edit_message_with_container, edit_original_response, followup_message, followup_with_container,
    get_original_response, launch_activity, respond_component_with_components_v2,
    respond_component_with_container, respond_modal_with_container, respond_to_interaction,
    respond_with_autocomplete_choices, respond_with_components_v2, respond_with_container,
    respond_with_message, respond_with_modal, respond_with_modal_typed, send_components_v2,
    send_container_message, send_message, send_to_channel, update_component_with_container,
    update_interaction_message,
};

#[cfg(all(feature = "gateway", feature = "sharding"))]
pub use gateway::ShardSupervisor;
#[cfg(feature = "gateway")]
pub use gateway::{
    BotClient, BotClientBuilder, Client, ClientBuilder, Context, EventHandler, ShardMessenger,
    TypeMap,
};

#[cfg(feature = "interactions")]
pub use interactions::{
    interactions_endpoint, try_interactions_endpoint, try_typed_interactions_endpoint,
    typed_interactions_endpoint, verify_discord_signature, InteractionHandler, InteractionResponse,
    TypedInteractionHandler,
};

#[cfg(feature = "collectors")]
pub use collector::{
    CollectorHub, ComponentCollector, InteractionCollector, MessageCollector, ModalCollector,
};
