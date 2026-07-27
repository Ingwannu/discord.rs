use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::command::validation;
use crate::constants::{button_style, component_type};
use crate::error::DiscordError;
use crate::types::{to_json_value, Emoji, SelectOption};

use super::container::{ContainerBuilder, SeparatorBuilder, TextDisplayBuilder};
use super::media::{FileBuilder, MediaGalleryBuilder, SectionBuilder};
use super::modal::TextInputBuilder;

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ButtonBuilder`.
pub struct ButtonBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    style: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disabled: Option<bool>,
}

impl ButtonBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::BUTTON,
            style: button_style::PRIMARY,
            label: None,
            emoji: None,
            custom_id: None,
            url: None,
            disabled: None,
        }
    }

    pub fn style(mut self, style: u8) -> Self {
        self.style = style;
        self
    }

    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.emoji = Some(emoji);
        self
    }

    pub fn emoji_unicode(mut self, emoji: &str) -> Self {
        self.emoji = Some(Emoji::unicode(emoji));
        self
    }

    pub fn custom_id(mut self, custom_id: &str) -> Self {
        self.custom_id = Some(custom_id.to_string());
        self.url = None;
        if self.style == button_style::LINK {
            self.style = button_style::PRIMARY;
        }
        self
    }

    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self.custom_id = None;
        self.style = button_style::LINK;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    fn normalize(mut self) -> Self {
        if self.url.is_some() {
            self.custom_id = None;
            self.style = button_style::LINK;
        } else if self.style == button_style::LINK {
            self.style = button_style::PRIMARY;
        }
        self
    }

    pub fn build_typed(self) -> Self {
        self.normalize()
    }

    pub fn build_value(self) -> Value {
        to_json_value(self.build_typed())
    }

    pub fn build(self) -> Value {
        self.build_value()
    }

    /// Validates the button against Discord's limits without consuming the
    /// builder. The same normalization applied by [`Self::build`] is taken
    /// into account.
    ///
    /// Premium buttons (style 6) require a `sku_id`, which this builder does
    /// not model; styles outside 1-5 are only length-checked.
    pub fn validate(&self) -> Result<(), DiscordError> {
        let normalized = self.clone().normalize();
        if let Some(custom_id) = &normalized.custom_id {
            validation::ensure_max_len("button custom_id", custom_id, 100)?;
        }
        if let Some(label) = &normalized.label {
            validation::ensure_max_len("button label", label, 80)?;
        }
        let is_known_style = (button_style::PRIMARY..=button_style::LINK)
            .contains(&normalized.style);
        if is_known_style {
            if normalized.style == button_style::LINK {
                if normalized.url.is_none() {
                    return Err(DiscordError::model(
                        "button with link style requires a url",
                    ));
                }
            } else if normalized.custom_id.is_none() {
                return Err(DiscordError::model(
                    "button custom_id is required for non-link, non-premium styles",
                ));
            }
            if normalized.label.is_none() && normalized.emoji.is_none() {
                return Err(DiscordError::model(
                    "button must have a label or emoji for non-premium styles",
                ));
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

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActionRowBuilder`.
pub struct ActionRowBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    components: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
}

