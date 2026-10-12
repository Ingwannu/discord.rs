// Thin facade: domain types live in `model/<domain>.rs` and are re-exported
// here so every `crate::model::X` / `discordrs::X` path stays stable while the
// files stay small enough to review and to own per domain (3.0 series 0, see
// docs/design/3.0-types-events-requests.md section 5). New model types go in
// the matching domain file plus one re-export line below; the submodules stay
// private so the flat `model::X` path remains the only public spelling. The
// one public exception is `discriminant`, the canonical module of the open
// discriminants (series 3 swapped its types in at these paths).

mod application;
mod application_identity;
mod audit_log;
mod channel;
mod command;
mod component;
pub mod discriminant;
#[cfg(test)]
mod doc_examples;
mod embed;
mod entity;
#[cfg(test)]
mod fixtures;
mod gateway;
mod gateway_payload;
mod guild;
mod guild_aux;
mod id;
mod interaction;
mod invite;
mod lobby;
mod media;
mod member;
mod message;
mod moderation;
mod monetization;
mod patch;
mod permissions;
mod poll;
mod presence;
mod role;
mod scheduled_event;
mod thread;
mod user;
mod voice;
mod webhook;

pub use application::{
    ActivityInstance, ActivityLocation, Application, ApplicationInstallParams,
    ApplicationIntegrationTypeConfig, ApplicationRoleConnectionMetadata, AuthorizationInformation,
    ModifyCurrentApplication, PartialApplication, Team, TeamMember,
};
pub use application_identity::{
    ApplicationIdentities, ApplicationIdentity, ApplicationIdentityProfile,
    ApplicationIdentityProfileData, ApplicationIdentityProviderId, DynamicProfileField,
    PrimaryProfileData, ProfileMedia, UpdateApplicationIdentityProfile, MAX_PROFILE_DATA_BYTES,
    MAX_PROFILE_DYNAMIC_FIELDS, MAX_PROFILE_STRING_CHARS, MAX_PROFILE_USERNAME_CHARS,
};
pub use audit_log::{
    AuditLog, AuditLogChange, AuditLogEntry, AuditLogEntryOptions, AuditLogPartialRole,
    AuditLogQuery,
};
pub use channel::{
    AddGroupDmRecipient, Attachment, Channel, CreateDmChannel, CreateGroupDmChannel,
    CreateGuildChannel, DefaultReaction, EditChannelPermission, FollowedChannel, ForumTag,
    ModifyGuildChannelPosition, OverwriteTarget, PermissionOverwrite,
};
pub use command::{
    ApplicationCommand, ApplicationCommandOption, ApplicationCommandOptionChoice,
    ApplicationCommandPermission, CommandOptionValue, CommandPermissionTarget,
    CommandPermissionsId, EditApplicationCommandPermissions, GuildApplicationCommandPermissions,
};
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use component::MessageSelectDefaultValue;
pub use component::{
    MentionableId, MessageComponent, MessageComponentIter, MessageComponentValue,
    MessageMediaGalleryItem, MessageSelectOption, SelectDefaultValue, UnfurledMediaItem,
};
pub use embed::{Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedMedia, EmbedProvider};
pub use entity::DiscordModel;
pub use gateway::{
    Gateway, GatewayBot, RequestChannelInfo, RequestGuildMembers, SessionStartLimit,
};
#[cfg(all(feature = "gateway", feature = "cache"))]
pub(crate) use gateway_payload::normalize_presence;
pub(crate) use gateway_payload::BorrowedKey;
pub use gateway_payload::{
    GuildCreate, GuildCreatePayload, MessageUpdate, PartialUser, UnavailableGuild,
};
pub use guild::{
    CreateGuild, CreateGuildFromTemplate, CreateGuildPlaceholderChannel, CreateGuildPlaceholderId,
    CreateGuildPlaceholderOverwrite, CreateGuildPlaceholderRole, CurrentUserGuild,
    CurrentUserGuildsQuery, GetGuildQuery, Guild, GuildIncidentsData, GuildMfaLevel, GuildPreview,
    ModifyGuild, ModifyGuildIncidentActions,
};
pub use guild_aux::{
    BeginGuildPruneRequest, GuildOnboarding, GuildPruneCount, GuildPruneResult, GuildTemplate,
    GuildWidget, GuildWidgetChannel, GuildWidgetImageStyle, GuildWidgetMember, GuildWidgetSettings,
    Integration, IntegrationAccount, IntegrationApplication, ModifyGuildOnboarding,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, OnboardingPrompt, OnboardingPromptOption,
    PartialIntegration, VanityUrl, WelcomeScreen, WelcomeScreenChannel,
};
pub use interaction::{
    AuthorizingIntegrationOwners, AutocompleteInteraction, ChatInputCommandInteraction,
    CommandInteractionData, CommandInteractionOption, ComponentInteraction,
    ComponentInteractionData, Interaction, InteractionCallbackActivityInstance,
    InteractionCallbackInteraction, InteractionCallbackResource, InteractionCallbackResponse,
    InteractionCallbackResult, InteractionContextData, InteractionGuild,
    MessageContextMenuInteraction, ModalSubmitInteraction, PingInteraction,
    PrimaryEntryPointInteraction, ResolvedData, UserContextMenuInteraction,
};
pub use invite::{CreateChannelInvite, Invite, InviteTargetUserIds, InviteTargetUsersJobStatus};
pub use lobby::{
    AddLobbyMember, CreateLobby, CreateLobbyMessage, GetOrCreateLobby, LinkLobbyChannel, Lobby,
    LobbyInvite, LobbyMember, LobbyMemberUpdate, LobbyMessage, LobbyMessageMember, ModifyLobby,
};
pub use media::{
    CreateGuildSticker, ModifyGuildSticker, SoundboardSound, SoundboardSoundList, Sticker,
    StickerItem, StickerPack, StickerPackList,
};
pub use member::{
    AddGuildMember, GuildMembersQuery, Member, ModifyCurrentMember, ModifyGuildMember,
    SearchGuildMembersQuery,
};
pub use message::{
    AllowedMentions, ChannelMention, ChannelPins, ChannelPinsQuery, CreateMessage,
    GetReactionsQuery, Message, MessageActivity, MessageCall, MessageInteraction,
    MessageInteractionMetadata, MessagePin, MessageReference, MessageSnapshot,
    MessageSnapshotMessage, Reaction, ReactionCountDetails, RoleSubscriptionData,
    SearchGuildMessagesQuery, SearchGuildMessagesResponse, SharedClientTheme,
};
pub use moderation::{
    AutoModerationAction, AutoModerationActionMetadata, AutoModerationRule,
    AutoModerationTriggerMetadata, Ban, BulkGuildBanRequest, BulkGuildBanResponse, CreateGuildBan,
    GuildBansQuery,
};
pub use monetization::{
    CreateTestEntitlement, Entitlement, EntitlementOwner, EntitlementQuery, Sku, Subscription,
    SubscriptionQuery,
};
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use permissions::PermissionsBitField;
// The permission set type of every model field. Not re-exported here: its
// public paths are `discordrs::Permissions` and `bitfield::Permissions`.
use crate::bitfield::Permissions;
pub use poll::{
    CreatePoll, Poll, PollAnswer, PollAnswerCount, PollAnswerVoters, PollMedia, PollResults,
};
pub use presence::{
    Activity, ActivityAssets, ActivityButton, ActivityParty, ActivitySecrets, ActivityTimestamps,
    ClientStatus, Presence, PresenceStatus, PresenceUser, UpdatePresence,
};
pub use role::{
    CreateGuildRole, ModifyGuildRole, ModifyGuildRolePosition, Role, RoleColors, RoleTags,
};
pub use scheduled_event::{
    GuildScheduledEvent, GuildScheduledEventEntityMetadata, GuildScheduledEventRecurrenceRule,
    GuildScheduledEventRecurrenceRuleNWeekday, GuildScheduledEventUser,
    GuildScheduledEventUsersQuery, GuildScheduledEventsQuery,
};
pub use thread::{
    ArchivedThreadsQuery, JoinedArchivedThreadsQuery, ThreadListResponse, ThreadMember,
    ThreadMemberQuery, ThreadMetadata,
};
pub use user::{
    AvatarDecorationData, DisplayNameStyles, ModifyCurrentUser,
    UpdateUserApplicationRoleConnection, User, UserApplicationRoleConnection, UserCollectibles,
    UserConnection, UserNameplate, UserPrimaryGuild,
};
pub use voice::{
    CreateStageInstance, ModifyCurrentUserVoiceState, ModifyStageInstance, ModifyUserVoiceState,
    SetVoiceChannelStatus, StageInstance, VoiceRegion, VoiceServerUpdate, VoiceState,
};
pub use webhook::{
    CreateWebhook, ModifyWebhook, ModifyWebhookWithToken, Webhook, WebhookExecuteQuery,
    WebhookMessageQuery,
};

