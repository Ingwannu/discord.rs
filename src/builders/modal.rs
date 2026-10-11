use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::command::validation;
use crate::error::DiscordError;
use crate::model::{ComponentId, ComponentType, TextInputStyle};
use crate::types::{to_json_value, SelectOption};

use super::components::{ActionRowBuilder, SelectMenuBuilder};
use super::limits;

#[derive(Clone, Serialize, Deserialize)]
/// Typed Discord API object for `TextInputBuilder`.
pub struct TextInputBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    custom_id: String,
    style: TextInputStyle,
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
    /// Creates a `new` value.
    pub fn new(custom_id: &str, label: &str, style: TextInputStyle) -> Self {
        Self {
            component_type: ComponentType::TEXT_INPUT,
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

    /// Creates a `short` value.
    pub fn short(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, TextInputStyle::SHORT)
    }

    /// Creates a `paragraph` value.
    pub fn paragraph(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, TextInputStyle::PARAGRAPH)
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

    /// Validates the text input against Discord's limits without consuming
    /// the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "text input custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_len(
            "text input label",
            &self.label,
            1,
            limits::MAX_TEXT_INPUT_LABEL_LENGTH,
        )?;
        if let Some(placeholder) = &self.placeholder {
            validation::ensure_max_len(
                "text input placeholder",
                placeholder,
                limits::MAX_TEXT_INPUT_PLACEHOLDER_LENGTH,
            )?;
        }
        if let Some(value) = &self.value {
            validation::ensure_max_len(
                "text input value",
                value,
                usize::from(limits::MAX_TEXT_INPUT_LENGTH),
            )?;
        }
        if let Some(min) = self.min_length {
            if min > limits::MAX_TEXT_INPUT_LENGTH {
                return Err(DiscordError::model(format!(
                    "text input min_length must be 0-{}, got {min}",
                    limits::MAX_TEXT_INPUT_LENGTH
                )));
            }
        }
        if let Some(max) = self.max_length {
            if !(1..=limits::MAX_TEXT_INPUT_LENGTH).contains(&max) {
                return Err(DiscordError::model(format!(
                    "text input max_length must be 1-{}, got {max}",
                    limits::MAX_TEXT_INPUT_LENGTH
                )));
            }
        }
        if let (Some(min), Some(max)) = (self.min_length, self.max_length) {
            if min > max {
                return Err(DiscordError::model(format!(
                    "text input min_length ({min}) must not exceed max_length ({max})"
                )));
            }
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

// [Decision Log]
// - 목적과 의도: 모달 전용 컴포넌트(라디오 그룹, 체크박스 그룹, 체크박스) 빌더가 Discord 문서
//   (components/reference, 3714862)대로 받아들여지는 페이로드만 만들게 한다.
// - 기존 구현 및 제약 조건: 세 빌더 모두 `disabled` setter가 있었다. 문서는 "모달에서 `disabled`를
//   쓰면 오류"라고 하고, 이 셋은 모달에만 들어간다. 체크박스는 `checked` 키로 초기 상태를 보냈지만
//   문서의 키는 `default`이고, 문서는 체크박스에 `required`를 둘 수 없다고 한다.
// - 검토한 주요 대안: (1) setter를 deprecated로 두고 계속 내보내기, (2) deprecated + 아무것도 하지
//   않기, (3) 삭제(컴파일 오류 + 마이그레이션 노트), 초기 상태는 `default_checked`로 내보내고
//   `checked`는 같은 일을 하는 deprecated 별칭.
// - 선택한 방식: (3).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 항상 거절될 페이로드를 계속 만든다. (2)는 사용자
//   의도를 조용히 버린다. 3.0은 어차피 파괴적 변경이므로 컴파일 오류로 알리는 편이 정확하다.
//   `checked`의 뜻(초기 상태)은 유효하므로 별칭으로 남겨 올바른 키를 내보낸다.
// - 장점, 단점 및 영향: `RadioGroupBuilder::disabled`, `CheckboxGroupBuilder::disabled`,
//   `CheckboxBuilder::{disabled, required}` 삭제. 필수 체크박스는 옵션 하나짜리 체크박스 그룹 +
//   `required(true)`로 만든다(문서의 권장).
/// A radio group (modals only; wrap it in a label). Modals cannot contain
/// disabled components, so there is no `disabled` setter.
#[derive(Clone, Serialize, Deserialize)]
pub struct RadioGroupBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    custom_id: String,
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ComponentId>,
}

