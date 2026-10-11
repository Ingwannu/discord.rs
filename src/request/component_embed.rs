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
//!
//! The model holds no `serde_json::Value`. Untyped JSON enters only through
//! the `TryFrom<&Value>` impls at the bottom of this file;
//! `TryFrom<ContainerBuilder>` reads the builder's JSON through the same
//! reader, because the message builders still store `Value`s. Shared types
//! come from the model: [`ComponentId`] for component `id`s and the open
//! [`SeparatorSpacing`] discriminant, narrowed to 1 and 2 by validation.

use std::collections::HashSet;
use std::fmt;
use std::io;

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::builders::ContainerBuilder;
use crate::command::validation::char_count;
use crate::error::DiscordError;
use crate::model::discriminant::{ButtonStyle, ComponentType, SeparatorSpacing};
use crate::model::{ComponentId, Nullable, NullablePatch, Patch};
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
    /// `<`, `>`, `&`, U+2028, U+2029, U+FFFE, and U+FFFF written as `\uXXXX`
    /// escapes (C0 controls are JSON escapes already). The same text is safe
    /// to serve as the linked JSON file and to inline in a `<script>` element,
    /// also in XHTML, and its byte length is what the 3,000-byte limit was
    /// checked against. Other non-ASCII text stays as UTF-8, so serve the
    /// page as UTF-8.
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
    /// href="…" />` element that points Discord at a JSON document served
    /// separately (the output of [`Self::to_json`]). It is self-closing, which
    /// HTML accepts and XHTML requires.
    ///
    /// Discord requires an absolute URL on the page's host, a subdomain of it,
    /// or its parent domain. Only "absolute `http`/`https` URL" can be checked
    /// here; the host relation is up to the caller.
    ///
    /// The element carries the URL as the parser serializes it, not the
    /// input text: ASCII only (non-ASCII percent-encoded, hosts in punycode),
    /// lowercase scheme and host, `/` for an empty path, then
    /// attribute-escaped. So no character that XML forbids can reach the page.
    /// An href with an ASCII control character (U+0000-U+001F, U+007F) or a
    /// leading or trailing space is rejected: the parser would silently drop
    /// those, and the element would point somewhere other than what was
    /// written.
    pub fn link_tag(href: &str) -> Result<String, ComponentEmbedError> {
        let invalid = || ComponentEmbedError::InvalidLinkHref {
            href: href.to_owned(),
        };
        if href.starts_with(' ')
            || href.ends_with(' ')
            || href.chars().any(|c| c.is_ascii_control())
        {
            return Err(invalid());
        }
        let url = url::Url::parse(href).map_err(|_parse_error| invalid())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(invalid());
        }
        Ok(format!(
            "<link rel=\"{}\" type=\"application/json\" href=\"{}\" />",
            Self::ELEMENT_NAME,
            escape_attribute(url.as_str())
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
    pub fn id(mut self, id: ComponentId) -> Self {
        self.embed.component.id = Some(id);
        self
    }

    /// Sets the container's accent colour (RGB, at most `0xFFFFFF`).
    pub fn accent_color(mut self, color: u32) -> Self {
        self.embed.component.accent_color = Patch::Value(Nullable::Value(color));
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
    pub id: Option<ComponentId>,
    /// Accent bar colour as RGB, `0x000000`-`0xFFFFFF`. The spec types it
    /// `?integer`, so omitted ([`Patch::Missing`]), explicit `null`
    /// ([`Patch::null`]), and a value are kept apart.
    pub accent_color: NullablePatch<u32>,
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
        if let Some(&color) = present(&self.accent_color) {
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
        map.serialize_entry("type", &ComponentType::CONTAINER)?;
        serialize_some(&mut map, "id", &self.id)?;
        serialize_present(&mut map, "accent_color", &self.accent_color)?;
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
            Self::Separator(separator) => separator.check(path, tally),
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
    pub id: Option<ComponentId>,
    /// The row's link buttons (serialized as `components`).
    pub buttons: Vec<PreviewLinkButton>,
}

impl PreviewActionRow {
    /// An empty row.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the row's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
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
        map.serialize_entry("type", &ComponentType::ACTION_ROW)?;
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
    pub id: Option<ComponentId>,
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
    pub fn id(mut self, id: ComponentId) -> Self {
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
        map.serialize_entry("type", &ComponentType::BUTTON)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("style", &ButtonStyle::LINK)?;
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
    pub id: Option<ComponentId>,
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
    pub fn id(mut self, id: ComponentId) -> Self {
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
        map.serialize_entry("type", &ComponentType::SECTION)?;
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
    pub id: Option<ComponentId>,
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
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }
}

impl Serialize for PreviewTextDisplay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::TEXT_DISPLAY)?;
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
    pub id: Option<ComponentId>,
    /// Image URL (`media.url`): `http` or `https`, at most 2,048 characters.
    pub url: String,
    /// Alt text, at most 1,024 characters. Nullable in the spec
    /// (`?string`): omitted, `null`, and a value are kept apart.
    pub description: NullablePatch<String>,
    /// Whether the thumbnail is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl PreviewThumbnail {
    /// A thumbnail of the image at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            url: url.into(),
            description: Patch::Missing,
            spoiler: None,
        }
    }

    /// Sets the thumbnail's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the alt text.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Patch::Value(Nullable::Value(description.into()));
        self
    }

    /// Marks the thumbnail as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        check_media(
            path,
            &self.url,
            present(&self.description).map(String::as_str),
        )
    }
}