impl ActionRowBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            component_type: component_type::ACTION_ROW,
            components: Vec::new(),
            id: None,
        }
    }

    pub fn add_button(mut self, button: ButtonBuilder) -> Self {
        self.components.push(button.build());
        self
    }

    pub fn add_select_menu(mut self, select_menu: SelectMenuBuilder) -> Self {
        self.components.push(select_menu.build());
        self
    }

    pub fn add_text_input(mut self, input: TextInputBuilder) -> Self {
        self.components.push(input.build());
        self
    }

    pub fn add_component(mut self, component: Value) -> Self {
        self.components.push(component);
        self
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn build_typed(self) -> Self {
        self
    }

    pub fn build_value(self) -> Value {
        to_json_value(self.build_typed())
    }

    pub fn build(self) -> Value {
        self.build_value()
    }

    /// Validates the row against Discord's limits without consuming the
    /// builder: 1-5 components per row, and a select menu or text input must
    /// be the only component in its row.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_count("action row components", self.components.len(), 1, 5)?;
        const SOLO_TYPES: [u8; 6] = [
            component_type::STRING_SELECT,
            component_type::TEXT_INPUT,
            component_type::USER_SELECT,
            component_type::ROLE_SELECT,
            component_type::MENTIONABLE_SELECT,
            component_type::CHANNEL_SELECT,
        ];
        if self.components.len() > 1 {
            for component in &self.components {
                if let Some(kind) = component.get("type").and_then(Value::as_u64) {
                    if u8::try_from(kind).is_ok_and(|kind| SOLO_TYPES.contains(&kind)) {
                        return Err(DiscordError::model(format!(
                            "action row component of type {kind} must be the only \
                             component in its row, got {} components",
                            self.components.len()
                        )));
                    }
                }
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

#[derive(Clone, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SelectMenuBuilder`.
pub struct SelectMenuBuilder {
    #[serde(rename = "type")]
    component_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
    custom_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    default_values: Vec<SelectDefaultValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_types: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disabled: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
/// Typed Discord API object for `SelectDefaultValue`.
pub struct SelectDefaultValue {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

impl SelectDefaultValue {
    /// Creates a `user` value.
    pub fn user(id: impl Into<String>) -> Self {
        Self::new(id, "user")
    }

    /// Creates a `role` value.
    pub fn role(id: impl Into<String>) -> Self {
        Self::new(id, "role")
    }

    /// Creates a `channel` value.
    pub fn channel(id: impl Into<String>) -> Self {
        Self::new(id, "channel")
    }

    fn new(id: impl Into<String>, kind: &'static str) -> Self {
        Self {
            id: id.into(),
            kind: kind.to_string(),
        }
    }
}

impl SelectMenuBuilder {
    /// Creates a `string` value.
    pub fn string(custom_id: &str) -> Self {
        Self {
            component_type: component_type::STRING_SELECT,
            id: None,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    /// Creates a `role` value.
    pub fn role(custom_id: &str) -> Self {
        Self {
            component_type: component_type::ROLE_SELECT,
            id: None,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    /// Creates a `channel` value.
    pub fn channel(custom_id: &str) -> Self {
        Self {
            component_type: component_type::CHANNEL_SELECT,
            id: None,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    /// Creates a `user` value.
    pub fn user(custom_id: &str) -> Self {
        Self {
            component_type: component_type::USER_SELECT,
            id: None,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    /// Creates a `mentionable` value.
    pub fn mentionable(custom_id: &str) -> Self {
        Self {
            component_type: component_type::MENTIONABLE_SELECT,
            id: None,
            custom_id: custom_id.to_string(),
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    pub fn default_value(mut self, value: SelectDefaultValue) -> Self {
        self.default_values.push(value);
        self
    }

    pub fn default_values(mut self, values: Vec<SelectDefaultValue>) -> Self {
        self.default_values.extend(values);
        self
    }

    pub fn default_user(self, id: impl Into<String>) -> Self {
        self.default_value(SelectDefaultValue::user(id))
    }

    pub fn default_role(self, id: impl Into<String>) -> Self {
        self.default_value(SelectDefaultValue::role(id))
    }

    pub fn default_channel(self, id: impl Into<String>) -> Self {
        self.default_value(SelectDefaultValue::channel(id))
    }

    pub fn channel_types(mut self, channel_types: Vec<u8>) -> Self {
        self.channel_types = Some(channel_types);
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

    fn normalize(mut self) -> Self {
        match self.component_type {
            component_type::STRING_SELECT => {
                self.channel_types = None;
                self.default_values.clear();
            }
            component_type::CHANNEL_SELECT => {
                self.options.clear();
            }
            _ => {
                self.options.clear();
                self.channel_types = None;
            }
        }
        self
    }

    pub fn build_typed(self) -> Self {
        self.normalize()
    }

    pub fn build_value(self) -> Value {
        to_json_value(self.build_typed())
    }

    pub fn build(self) -> Value {
        self.build_value()
    }

    /// Validates the select menu against Discord's limits without consuming
    /// the builder. The same normalization applied by [`Self::build`] is
    /// taken into account.
    pub fn validate(&self) -> Result<(), DiscordError> {
        let normalized = self.clone().normalize();
        validation::ensure_len("select menu custom_id", &normalized.custom_id, 1, 100)?;
        if let Some(placeholder) = &normalized.placeholder {
            validation::ensure_max_len("select menu placeholder", placeholder, 150)?;
        }
        if normalized.component_type == component_type::STRING_SELECT {
            validation::ensure_count("select menu options", normalized.options.len(), 1, 25)?;
            for option in &normalized.options {
                let label = &option.label;
                validation::ensure_len(
                    &format!("select menu option \"{label}\" label"),
                    label,
                    1,
                    100,
                )?;
                validation::ensure_len(
                    &format!("select menu option \"{label}\" value"),
                    &option.value,
                    1,
                    100,
                )?;
                if let Some(description) = &option.description {
                    validation::ensure_max_len(
                        &format!("select menu option \"{label}\" description"),
                        description,
                        100,
                    )?;
                }
            }
        }
        if let Some(min) = normalized.min_values {
            if min > 25 {
                return Err(DiscordError::model(format!(
                    "select menu min_values must be at most 25, got {min}"
                )));
            }
        }
        if let Some(max) = normalized.max_values {
            if !(1..=25).contains(&max) {
                return Err(DiscordError::model(format!(
                    "select menu max_values must be 1-25, got {max}"
                )));
            }
        }
        if let (Some(min), Some(max)) = (normalized.min_values, normalized.max_values) {
            if min > max {
                return Err(DiscordError::model(format!(
                    "select menu min_values ({min}) must not exceed max_values ({max})"
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

/// Typed Discord API object for `ComponentsV2Message`.
pub struct ComponentsV2Message {
    components: Vec<Value>,
}

impl Default for ComponentsV2Message {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentsV2Message {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn add_container(mut self, container: ContainerBuilder) -> Self {
        self.components.push(container.build());
        self
    }

    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.components.push(text.build());
        self
    }

    pub fn add_media_gallery(mut self, gallery: MediaGalleryBuilder) -> Self {
        self.components.push(gallery.build());
        self
    }

    pub fn add_separator(mut self, separator: SeparatorBuilder) -> Self {
        self.components.push(separator.build());
        self
    }

    pub fn add_section(mut self, section: SectionBuilder) -> Self {
        self.components.push(section.build());
        self
    }

    pub fn add_file(mut self, file: FileBuilder) -> Self {
        self.components.push(file.build());
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

    pub fn build(self) -> Vec<Value> {
        self.components
    }

    /// Validates the message component set against Discord's limits without
    /// consuming the builder: a components-V2 message allows at most 40
    /// components in total, so no more than 40 top-level components.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_max_count(
            "components v2 message components",
            self.components.len(),
            40,
        )
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Vec<Value>, DiscordError> {
        self.validate()?;
        Ok(self.components)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionRowBuilder, ButtonBuilder, ComponentsV2Message, SelectDefaultValue, SelectMenuBuilder,
    };
    use crate::builders::container::TextDisplayBuilder;
    use crate::builders::modal::TextInputBuilder;
    use crate::builders::{
        ContainerBuilder, FileBuilder, MediaGalleryBuilder, SectionBuilder, SeparatorBuilder,
    };
    use crate::constants::{button_style, component_type, text_input_style};
    use crate::types::{Emoji, MediaGalleryItem, SelectOption};
    use serde_json::json;

    #[test]
    fn button_url_omits_custom_id_and_forces_link_style() {
        let payload = ButtonBuilder::new()
            .custom_id("button")
            .url("https://example.com")
            .build();

        assert_eq!(
            payload.get("style").and_then(|value| value.as_u64()),
            Some(button_style::LINK as u64)
        );
        assert!(payload.get("custom_id").is_none());
        assert_eq!(
            payload.get("url").and_then(|value| value.as_str()),
            Some("https://example.com")
        );
    }

    #[test]
    fn button_custom_id_omits_url_and_clears_link_style() {
        let payload = ButtonBuilder::new()
            .url("https://example.com")
            .custom_id("button")
            .build();

        assert_eq!(
            payload.get("style").and_then(|value| value.as_u64()),
            Some(button_style::PRIMARY as u64)
        );
        assert_eq!(
            payload.get("custom_id").and_then(|value| value.as_str()),
            Some("button")
        );
        assert!(payload.get("url").is_none());
    }

    #[test]
    fn button_link_style_without_url_falls_back_to_primary() {
        let payload = ButtonBuilder::new().style(button_style::LINK).build();

        assert_eq!(
            payload.get("style").and_then(|value| value.as_u64()),
            Some(button_style::PRIMARY as u64)
        );
        assert!(payload.get("url").is_none());
    }

    #[test]
    fn component_build_typed_preserves_structs_before_json_conversion() {
        let button = ButtonBuilder::new()
            .url("https://example.com")
            .custom_id("next")
            .build_typed();
        assert_eq!(button.custom_id.as_deref(), Some("next"));
        assert_eq!(button.url, None);
        assert_eq!(button.style, button_style::PRIMARY);

        let select = SelectMenuBuilder::channel("channels")
            .add_option(SelectOption::new("Ignored", "ignored"))
            .channel_types(vec![0, 2])
            .build_typed();
        assert!(select.options.is_empty());
        assert_eq!(select.channel_types, Some(vec![0, 2]));

        let row = ActionRowBuilder::new()
            .add_component(json!({ "type": 99 }))
            .id(42)
            .build_typed();
        assert_eq!(row.id, Some(42));
        assert_eq!(row.components.len(), 1);
    }

    #[test]
    fn string_select_omits_channel_types() {
        let payload = SelectMenuBuilder::string("menu")
            .add_option(SelectOption::new("One", "one"))
            .channel_types(vec![0, 2])
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::STRING_SELECT as u64)
        );
        assert!(payload.get("options").is_some());
        assert!(payload.get("channel_types").is_none());
    }

    #[test]
    fn channel_select_omits_options() {
        let payload = SelectMenuBuilder::channel("menu")
            .add_option(SelectOption::new("One", "one"))
            .channel_types(vec![0, 2])
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::CHANNEL_SELECT as u64)
        );
        assert!(payload.get("options").is_none());
        assert_eq!(
            payload
                .get("channel_types")
                .and_then(|value| value.as_array())
                .map(|value| value.len()),
            Some(2)
        );
    }

    #[test]
    fn non_string_non_channel_select_omits_variant_specific_fields() {
        let payload = SelectMenuBuilder::role("menu")
            .add_option(SelectOption::new("One", "one"))
            .channel_types(vec![0, 2])
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::ROLE_SELECT as u64)
        );
        assert!(payload.get("options").is_none());
        assert!(payload.get("channel_types").is_none());
    }

    #[test]
    fn button_serializes_optional_fields() {
        let payload = ButtonBuilder::new()
            .style(button_style::SUCCESS)
            .label("Ship")
            .emoji(Emoji::custom("party", "123", true))
            .disabled(true)
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::BUTTON as u64)
        );
        assert_eq!(
            payload.get("style").and_then(|value| value.as_u64()),
            Some(button_style::SUCCESS as u64)
        );
        assert_eq!(
            payload.get("label").and_then(|value| value.as_str()),
            Some("Ship")
        );
        assert_eq!(
            payload
                .get("emoji")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("party")
        );
        assert_eq!(
            payload
                .get("emoji")
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str()),
            Some("123")
        );
        assert_eq!(
            payload
                .get("emoji")
                .and_then(|value| value.get("animated"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            payload.get("disabled").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn button_emoji_unicode_sets_name_only() {
        let payload = ButtonBuilder::new().emoji_unicode("?뵦").build();

        assert_eq!(
            payload
                .get("emoji")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("?뵦")
        );
        assert!(payload
            .get("emoji")
            .and_then(|value| value.get("id"))
            .is_none());
    }

    #[test]
    fn action_row_builds_mixed_components_with_id() {
        let payload = ActionRowBuilder::new()
            .add_button(ButtonBuilder::new().label("Go").custom_id("go"))
            .add_select_menu(
                SelectMenuBuilder::string("menu")
                    .placeholder("Pick")
                    .min_values(1)
                    .max_values(2)
                    .disabled(true)
                    .add_options(vec![
                        SelectOption::new("One", "one"),
                        SelectOption::new("Two", "two"),
                    ]),
            )
            .add_text_input(
                TextInputBuilder::short("topic", "Topic")
                    .placeholder("Tell me")
                    .required(true),
            )
            .add_component(json!({ "type": 99, "custom": "raw" }))
            .id(7)
            .build();

        let components = payload
            .get("components")
            .and_then(|value| value.as_array())
            .expect("components array");

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::ACTION_ROW as u64)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(7));
        assert_eq!(components.len(), 4);
        assert_eq!(
            components[0].get("label").and_then(|value| value.as_str()),
            Some("Go")
        );
        assert_eq!(
            components[1].get("type").and_then(|value| value.as_u64()),
            Some(component_type::STRING_SELECT as u64)
        );
        assert_eq!(
            components[1]
                .get("options")
                .and_then(|value| value.as_array())
                .map(|value| value.len()),
            Some(2)
        );
        assert_eq!(
            components[1]
                .get("placeholder")
                .and_then(|value| value.as_str()),
            Some("Pick")
        );
        assert_eq!(
            components[1]
                .get("min_values")
                .and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(
            components[1]
                .get("max_values")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            components[1]
                .get("disabled")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            components[2].get("type").and_then(|value| value.as_u64()),
            Some(component_type::TEXT_INPUT as u64)
        );
        assert_eq!(
            components[2].get("style").and_then(|value| value.as_u64()),
            Some(text_input_style::SHORT as u64)
        );
        assert_eq!(
            components[3].get("custom").and_then(|value| value.as_str()),
            Some("raw")
        );
    }

    #[test]
    fn mentionable_select_keeps_shared_fields_and_omits_variant_specific_fields() {
        let payload = SelectMenuBuilder::mentionable("menu")
            .placeholder("Pick a target")
            .add_option(SelectOption::new("One", "one"))
            .channel_types(vec![0, 2])
            .default_user("42")
            .default_role("43")
            .required(false)
            .min_values(1)
            .max_values(3)
            .disabled(true)
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::MENTIONABLE_SELECT as u64)
        );
        assert_eq!(
            payload.get("placeholder").and_then(|value| value.as_str()),
            Some("Pick a target")
        );
        assert_eq!(
            payload.get("min_values").and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(
            payload.get("max_values").and_then(|value| value.as_u64()),
            Some(3)
        );
        assert_eq!(
            payload.get("disabled").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            payload.get("required").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            payload
                .get("default_values")
                .and_then(|value| value.as_array())
                .map(|values| values.len()),
            Some(2)
        );
        assert!(payload.get("options").is_none());
        assert!(payload.get("channel_types").is_none());
    }

    #[test]
    fn user_select_builder_sets_expected_component_type() {
        let payload = SelectMenuBuilder::user("menu")
            .id(22)
            .default_values(vec![SelectDefaultValue::user("900")])
            .build();

        assert_eq!(
            payload.get("type").and_then(|value| value.as_u64()),
            Some(component_type::USER_SELECT as u64)
        );
        assert_eq!(payload.get("id").and_then(|value| value.as_u64()), Some(22));
        assert_eq!(
            payload
                .get("default_values")
                .and_then(|value| value.as_array())
                .and_then(|values| values.first())
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("user")
        );
    }

    #[test]
    fn string_select_omits_auto_populated_default_values() {
        let payload = SelectMenuBuilder::string("menu")
            .default_channel("123")
            .add_option(SelectOption::new("One", "one"))
            .build();

        assert!(payload.get("default_values").is_none());
        assert!(payload.get("options").is_some());
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
    fn button_try_build_matches_build_for_valid_button() {
        let make = || ButtonBuilder::new().label("Go").custom_id("go");
        assert_eq!(make().build(), make().try_build().expect("valid button"));

        let link = || ButtonBuilder::new().label("Docs").url("https://example.com");
        assert_eq!(link().build(), link().try_build().expect("valid link button"));
    }

    #[test]
    fn button_try_build_rejects_invalid_buttons() {
        expect_model_error(
            ButtonBuilder::new()
                .label("Go")
                .custom_id(&"i".repeat(101))
                .try_build(),
            "button custom_id must be at most 100 characters, got 101",
        );
        expect_model_error(
            ButtonBuilder::new()
                .label(&"l".repeat(81))
                .custom_id("go")
                .try_build(),
            "button label must be at most 80 characters, got 81",
        );
        expect_model_error(
            ButtonBuilder::new().custom_id("go").try_build(),
            "button must have a label or emoji",
        );
        expect_model_error(
            ButtonBuilder::new().label("Go").try_build(),
            "button custom_id is required",
        );
        // Emoji satisfies the label-or-emoji rule.
        ButtonBuilder::new()
            .emoji_unicode("A")
            .custom_id("go")
            .try_build()
            .expect("emoji-only button is valid");
    }

    #[test]
    fn select_menu_try_build_matches_build_and_rejects_invalid_menus() {
        let make = || {
            SelectMenuBuilder::string("menu")
                .placeholder("Pick")
                .add_option(SelectOption::new("One", "one"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid select"));

        expect_model_error(
            SelectMenuBuilder::string("").add_option(SelectOption::new("One", "one")).try_build(),
            "select menu custom_id must be 1-100 characters, got 0",
        );
        expect_model_error(
            SelectMenuBuilder::string("menu").try_build(),
            "select menu options must contain 1-25 items, got 0",
        );

        let mut overfull = SelectMenuBuilder::string("menu");
        for index in 0..26 {
            overfull = overfull.add_option(SelectOption::new(&format!("o{index}"), "v"));
        }
        expect_model_error(
            overfull.try_build(),
            "select menu options must contain 1-25 items, got 26",
        );

        expect_model_error(
            SelectMenuBuilder::string("menu")
                .add_option(SelectOption::new(&"l".repeat(101), "v"))
                .try_build(),
            "label must be 1-100 characters, got 101",
        );
        expect_model_error(
            SelectMenuBuilder::string("menu")
                .add_option(SelectOption::new("One", &"v".repeat(101)))
                .try_build(),
            "select menu option \"One\" value must be 1-100 characters, got 101",
        );
        expect_model_error(
            SelectMenuBuilder::string("menu")
                .add_option(SelectOption::new("One", "one").description(&"d".repeat(101)))
                .try_build(),
            "select menu option \"One\" description must be at most 100 characters, got 101",
        );
        expect_model_error(
            SelectMenuBuilder::string("menu")
                .placeholder(&"p".repeat(151))
                .add_option(SelectOption::new("One", "one"))
                .try_build(),
            "select menu placeholder must be at most 150 characters, got 151",
        );
        expect_model_error(
            SelectMenuBuilder::user("menu").max_values(26).try_build(),
            "select menu max_values must be 1-25, got 26",
        );
        expect_model_error(
            SelectMenuBuilder::user("menu")
                .min_values(3)
                .max_values(2)
                .try_build(),
            "select menu min_values (3) must not exceed max_values (2)",
        );
    }

    #[test]
    fn action_row_try_build_enforces_component_limits() {
        let make = || {
            ActionRowBuilder::new()
                .add_button(ButtonBuilder::new().label("Go").custom_id("go"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid row"));

        expect_model_error(
            ActionRowBuilder::new().try_build(),
            "action row components must contain 1-5 items, got 0",
        );

        let mut overfull = ActionRowBuilder::new();
        for index in 0..6 {
            overfull = overfull.add_button(
                ButtonBuilder::new()
                    .label("Go")
                    .custom_id(&format!("go{index}")),
            );
        }
        expect_model_error(
            overfull.try_build(),
            "action row components must contain 1-5 items, got 6",
        );

        expect_model_error(
            ActionRowBuilder::new()
                .add_select_menu(
                    SelectMenuBuilder::string("menu").add_option(SelectOption::new("One", "one")),
                )
                .add_button(ButtonBuilder::new().label("Go").custom_id("go"))
                .try_build(),
            "must be the only component in its row, got 2 components",
        );
    }

    #[test]
    fn components_v2_message_try_build_enforces_total_component_limit() {
        let make = || ComponentsV2Message::new().add_text_display(TextDisplayBuilder::new("hi"));
        assert_eq!(make().build(), make().try_build().expect("valid message"));

        let mut overfull = ComponentsV2Message::new();
        for _ in 0..41 {
            overfull = overfull.add_text_display(TextDisplayBuilder::new("x"));
        }
        expect_model_error(
            overfull.try_build(),
            "components v2 message components must contain at most 40 items, got 41",
        );
    }

    #[test]
    fn components_v2_message_preserves_component_order() {
        let payload = ComponentsV2Message::new()
            .add_text_display(TextDisplayBuilder::new("Intro").id(1))
            .add_action_row(
                ActionRowBuilder::new()
                    .add_button(ButtonBuilder::new().label("Continue").custom_id("continue")),
            )
            .add_component(json!({ "type": 255, "marker": "raw" }))
            .build();

        assert_eq!(payload.len(), 3);
        assert_eq!(
            payload[0].get("type").and_then(|value| value.as_u64()),
            Some(component_type::TEXT_DISPLAY as u64)
        );
        assert_eq!(
            payload[0].get("content").and_then(|value| value.as_str()),
            Some("Intro")
        );
        assert_eq!(
            payload[1].get("type").and_then(|value| value.as_u64()),
            Some(component_type::ACTION_ROW as u64)
        );
        assert_eq!(
            payload[2].get("marker").and_then(|value| value.as_str()),
            Some("raw")
        );
    }

    #[test]
    fn components_v2_message_supports_all_builder_entry_points() {
        let payload = ComponentsV2Message::new()
            .add_container(
                ContainerBuilder::new().add_text_display(TextDisplayBuilder::new("inside")),
            )
            .add_media_gallery(
                MediaGalleryBuilder::new()
                    .add_item(MediaGalleryItem::new("https://example.com/image.png")),
            )
            .add_separator(SeparatorBuilder::new())
            .add_section(SectionBuilder::new().add_text_display(TextDisplayBuilder::new("section")))
            .add_file(FileBuilder::new("https://example.com/file.txt"))
            .build();

        assert_eq!(
            payload
                .iter()
                .map(|component| component.get("type").and_then(|value| value.as_u64()))
                .collect::<Vec<_>>(),
            vec![
                Some(component_type::CONTAINER as u64),
                Some(component_type::MEDIA_GALLERY as u64),
                Some(component_type::SEPARATOR as u64),
                Some(component_type::SECTION as u64),
                Some(component_type::FILE as u64),
            ]
        );
    }
}
