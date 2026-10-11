//! Component Embed generator: the `discord:component-embed` payload a website
//! serves so that Discord renders its link preview from a Components V2
//! layout.
//!
//! Source: discord-api-docs `developers/link-previews/component-embeds.mdx`
//! (PR #8606, changelog 2026-10-05), read at commit `3714862` (2026-10-09),
//! plus the per-component structures it links to in
//! `developers/components/reference.mdx`. Discord says link previews "are
//! subject to change", so every limit is a named constant on
//! [`ComponentEmbed`] citing its sentence, and nothing the spec does not state
//! is rejected.
//!
//! Why a separate model instead of `ContainerBuilder`: a preview accepts only
//! a subset of components, link buttons with a fixed key set, and no raw JSON.
//! Encoding those rules in the types (no custom-ID button, no nested
//! container, no `Value` escape hatch can be written) is cheaper than checking
//! them at runtime, and the conversion from the message builders is a
//! `TryFrom` that names what does not fit. The full decision log is in
//! docs/design/3.0-types-events-requests.md §4 ("Component Embed 생성").

use std::collections::HashSet;
use std::fmt;
use std::io;

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::builders::ContainerBuilder;
use crate::command::validation::char_count;
use crate::constants::{button_style, component_type, separator_spacing};
use crate::error::DiscordError;
use crate::types::Emoji;

/// A website's Component Embed: the JSON document Discord reads from the
/// page's `<script id="discord:component-embed">` element, or from the URL in
/// its `<link rel="discord:component-embed">` element, and renders as the
/// page's link preview instead of the Open Graph card.
///
/// The document is a single [`PreviewContainer`]; Discord allows no other
/// top-level component and no container inside it, and neither can be
/// expressed here. Keep the Open Graph tags on the page as well: Discord falls
/// back to them whenever it cannot show the component embed.
///
/// [`to_json`](Self::to_json) and [`to_script_tag`](Self::to_script_tag)
/// validate before they return output; [`ComponentEmbedBuilder::build`]
/// does not. The `Serialize` impl (for nesting the document in your own
/// JSON) is neither escaped nor validated, so never put its output in HTML.
///
/// # Example
///
/// ```
/// use discordrs::request::{
///     ComponentEmbed, PreviewActionRow, PreviewLinkButton, PreviewMediaGallery,
///     PreviewMediaItem, PreviewSection, PreviewSeparator,
/// };
///
/// let embed = ComponentEmbed::builder()
///     .accent_color(0x1A_BC9C)
///     .add_component(
///         PreviewSection::new(
///             PreviewLinkButton::new("https://example.com/posts/v2").label("Open"),
///         )
///         .add_text("# New version out!\nThe dungeon is <deceptively> tougher."),
///     )
///     .add_component(
///         PreviewMediaGallery::new()
///             .add_item(PreviewMediaItem::new("https://example.com/1.png").description("Entrance")),
///     )
///     .add_component(PreviewSeparator::new())
///     .add_component(
///         PreviewActionRow::new()
///             .add_button(PreviewLinkButton::new("https://example.com/store").label("Store")),
///     )
///     .try_build()?;
///
/// // Server-render this into the page's <head>.
/// let tag = embed.to_script_tag()?;
/// assert!(tag.starts_with(r#"<script id="discord:component-embed" type="application/json">"#));
/// // `<` and `>` inside text are JSON escapes, so they cannot close the element.
/// assert!(tag.contains(r"\u003cdeceptively\u003e"));
///
/// // Or serve the JSON from a URL on the same site and link to it.
/// let json = embed.to_json()?;
/// assert!(json.len() <= ComponentEmbed::MAX_PAYLOAD_BYTES);
/// let link = ComponentEmbed::link_tag("https://example.com/embeds/v2.json")?;
/// assert!(link.starts_with(r#"<link rel="discord:component-embed""#));
/// # Ok::<(), discordrs::request::ComponentEmbedError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComponentEmbed {
    /// The single parent container.
    pub component: PreviewContainer,
}

impl ComponentEmbed {
    /// Size limit of the document in bytes, measured on the exact JSON text
    /// Discord reads, escape sequences at their full length. "For both
    /// options, there is a 3,000-byte limit. Escape sequences like `\u003c`
    /// also count toward it at their full length." (Adding the Payload)
    pub const MAX_PAYLOAD_BYTES: usize = 3_000;
    /// Components in one embed, the container included. "A component embed
    /// can contain up to 40 components, including the parent Container
    /// component." (Compatible Components)
    pub const MAX_COMPONENTS: usize = 40;
    /// Media gallery items across the whole embed. "Media Gallery items are
    /// capped at 10 across the entire embed, not per component. Thumbnails
    /// don't count toward this limit." (Compatible Components)
    pub const MAX_GALLERY_ITEMS: usize = 10;
    /// Items in one media gallery: 1-10 (components reference, Media Gallery
    /// Structure `items`).
    pub const MAX_ITEMS_PER_GALLERY: usize = 10;
    /// Thumbnail and gallery media URLs. "The URL must use `http` or `https`
    /// and be a max of 2,048 characters in length." (Media)
    pub const MAX_MEDIA_URL_LENGTH: usize = 2_048;
    /// Thumbnail and gallery item descriptions (alt text): at most 1,024
    /// characters (components reference, Thumbnail and Media Gallery Item
    /// Structure `description`).
    pub const MAX_MEDIA_DESCRIPTION_LENGTH: usize = 1_024;
    /// Link button URLs: at most 512 characters (components reference,
    /// Button Structure `url`).
    pub const MAX_BUTTON_URL_LENGTH: usize = 512;
    /// Button labels: at most 80 characters (components reference, Button
    /// Structure `label`).
    pub const MAX_BUTTON_LABEL_LENGTH: usize = 80;
    /// Buttons in one action row: 1-5 (components reference, Action Row
    /// Structure "Up to 5 interactive button components").
    pub const MAX_ACTION_ROW_BUTTONS: usize = 5;
    /// Text displays in one section: 1-3 (components reference, Section
    /// Structure "One to three child components").
    pub const MAX_SECTION_TEXTS: usize = 3;
    /// Largest container accent colour, "RGB from `0x000000` to `0xFFFFFF`"
    /// (components reference, Container Structure `accent_color`).
    pub const MAX_ACCENT_COLOR: u32 = 0xFF_FFFF;
    /// `id` of the inline `<script>` element and `rel` of the `<link>`
    /// element.
    pub const ELEMENT_NAME: &'static str = "discord:component-embed";