impl Serialize for PreviewThumbnail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::THUMBNAIL)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("media", &MediaRef { url: &self.url })?;
        serialize_present(&mut map, "description", &self.description)?;
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
    pub id: Option<ComponentId>,
    /// The gallery's images or videos.
    pub items: Vec<PreviewMediaItem>,
}

impl PreviewMediaGallery {
    /// An empty gallery.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the gallery's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
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
                present(&item.description).map(String::as_str),
            )?;
        }
        Ok(())
    }
}

impl Serialize for PreviewMediaGallery {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::MEDIA_GALLERY)?;
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
    /// Alt text, at most 1,024 characters. Nullable in the spec
    /// (`?string`): omitted, `null`, and a value are kept apart.
    pub description: NullablePatch<String>,
    /// Whether the item is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl PreviewMediaItem {
    /// An item showing the media at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            description: Patch::Missing,
            spoiler: None,
        }
    }

    /// Sets the alt text.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Patch::Value(Nullable::Value(description.into()));
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
        serialize_present(&mut map, "description", &self.description)?;
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
    pub id: Option<ComponentId>,
    /// Whether a divider line is shown (Discord's default: `true`).
    pub divider: Option<bool>,
    /// Padding size, [`SeparatorSpacing::SMALL`] (Discord's default) or
    /// [`SeparatorSpacing::LARGE`]; other values fail validation.
    pub spacing: Option<SeparatorSpacing>,
}

impl PreviewSeparator {
    /// A separator with Discord's defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the separator's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
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

    fn check(&self, path: &str, tally: &mut Tally) -> Result<(), ComponentEmbedError> {
        tally.component(self.id, path)?;
        // `SeparatorSpacing` is an open discriminant that can hold any `u8`;
        // Separator Structure allows only "`1` for small padding, `2` for
        // large padding".
        match self.spacing {
            None | Some(SeparatorSpacing::SMALL | SeparatorSpacing::LARGE) => Ok(()),
            Some(spacing) => Err(ComponentEmbedError::InvalidSeparatorSpacing {
                path: format!("{path}.spacing"),
                spacing,
            }),
        }
    }
}

