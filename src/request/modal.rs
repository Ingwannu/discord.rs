//! Modal request bodies: the `data` of a `MODAL` interaction callback
//! (type 9).
//!
//! [Decision Log]
//! - 목적과 의도: 모달 본문을 `Value` 없이 Discord 규격(discord-api-docs `3714862`,
//!   components/reference의 Label·Text Input·File Upload·Radio Group·Checkbox Group·Checkbox)이
//!   허용하는 형태만 표현하는 타입으로 만들고, REST 콜백·HTTP 엔드포인트·응답기가 같은 타입을
//!   한 번에 바이트로 직렬화하게 한다.
//! - 기존 구현 및 제약 조건: 2.x `ModalBuilder`는 `Vec<Value>`를 들고 `add_text_input`이 텍스트
//!   입력을 Action Row로 감쌌다(Discord가 deprecated로 바꾼 형태). `add_action_row`/`add_component`는
//!   메시지용 행과 임의 JSON을 받았다. 라디오·체크박스 검증기는 `builders/modal.rs`에 있었다.
//!   메시지 행·선택 메뉴 타입은 워커 A의 `request::component`가 소유한다.
//! - 검토한 주요 대안: (1) 모달 자식을 메시지 컴포넌트 enum 하나에 같이 두고 실행 시점에 검사,
//!   (2) 모달 전용 enum(`ModalComponent`, `LabelComponent`) + 선택 메뉴만 메시지와 공유,
//!   (3) 텍스트 입력에 `label` 필드를 두고 Label 안/밖에서 다르게 직렬화.
//! - 선택한 방식: (2). 최상위는 Label·Text Display·`Raw`이고, Action Row + Text Input은
//!   deprecated 생성자로만 만들 수 있는 `LegacyTextInputRow`다. 텍스트 입력에는 `label`이 없고,
//!   레거시 행이 직렬화할 때만 자기 `label`을 텍스트 입력 객체에 넣는다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 텍스트 입력을 메시지 행에 넣는 실수가 컴파일되고
//!   compile-fail 요구(§4)를 만족할 수 없다. (3)은 Label 안의 텍스트 입력에 deprecated `label`이
//!   실려 나갈 수 있다. (2)는 타입이 위치를 정하므로 모달 자식은 메시지 행 API에 들어갈 수 없다.
//! - 장점, 단점 및 영향: 모달 전용 타입은 `Deserialize`가 없다(나가는 DTO). 구조체는
//!   `#[non_exhaustive]`라 규격이 넓어져도 필드를 더할 수 있고, 외부에서는 `new`와 setter로만
//!   만든다. 미래의 컴포넌트는 `RawComponent`(검증된 JSON 텍스트, `Value` 트리 아님)로 보낸다.
//!   `validate()`는 문서에 적힌 한도와 custom_id·id 중복만 검사한다.

use std::collections::HashSet;

use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::Serialize;

use crate::builders::limits::{self, MAX_CUSTOM_ID_LENGTH};
use crate::command::validation;
use crate::error::DiscordError;
use crate::model::{ComponentId, ComponentType, TextInputStyle};
use crate::types::SelectOption;

use super::{
    ChannelSelect, MentionableSelect, RawComponent, RoleSelect, SelectMenu, StringSelect,
    TextDisplay, UserSelect,
};

/// The `data` of a `MODAL` interaction callback (type 9): a popup form.
///
/// Built with [`ModalBuilder`](crate::builders::ModalBuilder) or directly:
///
/// ```
/// use discordrs::request::{Label, Modal, TextInput};
///
/// let modal = Modal::new("feedback", "Feedback")
///     .component(Label::new("Your name", TextInput::short("name")))
///     .component(
///         Label::new("Anything else?", TextInput::paragraph("details").required(false))
///             .description("Optional"),
///     );
/// modal.validate().unwrap();
/// assert_eq!(
///     serde_json::to_string(&modal).unwrap(),
///     r#"{"custom_id":"feedback","title":"Feedback","components":[{"type":18,"label":"Your name","component":{"type":4,"custom_id":"name","style":1}},{"type":18,"label":"Anything else?","description":"Optional","component":{"type":4,"custom_id":"details","style":2,"required":false}}]}"#
/// );
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Modal {
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// Popup title, 1-45 characters.
    pub title: String,
    /// Top-level components, 1-5.
    pub components: Vec<ModalComponent>,
}

impl Modal {
    /// `custom_id`: 1-100 characters (Interaction Callback Data, "Modal").
    pub const MAX_CUSTOM_ID_LENGTH: usize = MAX_CUSTOM_ID_LENGTH;
    /// `title`: at most 45 characters (Interaction Callback Data, "Modal").
    pub const MAX_TITLE_LENGTH: usize = limits::MAX_MODAL_TITLE_LENGTH;
    /// `components`: between 1 and 5 inclusive (Interaction Callback Data,
    /// "Modal").
    pub const MAX_COMPONENTS: usize = limits::MAX_MODAL_COMPONENTS;