impl RadioGroupBuilder {
    /// Creates a `new` value.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: ComponentType::RADIO_GROUP,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            required: None,
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

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the radio group against Discord's documented limits without
    /// consuming the builder: `custom_id` 1-100 characters, 2-10 options, and
    /// option labels, values, and descriptions of at most 100 characters.
    ///
    /// [`LabelBuilder`] and [`ModalBuilder`] do not re-check a group they
    /// wrap, so call this before handing the builder to them.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "radio group custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_count(
            "radio group options",
            self.options.len(),
            limits::MIN_RADIO_GROUP_OPTIONS,
            limits::MAX_RADIO_GROUP_OPTIONS,
        )?;
        validate_group_options("radio group", &self.options)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

// [Decision Log]
// - 목적과 의도: 라디오 그룹, 체크박스 그룹, 체크박스도 다른 빌더처럼 `validate()`/`try_build()`로
//   문서(components/reference, 3714862)의 제한 위반을 Discord 400 전에 로컬에서 잡는다.
// - 기존 구현 및 제약 조건: `build()`는 검증하지 않는 동작을 유지한다. 두 그룹의 옵션 구조는
//   label/value/description에 "max 100 characters"만 적는데, 문자열 선택 메뉴 검증기는 label과
//   value를 1자 이상으로도 검사한다. `LabelBuilder`/`ModalBuilder`는 자식을 `Value`로 저장하므로
//   자식 빌더 검증을 따라갈 경로가 없다.
// - 검토한 주요 대안: (1) 선택 메뉴의 옵션 검사 루프를 components.rs 공용 함수로 빼서 같이 쓰기,
//   (2) 두 그룹만 쓰는 모듈 내부 함수, (3) 옵션 수와 min/max_values의 관계(max_values ≤ 옵션 수 등)도
//   검사하기.
// - 선택한 방식: (2). 문서에 적힌 규칙만 검사하고, min_values ≤ max_values는 "최소/최대 선택 수"의
//   정의상 항상 성립해야 하므로(선택 메뉴, 텍스트 입력과 같은 검사) 포함한다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 선택 메뉴의 최소 1자 규칙을 매개변수로 갈라야 해서
//   선택 메뉴 동작을 건드릴 위험만 늘린다. (3)은 문서에 없는 규칙이라 Discord가 받는 페이로드를
//   로컬에서 거절할 수 있다.
// - 장점, 단점 및 영향: 로컬 검증이 문서보다 엄격해지지 않는다. 대신 옵션 수보다 큰 min_values 같은
//   실수는 Discord가 알려 준다. 라벨이나 모달에 넣을 그룹은 넣기 전에 각자 `validate()`해야 한다.
/// Checks each radio or checkbox group option's `label`, `value`, and
/// `description` against the documented "max 100 characters". Unlike the
/// string select check, an empty label or value is not rejected: the group
/// option structures give only a maximum.
fn validate_group_options(component: &str, options: &[SelectOption]) -> Result<(), DiscordError> {
    for option in options {
        let label = &option.label;
        validation::ensure_max_len(
            &format!("{component} option \"{label}\" label"),
            label,
            limits::MAX_SELECT_OPTION_TEXT_LENGTH,
        )?;
        validation::ensure_max_len(
            &format!("{component} option \"{label}\" value"),
            &option.value,
            limits::MAX_SELECT_OPTION_TEXT_LENGTH,
        )?;
        if let Some(description) = &option.description {
            validation::ensure_max_len(
                &format!("{component} option \"{label}\" description"),
                description,
                limits::MAX_SELECT_OPTION_TEXT_LENGTH,
            )?;
        }
    }
    Ok(())
}

/// A checkbox group (modals only; wrap it in a label). Modals cannot contain
/// disabled components, so there is no `disabled` setter.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckboxGroupBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    custom_id: String,
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ComponentId>,
}

