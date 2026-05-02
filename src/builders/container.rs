use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{component_type, separator_spacing};
use crate::types::{to_json_value, ButtonConfig, MediaGalleryItem};

use super::components::{ActionRowBuilder, ButtonBuilder};
use super::media::{FileBuilder, MediaGalleryBuilder, SectionBuilder};

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `TextDisplayBuilder`.
pub struct TextDisplayBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        create_container, create_default_buttons, ContainerBuilder, SeparatorBuilder,
        TextDisplayBuilder,
    };
    use crate::builders::{
        ActionRowBuilder, ButtonBuilder, FileBuilder, MediaGalleryBuilder, SectionBuilder,
    };
    use crate::constants::{button_style, component_type, separator_spacing};
    use crate::types::{ButtonConfig, MediaGalleryItem};

    #[test]
    fn text_display_builder_serializes_content_and_id() {
        let default_payload = TextDisplayBuilder::new("hello").build();
        assert_eq!(
            default_payload,
            json!({
                "type": component_type::TEXT_DISPLAY,
                "content": "hello",
            })
        );

        let payload = TextDisplayBuilder::new("hello")
            .content("updated")
            .id(42)
            .build();

        assert_eq!(
            payload,
            json!({
                "type": component_type::TEXT_DISPLAY,
                "content": "updated",
                "id": 42,
            })
        );
    }

    #[test]
    fn separator_builder_serializes_defaults_and_overrides() {
        let default_payload = SeparatorBuilder::new().build();
        assert_eq!(
            default_payload,
            json!({
                "type": component_type::SEPARATOR,
                "divider": true,
                "spacing": separator_spacing::SMALL,
            })
        );

        let payload = SeparatorBuilder::new()
            .divider(false)
            .spacing(separator_spacing::LARGE)
            .id(7)
            .build();

        assert_eq!(
            payload,
            json!({
                "type": component_type::SEPARATOR,
                "divider": false,
                "spacing": separator_spacing::LARGE,
                "id": 7,
            })
        );
    }

    #[test]
    fn container_builder_serializes_nested_components_and_options() {
        let payload = ContainerBuilder::new()
            .accent_color(0xFFAA11)
            .spoiler(true)
            .id(99)
            .add_media_gallery(
                MediaGalleryBuilder::new()
                    .id(1)
                    .add_item(MediaGalleryItem::new("https://example.com/a.png")),
            )
            .add_text_display(TextDisplayBuilder::new("headline").id(2))
            .add_separator(SeparatorBuilder::new().id(3))
            .add_action_row(
                ActionRowBuilder::new()
                    .id(4)
                    .add_button(ButtonBuilder::new().label("Open").custom_id("open")),
            )
            .add_section(
                SectionBuilder::new()
                    .id(5)
                    .add_text_display(TextDisplayBuilder::new("body")),
            )
            .add_file(FileBuilder::new("https://example.com/file.txt").id(6))
            .add_component(json!({"type": 255, "name": "raw"}))
            .build();

        let components = payload
            .get("components")
            .and_then(|value| value.as_array())
            .expect("container components");

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::CONTAINER as u64)
        );
        assert_eq!(
            payload.get("accent_color").and_then(|value| value.as_u64()),
            Some(0xFFAA11)
        );
        assert_eq!(
            payload.get("spoiler").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(99));
        assert_eq!(components.len(), 7);
        assert_eq!(
            components
                .iter()
                .map(|component| {
                    component
                        .get("type")
                        .and_then(|value| value.as_u64())
                        .expect("component type")
                })
                .collect::<Vec<_>>(),
            vec![
                component_type::MEDIA_GALLERY as u64,
                component_type::TEXT_DISPLAY as u64,
                component_type::SEPARATOR as u64,
                component_type::ACTION_ROW as u64,
                component_type::SECTION as u64,
                component_type::FILE as u64,
                255,
            ]
        );
    }

    #[test]
    fn create_container_assembles_image_description_and_chunked_buttons() {
        let buttons = vec![
            ButtonConfig::new("first", "First")
                .style(button_style::PRIMARY)
                .emoji("A"),
            ButtonConfig::new("second", "Second").style(button_style::SECONDARY),
            ButtonConfig::new("third", "Third").style(button_style::SUCCESS),
            ButtonConfig::new("fourth", "Fourth").style(button_style::DANGER),
            ButtonConfig::new("fifth", "Fifth").style(button_style::PRIMARY),
            ButtonConfig::new("sixth", "Sixth").style(button_style::SECONDARY),
        ];

        let payload = create_container(
            "Status",
            "Everything is fine",
            buttons,
            Some("https://example.com/image.png"),
        )
        .build();

        let components = payload
            .get("components")
            .and_then(|value| value.as_array())
            .expect("container components");

        assert_eq!(components.len(), 8);
        assert_eq!(
            components
                .iter()
                .map(|component| component.get("type").and_then(|value| value.as_u64()))
                .collect::<Vec<_>>(),
            vec![
                Some(component_type::MEDIA_GALLERY as u64),
                Some(component_type::SEPARATOR as u64),
                Some(component_type::TEXT_DISPLAY as u64),
                Some(component_type::SEPARATOR as u64),
                Some(component_type::TEXT_DISPLAY as u64),
                Some(component_type::SEPARATOR as u64),
                Some(component_type::ACTION_ROW as u64),
                Some(component_type::ACTION_ROW as u64),
            ]
        );
        assert_eq!(
            components[0]
                .get("items")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|item| item.get("media"))
                .and_then(|media| media.get("url"))
                .and_then(|url| url.as_str()),
            Some("https://example.com/image.png")
        );
        assert_eq!(
            components[2]
                .get("content")
                .and_then(|value| value.as_str()),
            Some("**Status**")
        );
        assert_eq!(
            components[4]
                .get("content")
                .and_then(|value| value.as_str()),
            Some("Everything is fine")
        );
        assert_eq!(
            components[5]
                .get("divider")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            components[6]
                .get("components")
                .and_then(|value| value.as_array())
                .map(|buttons| buttons.len()),
            Some(5)
        );
        assert_eq!(
            components[7]
                .get("components")
                .and_then(|value| value.as_array())
                .map(|buttons| buttons.len()),
            Some(1)
        );
        assert_eq!(
            components[6]
                .get("components")
                .and_then(|value| value.as_array())
                .and_then(|buttons| buttons.first())
                .and_then(|button| button.get("emoji"))
                .and_then(|emoji| emoji.get("name"))
                .and_then(|name| name.as_str()),
            Some("A")
        );
    }

    #[test]
    fn create_default_buttons_returns_expected_configs() {
        let general = create_default_buttons("general");
        assert_eq!(general.len(), 1);
        assert_eq!(general[0].custom_id, "help_menu");
        assert_eq!(general[0].label, "Help");
        assert_eq!(general[0].style, button_style::SECONDARY);
        assert!(general[0].emoji.is_some());

        let status = create_default_buttons("status");
        assert_eq!(status.len(), 2);
        assert_eq!(status[0].custom_id, "view_work_status");
        assert_eq!(status[0].style, button_style::PRIMARY);
        assert!(status[0].emoji.is_some());
        assert_eq!(status[1].custom_id, "help_menu");
        assert_eq!(status[1].style, button_style::SECONDARY);

        let fallback = create_default_buttons("unknown");
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].custom_id, "help_menu");
        assert_eq!(fallback[0].style, button_style::SECONDARY);
        assert!(fallback[0].emoji.is_some());
    }
}