    /// A modal with no components yet; add 1-5 with [`Self::component`].
    pub fn new(custom_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            title: title.into(),
            components: Vec::new(),
        }
    }

    /// Appends a top-level component: a [`Label`], a text display, or a
    /// [`ModalComponent`].
    pub fn component(mut self, component: impl Into<ModalComponent>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Checks the modal against Discord's documented limits: `custom_id`
    /// 1-100 and `title` 1-45 characters, 1-5 components, every child's own
    /// limits, and no `custom_id` or non-zero component `id` used twice.
    ///
    /// Raw components are counted but not inspected. Errors name the
    /// failing component, for example `modal components[2] (label) ...`.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "modal custom_id",
            &self.custom_id,
            1,
            Self::MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_len("modal title", &self.title, 1, Self::MAX_TITLE_LENGTH)?;
        validation::ensure_count(
            "modal components",
            self.components.len(),
            1,
            Self::MAX_COMPONENTS,
        )?;
        let mut seen = SeenKeys::default();
        for (index, component) in self.components.iter().enumerate() {
            let path = format!("modal components[{index}]");
            component
                .check(&mut seen)
                .map_err(|error| at_path(&path, component.describe(), error))?;
        }
        Ok(())
    }
}

impl Serialize for Modal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("components", &self.components)?;
        map.end()
    }
}

/// A top-level modal component.
///
/// Discord accepts Labels (which wrap every input), Text Displays, and,
/// deprecated, Action Rows holding one Text Input. Message-only layouts
/// (sections, containers, button rows) have no variant here.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ModalComponent {
    /// Label (type 18) wrapping one input.
    Label(Label),
    /// Text Display (type 10): Markdown text between inputs.
    TextDisplay(TextDisplay),
    /// Action Row (type 1) with one Text Input, the form Discord deprecated
    /// in favour of [`Label`]. Only [`LegacyTextInputRow::new`] (deprecated)
    /// creates one.
    LegacyTextInputRow(LegacyTextInputRow),
    /// Any component as JSON text, for component types this crate does not
    /// model yet. Sent as written; [`Modal::validate`] does not inspect it.
    Raw(RawComponent),
}

impl ModalComponent {
    fn describe(&self) -> &'static str {
        match self {
            Self::Label(_) => "label",
            Self::TextDisplay(_) => "text display",
            Self::LegacyTextInputRow(_) => "action row",
            Self::Raw(_) => "raw component",
        }
    }

    fn check(&self, seen: &mut SeenKeys) -> Result<(), DiscordError> {
        match self {
            Self::Label(label) => label.check(seen),
            Self::TextDisplay(text) => {
                seen.id(text.id)?;
                text.validate()
            }
            Self::LegacyTextInputRow(row) => row.check(seen),
            Self::Raw(_) => Ok(()),
        }
    }
}

impl Serialize for ModalComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Label(label) => label.serialize(serializer),
            Self::TextDisplay(text) => text.serialize(serializer),
            Self::LegacyTextInputRow(row) => row.serialize(serializer),
            Self::Raw(raw) => raw.serialize(serializer),
        }
    }
}

impl From<Label> for ModalComponent {
    fn from(label: Label) -> Self {
        Self::Label(label)
    }
}

impl From<TextDisplay> for ModalComponent {
    fn from(text: TextDisplay) -> Self {
        Self::TextDisplay(text)
    }
}

impl From<LegacyTextInputRow> for ModalComponent {
    fn from(row: LegacyTextInputRow) -> Self {
        Self::LegacyTextInputRow(row)
    }
}

impl From<RawComponent> for ModalComponent {
    fn from(raw: RawComponent) -> Self {
        Self::Raw(raw)
    }
}

/// A Label (type 18): a title and optional description around one input.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Label {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// The label text, 1-45 characters.
    pub label: String,
    /// Optional description, at most 100 characters. Clients show it above
    /// or below the input depending on the platform.
    pub description: Option<String>,
    /// The wrapped input.
    pub component: LabelComponent,
}

impl Label {
    /// `label`: at most 45 characters (Label Structure).
    pub const MAX_LABEL_LENGTH: usize = limits::MAX_TEXT_INPUT_LABEL_LENGTH;
    /// `description`: at most 100 characters (Label Structure).
    pub const MAX_DESCRIPTION_LENGTH: usize = limits::MAX_LABEL_DESCRIPTION_LENGTH;

