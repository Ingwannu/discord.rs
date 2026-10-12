//! Builders for modals and their components.
//!
//! Each builder holds the typed request model from [`crate::request`] and
//! returns it from `build()` (no intermediate JSON); `try_build()` checks
//! the documented limits first and returns the same type. A modal's inputs
//! go into Labels by default; the Action Row + Text Input layout Discord
//! deprecated is only reachable through
//! [`ModalBuilder::add_text_input_row`].

use crate::command::validation;
use crate::error::DiscordError;
use crate::model::{ComponentId, TextInputStyle};
use crate::request::{
    Checkbox, CheckboxGroup, FileUpload, Label, LabelComponent, LegacyTextInputRow, Modal,
    ModalComponent, RadioGroup, SelectMenu, TextDisplay, TextInput,
};
use crate::types::SelectOption;

/// Builds a text input together with the text of the [`Label`] around it.
///
/// Discord moved a text input's `label` to the Label component, so
/// [`Self::build`] returns that Label; [`ModalBuilder::add_text_input`]
/// adds it to a modal.
///
/// ```
/// use discordrs::TextInputBuilder;
///
/// let label = TextInputBuilder::short("name", "Your name").required(true).build();
/// assert_eq!(
///     serde_json::to_string(&label).unwrap(),
///     r#"{"type":18,"label":"Your name","component":{"type":4,"custom_id":"name","style":1,"required":true}}"#
/// );
/// ```
#[derive(Clone, Debug)]
pub struct TextInputBuilder {
    label: String,
    description: Option<String>,
    input: TextInput,
}

impl TextInputBuilder {
    /// A text input with this `custom_id`, label text, and style.
    pub fn new(custom_id: &str, label: &str, style: TextInputStyle) -> Self {
        Self {
            label: label.to_owned(),
            description: None,
            input: TextInput::new(custom_id, style),
        }
    }

    /// A single-line text input.
    pub fn short(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, TextInputStyle::SHORT)
    }

    /// A multi-line text input.
    pub fn paragraph(custom_id: &str, label: &str) -> Self {
        Self::new(custom_id, label, TextInputStyle::PARAGRAPH)
    }

    /// Sets the description shown with the label (at most 100 characters).
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_owned());
        self
    }

    /// Sets the placeholder shown while the input is empty.
    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.input.placeholder = Some(placeholder.to_owned());
        self
    }

    /// Sets the pre-filled text.
    pub fn value(mut self, value: &str) -> Self {
        self.input.value = Some(value.to_owned());
        self
    }

    /// Sets whether the input must be filled.
    pub fn required(mut self, required: bool) -> Self {
        self.input.required = Some(required);
        self
    }

    /// Sets the minimum input length.
    pub fn min_length(mut self, min: u16) -> Self {
        self.input.min_length = Some(min);
        self
    }

    /// Sets the maximum input length.
    pub fn max_length(mut self, max: u16) -> Self {
        self.input.max_length = Some(max);
        self
    }

    /// Sets the text input's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.input.id = Some(id);
        self
    }

    /// Returns the [`Label`] holding the text input, without checking any
    /// limit.
    pub fn build(self) -> Label {
        let label = Label::new(self.label, self.input);
        match self.description {
            Some(description) => label.description(description),
            None => label,
        }
    }

    /// Checks the label text (1-45 characters), the description (at most
    /// 100), and the text input ([`TextInput::validate`]) without consuming
    /// the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len("text input label", &self.label, 1, Label::MAX_LABEL_LENGTH)?;
        if let Some(description) = &self.description {
            validation::ensure_max_len(
                "text input description",
                description,
                Label::MAX_DESCRIPTION_LENGTH,
            )?;
        }
        self.input.validate()
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Label, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }

    /// The text input in the deprecated action-row layout, with the label as
    /// the input's own `label`.
    fn into_legacy_row(self) -> LegacyTextInputRow {
        #[expect(
            deprecated,
            reason = "the only place the builders create the deprecated layout"
        )]
        LegacyTextInputRow::new(self.label, self.input)
    }
}

