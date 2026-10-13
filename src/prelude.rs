pub use crate::bitfield::{BitField, Intents, MessageFlags, Permissions};
pub use crate::builders::{
    create_container, ActionRowBuilder, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    EmbedBuilder, LinkButtonBuilder, ModalBuilder, SelectDefaultValue, SelectMenuBuilder,
    TextInputBuilder,
};
#[cfg(feature = "collectors")]
pub use crate::collector::{
    CollectorHub, ComponentCollector, EmojiKey, InteractionCollector, MessageCollector,
    MessageUpdateCollector, ModalCollector, ReactionCollector,
};
pub use crate::command::{
    CommandDefinition, CommandOptionBuilder, MessageCommandBuilder,
    PrimaryEntryPointCommandBuilder, SlashCommandBuilder, UserCommandBuilder,
};
pub use crate::constants::gateway_intents;
// The open discriminants builders take; they replace the 2.x `button_style`,
// `text_input_style`, `command_type`, and `option_type` constant modules this
// prelude used to export.
pub use crate::error::DiscordError;
pub use crate::event::Event;
pub use crate::formatters::{MentionResolver, TimestampStyle};
#[cfg(all(feature = "gateway", feature = "sharding"))]
pub use crate::gateway::ShardSupervisor;
#[cfg(feature = "gateway")]
pub use crate::gateway::{Client, Context, EventDispatchMode, EventHandler, ShardMessenger};
pub use crate::helpers::{
    launch_activity, respond_to_interaction, respond_with_message, send_message,
};
pub use crate::model::{
    ButtonStyle, ChannelType, CommandOptionType, CommandType, ComponentType, SeparatorSpacing,
    TextInputStyle,
};
// Deprecated in 3.0 in favour of `InteractionResponder`; kept so existing
// prelude users keep compiling (with a deprecation warning at the call site).
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use crate::helpers::{
    defer_interaction, defer_update_interaction, delete_followup_response,
    delete_original_response, edit_followup_response, edit_original_response, followup_message,
    get_original_response, respond_with_autocomplete_choices, respond_with_modal_typed,
    update_interaction_message,
};
pub use crate::http::{FileAttachment, FileUpload, RestClient};
pub use crate::model::{
    Activity, ActivityAssets, ActivityButton, ActivityInstance, ActivityLocation, ActivityParty,
    ActivitySecrets, ActivityTimestamps, ActivityType, AllowedMentions, Application,
    ApplicationCommandHandlerType, ApplicationCommandOptionChoice, ApplicationCommandPermission,
    ApplicationInstallParams, ApplicationIntegrationType, ApplicationIntegrationTypeConfig,
    ApplicationRoleConnectionMetadata, ArchivedThreadsQuery, AuditLog, AuditLogQuery,
    AuthorizationInformation, AutoModerationRule, BulkGuildBanResponse, ChannelPinsQuery,
    ClientStatus, CommandInteractionData, CommandInteractionOption, CommandOptionValue,
    CurrentUserGuild, CurrentUserGuildsQuery, Entitlement, EntitlementQuery, Gateway, GatewayBot,
    GetGuildQuery, GuildApplicationCommandPermissions, GuildBansQuery, GuildIncidentsData,
    GuildMembersQuery, GuildPreview, GuildPruneCount, GuildScheduledEventRecurrenceRule,
    GuildWidget, GuildWidgetImageStyle, Integration, Interaction, InteractionContextData,
    InteractionContextType, Invite, InviteTargetUserIds, InviteTargetUsersJobStatus,
    JoinedArchivedThreadsQuery, Lobby, LobbyMember, LobbyMemberUpdate, Message, MessageCall,
    MessageComponent, MessageInteraction, MessageInteractionMetadata, MessageSnapshot,
    PollAnswerVoters, ReactionCountDetails, RequestChannelInfo, RequestGuildMembers, ResolvedData,
    RoleColors, SearchGuildMembersQuery, SearchGuildMessagesQuery, SessionStartLimit,
    SharedClientTheme, Sku, SoundboardSound, SoundboardSoundList, Subscription, SubscriptionQuery,
    ThreadListResponse, ThreadMember, ThreadMemberQuery, UpdatePresence,
    UserApplicationRoleConnection, UserConnection, VanityUrl, VoiceRegion, VoiceServerUpdate,
    VoiceState, WebhookExecuteQuery, WebhookMessageQuery,
};
pub use crate::request::{
    AddGroupDmRecipient, AddGuildMember, AddLobbyMember, BeginGuildPruneRequest,
    BulkGuildBanRequest, CreateChannelInvite, CreateGroupDmChannel, CreateGuildBan,
    CreateGuildChannel, CreateGuildRole, CreateGuildSticker, CreateLobby, CreateMessage,
    CreateStageInstance, CreateTestEntitlement, CreateWebhook, EditApplicationCommandPermissions,
    EditChannelPermission, LinkLobbyChannel, ModifyCurrentApplication, ModifyCurrentMember,
    ModifyCurrentUser, ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildChannelPosition,
    ModifyGuildIncidentActions, ModifyGuildMember, ModifyGuildOnboarding, ModifyGuildRole,
    ModifyGuildRolePosition, ModifyGuildSticker, ModifyGuildWelcomeScreen,
    ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance, ModifyUserVoiceState,
    ModifyWebhook, ModifyWebhookWithToken, SetVoiceChannelStatus,
    UpdateUserApplicationRoleConnection,
};
// Typed IDs (docs/design/3.0-types-events-requests.md §1): the common
// resource kinds, plus `InvalidId` for annotating checked parsing. The other
// aliases (and `GenericId`, used only where Discord leaves the kind open) are
// imported from the crate root.
pub use crate::model::{ApplicationId, ChannelId, GuildId, InvalidId, MessageId, RoleId, UserId};
// Deprecated 2.x names; glob importers get a note pointing at the typed IDs
// and at `Permissions`.
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use crate::model::{PermissionsBitField, Snowflake};
pub use crate::oauth2::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken, OAuth2Scope,
    OAuth2TokenResponse,
};
pub use crate::permissions::{ChannelPermissions, ComputedPermissions, PermissionDataError};
pub use crate::request::{
    CreateFollowup, EditInteractionResponse, EndpointReply, InteractionMessage, InteractionResponse,
};
pub use crate::response::{InteractionResponder, InteractionResponsePhase, MessageBuilder};
// Deprecated in 3.0 in favour of `InteractionResponse`, the message request
// types, and `ApplicationCommandOptionChoice`.
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use crate::response::{AutocompleteChoice, InteractionReplyData, InteractionResponseBuilder};
#[cfg(feature = "voice")]
pub use crate::voice::{
    AudioTrack, VoiceConnectionConfig, VoiceEncryptionMode, VoiceGatewayCommand, VoiceManager,
    VoiceSpeakingFlags, VoiceTransportState, VoiceUdpDiscoveryPacket,
};
#[cfg(feature = "voice")]
pub use crate::voice_runtime::{
    connect as connect_voice_runtime, VoiceDaveFrame, VoiceDaveFrameDecryptor, VoiceDecodedPacket,
    VoiceOpusDecoder, VoiceOpusFrame, VoiceOutboundPacket, VoiceOutboundRtpState,
    VoiceRawUdpPacket, VoiceReceivedPacket, VoiceRuntimeConfig, VoiceRuntimeHandle,
};
#[cfg(all(feature = "voice", feature = "voice-encode"))]
pub use crate::voice_runtime::{AudioMixer, AudioSource, PcmFrame, VoiceOpusEncoder};
#[cfg(all(feature = "voice", feature = "dave"))]
pub use crate::voice_runtime::{VoiceDaveFrameEncryptor, VoiceDaveyDecryptor, VoiceDaveySession};
pub use crate::webhook_events::{
    parse_webhook_event_payload, ApplicationAuthorizedWebhookEvent,
    ApplicationDeauthorizedWebhookEvent, WebhookDeletedMessage, WebhookEvent, WebhookEventBody,
    WebhookEventPayload, WebhookPayloadType, WebhookSocialMessage,
};

