use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::button_style;
use crate::error::DiscordError;

/// Backward-compatible error type alias. Prefer `DiscordError` directly.
#[deprecated(since = "0.4.0", note = "Use DiscordError instead")]
pub type Error = DiscordError;

pub(crate) fn to_json_value<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub(crate) fn invalid_data_error(message: impl Into<String>) -> DiscordError {
    DiscordError::model(message)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Emoji`.
pub struct Emoji {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `animated`.
    pub animated: Option<bool>,
}

impl Emoji {
    /// Creates or returns `unicode` data.
    pub fn unicode(emoji: &str) -> Self {
        Self {
            name: Some(emoji.to_string()),
            id: None,
            animated: None,
        }
    }

    /// Creates or returns `custom` data.
    pub fn custom(name: &str, id: &str, animated: bool) -> Self {
        Self {
            name: Some(name.to_string()),
            id: Some(id.to_string()),
            animated: Some(animated),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MediaGalleryItem`.
pub struct MediaGalleryItem {
    /// Discord API payload field `media`.
    pub media: MediaInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `spoiler`.
    pub spoiler: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MediaInfo`.
pub struct MediaInfo {
    /// Discord API payload field `url`.
    pub url: String,
}

impl MediaGalleryItem {
    /// Creates or returns `new` data.
    pub fn new(url: &str) -> Self {
        Self {
            media: MediaInfo {
                url: url.to_string(),
            },
            description: None,
            spoiler: None,
        }
    }

    /// Runs the `description` operation.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Runs the `spoiler` operation.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SelectOption`.
pub struct SelectOption {
    /// Discord API payload field `label`.
    pub label: String,
    /// Discord API payload field `value`.
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default`.
    pub default: Option<bool>,
}

impl SelectOption {
    /// Creates or returns `new` data.
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            label: label.to_string(),
            value: value.to_string(),
            description: None,
            emoji: None,
            default: None,
        }
    }

    /// Runs the `description` operation.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Runs the `emoji` operation.
    pub fn emoji(mut self, emoji: &str) -> Self {
        self.emoji = Some(Emoji::unicode(emoji));
        self
    }

    /// Runs the `default_selected` operation.
    pub fn default_selected(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }
}

#[derive(Clone, Default)]
/// Typed Discord API object for `ButtonConfig`.
pub struct ButtonConfig {
    /// Discord API payload field `custom_id`.
    pub custom_id: String,
    /// Discord API payload field `label`.
    pub label: String,
    /// Discord API payload field `style`.
    pub style: u8,
    /// Discord API payload field `emoji`.
    pub emoji: Option<String>,
}

impl ButtonConfig {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str, label: &str) -> Self {
        Self {
            custom_id: custom_id.to_string(),
            label: label.to_string(),
            style: button_style::PRIMARY,
            emoji: None,
        }
    }

    /// Runs the `style` operation.
    pub fn style(mut self, style: u8) -> Self {
        self.style = style;
        self
    }

    /// Runs the `emoji` operation.
    pub fn emoji(mut self, emoji: &str) -> Self {
        self.emoji = Some(emoji.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        invalid_data_error, to_json_value, ButtonConfig, Emoji, MediaGalleryItem, SelectOption,
    };
    use crate::constants::button_style;
    use crate::error::DiscordError;

    #[test]
    fn helpers_preserve_model_error_shape() {
        let error = invalid_data_error("bad data");
        assert!(matches!(
            error,
            DiscordError::Model { message } if message == "bad data"
        ));

        let value = to_json_value(SelectOption::new("Alpha", "alpha"));
        assert_eq!(
            value,
            json!({
                "label": "Alpha",
                "value": "alpha"
            })
        );
    }

    #[test]
    fn emoji_builders_round_trip_through_serde() {
        let unicode = Emoji::unicode("?뵦");
        assert_eq!(unicode.name.as_deref(), Some("?뵦"));
        assert_eq!(unicode.id, None);
        assert_eq!(unicode.animated, None);
        assert_eq!(
            serde_json::to_value(&unicode).unwrap(),
            json!({ "name": "?뵦" })
        );

        let custom = Emoji::custom("party", "42", true);
        assert_eq!(custom.name.as_deref(), Some("party"));
        assert_eq!(custom.id.as_deref(), Some("42"));
        assert_eq!(custom.animated, Some(true));

        let serialized = serde_json::to_value(&custom).unwrap();
        assert_eq!(
            serialized,
            json!({
                "id": "42",
                "name": "party",
                "animated": true
            })
        );

        let round_trip: Emoji = serde_json::from_value(serialized).unwrap();
        assert_eq!(round_trip.id.as_deref(), Some("42"));
        assert_eq!(round_trip.name.as_deref(), Some("party"));
        assert_eq!(round_trip.animated, Some(true));
    }

    #[test]
    fn media_gallery_builder_sets_optional_fields_only_when_requested() {
        let item = MediaGalleryItem::new("https://cdn.example/image.png");
        assert_eq!(item.media.url, "https://cdn.example/image.png");
        assert_eq!(item.description, None);
        assert_eq!(item.spoiler, None);
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            json!({
                "media": {
                    "url": "https://cdn.example/image.png"
                }
            })
        );

        let detailed = item.clone().description("Preview").spoiler(true);
        assert_eq!(detailed.description.as_deref(), Some("Preview"));
        assert_eq!(detailed.spoiler, Some(true));
        assert_eq!(
            serde_json::to_value(&detailed).unwrap(),
            json!({
                "media": {
                    "url": "https://cdn.example/image.png"
                },
                "description": "Preview",
                "spoiler": true
            })
        );
    }

    #[test]
    fn select_option_builder_serializes_nested_unicode_emoji() {
        let option = SelectOption::new("Support", "support")
            .description("Open a support ticket")
            .emoji("?뵦")
            .default_selected(true);

        assert_eq!(option.label, "Support");
        assert_eq!(option.value, "support");
        assert_eq!(option.description.as_deref(), Some("Open a support ticket"));
        assert_eq!(option.default, Some(true));
        assert_eq!(
            option
                .emoji
                .as_ref()
                .and_then(|emoji| emoji.name.as_deref()),
            Some("?뵦")
        );

        let serialized = serde_json::to_value(&option).unwrap();
        assert_eq!(
            serialized,
            json!({
                "label": "Support",
                "value": "support",
                "description": "Open a support ticket",
                "emoji": {
                    "name": "?뵦"
                },
                "default": true
            })
        );
    }

    #[test]
    fn defaults_and_button_builder_behaviors_are_stable() {
        let emoji = Emoji::default();
        assert_eq!(emoji.id, None);
        assert_eq!(emoji.name, None);
        assert_eq!(emoji.animated, None);

        let media = MediaGalleryItem::default();
        assert_eq!(media.media.url, "");
        assert_eq!(media.description, None);
        assert_eq!(media.spoiler, None);

        let option = SelectOption::default();
        assert_eq!(option.label, "");
        assert_eq!(option.value, "");
        assert_eq!(option.description, None);
        assert!(option.emoji.is_none());
        assert_eq!(option.default, None);

        let button_default = ButtonConfig::default();
        assert_eq!(button_default.custom_id, "");
        assert_eq!(button_default.label, "");
        assert_eq!(button_default.style, 0);
        assert_eq!(button_default.emoji, None);

        let button = ButtonConfig::new("open-ticket", "Open")
            .style(button_style::DANGER)
            .emoji("?썱");
        assert_eq!(button.custom_id, "open-ticket");
        assert_eq!(button.label, "Open");
        assert_eq!(button.style, button_style::DANGER);
        assert_eq!(button.emoji.as_deref(), Some("?썱"));

        let primary_button = ButtonConfig::new("primary", "Primary");
        assert_eq!(primary_button.style, button_style::PRIMARY);
    }

    #[test]
    fn select_option_omits_absent_optional_fields() {
        let serialized = serde_json::to_value(SelectOption::new("Alpha", "alpha")).unwrap();
        let object = serialized
            .as_object()
            .expect("select option should serialize to an object");

        assert_eq!(
            object.get("label"),
            Some(&Value::String(String::from("Alpha")))
        );
        assert_eq!(
            object.get("value"),
            Some(&Value::String(String::from("alpha")))
        );
        assert!(!object.contains_key("description"));
        assert!(!object.contains_key("emoji"));
        assert!(!object.contains_key("default"));
    }
}
