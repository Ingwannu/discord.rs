use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::component_type;
use crate::types::{to_json_value, MediaGalleryItem, MediaInfo};

use super::components::ButtonBuilder;
use super::container::TextDisplayBuilder;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct MediaGalleryBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    items: Vec<MediaGalleryItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl MediaGalleryBuilder {
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
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