impl CheckboxGroupBuilder {
    /// Creates a `new` value.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: ComponentType::CHECKBOX_GROUP,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            min_values: None,
            max_values: None,
            required: None,
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

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the checkbox group against Discord's documented limits
    /// without consuming the builder: `custom_id` 1-100 characters, 1-10
    /// options whose labels, values, and descriptions are at most 100
    /// characters, `min_values` 0-10, `max_values` 1-10, `min_values` not
    /// above `max_values`, and `min_values` 0 only with `required(false)`.
    ///
    /// The values are not compared with the number of options because
    /// Discord documents no such rule. [`LabelBuilder`] and [`ModalBuilder`]
    /// do not re-check a group they wrap, so call this before handing the
    /// builder to them.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "checkbox group custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_count(
            "checkbox group options",
            self.options.len(),
            1,
            limits::MAX_CHECKBOX_GROUP_OPTIONS,
        )?;
        validate_group_options("checkbox group", &self.options)?;
        if let Some(min) = self.min_values {
            if min > limits::MAX_CHECKBOX_GROUP_VALUES {
                return Err(DiscordError::model(format!(
                    "checkbox group min_values must be at most {}, got {min}",
                    limits::MAX_CHECKBOX_GROUP_VALUES
                )));
            }
            // Checkbox Group Structure footnote: "`min_values` must be either
            // omitted or at least `1` if `required` is omitted or `true`."
            // `required` defaults to true, so only an explicit
            // `required(false)` allows 0.
            if min == 0 && self.required != Some(false) {
                return Err(DiscordError::model(
                    "checkbox group min_values must be at least 1 when required is omitted \
                     or true, got 0",
                ));
            }
        }
        if let Some(max) = self.max_values {
            if !(1..=limits::MAX_CHECKBOX_GROUP_VALUES).contains(&max) {
                return Err(DiscordError::model(format!(
                    "checkbox group max_values must be 1-{}, got {max}",
                    limits::MAX_CHECKBOX_GROUP_VALUES
                )));
            }
        }
        // Not a separate rule in the docs, but a minimum number of choices
        // above the maximum can never be met; select menus and text inputs
        // apply the same check.
        if let (Some(min), Some(max)) = (self.min_values, self.max_values) {
            if min > max {
                return Err(DiscordError::model(format!(
                    "checkbox group min_values ({min}) must not exceed max_values ({max})"
                )));
            }
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// A single yes/no checkbox (modals only; wrap it in a label).
///
/// Discord does not let a checkbox be required or disabled; for a box the
/// user must tick, use a [`CheckboxGroupBuilder`] with one option and
/// `required(true)`.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckboxBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    custom_id: String,
    /// Initial state, sent as `default`.
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ComponentId>,
}

impl CheckboxBuilder {
    /// Creates a `new` value.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: ComponentType::CHECKBOX,
            custom_id: custom_id.to_string(),
            default: None,
            id: None,
        }
    }

    /// Whether the checkbox starts checked (Discord's `default`).
    pub fn default_checked(mut self, checked: bool) -> Self {
        self.default = Some(checked);
        self
    }

    /// The 2.x name of [`Self::default_checked`]. 2.x sent it as a
    /// `checked` key, which Discord does not read; this now sends `default`.
    #[deprecated(since = "3.0.0", note = "use default_checked (sent as `default`)")]
    pub fn checked(self, checked: bool) -> Self {
        self.default_checked(checked)
    }

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }

    /// Validates the checkbox against Discord's documented limits without
    /// consuming the builder: `custom_id` 1-100 characters, the only limit
    /// the Checkbox Structure gives.
    ///
    /// [`LabelBuilder`] and [`ModalBuilder`] do not re-check a checkbox they
    /// wrap, so call this before handing the builder to them.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "checkbox custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

#[derive(Clone, Serialize, Deserialize)]
/// Typed Discord API object for `FileUploadBuilder`.
pub struct FileUploadBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ComponentId>,
}

impl FileUploadBuilder {
    /// Creates a `new` value.
    pub fn new(custom_id: &str) -> Self {
        Self {
            component_type: ComponentType::FILE_UPLOAD,
            custom_id: custom_id.to_string(),
            min_values: None,
            max_values: None,
            required: None,
            file_types: None,
            id: None,
        }
    }

    /// Restricts uploads to file types: preset groups (`"image"`,
    /// `"video"`, `"audio"`) or dot-prefixed extensions (`".pdf"`), max 10.
    /// Discord checks only the file name; validate contents yourself.
    pub fn file_types<I, S>(mut self, file_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.file_types = Some(file_types.into_iter().map(Into::into).collect());
        self
    }

