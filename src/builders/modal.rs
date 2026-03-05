use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{component_type, text_input_style};
use crate::types::{to_json_value, SelectOption};

use super::components::{ActionRowBuilder, SelectMenuBuilder};

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn short(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, text_input_style::SHORT)
    }

    pub fn paragraph(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, text_input_style::PARAGRAPH)
    }

    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    pub fn value(mut self, value: &str) -> Self {
        self.value = Some(value.to_string());
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    pub fn min_length(mut self, min: u16) -> Self {
        self.min_length = Some(min);
        self
    }

    pub fn max_length(mut self, max: u16) -> Self {
        self.max_length = Some(max);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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
    pub fn with_select_menu(label: &str, select: SelectMenuBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: select.build(),
            id: None,
        }
    }

    pub fn with_file_upload(label: &str, file_upload: FileUploadBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: file_upload.build(),
            id: None,
        }
    }

    pub fn with_radio_group(label: &str, radio_group: RadioGroupBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: radio_group.build(),
            id: None,
        }
    }

    pub fn with_checkbox_group(label: &str, checkbox_group: CheckboxGroupBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: checkbox_group.build(),
            id: None,
        }
    }

    pub fn with_checkbox(label: &str, checkbox: CheckboxBuilder) -> Self {
        Self {
            component_type: component_type::LABEL,
            label: label.to_string(),
            description: None,
            component: checkbox.build(),
            id: None,
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
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

    pub fn new(custom_id: &str, title: &str) -> Self {
        Self {
            custom_id: custom_id.to_string(),
            title: title.to_string(),
            components: Vec::new(),
        }
    }

    pub fn add_text_input(mut self, input: TextInputBuilder) -> Self {
        let row = ActionRowBuilder::new().add_component(input.build());
        self.components.push(row.build());
        self
    }

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

    pub fn add_label(mut self, label: LabelBuilder) -> Self {
        self.components.push(label.build());
        self
    }

    pub fn add_action_row(mut self, row: ActionRowBuilder) -> Self {
        self.components.push(row.build());
        self
    }

    pub fn add_component(mut self, component: Value) -> Self {
        self.components.push(component);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}
