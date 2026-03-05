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
