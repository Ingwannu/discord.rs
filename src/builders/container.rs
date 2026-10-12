//! Text display, separator, and container builders, and the
//! [`create_container`] helper. Each builder holds its typed request model
//! ([`TextDisplay`], [`Separator`], [`Container`]).

use crate::error::DiscordError;
use crate::model::{ButtonStyle, ComponentId, SeparatorSpacing};
use crate::request::{
    ActionRow, Component, Container, ContainerComponent, SectionAccessory, Separator, TextDisplay,
};
use crate::types::{ButtonConfig, MediaGalleryItem};

use super::components::{ActionRowBuilder, CustomButtonBuilder};
use super::limits;
use super::media::{FileBuilder, MediaGalleryBuilder, SectionBuilder};

/// Builds a [`TextDisplay`] (type 10).
#[derive(Clone, Debug)]
pub struct TextDisplayBuilder {
    inner: TextDisplay,
}

impl TextDisplayBuilder {
    /// A text display with this Markdown content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            inner: TextDisplay::new(content),
        }
    }

    /// Replaces the content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.inner.content = content.into();
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The text display, without checking limits.
    pub fn build(self) -> TextDisplay {
        self.inner
    }

    /// Checks the content length (1-4000 characters).
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<TextDisplay, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

impl From<TextDisplayBuilder> for TextDisplay {
    fn from(builder: TextDisplayBuilder) -> Self {
        builder.build()
    }
}

/// Builds a [`Separator`] (type 14). Starts with a divider and small
/// spacing written out, as in 2.x.
#[derive(Clone, Debug)]
pub struct SeparatorBuilder {
    inner: Separator,
}

impl SeparatorBuilder {
    /// A separator with a divider and small spacing.
    pub fn new() -> Self {
        Self {
            inner: Separator {
                divider: Some(true),
                spacing: Some(SeparatorSpacing::SMALL),
                ..Separator::new()
            },
        }
    }

    /// Shows or hides the divider line.
    pub fn divider(mut self, divider: bool) -> Self {
        self.inner.divider = Some(divider);
        self
    }

