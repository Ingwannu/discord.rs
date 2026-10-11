//! Typed outbound request models (3.0 phase 5).
//!
//! The phase 5 request facade (docs/design/3.0-types-events-requests.md §4):
//! request bodies are typed data that the builders produce, instead of
//! `serde_json::Value` assembled inside each builder. Response domain types
//! stay in [`crate::model`].
//!
//! So far this holds the Component Embed generator, which builds the
//! `discord:component-embed` payload a website serves so that Discord renders
//! its link preview from a Components V2 layout. It is unrelated to sending
//! messages; the receiving side of the same feature is
//! [`Embed::components`](crate::model::Embed::components).
//!
//! The submodules are private; every public item is re-exported here, so
//! `discordrs::request::X` is the only public path (the same layout as
//! `model`).

mod component_embed;

pub use component_embed::{
    ComponentEmbed, ComponentEmbedBuilder, ComponentEmbedError, PreviewAccessory, PreviewActionRow,
    PreviewComponent, PreviewContainer, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem,
    PreviewSection, PreviewSeparator, PreviewTextDisplay, PreviewThumbnail, SeparatorSpacing,
};