    /// Checks the `file_types` filter against Discord's documented limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(file_types) = &self.file_types {
            validation::ensure_file_types("file upload file_types", file_types)?;
        }
        Ok(())
    }

    /// Validates, then builds the component JSON.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
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

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build(self) -> Value {
        to_json_value(self)
    }
}

#[derive(Clone, Serialize, Deserialize)]
/// Typed Discord API object for `LabelBuilder`.
pub struct LabelBuilder {
    #[serde(rename = "type")]
    component_type: ComponentType,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    component: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ComponentId>,
}

impl LabelBuilder {
    /// Creates a `with_select_menu` value.
    pub fn with_select_menu(label: &str, select: SelectMenuBuilder) -> Self {
        Self {
            component_type: ComponentType::LABEL,
            label: label.to_string(),
            description: None,
            component: select.build(),
            id: None,
        }
    }

    /// Creates a `with_file_upload` value.
    pub fn with_file_upload(label: &str, file_upload: FileUploadBuilder) -> Self {
        Self {
            component_type: ComponentType::LABEL,
            label: label.to_string(),
            description: None,
            component: file_upload.build(),
            id: None,
        }
    }

    /// Creates a `with_radio_group` value.
    pub fn with_radio_group(label: &str, radio_group: RadioGroupBuilder) -> Self {
        Self {
            component_type: ComponentType::LABEL,
            label: label.to_string(),
            description: None,
            component: radio_group.build(),
            id: None,
        }
    }

    /// Creates a `with_checkbox_group` value.
    pub fn with_checkbox_group(label: &str, checkbox_group: CheckboxGroupBuilder) -> Self {
        Self {
            component_type: ComponentType::LABEL,
            label: label.to_string(),
            description: None,
            component: checkbox_group.build(),
            id: None,
        }
    }