    /// Wraps a container as a component embed.
    pub fn new(component: PreviewContainer) -> Self {
        Self { component }
    }

    /// Starts a [`ComponentEmbedBuilder`].
    pub fn builder() -> ComponentEmbedBuilder {
        ComponentEmbedBuilder::new()
    }

    /// Checks every documented limit, including the 3,000-byte size of the
    /// escaped JSON that [`Self::to_json`] returns. Fails with the first
    /// violated rule.
    pub fn validate(&self) -> Result<(), ComponentEmbedError> {
        self.to_json().map(drop)
    }

    /// Validates the embed and returns its JSON document, compact and with
    /// `<`, `>`, `&`, U+2028, and U+2029 written as `\uXXXX` escapes. The
    /// same text is safe to serve as the linked JSON file and to inline in a
    /// `<script>` element (also in XHTML), and its byte length is what the
    /// 3,000-byte limit was checked against. Other non-ASCII text stays as
    /// UTF-8, so serve the page as UTF-8.
    pub fn to_json(&self) -> Result<String, ComponentEmbedError> {
        self.check_structure()?;
        let json = self.encode()?;
        if json.len() > Self::MAX_PAYLOAD_BYTES {
            return Err(ComponentEmbedError::PayloadTooLarge {
                bytes: json.len(),
                max: Self::MAX_PAYLOAD_BYTES,
            });
        }
        Ok(json)
    }

    /// Validates the embed and returns the inline element for the page's
    /// `<head>`:
    /// `<script id="discord:component-embed" type="application/json">…</script>`
    /// around the output of [`Self::to_json`], with no whitespace inside the
    /// element, so the element's text is exactly the measured document.
    pub fn to_script_tag(&self) -> Result<String, ComponentEmbedError> {
        let json = self.to_json()?;
        Ok(format!(
            "<script id=\"{}\" type=\"application/json\">{json}</script>",
            Self::ELEMENT_NAME
        ))
    }

    /// Returns the `<link rel="discord:component-embed" type="application/json"
    /// href="…">` element that points Discord at a JSON document served
    /// separately (the output of [`Self::to_json`]).
    ///
    /// Discord requires an absolute URL on the page's host, a subdomain of it,
    /// or its parent domain. Only "absolute `http`/`https` URL" can be checked
    /// here; the host relation is up to the caller. The href is
    /// attribute-escaped.
    pub fn link_tag(href: &str) -> Result<String, ComponentEmbedError> {
        if !is_absolute_http_url(href) {
            return Err(ComponentEmbedError::InvalidLinkHref {
                href: href.to_owned(),
            });
        }
        Ok(format!(
            "<link rel=\"{}\" type=\"application/json\" href=\"{}\">",
            Self::ELEMENT_NAME,
            escape_attribute(href)
        ))
    }

    /// Serializes with [`ScriptSafeFormatter`], so the bytes produced here
    /// are the bytes that are measured and served.
    fn encode(&self) -> Result<String, ComponentEmbedError> {
        let mut out = Vec::with_capacity(Self::MAX_PAYLOAD_BYTES);
        let mut serializer = serde_json::Serializer::with_formatter(&mut out, ScriptSafeFormatter);
        self.serialize(&mut serializer)
            .map_err(|error| ComponentEmbedError::Encode(error.to_string()))?;
        String::from_utf8(out).map_err(|error| ComponentEmbedError::Encode(error.to_string()))
    }

    /// Every rule except the payload size, which needs the encoded text.
    fn check_structure(&self) -> Result<(), ComponentEmbedError> {
        let mut tally = Tally::default();
        self.component.check(&mut tally)?;
        if tally.components > Self::MAX_COMPONENTS {
            return Err(ComponentEmbedError::TooManyComponents {
                count: tally.components,
                max: Self::MAX_COMPONENTS,
            });
        }
        if tally.gallery_items > Self::MAX_GALLERY_ITEMS {
            return Err(ComponentEmbedError::TooManyGalleryItems {
                count: tally.gallery_items,
                max: Self::MAX_GALLERY_ITEMS,
            });
        }
        Ok(())
    }
}

impl Serialize for ComponentEmbed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("component", &self.component)?;
        map.end()
    }
}

impl From<PreviewContainer> for ComponentEmbed {
    fn from(component: PreviewContainer) -> Self {
        Self::new(component)
    }
}

/// Fluent builder for a [`ComponentEmbed`]: container options plus its child
/// components in order.
///
/// [`Self::build`] never validates; [`Self::try_build`] and
/// [`Self::validate`] check every limit (the same check as
/// [`ComponentEmbed::validate`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentEmbedBuilder {
    embed: ComponentEmbed,
}

impl ComponentEmbedBuilder {
    /// Starts an empty embed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the container's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.embed.component.id = Some(id);
        self
    }

    /// Sets the container's accent colour (RGB, at most `0xFFFFFF`).
    pub fn accent_color(mut self, color: u32) -> Self {
        self.embed.component.accent_color = Some(color);
        self
    }

    /// Marks the whole container as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.embed.component.spoiler = Some(spoiler);
        self
    }

    /// Appends a child component: a [`PreviewTextDisplay`],
    /// [`PreviewSection`], [`PreviewMediaGallery`], [`PreviewSeparator`],
    /// [`PreviewActionRow`], or a [`PreviewComponent`].
    pub fn add_component(mut self, component: impl Into<PreviewComponent>) -> Self {
        self.embed.component.components.push(component.into());
        self
    }

    /// Appends a text display with this Markdown content.
    pub fn add_text(self, content: impl Into<String>) -> Self {
        self.add_component(PreviewTextDisplay::new(content))
    }

    /// Checks every limit without consuming the builder.
    pub fn validate(&self) -> Result<(), ComponentEmbedError> {
        self.embed.validate()
    }

    /// Returns the embed without checking any limit.
    pub fn build(self) -> ComponentEmbed {
        self.embed
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<ComponentEmbed, ComponentEmbedError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// The parent Container (type 17) of a component embed. It can hold action
/// rows, sections, text displays, media galleries, and separators, never
/// another container.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewContainer {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// Accent bar colour as RGB, `0x000000`-`0xFFFFFF`.
    pub accent_color: Option<u32>,
    /// Whether the container is blurred as a spoiler (Discord's default:
    /// `false`).
    pub spoiler: Option<bool>,
    /// Child components, in display order.
    pub components: Vec<PreviewComponent>,
}