impl Serialize for PreviewSeparator {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::SEPARATOR)?;
        serialize_some(&mut map, "id", &self.id)?;
        serialize_some(&mut map, "divider", &self.divider)?;
        serialize_some(&mut map, "spacing", &self.spacing)?;
        map.end()
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
    /// A separator `spacing` other than [`SeparatorSpacing::SMALL`] (1) or
    /// [`SeparatorSpacing::LARGE`] (2).
    InvalidSeparatorSpacing {
        /// Location of the spacing.
        path: String,
        /// The value given.
        spacing: SeparatorSpacing,
    },
    /// Two components share a nonzero `id` (Discord requires unique IDs;
    /// [`ComponentId`] `0` means unset and may repeat).
    DuplicateComponentId {
        /// Location of the second component with the ID.
        path: String,
        /// The repeated ID.
        id: ComponentId,
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
    /// Conversion only: an explicit `null` for a field that cannot hold one,
    /// because the spec does not make it nullable or (emoji keys) the shared
    /// [`Emoji`] type cannot carry it. Omit the key instead; conversion never
    /// drops a `null` silently.
    NullNotAllowed {
        /// Location of the `null`.
        path: String,
    },
    /// Conversion only: a missing field or a value of the wrong JSON type.
    InvalidPayload {
        /// Location of the value.
        path: String,
        /// What was expected.
        reason: String,
    },
    /// [`ComponentEmbed::link_tag`] was given something other than an
    /// absolute `http` or `https` URL, or an href with an ASCII control
    /// character or a leading or trailing space, which the URL parser would
    /// silently drop.
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
            Self::InvalidSeparatorSpacing { path, spacing } => write!(
                f,
                "{path} must be 1 (small) or 2 (large), got {}",
                spacing.get()
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
            Self::NullNotAllowed { path } => write!(
                f,
                "{path} is null, which this field cannot hold in a component embed; omit the key"
            ),
            Self::InvalidPayload { path, reason } => write!(f, "{path}: {reason}"),
            Self::InvalidLinkHref { href } => write!(
                f,
                "component embed link href must be an absolute http or https URL without \
                 control characters or surrounding spaces, got {href:?}"
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
    ids: HashSet<ComponentId>,
}

impl Tally {
    /// Counts one component and records its `id`. `ComponentId` 0 is
    /// Discord's "unset" value (Discord assigns one), so it may repeat.
    fn component(
        &mut self,
        id: Option<ComponentId>,
        path: &str,
    ) -> Result<(), ComponentEmbedError> {
        self.components = self.components.saturating_add(1);
        match id {
            Some(id) if id.get() != 0 && !self.ids.insert(id) => {
                Err(ComponentEmbedError::DuplicateComponentId {
                    path: path.to_owned(),
                    id,
                })
            }
            _ => Ok(()),
        }
    }
}

/// The value of a nullable field, if it has one.
fn present<T>(field: &NullablePatch<T>) -> Option<&T> {
    match field {
        Patch::Value(Nullable::Value(value)) => Some(value),
        Patch::Value(Nullable::Null) | Patch::Missing => None,
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
    match ComponentType::new(component_type) {
        ComponentType::ACTION_ROW => "action row",
        ComponentType::BUTTON => "button",
        ComponentType::STRING_SELECT => "string select",
        ComponentType::TEXT_INPUT => "text input",
        ComponentType::USER_SELECT => "user select",
        ComponentType::ROLE_SELECT => "role select",
        ComponentType::MENTIONABLE_SELECT => "mentionable select",
        ComponentType::CHANNEL_SELECT => "channel select",
        ComponentType::SECTION => "section",
        ComponentType::TEXT_DISPLAY => "text display",
        ComponentType::THUMBNAIL => "thumbnail",
        ComponentType::MEDIA_GALLERY => "media gallery",
        ComponentType::FILE => "file",
        ComponentType::SEPARATOR => "separator",
        ComponentType::CONTAINER => "container",
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

/// Writes a nullable field unless it is omitted: `null` or the value.
fn serialize_present<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &NullablePatch<T>,
) -> Result<(), M::Error> {
    match value {
        Patch::Missing => Ok(()),
        Patch::Value(value) => map.serialize_entry(key, value),
    }
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

/// Compact JSON whose text can sit inside a `<script>` element of an HTML
/// page or an XHTML (XML 1.0) page, both of which Discord accepts.
///
/// serde_json already escapes `"`, `\`, and every C0 control character
/// (U+0000-U+001F, including tab, LF, and CR). In addition, these characters
/// inside a string become `\uXXXX` escapes, which JSON parsers read back as
/// the same character:
///
/// - `<`, so that no `</script` (any case) can close the element and no
///   `<!--` can switch the HTML tokenizer into its escaped script states;
/// - `&` and `>`, because XHTML parses script text as XML: a raw `&` or `<`
///   breaks the page and `]]>` ends a CDATA section. With `<` this is the
///   set Rails `json_escape` and Django `json_script` use;
/// - U+FFFE and U+FFFF, which XML 1.0 forbids (production `Char`);
/// - U+2028 and U+2029, because they end a line in pre-ES2019 JavaScript,
///   in case the JSON is ever pasted into a script.
///
/// That covers every character XML 1.0 forbids that a Rust `str` can hold:
/// the C0 controls (escaped by JSON) and U+FFFE/U+FFFF. Surrogates
/// (U+D800-U+DFFF) cannot occur in a `str`. The other noncharacters
/// (U+FDD0-U+FDEF, U+1FFFE, ...) and DEL/C1 controls (U+007F-U+009F) are
/// allowed XML characters; HTML reports them as parse errors but passes them
/// through unchanged, so they stay as UTF-8. The text never contains a raw
/// U+0000, which HTML would replace, or a raw CR, which both parsers
/// normalize.
///
/// These characters can occur only inside JSON strings, never in the
/// structure, so escaping string fragments covers every occurrence. Each
/// escape costs 5 more bytes for the ASCII ones and 3 more for the others,
/// and the limit is checked on this output. Other non-ASCII text stays UTF-8
/// rather than `\uXXXX`, which would triple its size.
struct ScriptSafeFormatter;

/// Characters [`ScriptSafeFormatter`] escapes beyond what JSON requires.
const SCRIPT_UNSAFE: [char; 7] = [
    '<', '>', '&', '\u{2028}', '\u{2029}', '\u{fffe}', '\u{ffff}',
];

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
        let container = read_required(map, "component", "$", Some, "a container")?;
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

/// JSON type names for `InvalidPayload` reasons.
const EXPECTED_U32: &str = "an integer from 0 to 4294967295";
const EXPECTED_INTEGER: &str = "an integer";
const EXPECTED_BOOL: &str = "a boolean";
const EXPECTED_STRING: &str = "a string";
const EXPECTED_OBJECT: &str = "a JSON object";

fn read_container(value: &Value, path: &str) -> Result<PreviewContainer, ComponentEmbedError> {
    let map = expect_object(value, path)?;
    let kind = read_type(map, path)?;
    if kind != u64::from(ComponentType::CONTAINER.get()) {
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
        id: read_component_id(map, path)?,
        accent_color: read_nullable(map, "accent_color", path, as_u32, EXPECTED_U32)?,
        spoiler: read_optional(map, "spoiler", path, Value::as_bool, EXPECTED_BOOL)?,
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
    match ComponentType::new(known) {
        ComponentType::ACTION_ROW => read_action_row(map, path).map(PreviewComponent::ActionRow),
        ComponentType::SECTION => read_section(map, path).map(PreviewComponent::Section),
        ComponentType::TEXT_DISPLAY => {
            read_text_display(map, path).map(PreviewComponent::TextDisplay)
        }
        ComponentType::MEDIA_GALLERY => {
            read_media_gallery(map, path).map(PreviewComponent::MediaGallery)
        }
        ComponentType::SEPARATOR => read_separator(map, path).map(PreviewComponent::Separator),
        ComponentType::CONTAINER => Err(ComponentEmbedError::NestedContainer {
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
            if kind != u64::from(ComponentType::BUTTON.get()) {
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
        id: read_component_id(map, path)?,
        buttons,
    })
}

fn read_link_button(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewLinkButton, ComponentEmbedError> {
    // Style first: a custom-ID button reports "not a link button", which says
    // more than "`custom_id` is not allowed".
    let style = read_required(map, "style", path, Value::as_u64, EXPECTED_INTEGER)?;
    if style != u64::from(ButtonStyle::LINK.get()) {
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
    let emoji = match read_optional(map, "emoji", path, Value::as_object, EXPECTED_OBJECT)? {
        Some(emoji) => Some(read_emoji(emoji, &format!("{path}.emoji"))?),
        None => None,
    };
    Ok(PreviewLinkButton {
        id: read_component_id(map, path)?,
        url: read_required(map, "url", path, as_string, EXPECTED_STRING)?,
        label: read_optional(map, "label", path, as_string, EXPECTED_STRING)?,
        emoji,
        disabled: read_optional(map, "disabled", path, Value::as_bool, EXPECTED_BOOL)?,
    })
}

/// Reads a partial emoji into the shared [`Emoji`] type. The emoji `id` is
/// nullable in Discord's emoji object, but `Emoji` stores "no ID" as an
/// omitted key and cannot carry an explicit `null`, so a `null` key fails
/// here instead of being dropped; omitting the key means the same thing.
fn read_emoji(map: &Map<String, Value>, path: &str) -> Result<Emoji, ComponentEmbedError> {
    const KEYS: [&str; 3] = ["id", "name", "animated"];
    only_keys(map, &KEYS, path)?;
    if let Some(key) = KEYS
        .into_iter()
        .find(|key| map.get(*key) == Some(&Value::Null))
    {
        return Err(null_not_allowed(path, key));
    }
    serde_json::from_value(Value::Object(map.clone())).map_err(|error| {
        ComponentEmbedError::InvalidPayload {
            path: path.to_owned(),
            reason: format!("expected a partial emoji: {error}"),
        }
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
            if kind != u64::from(ComponentType::TEXT_DISPLAY.get()) {
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
    let accessory = read_required(map, "accessory", path, Value::as_object, EXPECTED_OBJECT)?;
    let kind = read_type(accessory, &accessory_path)?;
    let accessory = if kind == u64::from(ComponentType::BUTTON.get()) {
        PreviewAccessory::Button(read_link_button(accessory, &accessory_path)?)
    } else if kind == u64::from(ComponentType::THUMBNAIL.get()) {
        PreviewAccessory::Thumbnail(read_thumbnail(accessory, &accessory_path)?)
    } else {
        return Err(ComponentEmbedError::UnsupportedComponent {
            path: accessory_path,
            component_type: kind,
            allowed: "a link button or a thumbnail",
        });
    };
    Ok(PreviewSection {
        id: read_component_id(map, path)?,
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
        id: read_component_id(map, path)?,
        content: read_required(map, "content", path, as_string, EXPECTED_STRING)?,
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
        id: read_component_id(map, path)?,
        url: read_media_url(map, path)?,
        description: read_nullable(map, "description", path, as_string, EXPECTED_STRING)?,
        spoiler: read_optional(map, "spoiler", path, Value::as_bool, EXPECTED_BOOL)?,
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
                description: read_nullable(
                    item,
                    "description",
                    &item_path,
                    as_string,
                    EXPECTED_STRING,
                )?,
                spoiler: read_optional(item, "spoiler", &item_path, Value::as_bool, EXPECTED_BOOL)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreviewMediaGallery {
        id: read_component_id(map, path)?,
        items,
    })
}

fn read_separator(
    map: &Map<String, Value>,
    path: &str,
) -> Result<PreviewSeparator, ComponentEmbedError> {
    only_keys(map, &["type", "id", "divider", "spacing"], path)?;
    Ok(PreviewSeparator {
        id: read_component_id(map, path)?,
        divider: read_optional(map, "divider", path, Value::as_bool, EXPECTED_BOOL)?,
        // Any `u8` converts, like the open discriminant it becomes; values
        // other than 1 and 2 fail in `validate()` with the other limits.
        spacing: read_optional(
            map,
            "spacing",
            path,
            |raw| {
                raw.as_u64()
                    .and_then(|raw| u8::try_from(raw).ok())
                    .map(SeparatorSpacing::new)
            },
            "an integer from 0 to 255",
        )?,
    })
}

/// Reads an unfurled media item, of which only `url` is settable; the other
/// fields are filled in by Discord.
fn read_media_url(map: &Map<String, Value>, path: &str) -> Result<String, ComponentEmbedError> {
    let media_path = format!("{path}.media");
    let media = read_required(map, "media", path, Value::as_object, EXPECTED_OBJECT)?;
    only_keys(media, &["url"], &media_path)?;
    read_required(media, "url", &media_path, as_string, EXPECTED_STRING)
}

fn read_component_id(
    map: &Map<String, Value>,
    path: &str,
) -> Result<Option<ComponentId>, ComponentEmbedError> {
    read_optional(map, "id", path, as_u32, EXPECTED_U32).map(|id| id.map(ComponentId::new))
}

fn read_type(map: &Map<String, Value>, path: &str) -> Result<u64, ComponentEmbedError> {
    read_required(map, "type", path, Value::as_u64, EXPECTED_INTEGER)
}

fn read_array<'v>(
    map: &'v Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'v [Value], ComponentEmbedError> {
    read_required(map, key, path, Value::as_array, "an array").map(Vec::as_slice)
}

/// A key's state. Conversion is lossless or fails, so `null` is never folded
/// into "absent": fields the spec marks nullable (`?type`) keep it, and every
/// other field rejects it with its path.
enum Field<'v> {
    Absent,
    Null,
    Present(&'v Value),
}

fn field<'v>(map: &'v Map<String, Value>, key: &str) -> Field<'v> {
    match map.get(key) {
        None => Field::Absent,
        Some(Value::Null) => Field::Null,
        Some(value) => Field::Present(value),
    }
}

/// An optional field that the spec does not let be `null`.
fn read_optional<'v, T>(
    map: &'v Map<String, Value>,
    key: &str,
    path: &str,
    parse: impl FnOnce(&'v Value) -> Option<T>,
    expected: &str,
) -> Result<Option<T>, ComponentEmbedError> {
    match field(map, key) {
        Field::Absent => Ok(None),
        Field::Null => Err(null_not_allowed(path, key)),
        Field::Present(value) => parse(value)
            .map(Some)
            .ok_or_else(|| wrong_type(path, key, expected)),
    }
}

/// A required field (never `null` in the preview subset).
fn read_required<'v, T>(
    map: &'v Map<String, Value>,
    key: &str,
    path: &str,
    parse: impl FnOnce(&'v Value) -> Option<T>,
    expected: &str,
) -> Result<T, ComponentEmbedError> {
    read_optional(map, key, path, parse, expected)?.ok_or_else(|| missing(path, key))
}

/// An optional field that the spec marks nullable: absent, `null`, and a value
/// stay distinct ([`Patch::Missing`], [`Patch::null`], a value).
fn read_nullable<'v, T>(
    map: &'v Map<String, Value>,
    key: &str,
    path: &str,
    parse: impl FnOnce(&'v Value) -> Option<T>,
    expected: &str,
) -> Result<NullablePatch<T>, ComponentEmbedError> {
    match field(map, key) {
        Field::Absent => Ok(Patch::Missing),
        Field::Null => Ok(Patch::null()),
        Field::Present(value) => parse(value)
            .map(|value| Patch::Value(Nullable::Value(value)))
            .ok_or_else(|| wrong_type(path, key, expected)),
    }
}

fn as_u32(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|number| u32::try_from(number).ok())
}

fn as_string(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn expect_object<'v>(
    value: &'v Value,
    path: &str,
) -> Result<&'v Map<String, Value>, ComponentEmbedError> {
    value
        .as_object()
        .ok_or_else(|| ComponentEmbedError::InvalidPayload {
            path: path.to_owned(),
            reason: format!("expected {EXPECTED_OBJECT}"),
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

fn missing(path: &str, key: &str) -> ComponentEmbedError {
    ComponentEmbedError::InvalidPayload {
        path: path.to_owned(),
        reason: format!("missing required field `{key}`"),
    }
}

fn null_not_allowed(path: &str, key: &str) -> ComponentEmbedError {
    ComponentEmbedError::NullNotAllowed {
        path: format!("{path}.{key}"),
    }
}

fn wrong_type(path: &str, key: &str, expected: &str) -> ComponentEmbedError {
    ComponentEmbedError::InvalidPayload {
        path: format!("{path}.{key}"),
        reason: format!("expected {expected}"),
    }
}
