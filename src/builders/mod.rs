//! Canonical builder imports live on `discordrs::builders::{...}` and the crate root re-exports.
//! The submodules below are implementation detail modules behind those stable re-exports.

mod components;
mod container;
mod embed;
pub(crate) mod limits;
mod media;
mod message;
mod modal;

/// Select menu defaults are the model type, shared with received components.
pub use crate::model::SelectDefaultValue;
// The 2.x generic button builder; use the three split builders.
#[expect(deprecated, reason = "deliberate re-export of a deprecated item")]
pub use components::ButtonBuilder;
pub use components::{
    ActionRowBuilder, ChannelSelectBuilder, ComponentsV2Message, CustomButtonBuilder,
    LinkButtonBuilder, MentionableSelectBuilder, PremiumButtonBuilder, RoleSelectBuilder,
    SelectMenuBuilder, StringSelectBuilder, UserSelectBuilder,
};
pub use container::{
    create_container, create_default_buttons, ContainerBuilder, SeparatorBuilder,
    TextDisplayBuilder,
};
pub use embed::EmbedBuilder;
pub use media::{FileBuilder, MediaGalleryBuilder, SectionBuilder, ThumbnailBuilder};
pub use message::MessageBuilder;
pub use modal::{
    CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, LabelBuilder, ModalBuilder,
    RadioGroupBuilder, TextInputBuilder,
};
