pub use crate::bitfield::{BitField, Intents, MessageFlags, Permissions};
pub use crate::builders::{
    create_container, ActionRowBuilder, ButtonBuilder, ComponentsV2Message, ContainerBuilder,
    EmbedBuilder, ModalBuilder, SelectDefaultValue, SelectMenuBuilder, TextInputBuilder,
};
#[cfg(feature = "collectors")]
pub use crate::collector::{
    CollectorHub, ComponentCollector, EmojiKey, InteractionCollector, MessageCollector,
    ModalCollector, ReactionCollector,
};
pub use crate::command::{
    command_type, option_type, CommandDefinition, CommandOptionBuilder, MessageCommandBuilder,
    PrimaryEntryPointCommandBuilder, SlashCommandBuilder, UserCommandBuilder,
};
pub use crate::constants::gateway_intents;
// The open discriminants builders take; they replace the 2.x `button_style`
// and `text_input_style` constant modules this prelude used to export.
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
pub use crate::model::{ButtonStyle, ChannelType, ComponentType, SeparatorSpacing, TextInputStyle};
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
    ActivitySecrets, ActivityTimestamps, ActivityType, AddGroupDmRecipient, AddGuildMember,
    AddLobbyMember, AllowedMentions, Application, ApplicationCommandHandlerType,
    ApplicationCommandOptionChoice, ApplicationCommandPermission, ApplicationInstallParams,
    ApplicationIntegrationType, ApplicationIntegrationTypeConfig,
    ApplicationRoleConnectionMetadata, ArchivedThreadsQuery, AuditLog, AuditLogQuery,
    AuthorizationInformation, AutoModerationRule, BeginGuildPruneRequest, BulkGuildBanRequest,
    BulkGuildBanResponse, ChannelPinsQuery, ClientStatus, CommandInteractionData,
    CommandInteractionOption, CreateChannelInvite, CreateGroupDmChannel, CreateGuildBan,
    CreateGuildChannel, CreateGuildRole, CreateGuildSticker, CreateLobby, CreateMessage,
    CreateStageInstance, CreateTestEntitlement, CreateWebhook, CurrentUserGuild,
    CurrentUserGuildsQuery, EditApplicationCommandPermissions, EditChannelPermission, Entitlement,
    EntitlementQuery, Gateway, GatewayBot, GetGuildQuery, GuildApplicationCommandPermissions,
    GuildBansQuery, GuildIncidentsData, GuildMembersQuery, GuildPreview, GuildPruneCount,
    GuildScheduledEventRecurrenceRule, GuildWidget, GuildWidgetImageStyle, Integration,
    Interaction, InteractionCallbackResponse, InteractionContextData, InteractionContextType,
    Invite, InviteTargetUserIds, InviteTargetUsersJobStatus, JoinedArchivedThreadsQuery,
    LinkLobbyChannel, Lobby, LobbyMember, LobbyMemberUpdate, Message, MessageCall,
    MessageComponent, MessageInteraction, MessageInteractionMetadata, MessageSnapshot,
    ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUser, ModifyCurrentUserVoiceState,
    ModifyGuild, ModifyGuildChannelPosition, ModifyGuildIncidentActions, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance,
    ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken, PermissionsBitField,
    PollAnswerVoters, ReactionCountDetails, RequestChannelInfo, RequestGuildMembers, ResolvedData,
    RoleColors, SearchGuildMembersQuery, SearchGuildMessagesQuery, SessionStartLimit,
    SetVoiceChannelStatus, SharedClientTheme, Sku, SoundboardSound, SoundboardSoundList,
    Subscription, SubscriptionQuery, ThreadListResponse, ThreadMember, ThreadMemberQuery,
    UpdatePresence, UpdateUserApplicationRoleConnection, UserApplicationRoleConnection,
    UserConnection, VanityUrl, VoiceRegion, VoiceServerUpdate, VoiceState, WebhookExecuteQuery,
    WebhookMessageQuery,
};
// Typed IDs (docs/design/3.0-types-events-requests.md §1): the common
// resource kinds, plus `InvalidId` for annotating checked parsing. The other
// aliases (and `GenericId`, used only where Discord leaves the kind open) are
// imported from the crate root.
pub use crate::model::{ApplicationId, ChannelId, GuildId, InvalidId, MessageId, RoleId, UserId};
// Deprecated 2.x name; glob importers get a note pointing at the typed IDs.
#[expect(deprecated, reason = "deliberate re-export of deprecated items")]
pub use crate::model::Snowflake;
pub use crate::oauth2::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken, OAuth2Scope,
    OAuth2TokenResponse,
};
pub use crate::permissions::{ChannelPermissions, ComputedPermissions, PermissionDataError};
pub use crate::response::{
    AutocompleteChoice, InteractionReplyData, InteractionResponder, InteractionResponseBuilder,
    InteractionResponsePhase, MessageBuilder,
};
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
        ApplicationIntegrationTypeConfig, AuthorizationInformation, ButtonBuilder, ButtonStyle,
        CreateWebhook, Gateway, MessageBuilder, ModifyCurrentApplication, ModifyWebhook,
        ModifyWebhookWithToken, ReactionCountDetails, SlashCommandBuilder, WebhookExecuteQuery,
        WebhookMessageQuery,
    };

    #[test]
    fn prelude_surfaces_common_gateway_and_command_types() {
        let _intents = gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES;
        let _button = ButtonBuilder::new().style(ButtonStyle::PRIMARY);
        let _command =
            SlashCommandBuilder::new("ping", "Ping").string_option("target", "Target", false);
        let _message = MessageBuilder::new().content("hello");
        let _application_edit = ModifyCurrentApplication {
            install_params: Some(ApplicationInstallParams {
                scopes: vec!["bot".to_string()],
                permissions: crate::prelude::PermissionsBitField(0),
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
        let _authorization = AuthorizationInformation::default();
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
        let _reply = InteractionReplyData::from("hi").ephemeral();
        let _choice = AutocompleteChoice::new("name", "value");
    }
}