// Phase 3 primitives. Since series 2 step B every ID field and parameter has
// its per-resource alias (`GenericId` only where the kind is unknown); since
// series 3 every discriminant field uses its open newtype from
// `discriminant` (the canonical module, also public). `Option<Option<T>>`
// stays until series 4 migrates it.
pub use discriminant::{
    ActivityType, ApplicationCommandHandlerType, ApplicationEventWebhookStatus,
    ApplicationIntegrationType, AuditLogActionType, AutoModerationActionType,
    AutoModerationEventType, AutoModerationTriggerType, BaseTheme, ButtonStyle, ChannelType,
    CommandOptionType, CommandPermissionType, CommandType, ComponentType, ConnectionVisibility,
    DefaultMessageNotificationLevel, DisplayNameEffect, DisplayNameFont, EntitlementOwnerType,
    EntitlementType, ExplicitContentFilterLevel, ForumLayout, ForumSortOrder, GuildNsfwLevel,
    IntegrationExpireBehavior, InteractionContextType, InteractionResponseType, InteractionType,
    InviteTargetType, InviteTargetUsersJobStatusType, InviteType, KeywordPresetType,
    MessageActivityType, MessageReferenceType, MessageType, MfaLevel, OnboardingMode,
    OnboardingPromptType, OverwriteType, PollLayoutType, PremiumTier, ReactionType,
    RecurrenceRuleFrequency, RecurrenceRuleMonth, RecurrenceRuleWeekday,
    RoleConnectionMetadataType, ScheduledEventEntityType, ScheduledEventPrivacyLevel,
    ScheduledEventStatus, SeparatorSpacing, SkuAccessType, SkuType, StageInstancePrivacyLevel,
    StickerFormatType, StickerType, SubscriptionStatus, TeamMembershipState, TextInputStyle,
    UserPremiumType, VerificationLevel, VideoQualityMode, VoiceChannelEffectAnimationType,
    WebhookPayloadType, WebhookType,
};
pub use id::{
    marker, ActivityLaunchId, ApplicationId, AttachmentId, AuditLogEntryId, AutoModerationRuleId,
    ChannelId, CommandId, CommandVersionId, ComponentId, EmojiId, EntitlementId, ForumTagId,
    GenericId, GuildId, GuildInstallOwner, Id, IdMarker, IntegrationId, InteractionId, InvalidId,
    InvalidIdKind, LobbyId, MessageId, OnboardingOptionId, OnboardingPromptId, PollAnswerId,
    PromotionId, RoleId, RoleSubscriptionListingId, ScheduledEventId, SkuId, SoundboardSoundId,
    StageInstanceId, StickerId, StickerPackId, SubscriptionId, TeamId, UploadId, UserId, WebhookId,
    WidgetUserId,
};
pub use patch::{Nullable, NullablePatch, Patch};

