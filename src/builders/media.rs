use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::command::validation;
use crate::constants::component_type;
use crate::error::DiscordError;
use crate::types::{to_json_value, MediaGalleryItem, MediaInfo};

use super::components::ButtonBuilder;
use super::container::TextDisplayBuilder;

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MediaGalleryBuilder`.
pub struct MediaGalleryBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    items: Vec<MediaGalleryItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl MediaGalleryBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::MEDIA_GALLERY,
            items: Vec::new(),
            id: None,
        }
    }

    pub fn add_item(mut self, item: MediaGalleryItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn add_items(mut self, items: Vec<MediaGalleryItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the gallery against Discord's limits without consuming the
    /// builder: 1-10 items, item descriptions at most 1024 characters.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_count("media gallery items", self.items.len(), 1, 10)?;
        for (index, item) in self.items.iter().enumerate() {
            if let Some(description) = &item.description {
                validation::ensure_max_len(
                    &format!("media gallery item {index} description"),
                    description,
                    1024,
                )?;
            }
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThumbnailBuilder`.
pub struct ThumbnailBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    media: MediaInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spoiler: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl ThumbnailBuilder {
    /// Creates a `new` value.
    pub fn new(url: &str) -> Self {
        Self {
            component_type: component_type::THUMBNAIL,
            media: MediaInfo {
                url: url.to_string(),
            },
            description: None,
            spoiler: None,
            id: None,
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
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

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the thumbnail against Discord's limits without consuming
    /// the builder: description at most 1024 characters.
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(description) = &self.description {
            validation::ensure_max_len("thumbnail description", description, 1024)?;
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `FileBuilder`.
pub struct FileBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    file: MediaInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    spoiler: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl FileBuilder {
    /// Creates a `new` value.
    pub fn new(url: &str) -> Self {
        Self {
            component_type: component_type::FILE,
            file: MediaInfo {
                url: url.to_string(),
            },
            spoiler: None,
            id: None,
        }
    }

    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
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
/// Typed Discord API object for `SectionBuilder`.
pub struct SectionBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    components: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accessory: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl SectionBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::SECTION,
            components: Vec::new(),
            accessory: None,
            id: None,
        }
    }

    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.components.push(text.build());
        self
    }

    pub fn set_thumbnail_accessory(mut self, thumbnail: ThumbnailBuilder) -> Self {
        self.accessory = Some(thumbnail.build());
        self
    }

    pub fn set_button_accessory(mut self, button: ButtonBuilder) -> Self {
        self.accessory = Some(button.build());
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the section against Discord's limits without consuming the
    /// builder: 1-3 child components and a required accessory.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_count("section components", self.components.len(), 1, 3)?;
        if self.accessory.is_none() {
            return Err(DiscordError::model(
                "section accessory is required (set a thumbnail or button accessory)",
            ));
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{FileBuilder, MediaGalleryBuilder, SectionBuilder, ThumbnailBuilder};
    use crate::builders::{ButtonBuilder, TextDisplayBuilder};
    use crate::constants::{button_style, component_type};
    use crate::types::MediaGalleryItem;

    #[test]
    fn media_gallery_builder_serializes_items_and_id() {
        let payload = MediaGalleryBuilder::new()
            .add_item(MediaGalleryItem::new("https://example.com/one.png"))
            .add_items(vec![
                MediaGalleryItem::new("https://example.com/two.png").description("second"),
                MediaGalleryItem::new("https://example.com/three.png").spoiler(true),
            ])
            .id(7)
            .build();

        let items = payload
            .get("items")
            .and_then(|value| value.as_array())
            .expect("gallery items");

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::MEDIA_GALLERY as u64)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(7));
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[1].get("description").and_then(|value| value.as_str()),
            Some("second")
        );
        assert_eq!(
            items[2].get("spoiler").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn thumbnail_builder_serializes_optional_fields() {
        let default_payload = ThumbnailBuilder::new("https://example.com/thumb.png").build();
        assert_eq!(
            default_payload,
            json!({
                "type": component_type::THUMBNAIL,
                "media": {"url": "https://example.com/thumb.png"},
            })
        );

        let payload = ThumbnailBuilder::new("https://example.com/thumb.png")
            .description("preview")
            .spoiler(true)
            .id(3)
            .build();

        assert_eq!(
            payload,
            json!({
                "type": component_type::THUMBNAIL,
                "media": {"url": "https://example.com/thumb.png"},
                "description": "preview",
                "spoiler": true,
                "id": 3,
            })
        );
    }

    #[test]
    fn file_builder_serializes_optional_fields() {
        let default_payload = FileBuilder::new("https://example.com/file.txt").build();
        assert_eq!(
            default_payload,
            json!({
                "type": component_type::FILE,
                "file": {"url": "https://example.com/file.txt"},
            })
        );

        let payload = FileBuilder::new("https://example.com/file.txt")
            .spoiler(true)
            .id(11)
            .build();

        assert_eq!(
            payload,
            json!({
                "type": component_type::FILE,
                "file": {"url": "https://example.com/file.txt"},
                "spoiler": true,
                "id": 11,
            })
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
    fn media_gallery_try_build_matches_build_and_rejects_invalid_galleries() {
        let make = || {
            MediaGalleryBuilder::new().add_item(MediaGalleryItem::new("https://example.com/a.png"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid gallery"));

        expect_model_error(
            MediaGalleryBuilder::new().try_build(),
            "media gallery items must contain 1-10 items, got 0",
        );

        let mut overfull = MediaGalleryBuilder::new();
        for _ in 0..11 {
            overfull = overfull.add_item(MediaGalleryItem::new("https://example.com/a.png"));
        }
        expect_model_error(
            overfull.try_build(),
            "media gallery items must contain 1-10 items, got 11",
        );

        expect_model_error(
            MediaGalleryBuilder::new()
                .add_item(
                    MediaGalleryItem::new("https://example.com/a.png")
                        .description(&"d".repeat(1025)),
                )
                .try_build(),
            "media gallery item 0 description must be at most 1024 characters, got 1025",
        );
    }

    #[test]
    fn thumbnail_try_build_matches_build_and_rejects_long_description() {
        let make = || ThumbnailBuilder::new("https://example.com/thumb.png").description("ok");
        assert_eq!(make().build(), make().try_build().expect("valid thumbnail"));

        expect_model_error(
            ThumbnailBuilder::new("https://example.com/thumb.png")
                .description(&"d".repeat(1025))
                .try_build(),
            "thumbnail description must be at most 1024 characters, got 1025",
        );
    }

    #[test]
    fn section_try_build_matches_build_and_rejects_invalid_sections() {
        let make = || {
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("body"))
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid section"));

        expect_model_error(
            SectionBuilder::new()
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
                .try_build(),
            "section components must contain 1-3 items, got 0",
        );
        expect_model_error(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("one"))
                .add_text_display(TextDisplayBuilder::new("two"))
                .add_text_display(TextDisplayBuilder::new("three"))
                .add_text_display(TextDisplayBuilder::new("four"))
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
                .try_build(),
            "section components must contain 1-3 items, got 4",
        );
        expect_model_error(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("body"))
                .try_build(),
            "section accessory is required",
        );
    }

    #[test]
    fn section_builder_serializes_text_and_thumbnail_accessory() {
        let payload = SectionBuilder::new()
            .add_text_display(TextDisplayBuilder::new("title"))
            .set_thumbnail_accessory(
                ThumbnailBuilder::new("https://example.com/thumb.png")
                    .description("preview")
                    .id(8),
            )
            .id(4)
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::SECTION as u64)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(4));
        assert_eq!(
            payload
                .get("components")
                .and_then(|value| value.as_array())
                .map(|components| components.len()),
            Some(1)
        );
        assert_eq!(
            payload
                .get("accessory")
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_u64()),
            Some(component_type::THUMBNAIL as u64)
        );
    }

    #[test]
    fn section_builder_can_replace_accessory_with_button() {
        let payload = SectionBuilder::new()
            .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/thumb.png"))
            .set_button_accessory(
                ButtonBuilder::new()
                    .label("Open")
                    .style(button_style::SECONDARY)
                    .custom_id("open"),
            )
            .build();

        assert_eq!(
            payload
                .get("accessory")
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_u64()),
            Some(component_type::BUTTON as u64)
        );
        assert_eq!(
            payload
                .get("accessory")
                .and_then(|value| value.get("custom_id"))
                .and_then(|value| value.as_str()),
            Some("open")
        );
    }
}
