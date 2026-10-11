//! Message components (legacy and Components V2) and their parts.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{ComponentId, GenericId};

/// A single component attached to a received message, modal, or snapshot.
///
/// Discord components form a tree (action rows, sections, containers, and
/// labels nest child components). Rather than one struct per component
/// type, this is deliberately a *wide* struct: [`Self::kind`] carries the
/// raw component `type` and every known field across all component kinds
/// is optional (or defaults to empty). The tradeoff: the compiler cannot
/// guarantee which fields are populated for a given `kind`, but decoding
/// never fails when Discord ships new component types or fields, and one
/// type covers the whole tree recursively.
///
/// Component `type` values are exposed as associated constants, e.g.
/// [`MessageComponent::BUTTON`].
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct MessageComponent {
    /// Raw Discord component type (see the associated constants).
    #[serde(rename = "type", default)]
    pub kind: u8,
    /// Numeric component ID (32-bit, unique within the message). Not a
    /// snowflake; see [`ComponentId`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<ComponentId>,
    /// Developer-defined identifier (buttons, selects, text inputs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Button or text-input style value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<u64>,
    /// Button label or label-component text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Description for label and thumbnail components.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Emoji shown on a button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    /// URL for link-style buttons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// SKU targeted by premium-style buttons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<GenericId>,
    /// Whether the interactive component is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// Whether the component must be filled in a modal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Choices of a string select menu.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<MessageSelectOption>,
    /// Channel types a channel select menu is limited to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_types: Vec<u8>,
    /// Placeholder text for selects and text inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Pre-selected values of auto-populated select menus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_values: Vec<MessageSelectDefaultValue>,
    /// Minimum number of select values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u64>,
    /// Maximum number of select values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u64>,
    /// Minimum text-input length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,
    /// Maximum text-input length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    /// Submitted or pre-filled text-input value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Child components (action rows, sections, containers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<MessageComponent>,
    /// Single child component of a label component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<Box<MessageComponent>>,
    /// Accessory (thumbnail or button) of a section component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<Box<MessageComponent>>,
    /// Markdown content of a text display component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Media of a thumbnail component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<UnfurledMediaItem>,
    /// Media of a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<UnfurledMediaItem>,
    /// File name reported for a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// File size in bytes reported for a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Items of a media gallery component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<MessageMediaGalleryItem>,
    /// Whether the media or container is a spoiler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
    /// Whether a separator renders a visual divider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divider: Option<bool>,
    /// Separator spacing size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<u64>,
    /// Container accent color (RGB integer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<u64>,
}

impl MessageComponent {
    /// Component type for action rows.
    pub const ACTION_ROW: u8 = 1;
    /// Component type for buttons.
    pub const BUTTON: u8 = 2;
    /// Component type for string select menus.
    pub const STRING_SELECT: u8 = 3;
    /// Component type for text inputs.
    pub const TEXT_INPUT: u8 = 4;
    /// Component type for user select menus.
    pub const USER_SELECT: u8 = 5;
    /// Component type for role select menus.
    pub const ROLE_SELECT: u8 = 6;
    /// Component type for mentionable select menus.
    pub const MENTIONABLE_SELECT: u8 = 7;
    /// Component type for channel select menus.
    pub const CHANNEL_SELECT: u8 = 8;
    /// Component type for sections.
    pub const SECTION: u8 = 9;
    /// Component type for text displays.
    pub const TEXT_DISPLAY: u8 = 10;
    /// Component type for thumbnails.
    pub const THUMBNAIL: u8 = 11;
    /// Component type for media galleries.
    pub const MEDIA_GALLERY: u8 = 12;
    /// Component type for file attachments.
    pub const FILE: u8 = 13;
    /// Component type for separators.
    pub const SEPARATOR: u8 = 14;
    /// Component type for containers.
    pub const CONTAINER: u8 = 17;
    /// Component type for labels.
    pub const LABEL: u8 = 18;

    /// Depth-first iterator over this component and every nested child
    /// (child `components`, label `component`, and section `accessory`).
    pub fn iter(&self) -> MessageComponentIter<'_> {
        MessageComponentIter { stack: vec![self] }
    }
}

/// Depth-first iterator returned by [`MessageComponent::iter`].
pub struct MessageComponentIter<'a> {
    stack: Vec<&'a MessageComponent>,
}

impl<'a> Iterator for MessageComponentIter<'a> {
    type Item = &'a MessageComponent;

    fn next(&mut self) -> Option<Self::Item> {
        let component = self.stack.pop()?;
        if let Some(accessory) = component.accessory.as_deref() {
            self.stack.push(accessory);
        }
        if let Some(child) = component.component.as_deref() {
            self.stack.push(child);
        }
        for child in component.components.iter().rev() {
            self.stack.push(child);
        }
        Some(component)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Option of a string select menu on a received message component.
pub struct MessageSelectOption {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Default value of an auto-populated select menu component.
pub struct MessageSelectDefaultValue {
    pub id: GenericId,
    /// Target type: `"user"`, `"role"`, or `"channel"`.
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Media reference attached to a Components V2 component.
pub struct UnfurledMediaItem {
    #[serde(default)]
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<GenericId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Single item of a media gallery component on a received message.
pub struct MessageMediaGalleryItem {
    #[serde(default)]
    pub media: UnfurledMediaItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
}
