//! Attachment metadata in message requests (Attachment Request Structure).
//!
//! The `attachments` array names two different things: attachments already
//! on a message, by their snowflake [`AttachmentId`], and files uploaded in
//! the same multipart request, by the `n` of their `files[n]` part
//! ([`UploadId`]). They share the JSON key `id` but not its meaning, so they
//! are two variants with two id types and cannot be mixed up. The file bytes
//! travel separately, as the multipart parts the REST client builds.

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use crate::builders::limits;
use crate::error::DiscordError;
use crate::model::{AttachmentId, UploadId};

use super::component::{check_max_len, entry_some};

/// One entry of a message's `attachments` array.
///
/// On edits the array lists every attachment the message keeps: existing
/// ones not listed are removed ("the `attachments` array must contain all
/// attachments that should be present after edit, including retained and
/// new attachments").
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MessageAttachment {
    /// An attachment already on the message, to keep.
    Existing(ExistingAttachment),
    /// A file uploaded as `files[n]` in the same request.
    Upload(AttachmentUpload),
}

impl MessageAttachment {
    /// Keeps the existing attachment `id` unchanged.
    pub fn keep(id: AttachmentId) -> Self {
        Self::Existing(ExistingAttachment::new(id))
    }

    /// Describes the upload `files[n]`.
    pub fn upload(id: UploadId) -> Self {
        Self::Upload(AttachmentUpload::new(id))
    }

    /// Checks the description length.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("")
    }

    pub(crate) fn check(&self, path: &str) -> Result<(), DiscordError> {
        let description = match self {
            Self::Existing(existing) => existing.description.as_deref(),
            Self::Upload(upload) => upload.description.as_deref(),
        };
        if let Some(description) = description {
            check_max_len(
                path,
                "attachment description",
                description,
                limits::MAX_ATTACHMENT_DESCRIPTION_LENGTH,
            )?;
        }
        Ok(())
    }
}

impl Serialize for MessageAttachment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Existing(existing) => existing.serialize(serializer),
            Self::Upload(upload) => upload.serialize(serializer),
        }
    }
}

impl From<ExistingAttachment> for MessageAttachment {
    fn from(existing: ExistingAttachment) -> Self {
        Self::Existing(existing)
    }
}

impl From<AttachmentUpload> for MessageAttachment {
    fn from(upload: AttachmentUpload) -> Self {
        Self::Upload(upload)
    }
}

/// An attachment already on the message. Only `description` and
/// `is_spoiler` can change on an existing attachment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExistingAttachment {
    /// The stored attachment.
    pub id: AttachmentId,
    /// New alt text, up to 1024 characters.
    pub description: Option<String>,
    /// Marks or unmarks the attachment as a spoiler.
    pub is_spoiler: Option<bool>,
}

impl ExistingAttachment {
    /// Keeps attachment `id` as it is.
    pub fn new(id: AttachmentId) -> Self {
        Self {
            id,
            description: None,
            is_spoiler: None,
        }
    }
}

impl Serialize for ExistingAttachment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        entry_some(&mut map, "description", &self.description)?;
        entry_some(&mut map, "is_spoiler", &self.is_spoiler)?;
        map.end()
    }
}

/// Metadata for the file uploaded as `files[n]`, where `n` is [`Self::id`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AttachmentUpload {
    /// The `n` of the `files[n]` part.
    pub id: UploadId,
    /// File name shown in Discord; defaults to the part's file name.
    pub filename: Option<String>,
    /// File title.
    pub title: Option<String>,
    /// Alt text, up to 1024 characters.
    pub description: Option<String>,
    /// Length of an audio or video file in seconds (required for voice
    /// messages).
    pub duration_secs: Option<f64>,
    /// Base64 sampled waveform (required for voice messages).
    pub waveform: Option<String>,
    /// Marks the attachment as a spoiler.
    pub is_spoiler: Option<bool>,
}

impl AttachmentUpload {
    /// Metadata for `files[id]`.
    pub fn new(id: UploadId) -> Self {
        Self {
            id,
            filename: None,
            title: None,
            description: None,
            duration_secs: None,
            waveform: None,
            is_spoiler: None,
        }
    }

    /// Sets the file name.
    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Sets the alt text.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Marks the attachment as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.is_spoiler = Some(spoiler);
        self
    }
}

impl Serialize for AttachmentUpload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        entry_some(&mut map, "filename", &self.filename)?;
        entry_some(&mut map, "title", &self.title)?;
        entry_some(&mut map, "description", &self.description)?;
        entry_some(&mut map, "duration_secs", &self.duration_secs)?;
        entry_some(&mut map, "waveform", &self.waveform)?;
        entry_some(&mut map, "is_spoiler", &self.is_spoiler)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentUpload, ExistingAttachment, MessageAttachment};
    use crate::model::{AttachmentId, UploadId};

    #[test]
    fn descriptions_are_limited_on_both_kinds() {
        let long = "d".repeat(1025);
        let mut existing = ExistingAttachment::new(AttachmentId::try_new(1).unwrap());
        existing.description = Some(long.clone());
        let upload = AttachmentUpload::new(UploadId::new(0)).description(long);
        for attachment in [
            MessageAttachment::from(existing),
            MessageAttachment::from(upload),
        ] {
            assert!(attachment
                .validate()
                .unwrap_err()
                .to_string()
                .contains("attachment description must be at most 1024 characters, got 1025"));
        }
        MessageAttachment::keep(AttachmentId::try_new(1).unwrap())
            .validate()
            .expect("no description");
        MessageAttachment::upload(UploadId::new(2))
            .validate()
            .expect("no description");
    }

    #[test]
    fn upload_setters() {
        let upload = AttachmentUpload::new(UploadId::new(1))
            .filename("a.png")
            .description("alt")
            .spoiler(true);
        assert_eq!(upload.filename.as_deref(), Some("a.png"));
        assert_eq!(upload.description.as_deref(), Some("alt"));
        assert_eq!(upload.is_spoiler, Some(true));
        assert_eq!(
            serde_json::to_string(&MessageAttachment::from(upload)).unwrap(),
            r#"{"id":1,"filename":"a.png","description":"alt","is_spoiler":true}"#
        );
    }
}
