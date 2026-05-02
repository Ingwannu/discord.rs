use reqwest::{
    header::HeaderValue,
    multipart::{Form, Part},
};
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

pub(crate) fn serialize_body<T: serde::Serialize + ?Sized>(
    body: &T,
) -> Result<Value, DiscordError> {
    serde_json::to_value(body).map_err(Into::into)
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

pub(crate) fn header_string(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
