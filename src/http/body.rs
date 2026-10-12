use reqwest::{
    header::HeaderValue,
    multipart::{Form, Part},
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::DiscordError;
use crate::http::FileAttachment;
use crate::types::invalid_data_error;

pub(crate) enum RequestBody {
    Json(Value),
    Multipart {
        payload_json: Value,
        files: Vec<FileAttachment>,
    },
    NamedFileMultipart {
        field_name: &'static str,
        file: FileAttachment,
    },
    PayloadAndNamedFileMultipart {
        payload_json: Value,
        field_name: &'static str,
        file: FileAttachment,
    },
    StickerMultipart {
        payload_json: Value,
        file: FileAttachment,
    },
}

impl RequestBody {
    /// Returns true when the payload asks Discord to deduplicate it: a
    /// message create with a non-null `nonce` and `enforce_nonce: true`.
    /// Discord then returns the original message for a repeated nonce
    /// instead of creating a second one, which makes the POST safe to replay.
    pub(crate) fn enforces_nonce(&self) -> bool {
        let payload = match self {
            Self::Json(payload)
            | Self::Multipart {
                payload_json: payload,
                ..
            } => payload,
            Self::NamedFileMultipart { .. }
            | Self::PayloadAndNamedFileMultipart { .. }
            | Self::StickerMultipart { .. } => return false,
        };
        payload.get("enforce_nonce").and_then(Value::as_bool) == Some(true)
            && payload.get("nonce").is_some_and(|nonce| !nonce.is_null())
    }
}

pub(crate) fn serialize_body<T: serde::Serialize + ?Sized>(
    body: &T,
) -> Result<Value, DiscordError> {
    serde_json::to_value(body).map_err(Into::into)
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
    serialize_body(body).map(|body| Some(RequestBody::Json(body)))
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
    serialize_body(body).map(|payload_json| {
        Some(RequestBody::Multipart {
            payload_json,
            files: files.to_vec(),
        })
    })
}

pub(crate) fn named_file_multipart_body(
    field_name: &'static str,
    file: &FileAttachment,
) -> RequestBody {
    RequestBody::NamedFileMultipart {
        field_name,
        file: file.clone(),
    }
}

pub(crate) fn payload_named_file_multipart_body<T: serde::Serialize + ?Sized>(
    body: &T,
    field_name: &'static str,
    file: &FileAttachment,
) -> Result<RequestBody, DiscordError> {
    Ok(RequestBody::PayloadAndNamedFileMultipart {
        payload_json: serialize_body(body)?,
        field_name,
        file: file.clone(),
    })
}

pub(crate) fn build_multipart_form(
    payload_json: &Value,
    files: &[FileAttachment],
) -> Result<Form, DiscordError> {
    let payload_json = serde_json::to_string(payload_json)?;
    let mut form = Form::new().text("payload_json", payload_json);

    for (index, file) in files.iter().enumerate() {
        if file.filename.trim().is_empty() {
            return Err(invalid_data_error("file filename must not be empty"));
        }

        let mut part = Part::bytes(file.data.clone()).file_name(file.filename.clone());
        if let Some(content_type) = &file.content_type {
            part = part.mime_str(content_type)?;
        }
        form = form.part(format!("files[{index}]"), part);
    }

    Ok(form)
}

pub(crate) fn build_named_file_form(
    payload_json: Option<&Value>,
    field_name: &'static str,
    file: &FileAttachment,
) -> Result<Form, DiscordError> {
    if field_name.trim().is_empty() {
        return Err(invalid_data_error(
            "multipart file field name must not be empty",
        ));
    }
    if file.filename.trim().is_empty() {
        return Err(invalid_data_error("file filename must not be empty"));
    }

    let mut form = Form::new();
    if let Some(payload_json) = payload_json {
        form = form.text("payload_json", serde_json::to_string(payload_json)?);
    }

    let mut part = Part::bytes(file.data.clone()).file_name(file.filename.clone());
    if let Some(content_type) = &file.content_type {
        part = part.mime_str(content_type)?;
    }

    Ok(form.part(field_name, part))
}

pub(crate) fn build_sticker_form(
    payload_json: &Value,
    file: &FileAttachment,
) -> Result<Form, DiscordError> {
    if file.filename.trim().is_empty() {
        return Err(invalid_data_error("file filename must not be empty"));
    }

    let mut form = Form::new();
    for field in ["name", "description", "tags"] {
        if let Some(value) = payload_json.get(field).and_then(Value::as_str) {
            form = form.text(field.to_string(), value.to_string());
        }
    }

    let mut part = Part::bytes(file.data.clone()).file_name(file.filename.clone());
    if let Some(content_type) = &file.content_type {
        part = part.mime_str(content_type)?;
    }

    Ok(form.part("file", part))
}

pub(crate) fn clone_json_body(body: &Value) -> Value {
    body.clone()
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

/// [`parse_body_value`] for raw bytes, without copying valid UTF-8 into a
/// new `String` first: empty is `Null`, JSON is parsed, other text becomes a
/// `Value::String`, and invalid UTF-8 is replaced before parsing, exactly as
/// before. It parses through `from_str`, the `&str` parser this crate already
/// compiles, because `from_slice` would add a second copy of the JSON parser
/// (about 3,500 LLVM IR lines) to builds without the gateway.
pub(crate) fn parse_body_bytes(body: &[u8]) -> Value {
    match std::str::from_utf8(body) {
        Ok("") => Value::Null,
        Ok(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())),
        Err(_) => parse_body_value(String::from_utf8_lossy(body).into_owned()),
    }
}

pub(crate) fn header_string(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
