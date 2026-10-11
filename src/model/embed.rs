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

/// Source site of an embed (Discord docs: Embed Provider Structure), such
/// as `YouTube` on a video link preview.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EmbedProvider {
    /// Name of the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// URL of the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
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
    pub provider: Option<EmbedProvider>,
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{Embed, EmbedProvider};

    /// A YouTube link preview as Discord sends it (Embed Structure:
    /// `provider` is an Embed Provider object).
    const VIDEO_EMBED: &str = r#"{
        "type": "video",
        "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "title": "Never Gonna Give You Up",
        "color": 16711680,
        "provider": { "name": "YouTube", "url": "https://www.youtube.com" },
        "author": { "name": "Rick Astley", "url": "https://www.youtube.com/@RickAstleyYT" }
    }"#;

    fn youtube() -> EmbedProvider {
        EmbedProvider {
            name: Some("YouTube".to_string()),
            url: Some("https://www.youtube.com".to_string()),
        }
    }

    #[test]
    fn provider_decodes_from_text_and_value_and_writes_the_same_keys() {
        let from_text: Embed = serde_json::from_str(VIDEO_EMBED).unwrap();
        assert_eq!(from_text.provider, Some(youtube()));

        let wire: Value = serde_json::from_str(VIDEO_EMBED).unwrap();
        let from_value: Embed = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(from_value.provider, Some(youtube()));

        let written = serde_json::to_value(&from_value).unwrap();
        assert_eq!(written["provider"], wire["provider"]);

        // Both provider fields are optional; a name-only provider stays so.
        let name_only: EmbedProvider = serde_json::from_value(json!({ "name": "Site" })).unwrap();
        assert_eq!(name_only.url, None);
        assert_eq!(
            serde_json::to_value(&name_only).unwrap(),
            json!({ "name": "Site" })
        );
    }

    #[test]
    fn provider_is_typed_inside_a_message_from_text_and_value() {
        let embed: Value = serde_json::from_str(VIDEO_EMBED).unwrap();
        let data = json!({
            "id": "334385199974967042",
            "channel_id": "290926798999357250",
            "content": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "embeds": [embed],
            "type": 0
        });
        // The gateway decodes `d` from its text once the phase 4 envelope
        // lands and from a cloned `Value` before that; both must type it.
        let from_text: crate::model::Message = serde_json::from_str(&data.to_string()).unwrap();
        assert_eq!(from_text.embeds[0].provider, Some(youtube()));
        let from_value: crate::model::Message = serde_json::from_value(data).unwrap();
        assert_eq!(from_value.embeds[0].provider, Some(youtube()));
    }
}
