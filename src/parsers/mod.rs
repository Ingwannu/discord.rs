use serde_json::Value;

use crate::error::DiscordError;
use crate::types::invalid_data_error;

/// Public module for `interaction` APIs.
pub mod interaction;
/// Public module for `modal` APIs.
pub mod modal;

pub use interaction::{
    parse_interaction, parse_interaction_context, parse_raw_interaction, InteractionContext,
    RawInteraction,
};
pub use modal::{parse_modal_submission, V2ModalComponent, V2ModalSubmission};

pub(crate) fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(inner) => Some(inner.clone()),
        Value::Number(inner) => Some(inner.to_string()),
        _ => None,
    }
}

pub(crate) fn value_to_u8(value: &Value) -> Option<u8> {
    match value {
        Value::Number(inner) => inner.as_u64().and_then(|raw| u8::try_from(raw).ok()),
        Value::String(inner) => inner.parse::<u8>().ok(),
        _ => None,
    }
}

pub(crate) fn optional_string_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(value_to_string)
}

pub(crate) fn required_string_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<String, DiscordError> {
    value
        .get(field)
        .and_then(value_to_string)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid {context}.{field}")))
}

pub(crate) fn required_u8_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<u8, DiscordError> {
    value
        .get(field)
        .and_then(value_to_u8)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid {context}.{field}")))
}

pub(crate) fn required_object_field<'a>(
    value: &'a Value,
    field: &str,
    context: &str,
) -> Result<&'a Value, DiscordError> {
    match value.get(field) {
        Some(inner) if inner.is_object() => Ok(inner),
        Some(_) => Err(invalid_data_error(format!(
            "{context}.{field} must be an object"
        ))),
        None => Err(invalid_data_error(format!("missing {context}.{field}"))),
    }
}

pub(crate) fn required_array_field<'a>(
    value: &'a Value,
    field: &str,
    context: &str,
) -> Result<&'a [Value], DiscordError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid {context}.{field}")))
}

pub(crate) fn required_bool_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<bool, DiscordError> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid {context}.{field}")))
}

pub(crate) fn required_string_values_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<Vec<String>, DiscordError> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid {context}.{field}")))?;

    let mut parsed_values = Vec::with_capacity(values.len());
    for entry in values {
        let parsed = value_to_string(entry)
            .ok_or_else(|| invalid_data_error(format!("{context}.{field} must contain strings")))?;
        parsed_values.push(parsed);
    }

    Ok(parsed_values)
}

pub(crate) fn optional_string_values_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<Option<Vec<String>>, DiscordError> {
    match value.get(field) {
        Some(Value::Array(values)) => {
            let mut parsed_values = Vec::with_capacity(values.len());
            for entry in values {
                let parsed = value_to_string(entry).ok_or_else(|| {
                    invalid_data_error(format!("{context}.{field} must contain strings"))
                })?;
                parsed_values.push(parsed);
            }
            Ok(Some(parsed_values))
        }
        Some(_) => Err(invalid_data_error(format!(
            "{context}.{field} must be an array"
        ))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        optional_string_field, optional_string_values_field, required_array_field,
        required_bool_field, required_object_field, required_string_field,
        required_string_values_field, required_u8_field, value_to_string, value_to_u8,
    };
    use crate::error::DiscordError;

    fn model_message(error: DiscordError) -> String {
        match error {
            DiscordError::Model { message } => message,
            other => panic!("expected model error, got {other:?}"),
        }
    }

    #[test]
    fn value_converters_accept_strings_and_numbers() {
        assert_eq!(value_to_string(&json!("123")), Some(String::from("123")));
        assert_eq!(value_to_string(&json!(456)), Some(String::from("456")));
        assert_eq!(value_to_string(&json!(true)), None);

        assert_eq!(value_to_u8(&json!(7)), Some(7));
        assert_eq!(value_to_u8(&json!("8")), Some(8));
        assert_eq!(value_to_u8(&json!(300)), None);
        assert_eq!(value_to_u8(&json!("bad")), None);
    }

    #[test]
    fn field_helpers_extract_required_and_optional_values() {
        let value = json!({
            "id": "123",
            "count": "7",
            "payload": { "ok": true },
            "items": [1, 2],
            "enabled": true,
        });

        assert_eq!(
            optional_string_field(&value, "id"),
            Some(String::from("123"))
        );
        assert_eq!(optional_string_field(&value, "missing"), None);
        assert_eq!(
            required_string_field(&value, "id", "interaction").unwrap(),
            "123"
        );
        assert_eq!(
            required_u8_field(&value, "count", "interaction").unwrap(),
            7
        );
        assert_eq!(
            required_object_field(&value, "payload", "interaction")
                .unwrap()
                .get("ok"),
            Some(&json!(true))
        );
        assert_eq!(
            required_array_field(&value, "items", "interaction").unwrap(),
            &[json!(1), json!(2)]
        );
        assert!(required_bool_field(&value, "enabled", "interaction").unwrap());
    }

    #[test]
    fn field_helpers_report_invalid_shapes() {
        let value = json!({
            "wrong_string": false,
            "wrong_number": 512,
            "wrong_object": [],
            "wrong_array": {},
            "wrong_bool": "true",
        });

        assert_eq!(
            model_message(
                required_string_field(&value, "wrong_string", "interaction").unwrap_err()
            ),
            "missing or invalid interaction.wrong_string"
        );
        assert_eq!(
            model_message(required_u8_field(&value, "wrong_number", "interaction").unwrap_err()),
            "missing or invalid interaction.wrong_number"
        );
        assert_eq!(
            model_message(
                required_object_field(&value, "wrong_object", "interaction").unwrap_err()
            ),
            "interaction.wrong_object must be an object"
        );
        assert_eq!(
            model_message(required_array_field(&value, "wrong_array", "interaction").unwrap_err()),
            "missing or invalid interaction.wrong_array"
        );
        assert_eq!(
            model_message(required_bool_field(&value, "wrong_bool", "interaction").unwrap_err()),
            "missing or invalid interaction.wrong_bool"
        );
        assert_eq!(
            model_message(required_object_field(&value, "missing", "interaction").unwrap_err()),
            "missing interaction.missing"
        );
    }

    #[test]
    fn string_values_helpers_parse_arrays_and_reject_invalid_entries() {
        let valid = json!({
            "values": ["one", 2, "three"],
            "tags": ["alpha", "beta"],
        });
        let invalid_entry = json!({ "values": ["ok", true] });
        let invalid_shape = json!({ "values": "nope" });

        assert_eq!(
            required_string_values_field(&valid, "values", "component_data").unwrap(),
            vec![
                String::from("one"),
                String::from("2"),
                String::from("three")
            ]
        );
        assert_eq!(
            optional_string_values_field(&valid, "tags", "component_data").unwrap(),
            Some(vec![String::from("alpha"), String::from("beta")])
        );
        assert_eq!(
            optional_string_values_field(&valid, "missing", "component_data").unwrap(),
            None
        );
        assert_eq!(
            model_message(
                required_string_values_field(&invalid_entry, "values", "component_data")
                    .unwrap_err()
            ),
            "component_data.values must contain strings"
        );
        assert_eq!(
            model_message(
                optional_string_values_field(&invalid_entry, "values", "component_data")
                    .unwrap_err()
            ),
            "component_data.values must contain strings"
        );
        assert_eq!(
            model_message(
                optional_string_values_field(&invalid_shape, "values", "component_data")
                    .unwrap_err()
            ),
            "component_data.values must be an array"
        );
    }
}
