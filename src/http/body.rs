use bytes::Bytes;
use reqwest::{
    header::HeaderValue,
    multipart::{Form, Part},
    Body,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::DiscordError;
use crate::http::FileAttachment;
use crate::model::CreateGuildSticker;
use crate::types::invalid_data_error;

// [Decision Log] Encoded request bodies
// - Purpose: encode an outgoing body once, straight from the typed request
//   into bytes, and send those bytes on every attempt (3.0 phase 5,
//   docs/design/3.0-types-events-requests.md "5단계 실행 결정").
// - Before: `serialize_body` turned every body into a `serde_json::Value`
//   tree (`to_value`), the transport re-encoded that tree on every attempt
//   (`RequestBuilder::json`, `to_string` for `payload_json`), and the replay
//   policy and the sticker form read fields back out of the tree.
// - Alternatives: (1) keep `Value` and only cache the encoded bytes; (2) hold
//   the typed body behind `&dyn erased_serde::Serialize` and encode per
//   attempt; (3) encode with `serde_json::to_vec` at the call site and keep
//   `Bytes`, with the metadata the transport needs (nonce replay) recorded
//   next to them from the typed request.
// - Chosen: (3). `encode_json` is the only encoder; every body type
//   serializes through serde_json's one `Serializer<&mut Vec<u8>>`, which
//   `to_string` also uses, so no second serializer is instantiated. Files
//   are copied into `Bytes` once at encode time; a retry clones handles.
// - Why: (1) still builds the tree (most of the transport's allocations for
//   a Components V2 message) and the reads of it. (2) adds a dependency and
//   re-encodes per attempt. With (3) the body cannot change between
//   attempts, and nothing reads JSON back.
// - Trade-offs: object keys now follow the request type's field order
//   instead of `Value`'s sorted order (same JSON meaning; Discord does not
//   depend on key order). A body that fails to encode fails before anything
//   is sent, as before. `bytes` becomes a direct dependency (it already was
//   one of reqwest's).

/// An encoded request body, ready to be sent any number of times.
pub(crate) enum RequestBody {
    /// `application/json`.
    Json {
        bytes: Bytes,
        /// The body is a message create with `nonce` + `enforce_nonce`,
        /// read from the typed request when it was encoded.
        enforces_nonce: bool,
    },
    /// `multipart/form-data`: `payload_json` plus `files[n]`.
    Multipart {
        payload_json: Bytes,
        files: Vec<EncodedFile>,
        enforces_nonce: bool,
    },
    /// One file under a route-specific field name and nothing else.
    NamedFileMultipart {
        field_name: &'static str,
        file: EncodedFile,
    },
    /// `payload_json` plus one file under a route-specific field name.
    PayloadAndNamedFileMultipart {
        payload_json: Bytes,
        field_name: &'static str,
        file: EncodedFile,
    },
    /// Create Guild Sticker's form: text fields, not `payload_json`.
    StickerMultipart {
        name: String,
        description: String,
        tags: String,
        file: EncodedFile,
    },
}

/// A file part: name, content type, and the data as shared bytes.
#[derive(Clone)]
pub(crate) struct EncodedFile {
    filename: String,
    content_type: Option<String>,
    data: Bytes,
}

impl EncodedFile {
    /// Copies `file` once. An empty file name fails here, before anything is
    /// sent.
    fn new(file: &FileAttachment) -> Result<Self, DiscordError> {
        Self::check(file)?;
        Ok(Self {
            filename: file.filename.clone(),
            content_type: file.content_type.clone(),
            data: Bytes::copy_from_slice(&file.data),
        })
    }

    /// [`Self::new`] without copying the data.
    fn owned(file: FileAttachment) -> Result<Self, DiscordError> {
        Self::check(&file)?;
        Ok(Self {
            filename: file.filename,
            content_type: file.content_type,
            data: Bytes::from(file.data),
        })
    }

    fn check(file: &FileAttachment) -> Result<(), DiscordError> {
        if file.filename.trim().is_empty() {
            return Err(invalid_data_error("file filename must not be empty"));
        }
        Ok(())
    }

    fn part(&self) -> Result<Part, DiscordError> {
        let mut part = Part::stream(Body::from(self.data.clone())).file_name(self.filename.clone());
        if let Some(content_type) = &self.content_type {
            part = part.mime_str(content_type)?;
        }
        Ok(part)
    }
}

impl RequestBody {
    /// A JSON body from bytes the caller already encoded (for example with
    /// [`encode_json`]); sent as-is on every attempt.
    pub(crate) fn json(bytes: Vec<u8>) -> Self {
        Self::Json {
            bytes: Bytes::from(bytes),
            enforces_nonce: false,
        }
    }

    /// A `payload_json` + `files[n]` body from bytes the caller already
    /// encoded.
    pub(crate) fn multipart(payload_json: Vec<u8>, files: &[FileAttachment]) -> EncodedBody {
        encode_files(files).map(|files| {
            Some(Self::Multipart {
                payload_json: Bytes::from(payload_json),
                files,
                enforces_nonce: false,
            })
        })
    }

    /// Returns true when the payload asks Discord to deduplicate it: a
    /// message create with a non-null `nonce` and `enforce_nonce: true`.
    /// Discord then returns the original message for a repeated nonce
    /// instead of creating a second one, which makes the POST safe to replay.
    /// The flag comes from the typed request, never from the encoded bytes.
    pub(crate) fn enforces_nonce(&self) -> bool {
        match self {
            Self::Json { enforces_nonce, .. } | Self::Multipart { enforces_nonce, .. } => {
                *enforces_nonce
            }
            Self::NamedFileMultipart { .. }
            | Self::PayloadAndNamedFileMultipart { .. }
            | Self::StickerMultipart { .. } => false,
        }
    }

    /// True for the `multipart/form-data` variants.
    pub(crate) fn is_multipart(&self) -> bool {
        !matches!(self, Self::Json { .. })
    }

    /// Hands this body to `request` for one attempt. JSON and `payload_json`
    /// bytes are shared, not copied; a multipart form is rebuilt per attempt
    /// because reqwest consumes it.
    pub(crate) fn attach(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, DiscordError> {
        Ok(match self {
            Self::Json { bytes, .. } => request.body(bytes.clone()),
            Self::Multipart {
                payload_json,
                files,
                ..
            } => {
                let mut form = Form::new().part("payload_json", bytes_part(payload_json));
                for (index, file) in files.iter().enumerate() {
                    form = form.part(format!("files[{index}]"), file.part()?);
                }
                request.multipart(form)
            }
            Self::NamedFileMultipart { field_name, file } => {
                request.multipart(Form::new().part(*field_name, file.part()?))
            }
            Self::PayloadAndNamedFileMultipart {
                payload_json,
                field_name,
                file,
            } => request.multipart(
                Form::new()
                    .part("payload_json", bytes_part(payload_json))
                    .part(*field_name, file.part()?),
            ),
            Self::StickerMultipart {
                name,
                description,
                tags,
                file,
            } => request.multipart(
                Form::new()
                    .text("name", name.clone())
                    .text("description", description.clone())
                    .text("tags", tags.clone())
                    .part("file", file.part()?),
            ),
        })
    }
}

/// A text part over shared bytes (no copy; the length stays known, so the
/// form keeps its `Content-Length`).
fn bytes_part(bytes: &Bytes) -> Part {
    Part::stream(Body::from(bytes.clone()))
}

fn encode_files(files: &[FileAttachment]) -> Result<Vec<EncodedFile>, DiscordError> {
    files.iter().map(EncodedFile::new).collect()
}

/// Encodes `body` as JSON with `serde_json::to_vec`. The one JSON encoder
/// for outgoing bodies; see the Decision Log above [`RequestBody`].
pub(crate) fn encode_json<T: serde::Serialize + ?Sized>(body: &T) -> Result<Vec<u8>, DiscordError> {
    serde_json::to_vec(body).map_err(Into::into)
}

/// A request body ready for the transport, or the error that encoding it
/// produced. Endpoints build it before the single await and pass it to the
/// shims in `http.rs`, which hand it to `RestClient::execute` without an
/// early-return branch of their own; `execute` returns the error before
/// anything is sent.
pub(crate) type EncodedBody = Result<Option<RequestBody>, DiscordError>;

/// A request without a body.
pub(crate) const NO_BODY: EncodedBody = Ok(None);

/// Encodes a JSON request body.
pub(crate) fn json_body<T: serde::Serialize + ?Sized>(body: &T) -> EncodedBody {
    encode_json(body).map(|bytes| Some(RequestBody::json(bytes)))
}

/// [`json_body`] for a message create, recording whether it enforces its
/// nonce (read from the typed request by the caller).
pub(crate) fn message_json_body<T: serde::Serialize + ?Sized>(
    body: &T,
    enforces_nonce: bool,
) -> EncodedBody {
    encode_json(body).map(|bytes| {
        Some(RequestBody::Json {
            bytes: Bytes::from(bytes),
            enforces_nonce,
        })
    })
}

/// [`json_body`] for an optional body.
pub(crate) fn optional_json_body<T: serde::Serialize + ?Sized>(body: Option<&T>) -> EncodedBody {
    match body {
        Some(body) => json_body(body),
        None => NO_BODY,
    }
}

/// Encodes a `multipart/form-data` body: `payload_json` plus `files[n]`.
pub(crate) fn multipart_body<T: serde::Serialize + ?Sized>(
    body: &T,
    files: &[FileAttachment],
) -> EncodedBody {
    RequestBody::multipart(encode_json(body)?, files)
}

/// [`multipart_body`] for a message create; see [`message_json_body`].
pub(crate) fn message_multipart_body<T: serde::Serialize + ?Sized>(
    body: &T,
    files: &[FileAttachment],
    enforces_nonce: bool,
) -> EncodedBody {
    let payload_json = encode_json(body)?;
    Ok(Some(RequestBody::Multipart {
        payload_json: Bytes::from(payload_json),
        files: encode_files(files)?,
        enforces_nonce,
    }))
}

pub(crate) fn named_file_multipart_body(
    field_name: &'static str,
    file: &FileAttachment,
) -> EncodedBody {
    if field_name.trim().is_empty() {
        return Err(invalid_data_error(
            "multipart file field name must not be empty",
        ));
    }
    Ok(Some(RequestBody::NamedFileMultipart {
        field_name,
        file: EncodedFile::new(file)?,
    }))
}

pub(crate) fn payload_named_file_multipart_body<T: serde::Serialize + ?Sized>(
    body: &T,
    field_name: &'static str,
    file: &FileAttachment,
) -> EncodedBody {
    if field_name.trim().is_empty() {
        return Err(invalid_data_error(
            "multipart file field name must not be empty",
        ));
    }
    Ok(Some(RequestBody::PayloadAndNamedFileMultipart {
        payload_json: Bytes::from(encode_json(body)?),
        field_name,
        file: EncodedFile::new(file)?,
    }))
}

/// Create Guild Sticker's form from the typed request's fields. Takes the
/// file by value so its data moves into the body without a copy.
pub(crate) fn sticker_body(body: &CreateGuildSticker, file: FileAttachment) -> EncodedBody {
    Ok(Some(RequestBody::StickerMultipart {
        name: body.name.clone(),
        description: body.description.clone(),
        tags: body.tags.clone(),
        file: EncodedFile::owned(file)?,
    }))
}

pub(crate) fn parse_body_value(response_text: String) -> Value {
    if response_text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&response_text).unwrap_or(Value::String(response_text))
    }
}