    /// A label with this text around `component`.
    pub fn new(label: impl Into<String>, component: impl Into<LabelComponent>) -> Self {
        Self {
            id: None,
            label: label.into(),
            description: None,
            component: component.into(),
        }
    }

    /// Sets the description shown with the label.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Checks the label text (1-45 characters), the description (at most
    /// 100), and the wrapped input's own limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check(&mut SeenKeys::default())
    }

    fn check(&self, seen: &mut SeenKeys) -> Result<(), DiscordError> {
        seen.id(self.id)?;
        validation::ensure_len("label text", &self.label, 1, Self::MAX_LABEL_LENGTH)?;
        if let Some(description) = &self.description {
            validation::ensure_max_len(
                "label description",
                description,
                Self::MAX_DESCRIPTION_LENGTH,
            )?;
        }
        self.component.check(seen)
    }
}

impl Serialize for Label {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::LABEL)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("label", &self.label)?;
        serialize_some(&mut map, "description", &self.description)?;
        map.serialize_entry("component", &self.component)?;
        map.end()
    }
}

/// The input inside a [`Label`]: exactly the Label child components of the
/// Discord docs.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum LabelComponent {
    /// Text Input (type 4).
    TextInput(TextInput),
    /// String Select (type 3).
    StringSelect(StringSelect),
    /// User Select (type 5).
    UserSelect(UserSelect),
    /// Role Select (type 6).
    RoleSelect(RoleSelect),
    /// Mentionable Select (type 7).
    MentionableSelect(MentionableSelect),
    /// Channel Select (type 8).
    ChannelSelect(ChannelSelect),
    /// File Upload (type 19).
    FileUpload(FileUpload),
    /// Radio Group (type 21).
    RadioGroup(RadioGroup),
    /// Checkbox Group (type 22).
    CheckboxGroup(CheckboxGroup),
    /// Checkbox (type 23).
    Checkbox(Checkbox),
    /// Any input as JSON text, for component types this crate does not
    /// model yet. Sent as written and not inspected by `validate()`.
    Raw(RawComponent),
}

impl LabelComponent {
    fn check(&self, seen: &mut SeenKeys) -> Result<(), DiscordError> {
        match self {
            Self::TextInput(input) => {
                seen.input(input.id, &input.custom_id)?;
                input.validate()
            }
            Self::StringSelect(select) => {
                seen.input(select.id, &select.custom_id)?;
                select.validate()?;
                check_modal_select(select.disabled, select.min_values, select.required)
            }
            Self::UserSelect(select) => {
                seen.input(select.id, &select.custom_id)?;
                select.validate()?;
                check_modal_select(select.disabled, select.min_values, select.required)
            }
            Self::RoleSelect(select) => {
                seen.input(select.id, &select.custom_id)?;
                select.validate()?;
                check_modal_select(select.disabled, select.min_values, select.required)
            }
            Self::MentionableSelect(select) => {
                seen.input(select.id, &select.custom_id)?;
                select.validate()?;
                check_modal_select(select.disabled, select.min_values, select.required)
            }
            Self::ChannelSelect(select) => {
                seen.input(select.id, &select.custom_id)?;
                select.validate()?;
                check_modal_select(select.disabled, select.min_values, select.required)
            }
            Self::FileUpload(upload) => {
                seen.input(upload.id, &upload.custom_id)?;
                upload.validate()
            }
            Self::RadioGroup(group) => {
                seen.input(group.id, &group.custom_id)?;
                group.validate()
            }
            Self::CheckboxGroup(group) => {
                seen.input(group.id, &group.custom_id)?;
                group.validate()
            }
            Self::Checkbox(checkbox) => {
                seen.input(checkbox.id, &checkbox.custom_id)?;
                checkbox.validate()
            }
            Self::Raw(_) => Ok(()),
        }
    }
}

impl Serialize for LabelComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::TextInput(input) => input.serialize(serializer),
            Self::StringSelect(select) => select.serialize(serializer),
            Self::UserSelect(select) => select.serialize(serializer),
            Self::RoleSelect(select) => select.serialize(serializer),
            Self::MentionableSelect(select) => select.serialize(serializer),
            Self::ChannelSelect(select) => select.serialize(serializer),
            Self::FileUpload(upload) => upload.serialize(serializer),
            Self::RadioGroup(group) => group.serialize(serializer),
            Self::CheckboxGroup(group) => group.serialize(serializer),
            Self::Checkbox(checkbox) => checkbox.serialize(serializer),
            Self::Raw(raw) => raw.serialize(serializer),
        }
    }
}

impl From<TextInput> for LabelComponent {
    fn from(input: TextInput) -> Self {
        Self::TextInput(input)
    }
}

impl From<StringSelect> for LabelComponent {
    fn from(select: StringSelect) -> Self {
        Self::StringSelect(select)
    }
}