    /// Creates a `with_checkbox` value.
    pub fn with_checkbox(label: &str, checkbox: CheckboxBuilder) -> Self {
        Self {
            component_type: ComponentType::LABEL,
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

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

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

    /// Creates a `new` value.
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

    /// Validates the modal against Discord's limits without consuming the
    /// builder: custom_id 1-100 characters, title 1-45 characters, and 1-5
    /// top-level components.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "modal custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_len(
            "modal title",
            &self.title,
            1,
            limits::MAX_MODAL_TITLE_LENGTH,
        )?;
        validation::ensure_count(
            "modal components",
            self.components.len(),
            1,
            limits::MAX_MODAL_COMPONENTS,
        )
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Value, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{ComponentId, ComponentType, TextInputStyle};
    use serde_json::json;

    use super::{
        CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, LabelBuilder, ModalBuilder,
        RadioGroupBuilder, TextInputBuilder,
    };
    use crate::builders::{ActionRowBuilder, ButtonBuilder, SelectMenuBuilder};
    use crate::types::SelectOption;

    #[test]
    fn text_input_builder_serializes_helpers_and_optional_fields() {
        let short = TextInputBuilder::short("short-id", "Short").build();
        assert_eq!(
            short.get("style").and_then(|value| value.as_u64()),
            Some(u64::from(TextInputStyle::SHORT.get()))
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
                "type": ComponentType::TEXT_INPUT,
                "custom_id": "paragraph-id",
                "style": TextInputStyle::PARAGRAPH,
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
            .id(ComponentId::new(4))
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(u64::from(ComponentType::RADIO_GROUP.get()))
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
        assert!(payload.get("disabled").is_none());
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
            .id(ComponentId::new(8))
            .build();

        assert_eq!(
            payload,
            json!({
                "type": ComponentType::CHECKBOX_GROUP,
                "custom_id": "checks",
                "options": [
                    {"label": "One", "value": "1"},
                    {"label": "Two", "value": "2"}
                ],
                "min_values": 1,
                "max_values": 2,
                "required": true,
                "id": 8,
            })
        );
    }

    #[test]
    fn checkbox_and_file_upload_builders_serialize_optional_fields() {
        let checkbox = CheckboxBuilder::new("terms")
            .default_checked(true)
            .id(ComponentId::new(3))
            .build();

        // Discord's create field for the initial state is `default`.
        assert_eq!(
            checkbox,
            json!({
                "type": ComponentType::CHECKBOX,
                "custom_id": "terms",
                "default": true,
                "id": 3,
            })
        );
        #[expect(deprecated, reason = "the 2.x setter must send the documented key too")]
        let legacy = CheckboxBuilder::new("terms").checked(false).build();
        assert_eq!(legacy["default"], json!(false));
        assert!(legacy.get("checked").is_none());

        let upload = FileUploadBuilder::new("attachment")
            .min_values(1)
            .max_values(3)
            .required(true)
            .id(ComponentId::new(6))
            .build();

        assert_eq!(
            upload,
            json!({
                "type": ComponentType::FILE_UPLOAD,
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
                .id(ComponentId::new(1))
                .build(),
                ComponentType::STRING_SELECT,
            ),
            (
                LabelBuilder::with_file_upload("Upload", FileUploadBuilder::new("upload")).build(),
                ComponentType::FILE_UPLOAD,
            ),
            (
                LabelBuilder::with_radio_group(
                    "Radio",
                    RadioGroupBuilder::new("radio").add_option(SelectOption::new("One", "1")),
                )
                .build(),
                ComponentType::RADIO_GROUP,
            ),
            (
                LabelBuilder::with_checkbox_group(
                    "Checks",
                    CheckboxGroupBuilder::new("checks").add_option(SelectOption::new("One", "1")),
                )
                .build(),
                ComponentType::CHECKBOX_GROUP,
            ),
            (
                LabelBuilder::with_checkbox("Accept", CheckboxBuilder::new("accept")).build(),
                ComponentType::CHECKBOX,
            ),
        ];

        for (index, (payload, expected_type)) in cases.into_iter().enumerate() {
            assert_eq!(
                payload.get("type").and_then(|value| value.as_u64()),
                Some(u64::from(ComponentType::LABEL.get()))
            );
            assert_eq!(
                payload
                    .get("component")
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_u64()),
                Some(u64::from(expected_type.get()))
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

    fn expect_model_error<T: std::fmt::Debug>(
        result: Result<T, crate::error::DiscordError>,
        needle: &str,
    ) {
        let err = result.expect_err("expected validation failure").to_string();
        assert!(
            err.contains(needle),
            "error message {err:?} should contain {needle:?}"
        );
    }

    #[test]
    fn text_input_try_build_matches_build_for_valid_input() {
        let make = || {
            TextInputBuilder::short("topic", "Topic")
                .placeholder("Tell me")
                .min_length(1)
                .max_length(100)
                .required(true)
        };
        assert_eq!(make().build(), make().try_build().expect("valid input"));
    }

    #[test]
    fn text_input_try_build_rejects_invalid_fields() {
        expect_model_error(
            TextInputBuilder::short(&"i".repeat(101), "Topic").try_build(),
            "text input custom_id must be 1-100 characters, got 101",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "").try_build(),
            "text input label must be 1-45 characters, got 0",
        );
        expect_model_error(
            TextInputBuilder::short("topic", &"l".repeat(46)).try_build(),
            "text input label must be 1-45 characters, got 46",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "Topic")
                .placeholder(&"p".repeat(101))
                .try_build(),
            "text input placeholder must be at most 100 characters, got 101",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "Topic")
                .value(&"v".repeat(4001))
                .try_build(),
            "text input value must be at most 4000 characters, got 4001",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "Topic")
                .min_length(4001)
                .try_build(),
            "text input min_length must be 0-4000, got 4001",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "Topic")
                .max_length(0)
                .try_build(),
            "text input max_length must be 1-4000, got 0",
        );
        expect_model_error(
            TextInputBuilder::short("topic", "Topic")
                .min_length(10)
                .max_length(5)
                .try_build(),
            "text input min_length (10) must not exceed max_length (5)",
        );
    }