/// Decodes a response body into `T`.
///
/// [Decision Log] Response decoding
/// - Purpose: one decoder for every typed REST response, and every model's
///   `Deserialize` compiled once in the whole crate.
/// - Constraint: `T`'s `Deserialize` impl is compiled once per deserializer
///   type. Since phase 4 B the gateway decodes the same models (`Message`,
///   `Guild`, `Member`, ...) from the dispatch text with `serde_json::from_str`
///   (the `&str` reader), and debug builds of downstream crates reuse those
///   instances from this crate.
/// - Alternatives: (1) parse the bytes into a `Value` and decode `T` from it
///   (until 3.0 phase 4); (2) `serde_json::from_slice::<T>` on the bytes;
///   (3) `serde_json::from_str::<T>` on the bytes read as UTF-8.
/// - Chosen: (3). The generic part is the `from_str` call; reading the bytes
///   as text ([`body_text`]) and the non-JSON fallback ([`quote_non_json`])
///   are not generic. Valid JSON is scanned once; only a failed decode checks
///   whether the body was JSON at all.
/// - Why: (1) instantiated every model a second time for the `Value` reader
///   once the gateway moved to text (+354k LLVM IR lines in the all-features
///   lib build, about 80k of it duplicated models), and builds a `Value` tree
///   per response. (2) is a third reader type (`SliceRead`) and a second copy
///   of the JSON parser. (3) shares the gateway's instances.
/// - Trade-offs (3.0 behaviour change): errors carry serde_json's position
///   (`... at line 1 column 7`); a duplicated key in an object decoded into a
///   struct is a "duplicate field" error instead of the last value winning;
///   a number out of `f64` range fails only where it is read (an ignored
///   field no longer turns the whole body into a string). Empty bodies,
///   non-JSON text, and invalid UTF-8 read as before.
pub(crate) fn decode_json_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, DiscordError> {
    let text = body_text(body);
    // `match` instead of `map_err`/`or_else`, which would be one more
    // instance per `T`.
    match serde_json::from_str(&text) {
        Ok(value) => Ok(value),
        Err(error) => match quote_non_json(&text) {
            Some(quoted) => match serde_json::from_str(&quoted) {
                Ok(value) => Ok(value),
                Err(error) => Err(error.into()),
            },
            None => Err(error.into()),
        },
    }
}

/// The body as text: borrowed when it is valid UTF-8, replaced (lossy)
/// otherwise, and `null` when empty.
fn body_text(body: &[u8]) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;

    match std::str::from_utf8(body) {
        Ok("") => Cow::Borrowed("null"),
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(String::from_utf8_lossy(body).into_owned()),
    }
}

/// `text` as a JSON string when it is not JSON (a proxy's HTML page, a
/// plain-text answer), so a `String` or `Value` target still receives it as
/// text, as before; `None` when it is JSON.
fn quote_non_json(text: &str) -> Option<String> {
    if serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok() {
        return None;
    }
    // Serializing a `str` cannot fail.
    serde_json::to_string(text).ok()
}

pub(crate) fn header_string(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
