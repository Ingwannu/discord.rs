//! Typed outbound request models (3.0 phase 5).
//!
//! The phase 5 request facade (docs/design/3.0-types-events-requests.md §4):
//! request bodies are typed data that the builders produce, instead of
//! `serde_json::Value` assembled inside each builder. Response domain types
//! stay in [`crate::model`].
//!
//! - Message bodies ([`CreateMessage`], [`EditMessage`], [`InteractionMessage`],
//!   [`UpdateMessage`], [`CreateFollowup`], [`ExecuteWebhook`],
//!   [`EditWebhookMessage`]) and their
//!   parts: components ([`Component`] and the types it holds), [`Embed`],
//!   [`CreatePoll`], and attachment metadata ([`MessageAttachment`]).
//! - REST bodies of the other domains (3.0 phase 5 B): channels and threads
//!   ([`ModifyChannel`], [`StartThreadFromMessage`],
//!   [`StartThreadWithoutMessage`], [`StartForumThread`]), auto moderation,
//!   emoji and soundboard, guild templates, scheduled events, and
//!   [`EditApplicationCommand`].
//! - The 2.x REST bodies that lived in [`crate::model`] until checkpoint 5C
//!   (guilds, roles, members, channels, invites, webhooks, voice, lobbies,
//!   users, applications, bans, stickers, test entitlements). They keep
//!   their public fields and `Default`; `model::X` stays as a deprecated
//!   alias until 4.0.
//! - The Component Embed generator, which builds the
//!   `discord:component-embed` payload a website serves so that Discord
//!   renders its link preview from a Components V2 layout. It is unrelated to
//!   sending messages; the receiving side of the same feature is
//!   [`Embed::components`](crate::model::Embed::components).
//!
//! The submodules are private; every public item is re-exported here, so
//! `discordrs::request::X` is the only public path (the same layout as
//! `model`).

mod application;
mod attachment;
mod channel;
mod command;
mod component;
mod component_embed;
mod embed;
mod fields;
mod guild;
mod interaction;
mod invite;
mod lobby;
mod media;
mod member;
mod message;
mod modal;
mod moderation;
mod monetization;
mod poll;
mod role;
mod scheduled_event;
mod user;
mod voice;
mod webhook;

pub(crate) use component::{check_message_components, ComponentsMode};

pub use application::{ModifyCurrentApplication, UpdateApplicationIdentityProfile};
pub use attachment::{AttachmentUpload, ExistingAttachment, MessageAttachment};
pub use channel::{
    AddGroupDmRecipient, AvailableForumTag, CreateDmChannel, CreateGroupDmChannel,
    CreateGuildChannel, EditChannelPermission, ForumThreadMessage, ModifyChannel,
    ModifyGuildChannelPosition, StartForumThread, StartThreadFromMessage,
    StartThreadWithoutMessage,
};
pub use command::{EditApplicationCommand, EditApplicationCommandPermissions};
pub use component::{
    ActionRow, Button, ButtonRow, ChannelSelect, Component, Container, ContainerComponent,
    CustomButton, FileComponent, LinkButton, MediaGallery, MentionableSelect, PremiumButton,
    RawComponent, RoleSelect, Section, SectionAccessory, SelectMenu, SelectRow, Separator,
    StringSelect, TextDisplay, Thumbnail, UserSelect,
};
pub use component_embed::{
    ComponentEmbed, ComponentEmbedBuilder, ComponentEmbedError, PreviewAccessory, PreviewActionRow,
    PreviewComponent, PreviewContainer, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem,
    PreviewSection, PreviewSeparator, PreviewTextDisplay, PreviewThumbnail,
};
pub use embed::{Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedImage};
pub use guild::{
    BeginGuildPruneRequest, CreateGuild, CreateGuildFromTemplate, CreateGuildPlaceholderChannel,
    CreateGuildPlaceholderId, CreateGuildPlaceholderOverwrite, CreateGuildPlaceholderRole,
    CreateGuildTemplate, ModifyGuild, ModifyGuildIncidentActions, ModifyGuildOnboarding,
    ModifyGuildTemplate, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings,
};
pub(crate) use interaction::encode_interaction_response;
pub use interaction::{EndpointReply, InteractionResponse, RawInteractionResponse};
pub use invite::CreateChannelInvite;
pub use lobby::{
    AddLobbyMember, CreateLobby, CreateLobbyMessage, GetOrCreateLobby, LinkLobbyChannel,
    ModifyLobby,
};
pub use media::{
    CreateApplicationEmoji, CreateGuildEmoji, CreateGuildSoundboardSound, CreateGuildSticker,
    ModifyApplicationEmoji, ModifyGuildEmoji, ModifyGuildSoundboardSound, ModifyGuildSticker,
    SendSoundboardSound,
};
pub use member::{AddGuildMember, ModifyCurrentMember, ModifyGuildMember};
pub use message::{
    CreateFollowup, CreateMessage, EditInteractionResponse, EditMessage, EditWebhookMessage,
    ExecuteWebhook, InteractionMessage, UpdateMessage, WithDefaultMentions,
};
pub use modal::{
    Checkbox, CheckboxGroup, FileUpload, Label, LabelComponent, LegacyTextInputRow, Modal,
    ModalComponent, RadioGroup, TextInput,
};
pub use moderation::{
    BulkGuildBanRequest, CreateAutoModerationRule, CreateGuildBan, ModifyAutoModerationRule,
};
pub use monetization::CreateTestEntitlement;
pub use poll::{CreatePoll, CreatePollAnswer};
pub use role::{CreateGuildRole, ModifyGuildRole, ModifyGuildRolePosition};
pub use scheduled_event::{CreateGuildScheduledEvent, ModifyGuildScheduledEvent};
pub use user::{ModifyCurrentUser, UpdateUserApplicationRoleConnection};
pub use voice::{
    CreateStageInstance, ModifyCurrentUserVoiceState, ModifyStageInstance, ModifyUserVoiceState,
    SetVoiceChannelStatus,
};
pub use webhook::{CreateWebhook, ModifyWebhook, ModifyWebhookWithToken};