    #[test]
    fn modal_try_build_matches_build_and_rejects_invalid_modals() {
        let make = || {
            ModalBuilder::new("modal-id", "Modal Title")
                .add_text_input(TextInputBuilder::short("name", "Name"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid modal"));

        expect_model_error(
            ModalBuilder::new(&"i".repeat(101), "Title")
                .add_text_input(TextInputBuilder::short("name", "Name"))
                .try_build(),
            "modal custom_id must be 1-100 characters, got 101",
        );
        expect_model_error(
            ModalBuilder::new("modal-id", &"t".repeat(46))
                .add_text_input(TextInputBuilder::short("name", "Name"))
                .try_build(),
            "modal title must be 1-45 characters, got 46",
        );
        expect_model_error(
            ModalBuilder::new("modal-id", "Title").try_build(),
            "modal components must contain 1-5 items, got 0",
        );

        let mut overfull = ModalBuilder::new("modal-id", "Title");
        for index in 0..6 {
            overfull =
                overfull.add_text_input(TextInputBuilder::short(&format!("input{index}"), "Label"));
        }
        expect_model_error(
            overfull.try_build(),
            "modal components must contain 1-5 items, got 6",
        );
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
            Some(u64::from(ComponentType::ACTION_ROW.get()))
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

    #[test]
    fn file_upload_builder_supports_file_type_filters() {
        let built = super::FileUploadBuilder::new("upload")
            .file_types(["video", ".mov"])
            .try_build()
            .expect("valid filter");
        assert_eq!(built["file_types"], serde_json::json!(["video", ".mov"]));
        assert!(super::FileUploadBuilder::new("upload")
            .file_types((0..11).map(|index| format!(".e{index}")))
            .try_build()
            .is_err());
        assert!(super::FileUploadBuilder::new("upload")
            .build()
            .get("file_types")
            .is_none());
    }

    /// `count` options with short, distinct labels and values.
    fn numbered_options(count: usize) -> Vec<SelectOption> {
        (0..count)
            .map(|index| SelectOption::new(&format!("Option {index}"), &index.to_string()))
            .collect()
    }

    #[test]
    fn radio_group_try_build_matches_build_for_valid_group() {
        let make = || {
            RadioGroupBuilder::new("class")
                .add_option(SelectOption::new("Mage", "mage").description("Ranged"))
                .add_option(SelectOption::new("Rogue", "rogue").default_selected(true))
                .required(false)
                .id(ComponentId::new(2))
        };
        assert_eq!(
            make().build(),
            make().try_build().expect("valid radio group")
        );
    }

    #[test]
    fn radio_group_validate_enforces_documented_limits() {
        let with_options =
            |count| RadioGroupBuilder::new("class").add_options(numbered_options(count));

        // custom_id: 1-100 characters.
        RadioGroupBuilder::new(&"i".repeat(100))
            .add_options(numbered_options(2))
            .validate()
            .expect("100-character custom_id");
        expect_model_error(
            RadioGroupBuilder::new(&"i".repeat(101))
                .add_options(numbered_options(2))
                .try_build(),
            "radio group custom_id must be 1-100 characters, got 101",
        );
        expect_model_error(
            RadioGroupBuilder::new("")
                .add_options(numbered_options(2))
                .try_build(),
            "radio group custom_id must be 1-100 characters, got 0",
        );

        // options: min 2, max 10.
        with_options(2).validate().expect("two options");
        expect_model_error(
            with_options(1).try_build(),
            "radio group options must contain 2-10 items, got 1",
        );
        with_options(10).validate().expect("ten options");
        expect_model_error(
            with_options(11).try_build(),
            "radio group options must contain 2-10 items, got 11",
        );

        // Option label, value, and description: max 100 characters each.
        let with_option = |option| with_options(1).add_option(option);
        with_option(
            SelectOption::new(&"l".repeat(100), &"v".repeat(100)).description(&"d".repeat(100)),
        )
        .validate()
        .expect("100-character option text");
        expect_model_error(
            with_option(SelectOption::new(&"l".repeat(101), "v")).try_build(),
            "label must be at most 100 characters, got 101",
        );
        expect_model_error(
            with_option(SelectOption::new("Label", &"v".repeat(101))).try_build(),
            "radio group option \"Label\" value must be at most 100 characters, got 101",
        );
        expect_model_error(
            with_option(SelectOption::new("Label", "v").description(&"d".repeat(101))).try_build(),
            "radio group option \"Label\" description must be at most 100 characters, got 101",
        );
        // The docs give option text only a maximum, so empty text is not
        // rejected locally.
        with_option(SelectOption::new("", ""))
            .validate()
            .expect("empty option text has no documented minimum");
    }

    #[test]
    fn checkbox_group_try_build_matches_build_for_valid_group() {
        let make = || {
            CheckboxGroupBuilder::new("events")
                .add_options(numbered_options(3))
                .min_values(1)
                .max_values(2)
                .required(true)
                .id(ComponentId::new(5))
        };
        assert_eq!(
            make().build(),
            make().try_build().expect("valid checkbox group")
        );
    }

    #[test]
    fn checkbox_group_validate_enforces_documented_limits() {
        let with_options =
            |count| CheckboxGroupBuilder::new("events").add_options(numbered_options(count));

        // custom_id: 1-100 characters.
        CheckboxGroupBuilder::new(&"i".repeat(100))
            .add_options(numbered_options(1))
            .validate()
            .expect("100-character custom_id");
        expect_model_error(
            CheckboxGroupBuilder::new(&"i".repeat(101))
                .add_options(numbered_options(1))
                .try_build(),
            "checkbox group custom_id must be 1-100 characters, got 101",
        );

        // options: min 1, max 10.
        with_options(1).validate().expect("one option");
        expect_model_error(
            with_options(0).try_build(),
            "checkbox group options must contain 1-10 items, got 0",
        );
        with_options(10).validate().expect("ten options");
        expect_model_error(
            with_options(11).try_build(),
            "checkbox group options must contain 1-10 items, got 11",
        );

        // Option label, value, and description: max 100 characters each.
        let with_option = |option| with_options(1).add_option(option);
        with_option(
            SelectOption::new(&"l".repeat(100), &"v".repeat(100)).description(&"d".repeat(100)),
        )
        .validate()
        .expect("100-character option text");
        expect_model_error(
            with_option(SelectOption::new(&"l".repeat(101), "v")).try_build(),
            "label must be at most 100 characters, got 101",
        );
        expect_model_error(
            with_option(SelectOption::new("Label", &"v".repeat(101))).try_build(),
            "checkbox group option \"Label\" value must be at most 100 characters, got 101",
        );
        expect_model_error(
            with_option(SelectOption::new("Label", "v").description(&"d".repeat(101))).try_build(),
            "checkbox group option \"Label\" description must be at most 100 characters, got 101",
        );

        // min_values: max 10; max_values: min 1, max 10.
        with_options(10)
            .min_values(10)
            .max_values(10)
            .validate()
            .expect("min_values and max_values at 10");
        expect_model_error(
            with_options(10).min_values(11).try_build(),
            "checkbox group min_values must be at most 10, got 11",
        );
        with_options(1)
            .max_values(1)
            .validate()
            .expect("max_values at 1");
        expect_model_error(
            with_options(1).max_values(0).try_build(),
            "checkbox group max_values must be 1-10, got 0",
        );
        expect_model_error(
            with_options(10).max_values(11).try_build(),
            "checkbox group max_values must be 1-10, got 11",
        );

        // min_values must not exceed max_values.
        with_options(3)
            .min_values(2)
            .max_values(2)
            .validate()
            .expect("min_values equal to max_values");
        expect_model_error(
            with_options(3).min_values(3).max_values(2).try_build(),
            "checkbox group min_values (3) must not exceed max_values (2)",
        );

        // min_values 0 only when required is explicitly false.
        with_options(2)
            .min_values(0)
            .required(false)
            .validate()
            .expect("min_values 0 with required(false)");
        expect_model_error(
            with_options(2).min_values(0).try_build(),
            "checkbox group min_values must be at least 1 when required is omitted or true, got 0",
        );
        expect_model_error(
            with_options(2).min_values(0).required(true).try_build(),
            "checkbox group min_values must be at least 1 when required is omitted or true, got 0",
        );
    }

    #[test]
    fn checkbox_try_build_matches_build_and_enforces_custom_id_length() {
        let make = || {
            CheckboxBuilder::new("like")
                .default_checked(true)
                .id(ComponentId::new(7))
        };
        assert_eq!(make().build(), make().try_build().expect("valid checkbox"));

        CheckboxBuilder::new(&"i".repeat(100))
            .validate()
            .expect("100-character custom_id");
        expect_model_error(
            CheckboxBuilder::new(&"i".repeat(101)).try_build(),
            "checkbox custom_id must be 1-100 characters, got 101",
        );
        expect_model_error(
            CheckboxBuilder::new("").try_build(),
            "checkbox custom_id must be 1-100 characters, got 0",
        );
    }
}
