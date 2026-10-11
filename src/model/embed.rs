//! Message embeds.

use serde::{Deserialize, Serialize};

use super::{is_false, MessageComponent};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedField`.
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inline: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedAuthor`.
pub struct EmbedAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedFooter`.
pub struct EmbedFooter {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedMedia`.
pub struct EmbedMedia {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Embed`.
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
    /// Components of a `components` embed: the single Container a website
    /// supplied through a Component Embed link preview (Discord changelog
    /// 2026-10-05). Empty for every other embed type, and never serialized
    /// when empty, so outgoing embeds are unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<MessageComponent>,
    /// Embed flags bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

impl Embed {
    /// `type` value of an embed rendered from a website's Component Embed.
    pub const KIND_COMPONENTS: &'static str = "components";

    /// Returns true for a link preview built from a website's Component
    /// Embed; its layout is in [`Embed::components`].
    pub fn is_component_embed(&self) -> bool {
        self.kind.as_deref() == Some(Self::KIND_COMPONENTS)
    }
}
