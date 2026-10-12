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
//! - The Component Embed generator, which builds the
//!   `discord:component-embed` payload a website serves so that Discord
//!   renders its link preview from a Components V2 layout. It is unrelated to
//!   sending messages; the receiving side of the same feature is
//!   [`Embed::components`](crate::model::Embed::components).
//!
//! The submodules are private; every public item is re-exported here, so
//! `discordrs::request::X` is the only public path (the same layout as
//! `model`).

mod attachment;
mod component;
mod component_embed;
mod embed;
mod interaction;
mod message;
mod modal;
mod poll;

pub(crate) use component::{check_message_components, ComponentsMode};

pub use attachment::{AttachmentUpload, ExistingAttachment, MessageAttachment};
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
pub(crate) use interaction::encode_interaction_response;
pub use interaction::{EndpointReply, InteractionResponse, RawInteractionResponse};
pub use message::{
    CreateFollowup, CreateMessage, EditInteractionResponse, EditMessage, EditWebhookMessage,
    ExecuteWebhook, InteractionMessage, UpdateMessage, WithDefaultMentions,
};
pub use modal::{
    Checkbox, CheckboxGroup, FileUpload, Label, LabelComponent, LegacyTextInputRow, Modal,
    ModalComponent, RadioGroup, TextInput,
};
pub use poll::{CreatePoll, CreatePollAnswer};