impl From<UserSelect> for LabelComponent {
    fn from(select: UserSelect) -> Self {
        Self::UserSelect(select)
    }
}

impl From<RoleSelect> for LabelComponent {
    fn from(select: RoleSelect) -> Self {
        Self::RoleSelect(select)
    }
}

impl From<MentionableSelect> for LabelComponent {
    fn from(select: MentionableSelect) -> Self {
        Self::MentionableSelect(select)
    }
}

impl From<ChannelSelect> for LabelComponent {
    fn from(select: ChannelSelect) -> Self {
        Self::ChannelSelect(select)
    }
}

/// The select menu of a message row, moved into a label.
impl From<SelectMenu> for LabelComponent {
    fn from(select: SelectMenu) -> Self {
        match select {
            SelectMenu::String(select) => Self::StringSelect(select),
            SelectMenu::User(select) => Self::UserSelect(select),
            SelectMenu::Role(select) => Self::RoleSelect(select),
            SelectMenu::Mentionable(select) => Self::MentionableSelect(select),
            SelectMenu::Channel(select) => Self::ChannelSelect(select),
        }
    }
}

impl From<FileUpload> for LabelComponent {
    fn from(upload: FileUpload) -> Self {
        Self::FileUpload(upload)
    }
}

impl From<RadioGroup> for LabelComponent {
    fn from(group: RadioGroup) -> Self {
        Self::RadioGroup(group)
    }
}

impl From<CheckboxGroup> for LabelComponent {
    fn from(group: CheckboxGroup) -> Self {
        Self::CheckboxGroup(group)
    }
}

impl From<Checkbox> for LabelComponent {
    fn from(checkbox: Checkbox) -> Self {
        Self::Checkbox(checkbox)
    }
}

impl From<RawComponent> for LabelComponent {
    fn from(raw: RawComponent) -> Self {
        Self::Raw(raw)
    }
}

/// A Text Input (type 4): free-form text. Put it in a [`Label`]; the
/// label text lives there, not on the input (Discord deprecated the input's
/// own `label`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextInput {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// Single-line or paragraph.
    pub style: TextInputStyle,
    /// Minimum input length, 0-4000.
    pub min_length: Option<u16>,
    /// Maximum input length, 1-4000.
    pub max_length: Option<u16>,
    /// Whether the input must be filled (Discord's default: `true`).
    pub required: Option<bool>,
    /// Pre-filled text, at most 4000 characters.
    pub value: Option<String>,
    /// Placeholder shown while empty, at most 100 characters.
    pub placeholder: Option<String>,
}

impl TextInput {
    /// `min_length` (0-4000), `max_length` (1-4000), and `value` (at most
    /// 4000 characters) share this bound (Text Input Structure).
    pub const MAX_LENGTH: u16 = limits::MAX_TEXT_INPUT_LENGTH;
    /// `placeholder`: at most 100 characters (Text Input Structure).
    pub const MAX_PLACEHOLDER_LENGTH: usize = limits::MAX_TEXT_INPUT_PLACEHOLDER_LENGTH;
    /// Legacy `label` of a text input in an action row: at most 45
    /// characters (the Label component's limit, which replaced it).
    pub const MAX_LEGACY_LABEL_LENGTH: usize = limits::MAX_TEXT_INPUT_LABEL_LENGTH;

    /// A text input with this `custom_id` and style.
    pub fn new(custom_id: impl Into<String>, style: TextInputStyle) -> Self {
        Self {
            id: None,
            custom_id: custom_id.into(),
            style,
            min_length: None,
            max_length: None,
            required: None,
            value: None,
            placeholder: None,
        }
    }

    /// A single-line text input.
    pub fn short(custom_id: impl Into<String>) -> Self {
        Self::new(custom_id, TextInputStyle::SHORT)
    }