impl PreviewContainer {
    /// An empty container; add children through
    /// [`components`](Self::components) or use [`ComponentEmbedBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    fn check(&self, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        let path = ROOT_PATH;
        tally.component(self.id, path)?;
        if let Some(color) = self.accent_color {
            if color > ComponentEmbed::MAX_ACCENT_COLOR {
                return Err(ComponentEmbedError::AccentColorOutOfRange {
                    value: color,
                    max: ComponentEmbed::MAX_ACCENT_COLOR,
                });
            }
        }
        // The spec gives no minimum, but an empty container renders nothing
        // and `ContainerBuilder::validate` already rejects it for messages.
        if self.components.is_empty() {
            return Err(ComponentEmbedError::EmptyContainer);
        }
        for (index, component) in self.components.iter().enumerate() {
            component.check(&format!("{path}.components[{index}]"), tally)?;
        }
        Ok(())
    }
}

impl Serialize for PreviewContainer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::CONTAINER)?;
        serialize_some(&mut map, "id", &self.id)?;
        serialize_some(&mut map, "accent_color", &self.accent_color)?;
        serialize_some(&mut map, "spoiler", &self.spoiler)?;
        map.serialize_entry("components", &self.components)?;
        map.end()
    }
}

/// A child of the [`PreviewContainer`]: exactly the container children the
/// Component Embed spec allows. Buttons and thumbnails appear inside
/// [`PreviewActionRow`] and [`PreviewSection`]. There is no container variant
/// and no raw-JSON variant.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreviewComponent {
    /// Action Row (type 1) of link buttons.
    ActionRow(PreviewActionRow),
    /// Section (type 9): text with a link button or thumbnail beside it.
    Section(PreviewSection),
    /// Text Display (type 10).
    TextDisplay(PreviewTextDisplay),
    /// Media Gallery (type 12).
    MediaGallery(PreviewMediaGallery),
    /// Separator (type 14).
    Separator(PreviewSeparator),
}

impl PreviewComponent {
    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        match self {
            Self::ActionRow(row) => row.check(path, tally),
            Self::Section(section) => section.check(path, tally),
            Self::TextDisplay(text) => tally.component(text.id, path),
            Self::MediaGallery(gallery) => gallery.check(path, tally),
            Self::Separator(separator) => tally.component(separator.id, path),
        }
    }
}

impl Serialize for PreviewComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ActionRow(row) => row.serialize(serializer),
            Self::Section(section) => section.serialize(serializer),
            Self::TextDisplay(text) => text.serialize(serializer),
            Self::MediaGallery(gallery) => gallery.serialize(serializer),
            Self::Separator(separator) => separator.serialize(serializer),
        }
    }
}

impl From<PreviewActionRow> for PreviewComponent {
    fn from(row: PreviewActionRow) -> Self {
        Self::ActionRow(row)
    }
}

impl From<PreviewSection> for PreviewComponent {
    fn from(section: PreviewSection) -> Self {
        Self::Section(section)
    }
}

impl From<PreviewTextDisplay> for PreviewComponent {
    fn from(text: PreviewTextDisplay) -> Self {
        Self::TextDisplay(text)
    }
}

impl From<PreviewMediaGallery> for PreviewComponent {
    fn from(gallery: PreviewMediaGallery) -> Self {
        Self::MediaGallery(gallery)
    }
}

impl From<PreviewSeparator> for PreviewComponent {
    fn from(separator: PreviewSeparator) -> Self {
        Self::Separator(separator)
    }
}

/// Action Row (type 1) holding 1-5 link buttons. Select menus and buttons
/// with a `custom_id` cannot be added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewActionRow {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// The row's link buttons (serialized as `components`).
    pub buttons: Vec<PreviewLinkButton>,
}

impl PreviewActionRow {
    /// An empty row.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the row's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Appends a link button.
    pub fn add_button(mut self, button: PreviewLinkButton) -> Self {
        self.buttons.push(button);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        check_child_count(
            format!("{path}.components"),
            self.buttons.len(),
            ComponentEmbed::MAX_ACTION_ROW_BUTTONS,
        )?;
        for (index, button) in self.buttons.iter().enumerate() {
            button.check(&format!("{path}.components[{index}]"), tally)?;
        }
        Ok(())
    }
}

impl Serialize for PreviewActionRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::ACTION_ROW)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", &self.buttons)?;
        map.end()
    }
}

/// Link-style Button (type 2, style 5): the only button a component embed
/// allows, since a preview sends no interactions.
///
/// It serializes only the keys the spec permits (`type`, `id`, `style`,
/// `label`, `emoji`, `url`, `disabled`; "Any other key invalidates the
/// entire payload"), and it has no `custom_id` or `sku_id` to set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewLinkButton {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// Target URL, at most 512 characters.
    pub url: String,
    /// Button text, at most 80 characters. A button needs a label, an emoji,
    /// or both.
    pub label: Option<String>,
    /// Emoji shown on the button (`name`, `id`, `animated`).
    pub emoji: Option<Emoji>,
    /// Whether the button is shown disabled.
    pub disabled: Option<bool>,
}

impl PreviewLinkButton {
    /// A link button to `url`; give it a [`label`](Self::label),
    /// an [`emoji`](Self::emoji), or both.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            url: url.into(),
            label: None,
            emoji: None,
            disabled: None,
        }
    }

    /// Sets the button's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the button text.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the button emoji.
    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.emoji = Some(emoji);
        self
    }

    /// Shows the button disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        check_url_length(
            &format!("{path}.url"),
            &self.url,
            ComponentEmbed::MAX_BUTTON_URL_LENGTH,
        )?;
        if let Some(label) = &self.label {
            check_text_length(
                &format!("{path}.label"),
                label,
                ComponentEmbed::MAX_BUTTON_LABEL_LENGTH,
            )?;
        }
        // "with a `label`, an `emoji`, or both" (Compatible Components, *).
        if self.label.is_none() && self.emoji.is_none() {
            return Err(ComponentEmbedError::ButtonWithoutLabelOrEmoji {
                path: path.to_owned(),
            });
        }
        Ok(())
    }
}

impl Serialize for PreviewLinkButton {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::BUTTON)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("style", &button_style::LINK)?;
        serialize_some(&mut map, "label", &self.label)?;
        serialize_some(&mut map, "emoji", &self.emoji)?;
        map.serialize_entry("url", &self.url)?;
        serialize_some(&mut map, "disabled", &self.disabled)?;
        map.end()
    }
}

