//! Outgoing rich embeds.
//!
//! [`Embed`] holds exactly what a bot can set: Discord ignores `type`,
//! `provider`, `video`, and image `height`/`width`/`proxy_url` on requests
//! ("you can set every field except ..." in Create Message), so those are
//! not fields here. The received embed is [`crate::model::Embed`].
//! [`EmbedBuilder`](crate::builders::EmbedBuilder) builds this type.

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use crate::builders::limits;
use crate::command::validation::char_count;
use crate::error::DiscordError;

use super::component::{at, check_len, check_max_len, entry_some};

/// A `rich` embed to send.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Embed {
    /// Title, up to 256 characters.
    pub title: Option<String>,
    /// Description, up to 4096 characters.
    pub description: Option<String>,
    /// URL the title links to.
    pub url: Option<String>,
    /// ISO 8601 timestamp shown in the footer.
    pub timestamp: Option<String>,
    /// Sidebar colour as RGB.
    pub color: Option<u32>,
    /// Footer text and icon.
    pub footer: Option<EmbedFooter>,
    /// Large image.
    pub image: Option<EmbedImage>,
    /// Small image beside the description.
    pub thumbnail: Option<EmbedImage>,
    /// Author line.
    pub author: Option<EmbedAuthor>,
    /// Up to 25 fields.
    pub fields: Vec<EmbedField>,
}

impl Embed {
    /// An empty embed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Characters that count toward the 6000-character total: title,
    /// description, field names and values, footer text, and author name
    /// ("Embed Limits").
    pub fn text_length(&self) -> usize {
        let optional = |text: &Option<String>| text.as_deref().map_or(0, char_count);
        optional(&self.title)
            .saturating_add(optional(&self.description))
            .saturating_add(
                self.fields
                    .iter()
                    .map(|field| char_count(&field.name).saturating_add(char_count(&field.value)))
                    .fold(0, usize::saturating_add),
            )
            .saturating_add(self.footer.as_ref().map_or(0, |f| char_count(&f.text)))
            .saturating_add(self.author.as_ref().map_or(0, |a| char_count(&a.name)))
    }

    /// Checks the per-field limits and the 6000-character total of this
    /// embed. A message also limits the total across all its embeds; the
    /// message's `validate()` checks that.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("")?;
        let total = self.text_length();
        if total > limits::MAX_EMBED_TOTAL_LENGTH {
            return Err(DiscordError::model(format!(
                "embed total length must be at most {} characters, got {total}",
                limits::MAX_EMBED_TOTAL_LENGTH
            )));
        }
        Ok(())
    }

    pub(crate) fn check(&self, path: &str) -> Result<(), DiscordError> {
        if let Some(title) = &self.title {
            check_max_len(path, "embed title", title, limits::MAX_EMBED_TITLE_LENGTH)?;
        }
        if let Some(description) = &self.description {
            check_max_len(
                path,
                "embed description",
                description,
                limits::MAX_EMBED_DESCRIPTION_LENGTH,
            )?;
        }
        if self.fields.len() > limits::MAX_EMBED_FIELDS {
            return Err(at(
                path,
                format!(
                    "embed fields must contain at most {} items, got {}",
                    limits::MAX_EMBED_FIELDS,
                    self.fields.len()
                ),
            ));
        }
        for (index, field) in self.fields.iter().enumerate() {
            check_len(
                path,
                &format!("embed field {index} name"),
                &field.name,
                1,
                limits::MAX_EMBED_FIELD_NAME_LENGTH,
            )?;
            check_len(
                path,
                &format!("embed field {index} value"),
                &field.value,
                1,
                limits::MAX_EMBED_FIELD_VALUE_LENGTH,
            )?;
        }
        if let Some(footer) = &self.footer {
            check_max_len(
                path,
                "embed footer text",
                &footer.text,
                limits::MAX_EMBED_FOOTER_TEXT_LENGTH,
            )?;
        }
        if let Some(author) = &self.author {
            check_max_len(
                path,
                "embed author name",
                &author.name,
                limits::MAX_EMBED_AUTHOR_NAME_LENGTH,
            )?;
        }
        Ok(())
    }
}

impl Serialize for Embed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_some(&mut map, "title", &self.title)?;
        entry_some(&mut map, "description", &self.description)?;
        entry_some(&mut map, "url", &self.url)?;
        entry_some(&mut map, "timestamp", &self.timestamp)?;
        entry_some(&mut map, "color", &self.color)?;
        entry_some(&mut map, "footer", &self.footer)?;
        entry_some(&mut map, "image", &self.image)?;
        entry_some(&mut map, "thumbnail", &self.thumbnail)?;
        entry_some(&mut map, "author", &self.author)?;
        if !self.fields.is_empty() {
            map.serialize_entry("fields", &self.fields)?;
        }
        map.end()
    }
}

/// Embed footer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct EmbedFooter {
    /// Footer text, up to 2048 characters.
    pub text: String,
    /// Icon URL (`http(s)` or `attachment://`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

impl EmbedFooter {
    /// A footer with this text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon_url: None,
        }
    }
}

/// Embed image or thumbnail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct EmbedImage {
    /// Image URL (`http(s)` or `attachment://`).
    pub url: String,
}

impl EmbedImage {
    /// An image at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

/// Embed author line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct EmbedAuthor {
    /// Author name, up to 256 characters.
    pub name: String,
    /// URL the name links to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Icon URL (`http(s)` or `attachment://`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

impl EmbedAuthor {
    /// An author line with this name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: None,
            icon_url: None,
        }
    }
}

/// Embed field. `inline` is always written, as in 2.x.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct EmbedField {
    /// Field name, 1-256 characters.
    pub name: String,
    /// Field value, 1-1024 characters.
    pub value: String,
    /// Whether the field is shown beside its neighbours.
    pub inline: bool,
}

impl EmbedField {
    /// A field with this name, value, and inline setting.
    pub fn new(name: impl Into<String>, value: impl Into<String>, inline: bool) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            inline,
        }
    }
}