    /// A multi-line text input.
    pub fn paragraph(custom_id: impl Into<String>) -> Self {
        Self::new(custom_id, TextInputStyle::PARAGRAPH)
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the minimum input length.
    pub fn min_length(mut self, min: u16) -> Self {
        self.min_length = Some(min);
        self
    }

    /// Sets the maximum input length.
    pub fn max_length(mut self, max: u16) -> Self {
        self.max_length = Some(max);
        self
    }

    /// Sets whether the input must be filled.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Sets the pre-filled text.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Sets the placeholder.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Checks `custom_id` (1-100 characters), `placeholder` (at most 100),
    /// `value` (at most 4000), `min_length` (0-4000), `max_length`
    /// (1-4000), and `min_length <= max_length`.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "text input custom_id",
            &self.custom_id,
            1,
            MAX_CUSTOM_ID_LENGTH,
        )?;
        if let Some(placeholder) = &self.placeholder {
            validation::ensure_max_len(
                "text input placeholder",
                placeholder,
                Self::MAX_PLACEHOLDER_LENGTH,
            )?;
        }
        if let Some(value) = &self.value {
            validation::ensure_max_len("text input value", value, usize::from(Self::MAX_LENGTH))?;
        }
        if let Some(min) = self.min_length {
            if min > Self::MAX_LENGTH {
                return Err(DiscordError::model(format!(
                    "text input min_length must be 0-{}, got {min}",
                    Self::MAX_LENGTH
                )));
            }
        }
        if let Some(max) = self.max_length {
            if !(1..=Self::MAX_LENGTH).contains(&max) {
                return Err(DiscordError::model(format!(
                    "text input max_length must be 1-{}, got {max}",
                    Self::MAX_LENGTH
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

    /// Writes the input, with the deprecated `label` key only when a legacy
    /// action row supplies one.
    fn serialize_with_label<S: Serializer>(
        &self,
        label: Option<&str>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::TEXT_INPUT)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        map.serialize_entry("style", &self.style)?;
        serialize_some(&mut map, "label", &label)?;
        serialize_some(&mut map, "min_length", &self.min_length)?;
        serialize_some(&mut map, "max_length", &self.max_length)?;
        serialize_some(&mut map, "required", &self.required)?;
        serialize_some(&mut map, "value", &self.value)?;
        serialize_some(&mut map, "placeholder", &self.placeholder)?;
        map.end()
    }
}

impl Serialize for TextInput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_with_label(None, serializer)
    }
}

/// An Action Row (type 1) holding one Text Input with the input's own
/// `label`: the modal layout Discord deprecated in favour of [`Label`].
///
/// Kept so a bot can still send the 2.x shape on purpose; there is no
/// other way to put an action row in a [`Modal`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LegacyTextInputRow {
    /// Optional component identifier of the row.
    pub id: Option<ComponentId>,
    /// The text input's own `label`, 1-45 characters.
    pub label: String,
    /// The wrapped text input.
    pub input: TextInput,
}

impl LegacyTextInputRow {
    /// Wraps `input` in an action row with the input's own `label`.
    #[deprecated(
        since = "3.0.0",
        note = "Discord deprecated text inputs in action rows; use Label::new(label, input)"
    )]
    pub fn new(label: impl Into<String>, input: TextInput) -> Self {
        Self {
            id: None,
            label: label.into(),
            input,
        }
    }

    /// Sets the row's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    fn check(&self, seen: &mut SeenKeys) -> Result<(), DiscordError> {
        seen.id(self.id)?;
        validation::ensure_len(
            "text input label",
            &self.label,
            1,
            TextInput::MAX_LEGACY_LABEL_LENGTH,
        )?;
        seen.input(self.input.id, &self.input.custom_id)?;
        self.input.validate()
    }
}

impl Serialize for LegacyTextInputRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        /// The row's single child, carrying the legacy label.
        struct LabeledInput<'a>(&'a LegacyTextInputRow);

        impl Serialize for LabeledInput<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(1))?;
                seq.serialize_element(&InputWithLabel(self.0))?;
                seq.end()
            }
        }

        struct InputWithLabel<'a>(&'a LegacyTextInputRow);

        impl Serialize for InputWithLabel<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0
                    .input
                    .serialize_with_label(Some(&self.0.label), serializer)
            }
        }

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::ACTION_ROW)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", &LabeledInput(self))?;
        map.end()
    }
}

/// A File Upload (type 19): lets the user attach files. Put it in a
/// [`Label`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileUpload {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// Minimum number of files, 0-10 (Discord's default: 1). `0` needs
    /// `required(false)`.
    pub min_values: Option<u8>,
    /// Maximum number of files, 1-10 (Discord's default: 1).
    pub max_values: Option<u8>,
    /// Whether files must be uploaded to submit (Discord's default: `true`).
    pub required: Option<bool>,
    /// Allowed file types: preset groups (`"image"`, `"video"`, `"audio"`)
    /// or dot-prefixed extensions (`".pdf"`), at most 10. Discord checks
    /// only the file name; check contents yourself.
    pub file_types: Option<Vec<String>>,
}

impl FileUpload {
    /// `min_values` (0-10) and `max_values` (1-10) bound (File Upload
    /// Structure).
    pub const MAX_VALUES: u8 = limits::MAX_FILE_UPLOAD_VALUES;