/// Section (type 9): 1-3 text displays with a link button or thumbnail as
/// the accessory beside them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewSection {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// The section's text, 1-3 text displays.
    pub components: Vec<PreviewTextDisplay>,
    /// The component shown beside the text.
    pub accessory: PreviewAccessory,
}

impl PreviewSection {
    /// A section with this accessory (a [`PreviewLinkButton`] or a
    /// [`PreviewThumbnail`]); add its text with [`add_text`](Self::add_text).
    pub fn new(accessory: impl Into<PreviewAccessory>) -> Self {
        Self {
            id: None,
            components: Vec::new(),
            accessory: accessory.into(),
        }
    }

    /// Sets the section's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Appends a text display with this Markdown content.
    pub fn add_text(self, content: impl Into<String>) -> Self {
        self.add_text_display(PreviewTextDisplay::new(content))
    }

    /// Appends a text display.
    pub fn add_text_display(mut self, text: PreviewTextDisplay) -> Self {
        self.components.push(text);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        check_child_count(
            format!("{path}.components"),
            self.components.len(),
            ComponentEmbed::MAX_SECTION_TEXTS,
        )?;
        for (index, text) in self.components.iter().enumerate() {
            tally.component(text.id, &format!("{path}.components[{index}]"))?;
        }
        let accessory_path = format!("{path}.accessory");
        match &self.accessory {
            PreviewAccessory::Button(button) => button.check(&accessory_path, tally),
            PreviewAccessory::Thumbnail(thumbnail) => thumbnail.check(&accessory_path, tally),
        }
    }
}

impl Serialize for PreviewSection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::SECTION)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", &self.components)?;
        map.serialize_entry("accessory", &self.accessory)?;
        map.end()
    }
}

/// Accessory of a [`PreviewSection`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreviewAccessory {
    /// A link button.
    Button(PreviewLinkButton),
    /// A thumbnail image.
    Thumbnail(PreviewThumbnail),
}

impl Serialize for PreviewAccessory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Button(button) => button.serialize(serializer),
            Self::Thumbnail(thumbnail) => thumbnail.serialize(serializer),
        }
    }
}

impl From<PreviewLinkButton> for PreviewAccessory {
    fn from(button: PreviewLinkButton) -> Self {
        Self::Button(button)
    }
}

impl From<PreviewThumbnail> for PreviewAccessory {
    fn from(thumbnail: PreviewThumbnail) -> Self {
        Self::Thumbnail(thumbnail)
    }
}

/// Text Display (type 10) with Discord Markdown content.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewTextDisplay {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// Markdown text.
    pub content: String,
}

impl PreviewTextDisplay {
    /// A text display with this Markdown content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: None,
            content: content.into(),
        }
    }

    /// Sets the text display's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }
}

impl Serialize for PreviewTextDisplay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::TEXT_DISPLAY)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("content", &self.content)?;
        map.end()
    }
}

/// Thumbnail (type 11), usable as a [`PreviewSection`] accessory. Thumbnails
/// do not count toward the 10 gallery items.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewThumbnail {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// Image URL (`media.url`): `http` or `https`, at most 2,048 characters.
    pub url: String,
    /// Alt text, at most 1,024 characters.
    pub description: Option<String>,
    /// Whether the thumbnail is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl PreviewThumbnail {
    /// A thumbnail of the image at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            url: url.into(),
            description: None,
            spoiler: None,
        }
    }

    /// Sets the thumbnail's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the alt text.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Marks the thumbnail as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        check_media(path, &self.url, self.description.as_deref())
    }
}

impl Serialize for PreviewThumbnail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::THUMBNAIL)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("media", &MediaRef { url: &self.url })?;
        serialize_some(&mut map, "description", &self.description)?;
        serialize_some(&mut map, "spoiler", &self.spoiler)?;
        map.end()
    }
}

/// Media Gallery (type 12) of 1-10 items. Items count toward the embed-wide
/// cap of 10 across all galleries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewMediaGallery {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// The gallery's images or videos.
    pub items: Vec<PreviewMediaItem>,
}

impl PreviewMediaGallery {
    /// An empty gallery.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the gallery's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Appends an item.
    pub fn add_item(mut self, item: PreviewMediaItem) -> Self {
        self.items.push(item);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        let items_path = format!("{path}.items");
        check_child_count(
            items_path.clone(),
            self.items.len(),
            ComponentEmbed::MAX_ITEMS_PER_GALLERY,
        )?;
        tally.gallery_items = tally.gallery_items.saturating_add(self.items.len());
        for (index, item) in self.items.iter().enumerate() {
            check_media(
                &format!("{items_path}[{index}]"),
                &item.url,
                item.description.as_deref(),
            )?;
        }
        Ok(())
    }
}

impl Serialize for PreviewMediaGallery {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::MEDIA_GALLERY)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("items", &self.items)?;
        map.end()
    }
}

/// One image or video in a [`PreviewMediaGallery`]. Videos (`.mp4`, `.webm`,
/// `.mov`) show a preview frame and do not autoplay.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewMediaItem {
    /// Media URL (`media.url`): `http` or `https`, at most 2,048 characters.
    pub url: String,
    /// Alt text, at most 1,024 characters.
    pub description: Option<String>,
    /// Whether the item is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl PreviewMediaItem {
    /// An item showing the media at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            description: None,
            spoiler: None,
        }
    }

    /// Sets the alt text.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Marks the item as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }
}

impl Serialize for PreviewMediaItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("media", &MediaRef { url: &self.url })?;
        serialize_some(&mut map, "description", &self.description)?;
        serialize_some(&mut map, "spoiler", &self.spoiler)?;
        map.end()
    }
}

/// Separator (type 14): vertical padding, with or without a divider line.
/// Unset fields take Discord's defaults (divider shown, small spacing) and
/// cost no payload bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PreviewSeparator {
    /// Optional component identifier, unique within the embed.
    pub id: Option<u32>,
    /// Whether a divider line is shown (Discord's default: `true`).
    pub divider: Option<bool>,
    /// Padding size (Discord's default: small).
    pub spacing: Option<SeparatorSpacing>,
}

impl PreviewSeparator {
    /// A separator with Discord's defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the separator's component `id`.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Shows or hides the divider line.
    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = Some(divider);
        self
    }

    /// Sets the padding size.
    pub fn spacing(mut self, spacing: SeparatorSpacing) -> Self {
        self.spacing = Some(spacing);
        self
    }
}