impl TextDisplayBuilder {
    /// Creates a `new` value.
    pub fn new(content: &str) -> Self {
        Self {
            component_type: component_type::TEXT_DISPLAY,
            content: content.to_string(),
            id: None,
        }
    }

    pub fn content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SeparatorBuilder`.
pub struct SeparatorBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    divider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spacing: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl SeparatorBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::SEPARATOR,
            divider: Some(true),
            spacing: Some(separator_spacing::SMALL),
            id: None,
        }
    }

    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = Some(divider);
        self
    }

    pub fn spacing(mut self, spacing: u8) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ContainerBuilder`.
pub struct ContainerBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    components: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent_color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spoiler: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl ContainerBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::CONTAINER,
            components: Vec::new(),
            accent_color: None,
            spoiler: None,
            id: None,
        }
    }

    pub fn accent_color(mut self, color: u32) -> Self {
        self.accent_color = Some(color);
        self
    }

    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn add_media_gallery(mut self, gallery: MediaGalleryBuilder) -> Self {
        self.components.push(gallery.build());
        self
    }

    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.components.push(text.build());
        self
    }

    pub fn add_separator(mut self, separator: SeparatorBuilder) -> Self {
        self.components.push(separator.build());
        self
    }

    pub fn add_action_row(mut self, row: ActionRowBuilder) -> Self {
        self.components.push(row.build());
        self
    }

    pub fn add_section(mut self, section: SectionBuilder) -> Self {
        self.components.push(section.build());
        self
    }

    pub fn add_file(mut self, file: FileBuilder) -> Self {
        self.components.push(file.build());
        self
    }

    pub fn add_component(mut self, component: Value) -> Self {
        self.components.push(component);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

/// Provides the `create_container` helper.
pub fn create_container(
    title: &str,
    description: &str,
    buttons: Vec<ButtonConfig>,
    image_url: Option<&str>,
) -> ContainerBuilder {
    let mut container = ContainerBuilder::new();

    if let Some(url) = image_url {
        let gallery = MediaGalleryBuilder::new().add_item(MediaGalleryItem::new(url));
        container = container.add_media_gallery(gallery);

        container = container.add_separator(
            SeparatorBuilder::new()
                .divider(true)
                .spacing(separator_spacing::LARGE),
        );
    }

    container = container.add_text_display(TextDisplayBuilder::new(&format!("**{}**", title)));

    if !description.is_empty() {
        container = container.add_separator(
            SeparatorBuilder::new()
                .divider(true)
                .spacing(separator_spacing::SMALL),
        );
        container = container.add_text_display(TextDisplayBuilder::new(description));
    }

    if !buttons.is_empty() {
        container = container.add_separator(
            SeparatorBuilder::new()
                .divider(false)
                .spacing(separator_spacing::SMALL),
        );

        for chunk in buttons.chunks(5) {
            let mut row = ActionRowBuilder::new();
            for btn_config in chunk {
                let mut button = ButtonBuilder::new()
                    .label(&btn_config.label)
                    .style(btn_config.style)
                    .custom_id(&btn_config.custom_id);

                if let Some(ref emoji) = btn_config.emoji {
                    button = button.emoji_unicode(emoji);
                }

                row = row.add_button(button);
            }
            container = container.add_action_row(row);
        }
    }

    container
}

/// Provides the `create_default_buttons` helper.
pub fn create_default_buttons(button_type: &str) -> Vec<ButtonConfig> {
    match button_type {
        "general" => vec![ButtonConfig::new("help_menu", "Help")
            .style(crate::constants::button_style::SECONDARY)
            .emoji("❓")],
        "status" => vec![
            ButtonConfig::new("view_work_status", "Work Status")
                .style(crate::constants::button_style::PRIMARY)
                .emoji("📊"),
            ButtonConfig::new("help_menu", "Help")
                .style(crate::constants::button_style::SECONDARY)
                .emoji("❓"),
        ],
        _ => vec![ButtonConfig::new("help_menu", "Help")
            .style(crate::constants::button_style::SECONDARY)
            .emoji("❓")],
    }
}