    /// A file upload with this `custom_id`.
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            id: None,
            custom_id: custom_id.into(),
            min_values: None,
            max_values: None,
            required: None,
            file_types: None,
        }
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the minimum number of files.
    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Sets the maximum number of files.
    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Sets whether files must be uploaded.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Restricts uploads to these file types.
    pub fn file_types<I, S>(mut self, file_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.file_types = Some(file_types.into_iter().map(Into::into).collect());
        self
    }

    /// Checks `custom_id` (1-100 characters), `min_values` (0-10, and 0
    /// only with `required(false)`), `max_values` (1-10), `min_values <=
    /// max_values` with Discord's default of 1 for an omitted side (so
    /// `min_values(2)` needs a `max_values` of at least 2), and the
    /// `file_types` filter (at most 10 well-formed entries).
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "file upload custom_id",
            &self.custom_id,
            1,
            MAX_CUSTOM_ID_LENGTH,
        )?;
        check_value_range(
            "file upload",
            self.min_values,
            self.max_values,
            self.required,
            Self::MAX_VALUES,
            OmittedMax {
                value: 1,
                source: "Discord's default",
            },
        )?;
        if let Some(file_types) = &self.file_types {
            validation::ensure_file_types("file upload file_types", file_types)?;
        }
        Ok(())
    }
}

impl Serialize for FileUpload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::FILE_UPLOAD)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        serialize_some(&mut map, "min_values", &self.min_values)?;
        serialize_some(&mut map, "max_values", &self.max_values)?;
        serialize_some(&mut map, "required", &self.required)?;
        serialize_some(&mut map, "file_types", &self.file_types)?;
        map.end()
    }
}

// [Decision Log]
// - 목적과 의도: 모달 전용 컴포넌트(라디오 그룹, 체크박스 그룹, 체크박스)가 Discord 문서
//   (components/reference, 3714862)대로 받아들여지는 페이로드만 만들게 한다.
// - 기존 구현 및 제약 조건: 2.x 빌더에는 `disabled` setter가 있었다. 문서는 "모달에서 `disabled`를
//   쓰면 오류"라고 하고, 이 셋은 모달에만 들어간다. 체크박스의 초기 상태 키는 `default`이고,
//   체크박스에는 `required`를 둘 수 없다. 그룹 옵션 구조는 label/value/description/default뿐이고
//   길이는 "max 100 characters"만 적는다(선택 메뉴 옵션과 달리 최소 길이가 없다).
// - 검토한 주요 대안: (1) disabled/required를 필드로 두고 검증에서 거절, (2) 필드 자체를 두지
//   않기, (3) 옵션에 별도 타입을 두기, (4) 선택 메뉴 옵션 타입(`SelectOption`)을 재사용하고 그룹에
//   없는 emoji만 검증에서 거절.
// - 선택한 방식: (2)와 (4). 옵션 수와 min/max_values의 관계는 문서에 없으므로 검사하지 않고,
//   min_values ≤ max_values만 정의상 검사한다(선택 메뉴, 텍스트 입력과 같은 검사).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 항상 거절될 페이로드를 표현할 수 있게 둔다.
//   (3)은 2.x 빌더의 `add_option(SelectOption)` 호출을 모두 깨뜨린다. emoji를 조용히 버리면
//   사용자 의도가 사라지므로 검증에서 거절한다.
// - 장점, 단점 및 영향: 로컬 검증은 문서보다 엄격하지 않다(emoji는 문서에 없는 키다). 옵션 수보다
//   큰 min_values 같은 실수는 Discord가 알려 준다. 필수 체크박스는 옵션 하나짜리 체크박스 그룹 +
//   `required(true)`로 만든다(문서의 권장).

/// A Radio Group (type 21): pick exactly one option. Put it in a
/// [`Label`]. Modals cannot hold disabled components, so there is no
/// `disabled`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct RadioGroup {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// The options, 2-10. `emoji` is not part of a group option and is
    /// rejected by [`Self::validate`].
    pub options: Vec<SelectOption>,
    /// Whether a selection is required (Discord's default: `true`).
    pub required: Option<bool>,
}

impl RadioGroup {
    /// Minimum number of options (Radio Group Structure: "min 2").
    pub const MIN_OPTIONS: usize = limits::MIN_RADIO_GROUP_OPTIONS;
    /// Maximum number of options (Radio Group Structure: "max 10").
    pub const MAX_OPTIONS: usize = limits::MAX_RADIO_GROUP_OPTIONS;

    /// A radio group with this `custom_id` and no options yet.
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            id: None,
            custom_id: custom_id.into(),
            options: Vec::new(),
            required: None,
        }
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Appends an option.
    pub fn option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Sets whether a selection is required.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Checks `custom_id` (1-100 characters), 2-10 options, and each
    /// option's label, value, and description (at most 100 characters, no
    /// emoji).
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "radio group custom_id",
            &self.custom_id,
            1,
            MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_count(
            "radio group options",
            self.options.len(),
            Self::MIN_OPTIONS,
            Self::MAX_OPTIONS,
        )?;
        validate_group_options("radio group", &self.options)
    }
}