impl Serialize for PreviewSeparator {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &component_type::SEPARATOR)?;
        serialize_some(&mut map, "id", &self.id)?;
        serialize_some(&mut map, "divider", &self.divider)?;
        serialize_some(&mut map, "spacing", &self.spacing)?;
        map.end()
    }
}

/// Separator padding size; serialized as Discord's number (`1` or `2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SeparatorSpacing {
    /// Small padding (`1`).
    Small,
    /// Large padding (`2`).
    Large,
}

impl SeparatorSpacing {
    /// The wire value.
    pub const fn value(self) -> u8 {
        match self {
            Self::Small => separator_spacing::SMALL,
            Self::Large => separator_spacing::LARGE,
        }
    }

    fn from_value(value: u64) -> Option<Self> {
        match u8::try_from(value).ok()? {
            separator_spacing::SMALL => Some(Self::Small),
            separator_spacing::LARGE => Some(Self::Large),
            _ => None,
        }
    }
}

impl Serialize for SeparatorSpacing {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.value())
    }
}

/// A component embed rule that a payload breaks. Each variant is one rule;
/// `path` locates the offending component in the JSON document
/// (`component.components[2].accessory.url`), or in the converted value
/// (`$.components[0]`) for [`PreviewComponent`] conversions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComponentEmbedError {
    /// The escaped JSON document is larger than
    /// [`ComponentEmbed::MAX_PAYLOAD_BYTES`].
    PayloadTooLarge {
        /// Size of the escaped JSON in bytes.
        bytes: usize,
        /// The limit.
        max: usize,
    },
    /// More than [`ComponentEmbed::MAX_COMPONENTS`] components, counting the
    /// container, buttons, text displays, and thumbnails.
    TooManyComponents {
        /// Components in the embed.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// More than [`ComponentEmbed::MAX_GALLERY_ITEMS`] media gallery items
    /// across all galleries.
    TooManyGalleryItems {
        /// Gallery items in the embed.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// The container has no child components.
    EmptyContainer,
    /// An action row, section, or media gallery holds fewer than one or more
    /// than its maximum number of children.
    ChildCountOutOfRange {
        /// Location of the children.
        path: String,
        /// Number of children.
        count: usize,
        /// Fewest allowed.
        min: usize,
        /// Most allowed.
        max: usize,
    },
    /// A button label or media description is longer than its limit, in
    /// Unicode characters.
    TextTooLong {
        /// Location of the text.
        path: String,
        /// Length in characters.
        length: usize,
        /// The limit.
        max: usize,
    },
    /// A media or button URL is longer than its limit, in Unicode
    /// characters.
    UrlTooLong {
        /// Location of the URL.
        path: String,
        /// Length in characters.
        length: usize,
        /// The limit.
        max: usize,
    },
    /// A thumbnail or gallery URL is not an absolute `http` or `https` URL.
    MediaUrlNotHttp {
        /// Location of the URL.
        path: String,
    },
    /// A link button has neither a label nor an emoji.
    ButtonWithoutLabelOrEmoji {
        /// Location of the button.
        path: String,
    },
    /// The container accent colour is above
    /// [`ComponentEmbed::MAX_ACCENT_COLOR`].
    AccentColorOutOfRange {
        /// The colour given.
        value: u32,
        /// The limit.
        max: u32,
    },
    /// Two components share a nonzero `id` (Discord requires unique IDs; `0`
    /// means unset).
    DuplicateComponentId {
        /// Location of the second component with the ID.
        path: String,
        /// The repeated ID.
        id: u32,
    },
    /// Conversion only: a container inside the container.
    NestedContainer {
        /// Location of the inner container.
        path: String,
    },
    /// Conversion only: a component type that a component embed does not
    /// allow at this position (select menus, files, text inputs, ...).
    UnsupportedComponent {
        /// Location of the component.
        path: String,
        /// Its `type` value.
        component_type: u64,
        /// What is allowed at this position.
        allowed: &'static str,
    },
    /// Conversion only: a button whose style is not link (5), such as a
    /// button with a `custom_id` or a premium button.
    ButtonNotLink {
        /// Location of the button.
        path: String,
        /// Its `style` value.
        style: u64,
    },
    /// Conversion only: a key the preview model cannot carry, such as a
    /// button's `custom_id` ("Any other key invalidates the entire
    /// payload").
    UnsupportedField {
        /// Location of the object.
        path: String,
        /// The key.
        field: String,
    },
    /// Conversion only: a missing field or a value of the wrong JSON type.
    InvalidPayload {
        /// Location of the value.
        path: String,
        /// What was expected.
        reason: String,
    },
    /// [`ComponentEmbed::link_tag`] was given something other than an
    /// absolute `http` or `https` URL.
    InvalidLinkHref {
        /// The href given.
        href: String,
    },
    /// Serializing the document failed. Not expected with these types; kept
    /// so that encoding never panics.
    Encode(String),
}

impl fmt::Display for ComponentEmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { bytes, max } => write!(
                f,
                "component embed is {bytes} bytes as escaped JSON, over the {max}-byte limit"
            ),
            Self::TooManyComponents { count, max } => write!(
                f,
                "component embed has {count} components including the container, over the \
                 limit of {max}"
            ),
            Self::TooManyGalleryItems { count, max } => write!(
                f,
                "component embed has {count} media gallery items across all galleries, over \
                 the limit of {max}"
            ),
            Self::EmptyContainer => write!(
                f,
                "{ROOT_PATH}.components: the container must hold at least one component"
            ),
            Self::ChildCountOutOfRange {
                path,
                count,
                min,
                max,
            } => write!(f, "{path} must contain {min}-{max} items, got {count}"),
            Self::TextTooLong { path, length, max } | Self::UrlTooLong { path, length, max } => {
                write!(f, "{path} must be at most {max} characters, got {length}")
            }
            Self::MediaUrlNotHttp { path } => {
                write!(f, "{path} must be an absolute http or https URL")
            }
            Self::ButtonWithoutLabelOrEmoji { path } => {
                write!(f, "{path}: a link button needs a label, an emoji, or both")
            }
            Self::AccentColorOutOfRange { value, max } => write!(
                f,
                "{ROOT_PATH}.accent_color must be an RGB value up to {max:#08X}, got {value:#X}"
            ),
            Self::DuplicateComponentId { path, id } => write!(
                f,
                "{path}: component id {id} is already used by another component"
            ),
            Self::NestedContainer { path } => {
                write!(f, "{path}: a container cannot contain another container")
            }
            Self::UnsupportedComponent {
                path,
                component_type,
                allowed,
            } => write!(
                f,
                "{path}: {} (type {component_type}) is not allowed here in a component embed; \
                 expected {allowed}",
                component_name(*component_type)
            ),
            Self::ButtonNotLink { path, style } => write!(
                f,
                "{path}: button style {style} is not allowed; component embeds accept only \
                 link buttons (style 5), which send no interaction"
            ),
            Self::UnsupportedField { path, field } => {
                write!(
                    f,
                    "{path}: field `{field}` is not allowed in a component embed"
                )
            }
            Self::InvalidPayload { path, reason } => write!(f, "{path}: {reason}"),
            Self::InvalidLinkHref { href } => write!(
                f,
                "component embed link href must be an absolute http or https URL, got {href:?}"
            ),
            Self::Encode(message) => write!(f, "failed to encode the component embed: {message}"),
        }
    }
}