    /// Sets the padding size.
    pub fn spacing(mut self, spacing: SeparatorSpacing) -> Self {
        self.inner.spacing = Some(spacing);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The separator, without checking limits.
    pub fn build(self) -> Separator {
        self.inner
    }

    /// Checks that the spacing is small (1) or large (2).
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Separator, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Same as [`SeparatorBuilder::new`].
impl Default for SeparatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a [`Container`] (type 17).
#[derive(Clone, Debug, Default)]
pub struct ContainerBuilder {
    inner: Container,
}

impl ContainerBuilder {
    /// An empty container; add at least one child.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the accent colour (`0x000000`-`0xFFFFFF`).
    pub fn accent_color(mut self, color: u32) -> Self {
        self.inner.accent_color = Some(color);
        self
    }

    /// Blurs the container as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.inner.spoiler = Some(spoiler);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Appends a media gallery.
    pub fn add_media_gallery(mut self, gallery: MediaGalleryBuilder) -> Self {
        self.inner.components.push(gallery.build().into());
        self
    }

    /// Appends a text display.
    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.inner.components.push(text.build().into());
        self
    }

    /// Appends a separator.
    pub fn add_separator(mut self, separator: SeparatorBuilder) -> Self {
        self.inner.components.push(separator.build().into());
        self
    }

    /// Appends an action row: an [`ActionRowBuilder`] of buttons or a
    /// [`SelectMenuBuilder`](super::SelectMenuBuilder).
    pub fn add_action_row(mut self, row: impl Into<ActionRow>) -> Self {
        self.inner
            .components
            .push(ContainerComponent::ActionRow(row.into()));
        self
    }

    /// Appends a section (one with an accessory set).
    pub fn add_section(mut self, section: SectionBuilder<SectionAccessory>) -> Self {
        self.inner.components.push(section.build().into());
        self
    }

    /// Appends a file component.
    pub fn add_file(mut self, file: FileBuilder) -> Self {
        self.inner.components.push(file.build().into());
        self
    }

    /// Appends any typed container child, including a
    /// [`RawComponent`](crate::request::RawComponent).
    pub fn add_component(mut self, component: impl Into<ContainerComponent>) -> Self {
        self.inner.components.push(component.into());
        self
    }

    /// The container, without checking limits.
    pub fn build(self) -> Container {
        self.inner
    }

    /// Checks the container and its children against Discord's limits: at
    /// least one child, at most 40 counting nested ones, accent colour up to
    /// `0xFFFFFF`, and every child's own limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        crate::request::check_message_components(
            std::slice::from_ref(&Component::Container(self.inner.clone())),
            crate::request::ComponentsMode::V2,
        )
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Container, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

impl From<ContainerBuilder> for Container {
    fn from(builder: ContainerBuilder) -> Self {
        builder.build()
    }
}

impl From<ContainerBuilder> for Component {
    fn from(builder: ContainerBuilder) -> Self {
        Self::Container(builder.build())
    }
}

impl From<TextDisplayBuilder> for Component {
    fn from(builder: TextDisplayBuilder) -> Self {
        Self::TextDisplay(builder.build())
    }
}

impl From<TextDisplayBuilder> for ContainerComponent {
    fn from(builder: TextDisplayBuilder) -> Self {
        Self::TextDisplay(builder.build())
    }
}

impl From<SeparatorBuilder> for Component {
    fn from(builder: SeparatorBuilder) -> Self {
        Self::Separator(builder.build())
    }
}

impl From<SeparatorBuilder> for ContainerComponent {
    fn from(builder: SeparatorBuilder) -> Self {
        Self::Separator(builder.build())
    }
}

/// A container with an optional image, a bold title, an optional
/// description, and the buttons in rows of five.
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
                .spacing(SeparatorSpacing::LARGE),
        );
    }

    container = container.add_text_display(TextDisplayBuilder::new(format!("**{}**", title)));

    if !description.is_empty() {
        container = container.add_separator(
            SeparatorBuilder::new()
                .divider(true)
                .spacing(SeparatorSpacing::SMALL),
        );
        container = container.add_text_display(TextDisplayBuilder::new(description));
    }

    if !buttons.is_empty() {
        container = container.add_separator(
            SeparatorBuilder::new()
                .divider(false)
                .spacing(SeparatorSpacing::SMALL),
        );

        for chunk in buttons.chunks(limits::MAX_ACTION_ROW_COMPONENTS) {
            let mut row = ActionRowBuilder::new();
            for btn_config in chunk {
                let mut button = CustomButtonBuilder::new(btn_config.custom_id.as_str())
                    .label(btn_config.label.as_str())
                    .style(btn_config.style);

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

/// The help and work-status button sets used by the bundled examples.
pub fn create_default_buttons(button_type: &str) -> Vec<ButtonConfig> {
    match button_type {
        "general" => vec![ButtonConfig::new("help_menu", "Help")
            .style(ButtonStyle::SECONDARY)
            .emoji("❓")],
        "status" => vec![
            ButtonConfig::new("view_work_status", "Work Status")
                .style(ButtonStyle::PRIMARY)
                .emoji("📊"),
            ButtonConfig::new("help_menu", "Help")
                .style(ButtonStyle::SECONDARY)
                .emoji("❓"),
        ],
        _ => vec![ButtonConfig::new("help_menu", "Help")
            .style(ButtonStyle::SECONDARY)
            .emoji("❓")],
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{ButtonStyle, ComponentId, ComponentType, SeparatorSpacing};
    use crate::request::RawComponent;
    use serde_json::{json, Value};

    use super::{
        create_container, create_default_buttons, ContainerBuilder, SeparatorBuilder,
        TextDisplayBuilder,
    };
    use crate::builders::{
        ActionRowBuilder, CustomButtonBuilder, FileBuilder, MediaGalleryBuilder, SectionBuilder,
        ThumbnailBuilder,
    };
    use crate::types::{ButtonConfig, MediaGalleryItem};

    fn to_json<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("serializable")
    }

    #[test]
    fn text_display_builder_serializes_content_and_id() {
        assert_eq!(
            to_json(&TextDisplayBuilder::new("hello").build()),
            json!({"type": 10, "content": "hello"})
        );
        let payload = TextDisplayBuilder::new("hello")
            .content("updated")
            .id(ComponentId::new(42))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({"type": 10, "id": 42, "content": "updated"})
        );
    }

    #[test]
    fn separator_builder_serializes_defaults_and_overrides() {
        assert_eq!(
            to_json(&SeparatorBuilder::new().build()),
            json!({"type": 14, "divider": true, "spacing": 1})
        );
        let payload = SeparatorBuilder::new()
            .divider(false)
            .spacing(SeparatorSpacing::LARGE)
            .id(ComponentId::new(7))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({"type": 14, "id": 7, "divider": false, "spacing": 2})
        );
        assert!(SeparatorBuilder::new()
            .spacing(SeparatorSpacing::new(3))
            .try_build()
            .unwrap_err()
            .to_string()
            .contains("separator spacing must be 1 (small) or 2 (large), got 3"));
    }

    #[test]
    fn container_builder_serializes_nested_components_and_options() {
        let payload = ContainerBuilder::new()
            .accent_color(0xFFAA11)
            .spoiler(true)
            .id(ComponentId::new(99))
            .add_media_gallery(
                MediaGalleryBuilder::new()
                    .id(ComponentId::new(1))
                    .add_item(MediaGalleryItem::new("https://example.com/a.png")),
            )
            .add_text_display(TextDisplayBuilder::new("headline").id(ComponentId::new(2)))
            .add_separator(SeparatorBuilder::new().id(ComponentId::new(3)))
            .add_action_row(
                ActionRowBuilder::new()
                    .id(ComponentId::new(4))
                    .add_button(CustomButtonBuilder::new("open").label("Open")),
            )
            .add_section(
                SectionBuilder::new()
                    .id(ComponentId::new(5))
                    .add_text_display(TextDisplayBuilder::new("body"))
                    .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png")),
            )
            .add_file(FileBuilder::new("attachment://file.txt").id(ComponentId::new(6)))
            .add_component(RawComponent::from_json(r#"{"type":255,"name":"raw"}"#).unwrap())
            .build();
        let payload = to_json(&payload);

        assert_eq!(payload["type"], json!(17));
        assert_eq!(payload["accent_color"], json!(0xFFAA11));
        assert_eq!(payload["spoiler"], json!(true));
        assert_eq!(payload["id"], json!(99));
        assert_eq!(
            payload["components"]
                .as_array()
                .unwrap()
                .iter()
                .map(|component| component["type"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![
                u64::from(ComponentType::MEDIA_GALLERY.get()),
                u64::from(ComponentType::TEXT_DISPLAY.get()),
                u64::from(ComponentType::SEPARATOR.get()),
                u64::from(ComponentType::ACTION_ROW.get()),
                u64::from(ComponentType::SECTION.get()),
                u64::from(ComponentType::FILE.get()),
                255,
            ]
        );
    }

    fn expect_model_error<T: std::fmt::Debug>(
        result: Result<T, crate::error::DiscordError>,
        needle: &str,
    ) {
        let err = result.expect_err("expected validation failure").to_string();
        assert!(
            err.contains(needle),
            "error message {err:?} should contain {needle:?}"
        );
    }

    #[test]
    fn text_display_try_build_matches_build_and_rejects_invalid_content() {
        let make = || TextDisplayBuilder::new("hello").id(ComponentId::new(42));
        assert_eq!(
            make().build(),
            make().try_build().expect("valid text display")
        );
        expect_model_error(
            TextDisplayBuilder::new("").try_build(),
            "text display content must be 1-4000 characters, got 0",
        );
        expect_model_error(
            TextDisplayBuilder::new("c".repeat(4001)).try_build(),
            "text display content must be 1-4000 characters, got 4001",
        );
        // 4000 multibyte codepoints (8000 bytes) must pass: Discord counts codepoints.
        TextDisplayBuilder::new("é".repeat(4000))
            .validate()
            .expect("codepoint-length content should be valid");
    }

    #[test]
    fn container_try_build_matches_build_and_rejects_invalid_containers() {
        let make = || ContainerBuilder::new().add_text_display(TextDisplayBuilder::new("hi"));
        assert_eq!(make().build(), make().try_build().expect("valid container"));

        expect_model_error(
            ContainerBuilder::new().try_build(),
            "container components must contain 1-40 items, got 0",
        );
        let mut overfull = ContainerBuilder::new();
        for _ in 0..41 {
            overfull = overfull.add_text_display(TextDisplayBuilder::new("x"));
        }
        expect_model_error(
            overfull.try_build(),
            "container components must contain 1-40 items, got 41",
        );
        expect_model_error(
            ContainerBuilder::new()
                .accent_color(0x0100_0000)
                .add_text_display(TextDisplayBuilder::new("hi"))
                .try_build(),
            "container accent_color must be at most 0xFFFFFF",
        );
        // Children are checked recursively.
        expect_model_error(
            ContainerBuilder::new()
                .add_text_display(TextDisplayBuilder::new(""))
                .try_build(),
            "components[0].components[0]: text display content must be 1-4000 characters",
        );
    }

    #[test]
    fn create_container_assembles_image_description_and_chunked_buttons() {
        let buttons = vec![
            ButtonConfig::new("first", "First")
                .style(ButtonStyle::PRIMARY)
                .emoji("A"),
            ButtonConfig::new("second", "Second").style(ButtonStyle::SECONDARY),
            ButtonConfig::new("third", "Third").style(ButtonStyle::SUCCESS),
            ButtonConfig::new("fourth", "Fourth").style(ButtonStyle::DANGER),
            ButtonConfig::new("fifth", "Fifth").style(ButtonStyle::PRIMARY),
            ButtonConfig::new("sixth", "Sixth").style(ButtonStyle::SECONDARY),
        ];

        let container = create_container(
            "Status",
            "Everything is fine",
            buttons,
            Some("https://example.com/image.png"),
        );
        container.validate().expect("helper output is valid");
        let payload = to_json(&container.build());
        let components = payload["components"].as_array().unwrap();

        assert_eq!(components.len(), 8);
        assert_eq!(
            components
                .iter()
                .map(|component| component["type"].as_u64())
                .collect::<Vec<_>>(),
            vec![
                Some(u64::from(ComponentType::MEDIA_GALLERY.get())),
                Some(u64::from(ComponentType::SEPARATOR.get())),
                Some(u64::from(ComponentType::TEXT_DISPLAY.get())),
                Some(u64::from(ComponentType::SEPARATOR.get())),
                Some(u64::from(ComponentType::TEXT_DISPLAY.get())),
                Some(u64::from(ComponentType::SEPARATOR.get())),
                Some(u64::from(ComponentType::ACTION_ROW.get())),
                Some(u64::from(ComponentType::ACTION_ROW.get())),
            ]
        );
        assert_eq!(
            components[0]["items"][0]["media"]["url"],
            json!("https://example.com/image.png")
        );
        assert_eq!(components[2]["content"], json!("**Status**"));
        assert_eq!(components[4]["content"], json!("Everything is fine"));
        assert_eq!(components[5]["divider"], json!(false));
        assert_eq!(components[6]["components"].as_array().unwrap().len(), 5);
        assert_eq!(components[7]["components"].as_array().unwrap().len(), 1);
        assert_eq!(components[6]["components"][0]["emoji"]["name"], json!("A"));
    }

    #[test]
    fn create_default_buttons_returns_expected_configs() {
        let general = create_default_buttons("general");
        assert_eq!(general.len(), 1);
        assert_eq!(general[0].custom_id, "help_menu");
        assert_eq!(general[0].label, "Help");
        assert_eq!(general[0].style, ButtonStyle::SECONDARY);
        assert!(general[0].emoji.is_some());

        let status = create_default_buttons("status");
        assert_eq!(status.len(), 2);
        assert_eq!(status[0].custom_id, "view_work_status");
        assert_eq!(status[0].style, ButtonStyle::PRIMARY);
        assert!(status[0].emoji.is_some());
        assert_eq!(status[1].custom_id, "help_menu");
        assert_eq!(status[1].style, ButtonStyle::SECONDARY);

        let fallback = create_default_buttons("unknown");
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].custom_id, "help_menu");
        assert_eq!(fallback[0].style, ButtonStyle::SECONDARY);
        assert!(fallback[0].emoji.is_some());
    }
}
