use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{component_type, text_input_style};
use crate::types::{to_json_value, SelectOption};

use super::components::{ActionRowBuilder, SelectMenuBuilder};

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `TextInputBuilder`.
pub struct TextInputBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    custom_id: String,
    style: u8,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_length: Option<u16>,
}

impl TextInputBuilder {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str, label: &str, style: u8) -> Self {
        Self {
            component_type: component_type::TEXT_INPUT,
            custom_id: custom_id.to_string(),
            style,
            label: label.to_string(),
            placeholder: None,
            value: None,
            required: None,
            min_length: None,
            max_length: None,
        }
    }

    /// Creates or returns `short` data.
    pub fn short(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, text_input_style::SHORT)
    }

    /// Creates or returns `paragraph` data.
    pub fn paragraph(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, text_input_style::PARAGRAPH)
    }

    /// Runs the `placeholder` operation.
    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    /// Runs the `value` operation.
    pub fn value(mut self, value: &str) -> Self {
        self.value = Some(value.to_string());
        self
    }

    /// Runs the `required` operation.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Runs the `min_length` operation.
    pub fn min_length(mut self, min: u16) -> Self {
        self.min_length = Some(min);
        self
    }

    /// Runs the `max_length` operation.
    pub fn max_length(mut self, max: u16) -> Self {
        self.max_length = Some(max);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RadioGroupBuilder`.
pub struct RadioGroupBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    custom_id: String,
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl RadioGroupBuilder {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: component_type::RADIO_GROUP,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            required: None,
            disabled: None,
            id: None,
        }
    }

    /// Runs the `add_option` operation.
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Runs the `add_options` operation.
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    /// Runs the `required` operation.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Runs the `disabled` operation.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// Runs the `id` operation.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CheckboxGroupBuilder`.
pub struct CheckboxGroupBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    custom_id: String,
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl CheckboxGroupBuilder {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: component_type::CHECKBOX_GROUP,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
            id: None,
        }
    }

    /// Runs the `add_option` operation.
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Runs the `add_options` operation.
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    /// Runs the `min_values` operation.
    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Runs the `max_values` operation.
    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Runs the `required` operation.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Runs the `disabled` operation.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// Runs the `id` operation.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CheckboxBuilder`.
pub struct CheckboxBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl CheckboxBuilder {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: component_type::CHECKBOX,
            custom_id: custom_id.to_string(),
            checked: None,
            required: None,
            disabled: None,
            id: None,
        }
    }

    /// Runs the `checked` operation.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Runs the `required` operation.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Runs the `disabled` operation.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// Runs the `id` operation.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `FileUploadBuilder`.
pub struct FileUploadBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl FileUploadBuilder {
    /// Creates or returns `new` data.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: component_type::FILE_UPLOAD,
            custom_id: custom_id.to_string(),
            min_values: None,
            max_values: None,
            required: None,
            id: None,
        }
    }

    /// Runs the `min_values` operation.
    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Runs the `max_values` operation.
    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Runs the `required` operation.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Runs the `id` operation.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `LabelBuilder`.