impl std::error::Error for ComponentEmbedError {}

impl From<ComponentEmbedError> for DiscordError {
    fn from(error: ComponentEmbedError) -> Self {
        DiscordError::model(error.to_string())
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Path of the container in the document, the root of validation paths.
const ROOT_PATH: &str = "component";

/// Every child list (action row, section, gallery) needs at least one entry:
/// the spec says "One to three" and "1 to 10", and `ActionRowBuilder` already
/// rejects an empty row, which Discord does not render.
const MIN_CHILDREN: usize = 1;

/// Embed-wide counters filled while walking the tree.
#[derive(Default)]
struct Tally {
    components: usize,
    gallery_items: usize,
    ids: HashSet<u32>,
}

impl Tally {
    /// Counts one component and records its `id`. `0` is Discord's "unset"
    /// value, so it may repeat.
    fn component(&mut self, id: Option<u32>, path: &str) -> Result<(), ComponentEmbedError> {
        self.components = self.components.saturating_add(1);
        match id {
            Some(id) if id != 0 && !self.ids.insert(id) => {
                Err(ComponentEmbedError::DuplicateComponentId {
                    path: path.to_owned(),
                    id,
                })
            }
            _ => Ok(()),
        }
    }
}

fn check_child_count(
    children_path: String,
    count: usize,
    max: usize,
) -> Result<(), ComponentEmbedError> {
    if (MIN_CHILDREN..=max).contains(&count) {
        return Ok(());
    }
    Err(ComponentEmbedError::ChildCountOutOfRange {
        path: children_path,
        count,
        min: MIN_CHILDREN,
        max,
    })
}

fn check_text_length(path: &str, text: &str, max: usize) -> Result<(), ComponentEmbedError> {
    let length = char_count(text);
    if length > max {
        return Err(ComponentEmbedError::TextTooLong {
            path: path.to_owned(),
            length,
            max,
        });
    }
    Ok(())
}

fn check_url_length(path: &str, url: &str, max: usize) -> Result<(), ComponentEmbedError> {
    let length = char_count(url);
    if length > max {
        return Err(ComponentEmbedError::UrlTooLong {
            path: path.to_owned(),
            length,
            max,
        });
    }
    Ok(())
}

/// The "Media" rules shared by thumbnails and gallery items. File-extension
/// lists in the spec are not checked: media URLs often have no extension, and
/// Discord decides by the fetched content.
fn check_media(
    path: &str,
    url: &str,
    description: Option<&str>,
) -> Result<(), ComponentEmbedError> {
    let url_path = format!("{path}.media.url");
    check_url_length(&url_path, url, ComponentEmbed::MAX_MEDIA_URL_LENGTH)?;
    if !is_absolute_http_url(url) {
        return Err(ComponentEmbedError::MediaUrlNotHttp { path: url_path });
    }
    if let Some(description) = description {
        check_text_length(
            &format!("{path}.description"),
            description,
            ComponentEmbed::MAX_MEDIA_DESCRIPTION_LENGTH,
        )?;
    }
    Ok(())
}

fn is_absolute_http_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
}

fn component_name(component_type: u64) -> &'static str {
    let Ok(component_type) = u8::try_from(component_type) else {
        return "unknown component";
    };
    match component_type {
        component_type::ACTION_ROW => "action row",
        component_type::BUTTON => "button",
        component_type::STRING_SELECT => "string select",
        component_type::TEXT_INPUT => "text input",
        component_type::USER_SELECT => "user select",
        component_type::ROLE_SELECT => "role select",
        component_type::MENTIONABLE_SELECT => "mentionable select",
        component_type::CHANNEL_SELECT => "channel select",
        component_type::SECTION => "section",
        component_type::TEXT_DISPLAY => "text display",
        component_type::THUMBNAIL => "thumbnail",
        component_type::MEDIA_GALLERY => "media gallery",
        component_type::FILE => "file",
        component_type::SEPARATOR => "separator",
        component_type::CONTAINER => "container",
        _ => "unknown component",
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MediaRef<'a> {
    url: &'a str,
}

fn serialize_some<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Option<T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(key, value),
        None => Ok(()),
    }
}

/// Compact JSON whose text can sit inside a `<script>` element.
///
/// serde_json already escapes `"`, `\`, and control characters. In addition,
/// every `<`, `>`, `&`, U+2028, and U+2029 inside a string becomes a `\uXXXX`
/// escape, which JSON parsers read back as the same character:
///
/// - `<` so that no `</script` (any case) can close the element and no
///   `<!--` can switch the HTML tokenizer into its escaped script states;
/// - `&` and `>` because Discord also accepts `application/xhtml+xml` pages,
///   where script text is parsed as XML (a raw `&` or `<` breaks the page and
///   `]]>` ends a CDATA section). This is the set Rails `json_escape` and
///   Django `json_script` use;
/// - U+2028/U+2029 because they end a line in pre-ES2019 JavaScript, in case
///   the JSON is ever pasted into a script.
///
/// These characters can occur only inside JSON strings, never in the
/// structure, so escaping string fragments covers every occurrence. Each
/// escape costs 5 more bytes (U+2028/U+2029: 3 more), and the limit is
/// checked on this output. Other non-ASCII text stays UTF-8 rather than
/// `\uXXXX`, which would triple its size.
struct ScriptSafeFormatter;

/// Characters [`ScriptSafeFormatter`] escapes beyond what JSON requires.
const SCRIPT_UNSAFE: [char; 5] = ['<', '>', '&', '\u{2028}', '\u{2029}'];