impl Serialize for RadioGroup {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::RADIO_GROUP)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        map.serialize_entry("options", &self.options)?;
        serialize_some(&mut map, "required", &self.required)?;
        map.end()
    }
}

/// A Checkbox Group (type 22): pick any number of options. Put it in a
/// [`Label`]. There is no `disabled`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CheckboxGroup {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// The options, 1-10. `emoji` is rejected by [`Self::validate`].
    pub options: Vec<SelectOption>,
    /// Minimum number of checked options, 0-10 (Discord's default: 1). `0`
    /// needs `required(false)`.
    pub min_values: Option<u8>,
    /// Maximum number of checked options, 1-10 (Discord's default: the
    /// number of options).
    pub max_values: Option<u8>,
    /// Whether a selection is required (Discord's default: `true`).
    pub required: Option<bool>,
}

impl CheckboxGroup {
    /// Maximum number of options (Checkbox Group Structure: "max 10").
    pub const MAX_OPTIONS: usize = limits::MAX_CHECKBOX_GROUP_OPTIONS;
    /// `min_values` (0-10) and `max_values` (1-10) bound (Checkbox Group
    /// Structure).
    pub const MAX_VALUES: u8 = limits::MAX_CHECKBOX_GROUP_VALUES;

    /// A checkbox group with this `custom_id` and no options yet.
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            id: None,
            custom_id: custom_id.into(),
            options: Vec::new(),
            min_values: None,
            max_values: None,
            required: None,
        }
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Appends an option.
    pub fn option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Sets the minimum number of checked options.
    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Sets the maximum number of checked options.
    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Sets whether a selection is required.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Checks `custom_id` (1-100 characters), 1-10 options whose labels,
    /// values, and descriptions are at most 100 characters (no emoji),
    /// `min_values` 0-10 (0 only with `required(false)`), `max_values`
    /// 1-10, and `min_values <= max_values`, where an omitted `max_values`
    /// is the number of options (Discord's default).
    ///
    /// An explicit `max_values` is not compared with the number of options
    /// because Discord documents no such rule.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "checkbox group custom_id",
            &self.custom_id,
            1,
            MAX_CUSTOM_ID_LENGTH,
        )?;
        validation::ensure_count(
            "checkbox group options",
            self.options.len(),
            1,
            Self::MAX_OPTIONS,
        )?;
        validate_group_options("checkbox group", &self.options)?;
        check_value_range(
            "checkbox group",
            self.min_values,
            self.max_values,
            self.required,
            Self::MAX_VALUES,
            OmittedMax {
                // At most `MAX_OPTIONS` (10) here: the count was checked above.
                value: u8::try_from(self.options.len()).unwrap_or(u8::MAX),
                source: "the number of options",
            },
        )
    }
}

impl Serialize for CheckboxGroup {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::CHECKBOX_GROUP)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        map.serialize_entry("options", &self.options)?;
        serialize_some(&mut map, "min_values", &self.min_values)?;
        serialize_some(&mut map, "max_values", &self.max_values)?;
        serialize_some(&mut map, "required", &self.required)?;
        map.end()
    }
}

/// A single Checkbox (type 23) for a yes/no question. Put it in a
/// [`Label`].
///
/// Discord does not let a checkbox be required or disabled; for a box the
/// user must tick, use a [`CheckboxGroup`] with one option and
/// `required(true)`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Checkbox {
    /// Optional component identifier, unique within the modal.
    pub id: Option<ComponentId>,
    /// Identifier returned with the submission, 1-100 characters.
    pub custom_id: String,
    /// Whether the box starts checked (sent as `default`).
    pub default: Option<bool>,
}

impl Checkbox {
    /// A checkbox with this `custom_id`.
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            id: None,
            custom_id: custom_id.into(),
            default: None,
        }
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets whether the box starts checked.
    pub fn default_checked(mut self, checked: bool) -> Self {
        self.default = Some(checked);
        self
    }

    /// Checks `custom_id` (1-100 characters), the only limit the Checkbox
    /// Structure gives.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len(
            "checkbox custom_id",
            &self.custom_id,
            1,
            MAX_CUSTOM_ID_LENGTH,
        )
    }
}

impl Serialize for Checkbox {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::CHECKBOX)?;
        serialize_some(&mut map, "id", &self.id)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        serialize_some(&mut map, "default", &self.default)?;
        map.end()
    }
}

/// `custom_id`s and non-zero component `id`s already used in one modal.
#[derive(Default)]
struct SeenKeys {
    custom_ids: HashSet<String>,
    ids: HashSet<u32>,
}