pub struct LabelBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    component: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl LabelBuilder {
    /// Creates or returns `with_select_menu` data.
    pub fn with_select_menu(label: &str, select: SelectMenuBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: select.build(),
            id: None,
        }
    }

    /// Creates or returns `with_file_upload` data.
    pub fn with_file_upload(label: &str, file_upload: FileUploadBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: file_upload.build(),
            id: None,
        }
    }

    /// Creates or returns `with_radio_group` data.
    pub fn with_radio_group(label: &str, radio_group: RadioGroupBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: radio_group.build(),
            id: None,
        }
    }

    /// Creates or returns `with_checkbox_group` data.
    pub fn with_checkbox_group(label: &str, checkbox_group: CheckboxGroupBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: checkbox_group.build(),
            id: None,
        }
    }

    /// Creates or returns `with_checkbox` data.
    pub fn with_checkbox(label: &str, checkbox: CheckboxBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: checkbox.build(),
            id: None,
        }
    }

    /// Runs the `description` operation.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Runs the `id` operation.
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ModalBuilder`.
pub struct ModalBuilder {
    custom_id: String,
    title: String,
    components: Vec<Value>,
}

impl ModalBuilder {
    fn with_optional_description(label: LabelBuilder, description: Option<&str>) -> LabelBuilder {
        if let Some(description) = description {
            label.description(description)
        } else {
            label
        }
    }

    /// Creates or returns `new` data.
    pub fn new(custom_id: &str, title: &str) -> Self {
        Self {
            custom_id: custom_id.to_string(),
            title: title.to_string(),
            components: Vec::new(),
        }
    }

    /// Runs the `add_text_input` operation.
    pub fn add_text_input(mut self, input: TextInputBuilder) -> Self {
        let row = ActionRowBuilder::new().add_component(input.build());
        self.components.push(row.build());
        self
    }

    /// Runs the `add_select_menu` operation.
    pub fn add_select_menu(
        mut self,
        label: &str,
        description: Option<&str>,
        select: SelectMenuBuilder,
    ) -> Self {
        self.components.push(
            Self::with_optional_description(
                LabelBuilder::with_select_menu(label, select),
                description,
            )
            .build(),
        );
        self
    }

    /// Runs the `add_file_upload` operation.
    pub fn add_file_upload(
        mut self,
        label: &str,
        description: Option<&str>,
        file_upload: FileUploadBuilder,
    ) -> Self {
        self.components.push(
            Self::with_optional_description(
                LabelBuilder::with_file_upload(label, file_upload),
                description,
            )
            .build(),
        );
        self
    }

    /// Runs the `add_radio_group` operation.
    pub fn add_radio_group(
        mut self,
        label: &str,
        description: Option<&str>,
        radio_group: RadioGroupBuilder,
    ) -> Self {
        self.components.push(
            Self::with_optional_description(
                LabelBuilder::with_radio_group(label, radio_group),
                description,
            )
            .build(),
        );
        self
    }

    /// Runs the `add_checkbox_group` operation.
    pub fn add_checkbox_group(
        mut self,
        label: &str,
        description: Option<&str>,
        checkbox_group: CheckboxGroupBuilder,
    ) -> Self {
        self.components.push(
            Self::with_optional_description(
                LabelBuilder::with_checkbox_group(label, checkbox_group),
                description,
            )
            .build(),
        );
        self
    }

    /// Runs the `add_checkbox` operation.
    pub fn add_checkbox(
        mut self,
        label: &str,
        description: Option<&str>,
        checkbox: CheckboxBuilder,
    ) -> Self {
        self.components.push(
            Self::with_optional_description(
                LabelBuilder::with_checkbox(label, checkbox),
                description,
            )
            .build(),
        );
        self
    }

    /// Runs the `add_label` operation.
    pub fn add_label(mut self, label: LabelBuilder) -> Self {
        self.components.push(label.build());
        self
    }

    /// Runs the `add_action_row` operation.
    pub fn add_action_row(mut self, row: ActionRowBuilder) -> Self {
        self.components.push(row.build());
        self
    }

    /// Runs the `add_component` operation.
    pub fn add_component(mut self, component: Value) -> Self {
        self.components.push(component);
        self
    }

    /// Runs the `build` operation.
    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, LabelBuilder, ModalBuilder,
        RadioGroupBuilder, TextInputBuilder,
    };
    use crate::builders::{ActionRowBuilder, ButtonBuilder, SelectMenuBuilder};
    use crate::constants::{component_type, text_input_style};
    use crate::types::SelectOption;

    #[test]
    fn text_input_builder_serializes_helpers_and_optional_fields() {
        let short = TextInputBuilder::short("short-id", "Short").build();
        assert_eq!(
            short.get("style").and_then(|value| value.as_u64()),
            Some(text_input_style::SHORT as u64)
        );

        let paragraph = TextInputBuilder::paragraph("paragraph-id", "Paragraph")
            .placeholder("type here")
            .value("seed")
            .required(true)
            .min_length(5)
            .max_length(25)
            .build();

        assert_eq!(
            paragraph,
            json!({
                "type": component_type::TEXT_INPUT,
                "custom_id": "paragraph-id",
                "style": text_input_style::PARAGRAPH,
                "label": "Paragraph",
                "placeholder": "type here",
                "value": "seed",
                "required": true,
                "min_length": 5,
                "max_length": 25,
            })
        );
    }

    #[test]
    fn radio_group_builder_serializes_added_options_and_flags() {
        let payload = RadioGroupBuilder::new("radio")
            .add_option(SelectOption::new("One", "1"))
            .add_options(vec![SelectOption::new("Two", "2").default_selected(true)])
            .required(true)
            .disabled(false)
            .id(4)
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::RADIO_GROUP as u64)
        );
        assert_eq!(
            payload
                .get("options")
                .and_then(|value| value.as_array())
                .map(|options| options.len()),
            Some(2)
        );
        assert_eq!(
            payload.get("required").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            payload.get("disabled").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(4));
    }

    #[test]
    fn checkbox_group_builder_serializes_limits_and_flags() {
        let payload = CheckboxGroupBuilder::new("checks")
            .add_option(SelectOption::new("One", "1"))
            .add_options(vec![SelectOption::new("Two", "2")])
            .min_values(1)
            .max_values(2)
            .required(true)
            .disabled(true)
            .id(8)
            .build();

        assert_eq!(
            payload,
            json!({
                "type": component_type::CHECKBOX_GROUP,
                "custom_id": "checks",
                "options": [
                    {"label": "One", "value": "1"},
                    {"label": "Two", "value": "2"}
                ],
                "min_values": 1,
                "max_values": 2,
                "required": true,
                "disabled": true,
                "id": 8,
            })
        );
    }

    #[test]
    fn checkbox_and_file_upload_builders_serialize_optional_fields() {
        let checkbox = CheckboxBuilder::new("terms")
            .checked(true)
            .required(true)
            .disabled(false)
            .id(3)
            .build();

        assert_eq!(
            checkbox,
            json!({
                "type": component_type::CHECKBOX,
                "custom_id": "terms",
                "checked": true,
                "required": true,
                "disabled": false,
                "id": 3,
            })
        );

        let upload = FileUploadBuilder::new("attachment")
            .min_values(1)
            .max_values(3)
            .required(true)
            .id(6)
            .build();

        assert_eq!(
            upload,
            json!({
                "type": component_type::FILE_UPLOAD,
                "custom_id": "attachment",
                "min_values": 1,
                "max_values": 3,
                "required": true,
                "id": 6,
            })
        );
    }

    #[test]
    fn label_builder_helper_constructors_wrap_expected_component_types() {
        let cases = vec![
            (
                LabelBuilder::with_select_menu(
                    "Choose",
                    SelectMenuBuilder::string("select").add_option(SelectOption::new("One", "1")),
                )
                .description("pick one")
                .id(1)
                .build(),
                component_type::STRING_SELECT,
            ),
            (
                LabelBuilder::with_file_upload("Upload", FileUploadBuilder::new("upload")).build(),
                component_type::FILE_UPLOAD,
            ),
            (
                LabelBuilder::with_radio_group(
                    "Radio",
                    RadioGroupBuilder::new("radio").add_option(SelectOption::new("One", "1")),
                )
                .build(),
                component_type::RADIO_GROUP,
            ),
            (
                LabelBuilder::with_checkbox_group(
                    "Checks",
                    CheckboxGroupBuilder::new("checks").add_option(SelectOption::new("One", "1")),
                )
                .build(),
                component_type::CHECKBOX_GROUP,
            ),
            (
                LabelBuilder::with_checkbox("Accept", CheckboxBuilder::new("accept")).build(),
                component_type::CHECKBOX,
            ),
        ];

        for (index, (payload, expected_type)) in cases.into_iter().enumerate() {
            assert_eq!(
                payload.get("type").and_then(|value| value.as_u64()),
                Some(component_type::LABEL as u64)
            );
            assert_eq!(
                payload
                    .get("component")
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_u64()),
                Some(expected_type as u64)
            );
            if index == 0 {
                assert_eq!(
                    payload.get("description").and_then(|value| value.as_str()),
                    Some("pick one")
                );
                assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(1));
            } else {
                assert!(payload.get("description").is_none());
                assert!(payload.get("id").is_none());
            }
        }
    }

    #[test]
    fn modal_builder_composes_components_and_optional_descriptions() {
        let payload = ModalBuilder::new("modal-id", "Modal Title")
            .add_text_input(
                TextInputBuilder::short("name", "Name")
                    .placeholder("Jane")
                    .required(true),
            )
            .add_select_menu(
                "Choose",
                Some("Pick exactly one"),
                SelectMenuBuilder::string("select").add_option(SelectOption::new("One", "1")),
            )
            .add_file_upload("Upload", None, FileUploadBuilder::new("upload"))
            .add_radio_group(
                "Radio",
                Some("Required choice"),
                RadioGroupBuilder::new("radio").add_option(SelectOption::new("One", "1")),
            )
            .add_checkbox_group(
                "Checks",
                None,
                CheckboxGroupBuilder::new("checks").add_option(SelectOption::new("One", "1")),
            )
            .add_checkbox("Accept", Some("Terms"), CheckboxBuilder::new("accept"))
            .add_label(LabelBuilder::with_checkbox(
                "Standalone",
                CheckboxBuilder::new("solo"),
            ))
            .add_action_row(
                ActionRowBuilder::new().add_component(
                    ButtonBuilder::new()
                        .label("Ignored in modal tests")
                        .custom_id("btn")
                        .build(),
                ),
            )
            .add_component(json!({"type": 250, "custom": true}))
            .build();

        let components = payload
            .get("components")
            .and_then(|value| value.as_array())
            .expect("modal components");

        assert_eq!(
            payload.get("custom_id").and_then(|value| value.as_str()),
            Some("modal-id")
        );
        assert_eq!(
            payload.get("title").and_then(|value| value.as_str()),
            Some("Modal Title")
        );
        assert_eq!(components.len(), 9);
        assert_eq!(
            components[0].get("type").and_then(|value| value.as_u64()),
            Some(component_type::ACTION_ROW as u64)
        );
        assert_eq!(
            components[1]
                .get("description")
                .and_then(|value| value.as_str()),
            Some("Pick exactly one")
        );
        assert!(components[2].get("description").is_none());
        assert_eq!(
            components[3]
                .get("description")
                .and_then(|value| value.as_str()),
            Some("Required choice")
        );
        assert!(components[4].get("description").is_none());
        assert_eq!(
            components[5]
                .get("description")
                .and_then(|value| value.as_str()),
            Some("Terms")
        );
        assert_eq!(
            components[8].get("type").and_then(|value| value.as_u64()),
            Some(250)
        );
    }
}