impl serde_json::ser::Formatter for ScriptSafeFormatter {
    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut rest = fragment;
        while let Some(index) = rest.find(SCRIPT_UNSAFE) {
            let Some((plain, tail)) = rest.split_at_checked(index) else {
                break;
            };
            writer.write_all(plain.as_bytes())?;
            let mut chars = tail.chars();
            if let Some(unsafe_char) = chars.next() {
                write!(writer, "\\u{:04x}", u32::from(unsafe_char))?;
            }
            rest = chars.as_str();
        }
        writer.write_all(rest.as_bytes())
    }
}

/// Escapes a value for a double-quoted HTML attribute.
fn escape_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// Conversion from message component payloads
// ---------------------------------------------------------------------------

/// Converts the output of the message builders. Fails on anything a
/// component embed cannot carry instead of dropping it: buttons with a
/// `custom_id` or a premium SKU, select menus, files, nested containers, raw
/// components, and unknown keys. Limits are not checked here; call
/// [`ComponentEmbed::validate`] or let [`ComponentEmbed::to_json`] do it.
impl TryFrom<ContainerBuilder> for PreviewContainer {
    type Error = ComponentEmbedError;

    fn try_from(container: ContainerBuilder) -> Result<Self, Self::Error> {
        Self::try_from(&container.build())
    }
}

/// The `PreviewContainer` conversion, wrapped in an embed.
impl TryFrom<ContainerBuilder> for ComponentEmbed {
    type Error = ComponentEmbedError;

    fn try_from(container: ContainerBuilder) -> Result<Self, Self::Error> {
        PreviewContainer::try_from(container).map(Self::new)
    }
}

/// Reads a Container (type 17) payload, such as `ContainerBuilder::build()`
/// output. Errors locate the problem from `component`.
impl TryFrom<&Value> for PreviewContainer {
    type Error = ComponentEmbedError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        read_container(value, ROOT_PATH)
    }
}

/// Reads a whole component embed document, `{"component": {...}}`, for
/// example a hand-written file to validate before serving it.
impl TryFrom<&Value> for ComponentEmbed {
    type Error = ComponentEmbedError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let map = expect_object(value, "$")?;
        only_keys(map, &["component"], "$")?;
        let container = map
            .get("component")
            .ok_or_else(|| missing("$", "component"))?;
        read_container(container, ROOT_PATH).map(Self::new)
    }
}

/// Reads one container child payload, such as `SectionBuilder::build()` or
/// `ActionRowBuilder::build()` output. Errors locate the problem from `$`,
/// the value passed in.
impl TryFrom<&Value> for PreviewComponent {
    type Error = ComponentEmbedError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        read_child(value, "$")
    }
}

const CONTAINER_CHILDREN: &str =
    "an action row, section, text display, media gallery, or separator";