impl SeenKeys {
    /// Records a component `id`. `0` means "let Discord assign one" and may
    /// repeat.
    fn id(&mut self, id: Option<ComponentId>) -> Result<(), DiscordError> {
        match id.map(ComponentId::get) {
            Some(0) | None => Ok(()),
            Some(id) if self.ids.insert(id) => Ok(()),
            Some(id) => Err(DiscordError::model(format!(
                "component id {id} is used more than once in the modal"
            ))),
        }
    }

    /// Records an interactive component: its `id` and its `custom_id`,
    /// which Discord requires to be unique per component.
    fn input(&mut self, id: Option<ComponentId>, custom_id: &str) -> Result<(), DiscordError> {
        self.id(id)?;
        if self.custom_ids.insert(custom_id.to_owned()) {
            Ok(())
        } else {
            Err(DiscordError::model(format!(
                "custom_id {custom_id:?} is used more than once in the modal"
            )))
        }
    }
}

/// The modal-only select rules on top of the select's own `validate()`:
/// "Using `disabled` in a modal will result in an error", and `min_values`
/// "must be either omitted or at least `1` if `required` is omitted or
/// `true`" (`required` defaults to `true` in modals).
fn check_modal_select(
    disabled: Option<bool>,
    min_values: Option<u8>,
    required: Option<bool>,
) -> Result<(), DiscordError> {
    if disabled.is_some() {
        return Err(DiscordError::model(
            "select menu disabled cannot be set in a modal: Discord rejects modals with \
             disabled components",
        ));
    }
    if min_values == Some(0) && required != Some(false) {
        return Err(DiscordError::model(
            "select menu min_values must be at least 1 when required is omitted or true, got 0",
        ));
    }
    Ok(())
}

/// Prefixes a child's validation error with its place in the modal.
fn at_path(path: &str, kind: &str, error: DiscordError) -> DiscordError {
    match error {
        DiscordError::Model { message } => {
            DiscordError::model(format!("{path} ({kind}): {message}"))
        }
        other => other,
    }
}

/// What Discord uses for an omitted `max_values`: the value and how the
/// docs describe it (for the error message).
struct OmittedMax {
    value: u8,
    source: &'static str,
}

/// `min_values`/`max_values`/`required` rules shared by file uploads and
/// checkbox groups: `min_values` 0..=`max`, `max_values` 1..=`max`, the
/// footnote "`min_values` must be either omitted or at least `1` if
/// `required` is omitted or `true`" (`required` defaults to `true`, so only
/// an explicit `required(false)` allows 0), and `min <= max` with Discord's
/// defaults for an omitted side: `min_values` defaults to 1 for both
/// components, an omitted `max_values` is `omitted_max` (1 for a file
/// upload, the number of options for a checkbox group).
fn check_value_range(
    component: &str,
    min_values: Option<u8>,
    max_values: Option<u8>,
    required: Option<bool>,
    max: u8,
    omitted_max: OmittedMax,
) -> Result<(), DiscordError> {
    if let Some(min) = min_values {
        if min > max {
            return Err(DiscordError::model(format!(
                "{component} min_values must be at most {max}, got {min}"
            )));
        }
        if min == 0 && required != Some(false) {
            return Err(DiscordError::model(format!(
                "{component} min_values must be at least 1 when required is omitted or true, \
                 got 0"
            )));
        }
    }
    if let Some(max_values) = max_values {
        if !(1..=max).contains(&max_values) {
            return Err(DiscordError::model(format!(
                "{component} max_values must be 1-{max}, got {max_values}"
            )));
        }
    }
    // Not a separate rule in the docs, but a minimum above the maximum can
    // never be met; select menus and text inputs apply the same check. An
    // omitted side takes Discord's default, so `min_values(2)` alone on a
    // file upload (whose `max_values` defaults to 1) is caught too. With
    // `min_values` omitted the minimum is 1, which every valid `max_values`
    // allows.
    let Some(min) = min_values else {
        return Ok(());
    };
    match max_values {
        Some(max_values) if min > max_values => Err(DiscordError::model(format!(
            "{component} min_values ({min}) must not exceed max_values ({max_values})"
        ))),
        None if min > omitted_max.value => Err(DiscordError::model(format!(
            "{component} min_values ({min}) must not exceed max_values, which is {} when \
             omitted ({})",
            omitted_max.value, omitted_max.source
        ))),
        _ => Ok(()),
    }
}

/// Checks each radio or checkbox group option's `label`, `value`, and
/// `description` against the documented "max 100 characters", and rejects
/// `emoji`, which the group option structures do not have. Unlike the
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
        if option.emoji.is_some() {
            return Err(DiscordError::model(format!(
                "{component} option \"{label}\" has an emoji, which {component} options do not \
                 support"
            )));
        }
    }
    Ok(())
}

fn serialize_some<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Option<T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(key, value),
        None => Ok(()),
    }
}
