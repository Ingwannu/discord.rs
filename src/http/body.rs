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

/// Encodes an optional JSON request body for the transport. Returns the
/// `Result` instead of using `?` so the typed shims in `http.rs` hand it to
/// `RestClient::execute` without an early-return branch of their own.
pub(crate) fn json_body<T: serde::Serialize + ?Sized>(
    body: Option<&T>,
) -> Result<Option<RequestBody>, DiscordError> {
    body.map(serialize_body)
        .transpose()
        .map(|body| body.map(RequestBody::Json))
}

pub(crate) fn multipart_body<T: serde::Serialize + ?Sized>(
    body: &T,
    files: &[FileAttachment],
) -> Result<RequestBody, DiscordError> {
    Ok(RequestBody::Multipart {
        payload_json: serialize_body(body)?,
        files: files.to_vec(),
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
/// - Purpose: one decoder for every typed REST response, without the old
///   bytes -> lossy `String` copy -> [`parse_body_value`] -> `T` detour.
/// - Constraint: `T`'s `Deserialize` impl is compiled once per deserializer
///   type. Gateway events decode the same models (`Message`, `Guild`,
///   `Member`, ...); before phase 4 they did so from a `Value`, and debug
///   builds of downstream crates reuse those instances from this crate.
/// - Alternatives: (1) `serde_json::from_slice::<T>` on the bytes, with a
///   non-generic fallback for non-JSON bodies; (2) parse the bytes into a
///   `Value` and decode `T` from it.
/// - Chosen: (2). Measured on the all-features lib test build, (1) added
///   about 300k LLVM IR lines of `SliceRead` deserializers and removed only
///   63k of `Value` ones (+5.4% total), because every model the gateway
///   also decodes was instantiated twice. (2) only drops the `String` copy.
/// - Trade-offs: responses still build a `Value` tree. Since phase 4 B the
///   gateway decodes from the `d` text (`serde_json::from_str`), so models
///   both paths read are now instantiated twice (measured +354k IR lines in
///   the all-features lib build, about 80k of it duplicated models; see
///   `testing-and-coverage.md`). Moving this function to `from_str` too
///   removes the duplicates but changes error strings and duplicate-key
///   handling, which the decoder equivalence test pins; that decision is
///   open (phase 4 B record in `docs/design/3.0-types-events-requests.md`).
pub(crate) fn decode_json_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, DiscordError> {
    // A `match` instead of `map_err`, which would be one more instance per `T`.
    match serde_json::from_value(parse_body_bytes(body)) {
        Ok(value) => Ok(value),
        Err(error) => Err(error.into()),
    }
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