fn read_container(value: &Value, path: &str) -> Result<PreviewContainer, ComponentEmbedError> {
    let map = expect_object(value, path)?;
    let kind = read_type(map, path)?;
    if kind != u64::from(component_type::CONTAINER) {
        return Err(ComponentEmbedError::UnsupportedComponent {
            path: path.to_owned(),
            component_type: kind,
            allowed: "a container",
        });
    }
    only_keys(
        map,
        &["type", "id", "components", "accent_color", "spoiler"],
        path,
    )?;
    let components = read_array(map, "components", path)?
        .iter()
        .enumerate()
        .map(|(index, child)| read_child(child, &format!("{path}.components[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreviewContainer {
        id: read_u32(map, "id", path)?,
        accent_color: read_u32(map, "accent_color", path)?,
        spoiler: read_bool(map, "spoiler", path)?,
        components,
    })
}

fn read_child(value: &Value, path: &str) -> Result<PreviewComponent, ComponentEmbedError> {
    let map = expect_object(value, path)?;
    let kind = read_type(map, path)?;
    let unsupported = || ComponentEmbedError::UnsupportedComponent {
        path: path.to_owned(),
        component_type: kind,
        allowed: CONTAINER_CHILDREN,
    };
    let Ok(known) = u8::try_from(kind) else {
        return Err(unsupported());
    };
    match known {
        component_type::ACTION_ROW => read_action_row(map, path).map(PreviewComponent::ActionRow),
        component_type::SECTION => read_section(map, path).map(PreviewComponent::Section),
        component_type::TEXT_DISPLAY => {
            read_text_display(map, path).map(PreviewComponent::TextDisplay)
        }
        component_type::MEDIA_GALLERY => {
            read_media_gallery(map, path).map(PreviewComponent::MediaGallery)
        }
        component_type::SEPARATOR => read_separator(map, path).map(PreviewComponent::Separator),
        component_type::CONTAINER => Err(ComponentEmbedError::NestedContainer {
            path: path.to_owned(),
        }),
        _ => Err(unsupported()),
    }
}

fn read_action_row(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewActionRow, ComponentEmbedError> {
    only_keys(map, &["type", "id", "components"], path)?;
    let buttons = read_array(map, "components", path)?
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let child_path = format!("{path}.components[{index}]");
            let child = expect_object(child, &child_path)?;
            let kind = read_type(child, &child_path)?;
            if kind != u64::from(component_type::BUTTON) {
                return Err(ComponentEmbedError::UnsupportedComponent {
                    path: child_path,
                    component_type: kind,
                    allowed: "a link button",
                });
            }
            read_link_button(child, &child_path)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreviewActionRow {
        id: read_u32(map, "id", path)?,
        buttons,
    })
}

fn read_link_button(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewLinkButton, ComponentEmbedError> {
    // Style first: a custom-ID button reports "not a link button", which says
    // more than "`custom_id` is not allowed".
    let style = map
        .get("style")
        .ok_or_else(|| missing(path, "style"))?
        .as_u64()
        .ok_or_else(|| wrong_type(path, "style", "an integer"))?;
    if style != u64::from(button_style::LINK) {
        return Err(ComponentEmbedError::ButtonNotLink {
            path: path.to_owned(),
            style,
        });
    }
    only_keys(
        map,
        &["id", "type", "url", "style", "label", "emoji", "disabled"],
        path,
    )?;
    let emoji = match map.get("emoji") {
        None | Some(Value::Null) => None,
        Some(emoji) => Some(read_emoji(emoji, &format!("{path}.emoji"))?),
    };
    Ok(PreviewLinkButton {
        id: read_u32(map, "id", path)?,
        url: read_string(map, "url", path)?.ok_or_else(|| missing(path, "url"))?,
        label: read_string(map, "label", path)?,
        emoji,
        disabled: read_bool(map, "disabled", path)?,
    })
}

fn read_emoji(value: &Value, path: &str) -> Result<Emoji, ComponentEmbedError> {
    let map = expect_object(value, path)?;
    only_keys(map, &["id", "name", "animated"], path)?;
    serde_json::from_value(value.clone()).map_err(|error| ComponentEmbedError::InvalidPayload {
        path: path.to_owned(),
        reason: format!("expected a partial emoji: {error}"),
    })
}

fn read_section(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewSection, ComponentEmbedError> {
    only_keys(map, &["type", "id", "components", "accessory"], path)?;
    let components = read_array(map, "components", path)?
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let child_path = format!("{path}.components[{index}]");
            let child = expect_object(child, &child_path)?;
            let kind = read_type(child, &child_path)?;
            if kind != u64::from(component_type::TEXT_DISPLAY) {
                return Err(ComponentEmbedError::UnsupportedComponent {
                    path: child_path,
                    component_type: kind,
                    allowed: "a text display",
                });
            }
            read_text_display(child, &child_path)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let accessory_path = format!("{path}.accessory");
    let accessory = expect_object(
        map.get("accessory")
            .ok_or_else(|| missing(path, "accessory"))?,
        &accessory_path,
    )?;
    let kind = read_type(accessory, &accessory_path)?;
    let accessory = if kind == u64::from(component_type::BUTTON) {
        PreviewAccessory::Button(read_link_button(accessory, &accessory_path)?)
    } else if kind == u64::from(component_type::THUMBNAIL) {
        PreviewAccessory::Thumbnail(read_thumbnail(accessory, &accessory_path)?)
    } else {
        return Err(ComponentEmbedError::UnsupportedComponent {
            path: accessory_path,
            component_type: kind,
            allowed: "a link button or a thumbnail",
        });
    };
    Ok(PreviewSection {
        id: read_u32(map, "id", path)?,
        components,
        accessory,
    })
}

fn read_text_display(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewTextDisplay, ComponentEmbedError> {
    only_keys(map, &["type", "id", "content"], path)?;
    Ok(PreviewTextDisplay {
        id: read_u32(map, "id", path)?,
        content: read_string(map, "content", path)?.ok_or_else(|| missing(path, "content"))?,
    })
}

fn read_thumbnail(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewThumbnail, ComponentEmbedError> {
    only_keys(
        map,
        &["type", "id", "media", "description", "spoiler"],
        path,
    )?;
    Ok(PreviewThumbnail {
        id: read_u32(map, "id", path)?,
        url: read_media_url(map, path)?,
        description: read_string(map, "description", path)?,
        spoiler: read_bool(map, "spoiler", path)?,
    })
}

fn read_media_gallery(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewMediaGallery, ComponentEmbedError> {
    only_keys(map, &["type", "id", "items"], path)?;
    let items = read_array(map, "items", path)?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let item_path = format!("{path}.items[{index}]");
            let item = expect_object(item, &item_path)?;
            only_keys(item, &["media", "description", "spoiler"], &item_path)?;
            Ok(PreviewMediaItem {
                url: read_media_url(item, &item_path)?,
                description: read_string(item, "description", &item_path)?,
                spoiler: read_bool(item, "spoiler", &item_path)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreviewMediaGallery {
        id: read_u32(map, "id", path)?,
        items,
    })
}

fn read_separator(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewSeparator, ComponentEmbedError> {
    only_keys(map, &["type", "id", "divider", "spacing"], path)?;
    let spacing = match map.get("spacing") {
        None | Some(Value::Null) => None,
        Some(spacing) => Some(
            spacing
                .as_u64()
                .and_then(SeparatorSpacing::from_value)
                .ok_or_else(|| wrong_type(path, "spacing", "1 (small) or 2 (large)"))?,
        ),
    };
    Ok(PreviewSeparator {
        id: read_u32(map, "id", path)?,
        divider: read_bool(map, "divider", path)?,
        spacing,
    })
}

/// Reads an unfurled media item, of which only `url` is settable; the other
/// fields are filled in by Discord.
fn read_media_url(map: &Map<String, Value>, path: &str) -> Result<String, ComponentEmbedError> {
    let media_path = format!("{path}.media");
    let media = expect_object(
        map.get("media").ok_or_else(|| missing(path, "media"))?,
        &media_path,
    )?;
    only_keys(media, &["url"], &media_path)?;
    read_string(media, "url", &media_path)?.ok_or_else(|| missing(&media_path, "url"))
}

fn expect_object<'v>(
    value: &'v Value,
    path: &str,
) -> Result<&'v Map<String, Value>, ComponentEmbedError> {
    value
        .as_object()
        .ok_or_else(|| ComponentEmbedError::InvalidPayload {
            path: path.to_owned(),
            reason: "expected a JSON object".to_owned(),
        })
}

fn only_keys(
    map: &Map<String, Value>,
    allowed: &[&str],
    path: &str,
) -> Result<(), ComponentEmbedError> {
    match map.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(field) => Err(ComponentEmbedError::UnsupportedField {
            path: path.to_owned(),
            field: field.clone(),
        }),
        None => Ok(()),
    }
}

fn read_type(map: &Map<String, Value>, path: &str) -> Result<u64, ComponentEmbedError> {
    map.get("type")
        .ok_or_else(|| missing(path, "type"))?
        .as_u64()
        .ok_or_else(|| wrong_type(path, "type", "an integer"))
}

fn read_array<'v>(
    map: &'v Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'v [Value], ComponentEmbedError> {
    map.get(key)
        .ok_or_else(|| missing(path, key))?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| wrong_type(path, key, "an array"))
}

/// Optional fields: absent and `null` both mean unset.
fn read_u32(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<u32>, ComponentEmbedError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| wrong_type(path, key, "an integer from 0 to 4294967295")),
    }
}

fn read_bool(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<bool>, ComponentEmbedError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| wrong_type(path, key, "a boolean")),
    }
}

fn read_string(
    map: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, ComponentEmbedError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| wrong_type(path, key, "a string")),
    }
}

fn missing(path: &str, key: &str) -> ComponentEmbedError {
    ComponentEmbedError::InvalidPayload {
        path: path.to_owned(),
        reason: format!("missing required field `{key}`"),
    }
}

fn wrong_type(path: &str, key: &str, expected: &str) -> ComponentEmbedError {
    ComponentEmbedError::InvalidPayload {
        path: format!("{path}.{key}"),
        reason: format!("expected {expected}"),
    }
}