/// Builds a [`RadioGroup`]: pick exactly one option. Modals cannot contain
/// disabled components, so there is no `disabled` setter.
#[derive(Clone, Debug)]
pub struct RadioGroupBuilder {
    inner: RadioGroup,
}

impl RadioGroupBuilder {
    /// A radio group with this `custom_id`.
    pub fn new(custom_id: &str) -> Self {
        Self {
            inner: RadioGroup::new(custom_id),
        }
    }

    /// Appends an option.
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.inner.options.push(option);
        self
    }

    /// Appends several options.
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.inner.options.extend(options);
        self
    }

    /// Sets whether a selection is required.
    pub fn required(mut self, required: bool) -> Self {
        self.inner.required = Some(required);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Returns the radio group without checking any limit.
    pub fn build(self) -> RadioGroup {
        self.inner
    }

    /// Checks the documented limits without consuming the builder; see
    /// [`RadioGroup::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<RadioGroup, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// Builds a [`CheckboxGroup`]: pick any number of options. There is no
/// `disabled` setter.
#[derive(Clone, Debug)]
pub struct CheckboxGroupBuilder {
    inner: CheckboxGroup,
}

impl CheckboxGroupBuilder {
    /// A checkbox group with this `custom_id`.
    pub fn new(custom_id: &str) -> Self {
        Self {
            inner: CheckboxGroup::new(custom_id),
        }
    }

    /// Appends an option.
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.inner.options.push(option);
        self
    }

    /// Appends several options.
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.inner.options.extend(options);
        self
    }

    /// Sets the minimum number of checked options.
    pub fn min_values(mut self, min: u8) -> Self {
        self.inner.min_values = Some(min);
        self
    }

    /// Sets the maximum number of checked options.
    pub fn max_values(mut self, max: u8) -> Self {
        self.inner.max_values = Some(max);
        self
    }

    /// Sets whether a selection is required.
    pub fn required(mut self, required: bool) -> Self {
        self.inner.required = Some(required);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Returns the checkbox group without checking any limit.
    pub fn build(self) -> CheckboxGroup {
        self.inner
    }

    /// Checks the documented limits without consuming the builder; see
    /// [`CheckboxGroup::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<CheckboxGroup, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// Builds a single [`Checkbox`].
///
/// Discord does not let a checkbox be required or disabled; for a box the
/// user must tick, use a [`CheckboxGroupBuilder`] with one option and
/// `required(true)`.
#[derive(Clone, Debug)]
pub struct CheckboxBuilder {
    inner: Checkbox,
}

impl CheckboxBuilder {
    /// A checkbox with this `custom_id`.
    pub fn new(custom_id: &str) -> Self {
        Self {
            inner: Checkbox::new(custom_id),
        }
    }

    /// Whether the checkbox starts checked (Discord's `default`).
    pub fn default_checked(mut self, checked: bool) -> Self {
        self.inner.default = Some(checked);
        self
    }

    /// The 2.x name of [`Self::default_checked`]. 2.x sent it as a
    /// `checked` key, which Discord does not read; this now sends `default`.
    #[deprecated(since = "3.0.0", note = "use default_checked (sent as `default`)")]
    pub fn checked(self, checked: bool) -> Self {
        self.default_checked(checked)
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Returns the checkbox without checking any limit.
    pub fn build(self) -> Checkbox {
        self.inner
    }

    /// Checks the documented limits without consuming the builder; see
    /// [`Checkbox::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<Checkbox, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// Builds a [`FileUpload`].
#[derive(Clone, Debug)]
pub struct FileUploadBuilder {
    inner: FileUpload,
}

impl FileUploadBuilder {
    /// A file upload with this `custom_id`.
    pub fn new(custom_id: &str) -> Self {
        Self {
            inner: FileUpload::new(custom_id),
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
        self.inner.file_types = Some(file_types.into_iter().map(Into::into).collect());
        self
    }

    /// Sets the minimum number of files.
    pub fn min_values(mut self, min: u8) -> Self {
        self.inner.min_values = Some(min);
        self
    }

    /// Sets the maximum number of files.
    pub fn max_values(mut self, max: u8) -> Self {
        self.inner.max_values = Some(max);
        self
    }

    /// Sets whether files must be uploaded.
    pub fn required(mut self, required: bool) -> Self {
        self.inner.required = Some(required);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Returns the file upload without checking any limit.
    pub fn build(self) -> FileUpload {
        self.inner
    }

    /// Checks the documented limits without consuming the builder; see
    /// [`FileUpload::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<FileUpload, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// Builds a [`Label`] around one modal input.
#[derive(Clone, Debug)]
pub struct LabelBuilder {
    inner: Label,
}

impl LabelBuilder {
    fn with(label: &str, component: impl Into<LabelComponent>) -> Self {
        Self {
            inner: Label::new(label, component),
        }
    }

    /// A label around a text input. The builder's own label text is
    /// replaced by `label`.
    pub fn with_text_input(label: &str, input: TextInputBuilder) -> Self {
        Self::with(label, input.input)
    }

    /// A label around a select menu.
    pub fn with_select_menu(label: &str, select: impl Into<SelectMenu>) -> Self {
        Self::with(label, select_component(select))
    }

    /// A label around a file upload.
    pub fn with_file_upload(label: &str, file_upload: FileUploadBuilder) -> Self {
        Self::with(label, file_upload.build())
    }

    /// A label around a radio group.
    pub fn with_radio_group(label: &str, radio_group: RadioGroupBuilder) -> Self {
        Self::with(label, radio_group.build())
    }

    /// A label around a checkbox group.
    pub fn with_checkbox_group(label: &str, checkbox_group: CheckboxGroupBuilder) -> Self {
        Self::with(label, checkbox_group.build())
    }

    /// A label around a checkbox.
    pub fn with_checkbox(label: &str, checkbox: CheckboxBuilder) -> Self {
        Self::with(label, checkbox.build())
    }

    /// Sets the description shown with the label.
    pub fn description(mut self, desc: &str) -> Self {
        self.inner.description = Some(desc.to_owned());
        self
    }

    /// Sets the label's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// Returns the label without checking any limit.
    pub fn build(self) -> Label {
        self.inner
    }

    /// Checks the label and its input without consuming the builder; see
    /// [`Label::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`].
    pub fn try_build(self) -> Result<Label, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// A select menu (a per-kind select builder such as
/// [`SelectMenuBuilder::string`](super::SelectMenuBuilder::string), a model
/// struct, or a [`SelectMenu`]) as a label child.
fn select_component(select: impl Into<SelectMenu>) -> LabelComponent {
    LabelComponent::from(select.into())
}

/// Builds a [`Modal`]: a title and 1-5 top-level components.
///
/// Inputs are wrapped in Labels: [`Self::add_text_input`] uses the text
/// input builder's label text, the other `add_*` methods take the label
/// text and an optional description.
///
/// ```
/// use discordrs::{CheckboxBuilder, ModalBuilder, TextInputBuilder};
///
/// let modal = ModalBuilder::new("signup", "Sign up")
///     .add_text_input(TextInputBuilder::short("nick", "Nickname"))
///     .add_checkbox("Newsletter", None, CheckboxBuilder::new("news"))
///     .try_build()
///     .unwrap();
/// assert_eq!(modal.components.len(), 2);
/// ```
#[derive(Clone, Debug)]
pub struct ModalBuilder {
    inner: Modal,
}

impl ModalBuilder {
    fn label(
        label: &str,
        description: Option<&str>,
        component: impl Into<LabelComponent>,
    ) -> Label {
        let label = Label::new(label, component);
        match description {
            Some(description) => label.description(description),
            None => label,
        }
    }

    /// A modal with this `custom_id` and title.
    pub fn new(custom_id: &str, title: &str) -> Self {
        Self {
            inner: Modal::new(custom_id, title),
        }
    }

    /// Adds a text input inside a Label carrying the input builder's label
    /// text (and description, if set).
    pub fn add_text_input(mut self, input: TextInputBuilder) -> Self {
        self.inner.components.push(input.build().into());
        self
    }

    /// Adds a text input in an Action Row with the input's own `label`, the
    /// layout Discord deprecated in favour of Labels.
    #[deprecated(
        since = "3.0.0",
        note = "Discord deprecated text inputs in action rows; use add_text_input (a Label)"
    )]
    pub fn add_text_input_row(mut self, input: TextInputBuilder) -> Self {
        self.inner.components.push(input.into_legacy_row().into());
        self
    }

    /// Adds a select menu inside a Label.
    pub fn add_select_menu(
        mut self,
        label: &str,
        description: Option<&str>,
        select: impl Into<SelectMenu>,
    ) -> Self {
        self.inner
            .components
            .push(Self::label(label, description, select_component(select)).into());
        self
    }

    /// Adds a file upload inside a Label.
    pub fn add_file_upload(
        mut self,
        label: &str,
        description: Option<&str>,
        file_upload: FileUploadBuilder,
    ) -> Self {
        self.inner
            .components
            .push(Self::label(label, description, file_upload.build()).into());
        self
    }

    /// Adds a radio group inside a Label.
    pub fn add_radio_group(
        mut self,
        label: &str,
        description: Option<&str>,
        radio_group: RadioGroupBuilder,
    ) -> Self {
        self.inner
            .components
            .push(Self::label(label, description, radio_group.build()).into());
        self
    }

    /// Adds a checkbox group inside a Label.
    pub fn add_checkbox_group(
        mut self,
        label: &str,
        description: Option<&str>,
        checkbox_group: CheckboxGroupBuilder,
    ) -> Self {
        self.inner
            .components
            .push(Self::label(label, description, checkbox_group.build()).into());
        self
    }

    /// Adds a checkbox inside a Label.
    pub fn add_checkbox(
        mut self,
        label: &str,
        description: Option<&str>,
        checkbox: CheckboxBuilder,
    ) -> Self {
        self.inner
            .components
            .push(Self::label(label, description, checkbox.build()).into());
        self
    }

    /// Adds a Label built separately.
    pub fn add_label(mut self, label: LabelBuilder) -> Self {
        self.inner.components.push(label.build().into());
        self
    }

    /// Adds a Text Display: Markdown shown between the inputs.
    pub fn add_text_display(mut self, content: &str) -> Self {
        self.inner.components.push(TextDisplay::new(content).into());
        self
    }

    /// Adds any top-level modal component: a [`Label`], a
    /// [`TextDisplay`], or a [`RawComponent`](crate::request::RawComponent)
    /// for a component type this crate does not model yet.
    pub fn add_component(mut self, component: impl Into<ModalComponent>) -> Self {
        self.inner.components.push(component.into());
        self
    }

    /// Returns the modal without checking any limit.
    pub fn build(self) -> Modal {
        self.inner
    }

    /// Checks the modal and every component in it without consuming the
    /// builder; see [`Modal::validate`].
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Modal, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

impl From<ModalBuilder> for Modal {
    fn from(builder: ModalBuilder) -> Self {
        builder.build()
    }
}
#[cfg(test)]
mod tests {
    use crate::model::{ComponentId, ComponentType, TextInputStyle};
    use crate::request::{RawComponent, TextDisplay};
    use serde_json::{json, Value};

    use super::{
        CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, LabelBuilder, ModalBuilder,
        RadioGroupBuilder, TextInputBuilder,
    };
    use crate::builders::SelectMenuBuilder;
    use crate::types::SelectOption;

    /// What a built component sends.
    fn to_json(value: impl serde::Serialize) -> Value {
        serde_json::to_value(value).unwrap()
    }

    #[test]
    fn text_input_builder_builds_a_label_around_the_input() {
        let short = to_json(TextInputBuilder::short("short-id", "Short").build());
        assert_eq!(
            short["component"]["style"],
            json!(TextInputStyle::SHORT.get())
        );

        let paragraph = TextInputBuilder::paragraph("paragraph-id", "Paragraph")
            .description("tell us")
            .placeholder("type here")
            .value("seed")
            .required(true)
            .min_length(5)
            .max_length(25)
            .id(ComponentId::new(3))
            .build();

        // The label text moved to the Label; the input carries no `label`.
        assert_eq!(
            to_json(paragraph),
            json!({
                "type": ComponentType::LABEL,
                "label": "Paragraph",
                "description": "tell us",
                "component": {
                    "type": ComponentType::TEXT_INPUT,
                    "id": 3,
                    "custom_id": "paragraph-id",
                    "style": TextInputStyle::PARAGRAPH,
                    "min_length": 5,
                    "max_length": 25,
                    "required": true,
                    "value": "seed",
                    "placeholder": "type here",
                },
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
        let payload = to_json(payload);

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
            to_json(payload),
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
            to_json(checkbox),
            json!({
                "type": ComponentType::CHECKBOX,
                "custom_id": "terms",
                "default": true,
                "id": 3,
            })
        );
        #[expect(deprecated, reason = "the 2.x setter must send the documented key too")]
        let legacy = to_json(CheckboxBuilder::new("terms").checked(false).build());
        assert_eq!(legacy["default"], json!(false));
        assert!(legacy.get("checked").is_none());

        let upload = FileUploadBuilder::new("attachment")
            .min_values(1)
            .max_values(3)
            .required(true)
            .id(ComponentId::new(6))
            .build();

        assert_eq!(
            to_json(upload),
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
            let payload = to_json(payload);
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
        assert_eq!(
            to_json(make().build()),
            to_json(make().try_build().expect("valid input"))
        );
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
        assert_eq!(
            to_json(make().build()),
            to_json(make().try_build().expect("valid modal"))
        );

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
        #[expect(deprecated, reason = "covers the legacy action-row layout")]
        let builder = ModalBuilder::new("modal-id", "Modal Title")
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
            .add_text_display("Read this first")
            .add_component(TextDisplay::new("And this"))
            .add_component(RawComponent::from_json(r#"{"type":250,"custom":true}"#).unwrap())
            .add_text_input_row(TextInputBuilder::paragraph("legacy", "Legacy"));
        let payload = to_json(builder.build());

        let components = payload
            .get("components")
            .and_then(|value| value.as_array())
            .expect("modal components");

        assert_eq!(payload["custom_id"], json!("modal-id"));
        assert_eq!(payload["title"], json!("Modal Title"));
        assert_eq!(components.len(), 11);
        // Inputs are wrapped in Labels by default.
        for label in &components[..7] {
            assert_eq!(label["type"], json!(ComponentType::LABEL.get()));
        }
        assert_eq!(components[0]["label"], json!("Name"));
        assert_eq!(components[0]["component"]["type"], json!(4));
        assert!(components[0]["component"].get("label").is_none());
        assert_eq!(components[1]["description"], json!("Pick exactly one"));
        assert_eq!(components[1]["component"]["type"], json!(3));
        assert!(components[2].get("description").is_none());
        assert_eq!(components[3]["description"], json!("Required choice"));
        assert!(components[4].get("description").is_none());
        assert_eq!(components[5]["description"], json!("Terms"));
        assert_eq!(
            components[7],
            json!({"type": 10, "content": "Read this first"})
        );
        assert_eq!(components[8], json!({"type": 10, "content": "And this"}));
        assert_eq!(components[9], json!({"type": 250, "custom": true}));
        // Only the deprecated path produces an action row, with the input's
        // own label.
        assert_eq!(
            components[10],
            json!({
                "type": 1,
                "components": [
                    {"type": 4, "custom_id": "legacy", "style": 2, "label": "Legacy"}
                ]
            })
        );
    }

    #[test]
    fn file_upload_builder_supports_file_type_filters() {
        let built = to_json(
            super::FileUploadBuilder::new("upload")
                .file_types(["video", ".mov"])
                .try_build()
                .expect("valid filter"),
        );
        assert_eq!(built["file_types"], serde_json::json!(["video", ".mov"]));
        assert!(super::FileUploadBuilder::new("upload")
            .file_types((0..11).map(|index| format!(".e{index}")))
            .try_build()
            .is_err());
        assert!(to_json(super::FileUploadBuilder::new("upload").build())
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
            to_json(make().build()),
            to_json(make().try_build().expect("valid radio group"))
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
            to_json(make().build()),
            to_json(make().try_build().expect("valid checkbox group"))
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
        assert_eq!(
            to_json(make().build()),
            to_json(make().try_build().expect("valid checkbox"))
        );

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
