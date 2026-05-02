pub use crate::bitfield::{BitField, Intents, MessageFlags, Permissions};
pub use crate::builders::{
    create_container, ActionRowBuilder, ButtonBuilder, ComponentsV2Message, ContainerBuilder,
    EmbedBuilder, ModalBuilder, SelectDefaultValue, SelectMenuBuilder, TextInputBuilder,
};
#[cfg(feature = "collectors")]
pub use crate::collector::{
    CollectorHub, ComponentCollector, InteractionCollector, MessageCollector, ModalCollector,
};
pub use crate::command::{
    command_type, option_type, CommandDefinition, CommandOptionBuilder, MessageCommandBuilder,
    PrimaryEntryPointCommandBuilder, SlashCommandBuilder, UserCommandBuilder,
};
pub use crate::constants::{button_style, gateway_intents, text_input_style};
pub use crate::error::DiscordError;
pub use crate::event::Event;
#[cfg(all(feature = "gateway", feature = "sharding"))]
pub use crate::gateway::ShardSupervisor;
#[cfg(feature = "gateway")]
pub use crate::gateway::{Client, Context, EventHandler, ShardMessenger};
pub use crate::helpers::{
    defer_interaction, defer_update_interaction, delete_followup_response,
    delete_original_response, edit_followup_response, edit_original_response, followup_message,
    get_original_response, launch_activity, respond_to_interaction,
    respond_with_autocomplete_choices, respond_with_message, respond_with_modal_typed,
    send_message, update_interaction_message,
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
    Invite, InviteTargetUsersJobStatus, JoinedArchivedThreadsQuery, LinkLobbyChannel, Lobby,
    LobbyMember, LobbyMemberUpdate, Message, MessageCall, MessageSnapshot,
    ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUser, ModifyCurrentUserVoiceState,
    ModifyGuild, ModifyGuildChannelPosition, ModifyGuildIncidentActions, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance,
    ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken, PermissionsBitField,
    PollAnswerVoters, ReactionCountDetails, RequestChannelInfo, RequestGuildMembers, RoleColors,
    SearchGuildMembersQuery, SearchGuildMessagesQuery, SessionStartLimit, SetVoiceChannelStatus,
    SharedClientTheme, Sku, Snowflake, SoundboardSound, SoundboardSoundList, Subscription,
    SubscriptionQuery, ThreadListResponse, ThreadMember, ThreadMemberQuery, UpdatePresence,
    UpdateUserApplicationRoleConnection, UserApplicationRoleConnection, UserConnection, VanityUrl,
    VoiceRegion, VoiceServerUpdate, VoiceState, WebhookExecuteQuery, WebhookMessageQuery,
};
pub use crate::oauth2::{
    OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken, OAuth2Scope,
    OAuth2TokenResponse,
};
pub use crate::response::{InteractionResponseBuilder, MessageBuilder};
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
        button_style, gateway_intents, AllowedMentions, ApplicationInstallParams,
        ApplicationIntegrationTypeConfig, AuthorizationInformation, ButtonBuilder, CreateWebhook,
        Gateway, MessageBuilder, ModifyCurrentApplication, ModifyWebhook, ModifyWebhookWithToken,
        ReactionCountDetails, SlashCommandBuilder, WebhookExecuteQuery, WebhookMessageQuery,
    };

    #[test]
    fn prelude_surfaces_common_gateway_and_command_types() {
        let _intents = gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES;
        let _button = ButtonBuilder::new().style(button_style::PRIMARY);
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
}