#[cfg(test)]
pub(crate) use id::test_id;
pub(crate) use id::SNOWFLAKE_TIMESTAMP_SHIFT;

/// The 2.x ID type, now a plain alias for [`GenericId`].
///
/// 2.x `Snowflake` stored the decimal text (`String` plus a parsed `u64`),
/// accepted any string, and converted implicitly from `u64`/`&str`/`String`.
/// It is gone; this alias only exists so old code finds its way to the
/// replacement. Use the per-resource IDs ([`GuildId`], [`ChannelId`],
/// [`UserId`], ...) where the kind is known and [`GenericId`] where it is not.
/// IDs are now `Copy`, eight bytes, never zero, and built with checked
/// conversions:
///
/// ```
/// use discordrs::{DiscordError, GenericId, GuildId};
///
/// # fn main() -> Result<(), DiscordError> {
/// let from_text: GuildId = "81384788765712384".parse()?;
/// let from_number = GenericId::try_from(81384788765712384_u64)?;
/// assert_eq!(from_text.get(), from_number.get());
/// # Ok(())
/// # }
/// ```
#[deprecated(
    since = "3.0.0",
    note = "Snowflake is now the numeric, Copy GenericId; use the typed ids (GuildId, ChannelId, UserId, ...) or GenericId, built with `\"123\".parse()?` or `GenericId::try_from(123)?`"
)]
pub type Snowflake = GenericId;

// The response state machine lives in `response.rs` next to the methods that
// drive it; re-exported so `model::InteractionResponseState` keeps working.
pub use crate::response::InteractionResponseState;

// Shared `skip_serializing_if` helper for the message and embed modules.
fn is_false(v: &bool) -> bool {
    !v
}

#[cfg(test)]
#[path = "model/polymorphic_tests.rs"]
mod polymorphic_tests;
#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