#[cfg(test)]
mod tests {
    use crate::prelude::{
        gateway_intents, AllowedMentions, ApplicationInstallParams,
        ApplicationIntegrationTypeConfig, AuthorizationInformation, ButtonStyle, CreateWebhook,
        CustomButtonBuilder, Gateway, MessageBuilder, ModifyCurrentApplication, ModifyWebhook,
        ModifyWebhookWithToken, ReactionCountDetails, SlashCommandBuilder, WebhookExecuteQuery,
        WebhookMessageQuery,
    };

    #[test]
    fn prelude_surfaces_common_gateway_and_command_types() {
        let _intents = gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES;
        let _button = CustomButtonBuilder::new("id").style(ButtonStyle::PRIMARY);
        let _command =
            SlashCommandBuilder::new("ping", "Ping").string_option("target", "Target", false);
        let _message = MessageBuilder::new().content("hello");
        let _application_edit = ModifyCurrentApplication {
            install_params: Some(ApplicationInstallParams {
                scopes: vec!["bot".to_string()],
                permissions: crate::prelude::Permissions::from_bits(0),
            }),
            integration_types_config: Some(std::collections::HashMap::from([(
                "0".to_string(),
                ApplicationIntegrationTypeConfig::default(),
            )])),
            ..ModifyCurrentApplication::default()
        };
        let _webhook_create = CreateWebhook {
            name: "deployments".to_string(),
            avatar: None,
        };
        let _webhook_modify = ModifyWebhook::default();
        let _webhook_token_modify = ModifyWebhookWithToken::default();
        let _webhook_execute_query = WebhookExecuteQuery::default();
        let _webhook_message_query = WebhookMessageQuery::default();
        let _gateway = Gateway::default();
        // No `Default` since its `application` became a typed partial
        // application; naming the type is enough to check the export.
        let _authorization: Option<AuthorizationInformation> = None;
        let _allowed_mentions = AllowedMentions::default();
        let _reaction_count_details = ReactionCountDetails::default();
    }

    #[test]
    fn prelude_surfaces_the_interaction_responder() {
        use crate::prelude::*;

        // Generic over the trait from the prelude, so `prelude::*` is enough
        // to call responder methods on any interaction variant.
        fn phase_of<R: InteractionResponder>(responder: &R) -> InteractionResponsePhase {
            responder.response_phase()
        }

        let command = crate::model::ChatInputCommandInteraction::fixture();
        assert_eq!(phase_of(&command), InteractionResponsePhase::Unacknowledged);
        let _reply = InteractionMessage::from("hi").ephemeral(true);
        let _choice = ApplicationCommandOptionChoice::new("name", "value");
        let _reply: EndpointReply = InteractionResponse::defer(true).into();
    }
}
