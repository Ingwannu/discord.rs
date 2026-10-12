//! Button, action row, and select menu builders, and [`ComponentsV2Message`].
//!
//! Each builder holds the typed request model from [`crate::request`] (or,
//! for [`SelectMenuBuilder`], typed fields it turns into one), so `build()`
//! returns data that serializes straight to JSON bytes. `build()` does not
//! check limits; `try_build()` checks them and returns the same type.
//!
//! The 2.x [`ButtonBuilder`] described custom, link, and premium buttons with
//! one set of optional fields; it is deprecated in favour of
//! [`CustomButtonBuilder`], [`LinkButtonBuilder`], and
//! [`PremiumButtonBuilder`], and converts to a [`Button`] only through a
//! fallible conversion. [`ActionRowBuilder`] builds a row of buttons; a
//! select menu row is the [`SelectMenuBuilder`] itself, passed wherever an
//! action row is accepted (`impl Into<ActionRow>`), so a row mixing buttons
//! and a select menu does not compile.

use crate::error::DiscordError;
use crate::model::{
    ButtonStyle, ChannelId, ChannelType, ComponentId, ComponentType, MentionableId, RoleId,
    SelectDefaultValue, SkuId, UserId,
};
use crate::request::{
    ActionRow, Button, ButtonRow, ChannelSelect, Component, CustomButton, LinkButton,
    MentionableSelect, PremiumButton, RoleSelect, SectionAccessory, SelectMenu, StringSelect,
    UserSelect,
};
use crate::types::{Emoji, SelectOption};

use super::container::{ContainerBuilder, SeparatorBuilder, TextDisplayBuilder};
use super::media::{FileBuilder, MediaGalleryBuilder, SectionBuilder};

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Builds a [`CustomButton`]: a primary, secondary, success, or danger
/// button that sends an interaction with its `custom_id`.
#[derive(Clone, Debug)]
pub struct CustomButtonBuilder {
    inner: CustomButton,
}

impl CustomButtonBuilder {
    /// A [`ButtonStyle::PRIMARY`] button with this `custom_id`.
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            inner: CustomButton::new(ButtonStyle::PRIMARY, custom_id),
        }
    }

    /// Sets the style ([`ButtonStyle::PRIMARY`], `SECONDARY`, `SUCCESS`, or
    /// `DANGER`).
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.inner.style = style;
        self
    }

    /// Sets the button text (up to 80 characters).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.label = Some(label.into());
        self
    }

    /// Sets the emoji.
    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.inner.emoji = Some(emoji);
        self
    }

    /// Sets a Unicode emoji.
    pub fn emoji_unicode(mut self, emoji: &str) -> Self {
        self.inner.emoji = Some(Emoji::unicode(emoji));
        self
    }

    /// Shows the button disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.disabled = Some(disabled);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The button, without checking limits.
    pub fn build(self) -> CustomButton {
        self.inner
    }

    /// Checks the button against Discord's limits: `custom_id` 1-100
    /// characters, label up to 80, a label or an emoji, and a non-link,
    /// non-premium style.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<CustomButton, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Builds a [`LinkButton`] (style 5), which opens a URL.
#[derive(Clone, Debug)]
pub struct LinkButtonBuilder {
    inner: LinkButton,
}

impl LinkButtonBuilder {
    /// A link button to `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            inner: LinkButton::new(url),
        }
    }

    /// Sets the button text (up to 80 characters).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.label = Some(label.into());
        self
    }

    /// Sets the emoji.
    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.inner.emoji = Some(emoji);
        self
    }

    /// Sets a Unicode emoji.
    pub fn emoji_unicode(mut self, emoji: &str) -> Self {
        self.inner.emoji = Some(Emoji::unicode(emoji));
        self
    }

    /// Shows the button disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.disabled = Some(disabled);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The button, without checking limits.
    pub fn build(self) -> LinkButton {
        self.inner
    }

    /// Checks the button against Discord's limits: URL up to 512
    /// characters, label up to 80, and a label or an emoji.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<LinkButton, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Builds a [`PremiumButton`] (style 6), which opens the purchase flow for
/// an SKU. Discord draws its label and emoji.
#[derive(Clone, Debug)]
pub struct PremiumButtonBuilder {
    inner: PremiumButton,
}

impl PremiumButtonBuilder {
    /// A purchase button for `sku_id`.
    pub fn new(sku_id: SkuId) -> Self {
        Self {
            inner: PremiumButton::new(sku_id),
        }
    }

    /// Shows the button disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner.disabled = Some(disabled);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The button. A premium button has no limits to check, so this is
    /// also what [`Self::try_build`] returns.
    pub fn build(self) -> PremiumButton {
        self.inner
    }

    /// Always succeeds; kept so every builder has the same shape.
    pub fn validate(&self) -> Result<(), DiscordError> {
        Ok(())
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<PremiumButton, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// `From` a button builder into every button position.
macro_rules! button_builder_into {
    ($($builder:ty),*) => {$(
        impl From<$builder> for Button {
            fn from(builder: $builder) -> Self {
                builder.build().into()
            }
        }

        impl From<$builder> for SectionAccessory {
            fn from(builder: $builder) -> Self {
                Self::Button(builder.build().into())
            }
        }
    )*};
}

button_builder_into!(CustomButtonBuilder, LinkButtonBuilder, PremiumButtonBuilder);

/// The 2.x button builder: one set of optional fields for every button
/// kind, told apart by `style` and `url`.
///
/// It can describe buttons Discord rejects (a premium style without an SKU,
/// a custom style without a `custom_id`), so it converts to a [`Button`]
/// only through `Button::try_from` / [`Self::build`], which fail instead of
/// guessing. Use [`CustomButtonBuilder`], [`LinkButtonBuilder`], or
/// [`PremiumButtonBuilder`].
#[deprecated(
    since = "3.0.0",
    note = "use CustomButtonBuilder, LinkButtonBuilder, or PremiumButtonBuilder; \
            this converts to a Button only with Button::try_from"
)]
#[derive(Clone, Debug)]
pub struct ButtonBuilder {
    style: ButtonStyle,
    label: Option<String>,
    emoji: Option<Emoji>,
    custom_id: Option<String>,
    url: Option<String>,
    disabled: Option<bool>,
}

#[expect(deprecated, reason = "the deprecated adapter's own impl")]
impl ButtonBuilder {
    /// An empty [`ButtonStyle::PRIMARY`] button.
    pub fn new() -> Self {
        Self {
            style: ButtonStyle::PRIMARY,
            label: None,
            emoji: None,
            custom_id: None,
            url: None,
            disabled: None,
        }
    }

    /// Sets the style. A `url` forces [`ButtonStyle::LINK`].
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the button text.
    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Sets the emoji.
    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.emoji = Some(emoji);
        self
    }

    /// Sets a Unicode emoji.
    pub fn emoji_unicode(mut self, emoji: &str) -> Self {
        self.emoji = Some(Emoji::unicode(emoji));
        self
    }

    /// Sets the `custom_id` and clears any `url` (a link style becomes
    /// primary), as in 2.x.
    pub fn custom_id(mut self, custom_id: &str) -> Self {
        self.custom_id = Some(custom_id.to_string());
        self.url = None;
        if self.style == ButtonStyle::LINK {
            self.style = ButtonStyle::PRIMARY;
        }
        self
    }

    /// Sets the `url`, clears any `custom_id`, and switches to the link
    /// style, as in 2.x.
    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self.custom_id = None;
        self.style = ButtonStyle::LINK;
        self
    }

    /// Shows the button disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// Converts to a typed [`Button`] without checking limits. Fails when
    /// the fields do not describe a button: a premium style (this builder
    /// has no `sku_id`) or a non-link style without a `custom_id`.
    pub fn build(self) -> Result<Button, DiscordError> {
        Button::try_from(self)
    }

    /// Checks the button against Discord's limits without consuming the
    /// builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        Button::try_from(self.clone())?.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Button, DiscordError> {
        let button = Button::try_from(self)?;
        button.validate()?;
        Ok(button)
    }
}

/// Same as [`ButtonBuilder::new`].
#[expect(deprecated, reason = "the deprecated adapter's own impl")]
impl Default for ButtonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// The 2.x normalization, made explicit: a `url` makes a link button;
/// otherwise the style decides. Fails for [`ButtonStyle::PREMIUM`] (no
/// `sku_id` to carry) and for a custom style without a `custom_id`, rather
/// than sending a button Discord rejects.
#[expect(deprecated, reason = "converts the deprecated adapter")]
impl TryFrom<ButtonBuilder> for Button {
    type Error = DiscordError;

    fn try_from(builder: ButtonBuilder) -> Result<Self, Self::Error> {
        let ButtonBuilder {
            style,
            label,
            emoji,
            custom_id,
            url,
            disabled,
        } = builder;
        if let Some(url) = url {
            return Ok(Self::Link(LinkButton {
                label,
                emoji,
                disabled,
                ..LinkButton::new(url)
            }));
        }
        if style == ButtonStyle::PREMIUM {
            return Err(DiscordError::model(
                "a premium button needs a sku_id, which ButtonBuilder cannot hold; \
                 use PremiumButtonBuilder",
            ));
        }
        let custom_id = custom_id.ok_or_else(|| {
            DiscordError::model("button custom_id is required for non-link, non-premium styles")
        })?;
        // A link style without a url fell back to primary in 2.x.
        let style = if style == ButtonStyle::LINK {
            ButtonStyle::PRIMARY
        } else {
            style
        };
        Ok(Self::Custom(CustomButton {
            label,
            emoji,
            disabled,
            ..CustomButton::new(style, custom_id)
        }))
    }
}

// ---------------------------------------------------------------------------
// Action rows
// ---------------------------------------------------------------------------

/// Builds an action row of 1-5 buttons.
///
/// A row holds buttons or one select menu, never both. For a select menu
/// row pass the [`SelectMenuBuilder`] (or a [`SelectMenu`]) wherever an
/// action row is accepted.
#[derive(Clone, Debug, Default)]
pub struct ActionRowBuilder {
    inner: ButtonRow,
}

impl ActionRowBuilder {
    /// An empty row.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a button: a [`CustomButtonBuilder`], [`LinkButtonBuilder`],
    /// [`PremiumButtonBuilder`], or a typed button.
    pub fn add_button(mut self, button: impl Into<Button>) -> Self {
        self.inner.buttons.push(button.into());
        self
    }

    /// Sets the row's component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The row, without checking limits.
    pub fn build(self) -> ActionRow {
        ActionRow::Buttons(self.inner)
    }

    /// Checks the row against Discord's limits: 1-5 buttons, each valid.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<ActionRow, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

impl From<ActionRowBuilder> for ActionRow {
    fn from(builder: ActionRowBuilder) -> Self {
        builder.build()
    }
}

impl From<ActionRowBuilder> for Component {
    fn from(builder: ActionRowBuilder) -> Self {
        Self::ActionRow(builder.build())
    }
}

// ---------------------------------------------------------------------------
// Select menus
// ---------------------------------------------------------------------------

/// Builds one of the five select menus.
///
/// One builder covers every kind, as in 2.x, so a setter can name a field
/// the chosen kind does not have (options on a user select, a channel
/// default on a role select). [`Self::build`] leaves those out, as 2.x did;
/// [`Self::validate`] and [`Self::try_build`] report them.
#[derive(Clone, Debug)]
pub struct SelectMenuBuilder {
    kind: ComponentType,
    id: Option<ComponentId>,
    custom_id: String,
    placeholder: Option<String>,
    min_values: Option<u8>,
    max_values: Option<u8>,
    required: Option<bool>,
    disabled: Option<bool>,
    options: Vec<SelectOption>,
    default_values: Vec<SelectDefaultValue>,
    channel_types: Option<Vec<ChannelType>>,
}

impl SelectMenuBuilder {
    fn with_kind(kind: ComponentType, custom_id: impl Into<String>) -> Self {
        Self {
            kind,
            id: None,
            custom_id: custom_id.into(),
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
            options: Vec::new(),
            default_values: Vec::new(),
            channel_types: None,
        }
    }

    /// A string select (type 3) of developer-defined options.
    pub fn string(custom_id: impl Into<String>) -> Self {
        Self::with_kind(ComponentType::STRING_SELECT, custom_id)
    }

    /// A role select (type 6).
    pub fn role(custom_id: impl Into<String>) -> Self {
        Self::with_kind(ComponentType::ROLE_SELECT, custom_id)
    }

    /// A channel select (type 8).
    pub fn channel(custom_id: impl Into<String>) -> Self {
        Self::with_kind(ComponentType::CHANNEL_SELECT, custom_id)
    }

    /// A user select (type 5).
    pub fn user(custom_id: impl Into<String>) -> Self {
        Self::with_kind(ComponentType::USER_SELECT, custom_id)
    }

    /// A mentionable (user and role) select (type 7).
    pub fn mentionable(custom_id: impl Into<String>) -> Self {
        Self::with_kind(ComponentType::MENTIONABLE_SELECT, custom_id)
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the placeholder (up to 150 characters).
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Appends an option (string selects).
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.options.push(option);
        self
    }

    /// Appends options (string selects).
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.options.extend(options);
        self
    }

    /// Pre-selects a value (user, role, mentionable, and channel selects).
    pub fn default_value(mut self, value: SelectDefaultValue) -> Self {
        self.default_values.push(value);
        self
    }

    /// Pre-selects values.
    pub fn default_values(mut self, values: Vec<SelectDefaultValue>) -> Self {
        self.default_values.extend(values);
        self
    }

    /// Pre-selects a user (user and mentionable selects).
    pub fn default_user(self, id: UserId) -> Self {
        self.default_value(SelectDefaultValue::User(id))
    }

    /// Pre-selects a role (role and mentionable selects).
    pub fn default_role(self, id: RoleId) -> Self {
        self.default_value(SelectDefaultValue::Role(id))
    }

    /// Pre-selects a channel (channel selects).
    pub fn default_channel(self, id: ChannelId) -> Self {
        self.default_value(SelectDefaultValue::Channel(id))
    }

    /// Pre-selects a user or role in a mentionable select.
    pub fn default_mentionable(self, id: MentionableId) -> Self {
        self.default_value(SelectDefaultValue::from(id))
    }

    /// Restricts a channel select to these channel types.
    pub fn channel_types(mut self, channel_types: impl IntoIterator<Item = ChannelType>) -> Self {
        self.channel_types = Some(channel_types.into_iter().collect());
        self
    }

    /// Sets the fewest items to choose (0-25).
    pub fn min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Sets the most items to choose (1-25).
    pub fn max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Whether an answer is required (modals only; ignored in messages).
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Shows the menu disabled (messages only; modals reject it).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// The fields set on this builder that its kind does not have, which
    /// [`Self::build`] leaves out.
    fn stray_fields(&self) -> Vec<String> {
        let mut stray = Vec::new();
        let is_string = self.kind == ComponentType::STRING_SELECT;
        if !is_string && !self.options.is_empty() {
            stray.push("options".to_owned());
        }
        if self.kind != ComponentType::CHANNEL_SELECT && self.channel_types.is_some() {
            stray.push("channel_types".to_owned());
        }
        for value in &self.default_values {
            let fits = match value {
                SelectDefaultValue::User(_) => matches!(
                    self.kind,
                    ComponentType::USER_SELECT | ComponentType::MENTIONABLE_SELECT
                ),
                SelectDefaultValue::Role(_) => matches!(
                    self.kind,
                    ComponentType::ROLE_SELECT | ComponentType::MENTIONABLE_SELECT
                ),
                SelectDefaultValue::Channel(_) => self.kind == ComponentType::CHANNEL_SELECT,
                SelectDefaultValue::Unknown { .. } => false,
            };
            if !fits {
                stray.push(format!("{} default value", value.kind()));
            }
        }
        stray
    }

    /// The typed select menu. Fields the kind does not have (see the type
    /// docs) are left out, as in 2.x; limits are not checked.
    pub fn build(self) -> SelectMenu {
        let Self {
            kind,
            id,
            custom_id,
            placeholder,
            min_values,
            max_values,
            required,
            disabled,
            options,
            default_values,
            channel_types,
        } = self;
        macro_rules! common {
            ($ty:ident { $($extra:ident: $value:expr),* }) => {
                $ty {
                    id,
                    placeholder,
                    min_values,
                    max_values,
                    required,
                    disabled,
                    $($extra: $value,)*
                    ..$ty::new(custom_id)
                }
            };
        }
        let values = default_values.iter();
        match kind {
            ComponentType::USER_SELECT => SelectMenu::User(common!(UserSelect {
                default_values: values
                    .filter_map(|value| match value {
                        SelectDefaultValue::User(id) => Some(*id),
                        _ => None,
                    })
                    .collect()
            })),
            ComponentType::ROLE_SELECT => SelectMenu::Role(common!(RoleSelect {
                default_values: values
                    .filter_map(|value| match value {
                        SelectDefaultValue::Role(id) => Some(*id),
                        _ => None,
                    })
                    .collect()
            })),
            ComponentType::MENTIONABLE_SELECT => {
                SelectMenu::Mentionable(common!(MentionableSelect {
                    default_values: values
                        .filter_map(|value| match value {
                            SelectDefaultValue::User(id) => Some(MentionableId::User(*id)),
                            SelectDefaultValue::Role(id) => Some(MentionableId::Role(*id)),
                            _ => None,
                        })
                        .collect()
                }))
            }
            ComponentType::CHANNEL_SELECT => SelectMenu::Channel(common!(ChannelSelect {
                default_values: values
                    .filter_map(|value| match value {
                        SelectDefaultValue::Channel(id) => Some(*id),
                        _ => None,
                    })
                    .collect(),
                channel_types: channel_types
            })),
            // The constructors only make the five kinds above and this one.
            _ => SelectMenu::String(common!(StringSelect { options: options })),
        }
    }

    /// Checks the select menu against Discord's limits, and that no field
    /// is set that its kind does not have.
    pub fn validate(&self) -> Result<(), DiscordError> {
        let stray = self.stray_fields();
        if !stray.is_empty() {
            return Err(DiscordError::model(format!(
                "select menu of type {} does not take: {}",
                self.kind.get(),
                stray.join(", ")
            )));
        }
        self.clone().build().validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<SelectMenu, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

impl From<SelectMenuBuilder> for SelectMenu {
    fn from(builder: SelectMenuBuilder) -> Self {
        builder.build()
    }
}

/// The select menu on its own row.
impl From<SelectMenuBuilder> for ActionRow {
    fn from(builder: SelectMenuBuilder) -> Self {
        builder.build().into()
    }
}

/// The select menu on its own row.
impl From<SelectMenuBuilder> for Component {
    fn from(builder: SelectMenuBuilder) -> Self {
        Self::ActionRow(builder.build().into())
    }
}

// ---------------------------------------------------------------------------
// Components V2 message
// ---------------------------------------------------------------------------

/// The top-level components of a Components V2 message, for
/// [`MessageBuilder::components_v2`](crate::builders::MessageBuilder::components_v2),
/// which also sets the `IS_COMPONENTS_V2` flag.
#[derive(Clone, Debug, Default)]
pub struct ComponentsV2Message {
    components: Vec<Component>,
}

impl ComponentsV2Message {
    /// An empty message.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a container.
    pub fn add_container(mut self, container: ContainerBuilder) -> Self {
        self.components.push(container.build().into());
        self
    }

    /// Appends a text display.
    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.components.push(text.build().into());
        self
    }

    /// Appends a media gallery.
    pub fn add_media_gallery(mut self, gallery: MediaGalleryBuilder) -> Self {
        self.components.push(gallery.build().into());
        self
    }

    /// Appends a separator.
    pub fn add_separator(mut self, separator: SeparatorBuilder) -> Self {
        self.components.push(separator.build().into());
        self
    }

    /// Appends a section (one with an accessory set).
    pub fn add_section(mut self, section: SectionBuilder<SectionAccessory>) -> Self {
        self.components.push(section.build().into());
        self
    }

    /// Appends a file component.
    pub fn add_file(mut self, file: FileBuilder) -> Self {
        self.components.push(file.build().into());
        self
    }

    /// Appends an action row: an [`ActionRowBuilder`] of buttons or a
    /// [`SelectMenuBuilder`].
    pub fn add_action_row(mut self, row: impl Into<ActionRow>) -> Self {
        self.components.push(Component::ActionRow(row.into()));
        self
    }

    /// Appends any typed component, including a
    /// [`RawComponent`](crate::request::RawComponent).
    pub fn add_component(mut self, component: impl Into<Component>) -> Self {
        self.components.push(component.into());
        self
    }

    /// The components, without checking limits.
    pub fn build(self) -> Vec<Component> {
        self.components
    }

    /// Checks every component and the message-wide rules: at most 40
    /// components counting nested ones, unique `id`s and `custom_id`s.
    pub fn validate(&self) -> Result<(), DiscordError> {
        crate::request::check_message_components(
            &self.components,
            crate::request::ComponentsMode::V2,
        )
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Vec<Component>, DiscordError> {
        self.validate()?;
        Ok(self.components)
    }
}

#[cfg(test)]
mod tests {
    #![expect(deprecated, reason = "covers the deprecated ButtonBuilder adapter")]

    use super::{
        ActionRowBuilder, ButtonBuilder, ComponentsV2Message, CustomButtonBuilder,
        LinkButtonBuilder, PremiumButtonBuilder, SelectDefaultValue, SelectMenuBuilder,
    };
    use crate::builders::container::TextDisplayBuilder;
    use crate::builders::{
        ContainerBuilder, FileBuilder, MediaGalleryBuilder, SectionBuilder, SeparatorBuilder,
        ThumbnailBuilder,
    };
    use crate::model::{test_id, ButtonStyle, ChannelType, ComponentId, ComponentType};
    use crate::request::{Button, RawComponent, SelectMenu};
    use crate::types::{Emoji, MediaGalleryItem, SelectOption};
    use serde_json::{json, Value};

    fn to_json<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("serializable")
    }

    #[test]
    fn deprecated_button_url_makes_a_link_button_without_custom_id() {
        let button = ButtonBuilder::new()
            .custom_id("button")
            .url("https://example.com")
            .build()
            .unwrap();
        assert!(matches!(button, Button::Link(_)));
        assert_eq!(
            to_json(&button),
            json!({"type": 2, "style": 5, "url": "https://example.com"})
        );
    }

    #[test]
    fn deprecated_button_custom_id_clears_url_and_link_style() {
        let button = ButtonBuilder::new()
            .url("https://example.com")
            .custom_id("button")
            .label("Go")
            .build()
            .unwrap();
        assert_eq!(
            to_json(&button),
            json!({"type": 2, "style": 1, "label": "Go", "custom_id": "button"})
        );
    }

    #[test]
    fn deprecated_button_conversion_fails_instead_of_guessing() {
        let premium = ButtonBuilder::new()
            .style(ButtonStyle::PREMIUM)
            .label("Buy");
        assert!(Button::try_from(premium)
            .unwrap_err()
            .to_string()
            .contains("premium button needs a sku_id"));
        let no_id = ButtonBuilder::new().style(ButtonStyle::LINK).label("Go");
        assert!(no_id
            .build()
            .unwrap_err()
            .to_string()
            .contains("button custom_id is required"));
    }

    #[test]
    fn button_builders_serialize_their_kind() {
        let custom = CustomButtonBuilder::new("ship")
            .style(ButtonStyle::SUCCESS)
            .label("Ship")
            .emoji(Emoji::custom("party", test_id(123), true))
            .disabled(true)
            .build();
        assert_eq!(
            to_json(&custom),
            json!({
                "type": 2, "style": 3, "label": "Ship",
                "emoji": {"id": "123", "name": "party", "animated": true},
                "custom_id": "ship", "disabled": true
            })
        );
        let link = LinkButtonBuilder::new("https://example.com")
            .emoji_unicode("🔗")
            .build();
        assert_eq!(
            to_json(&link),
            json!({"type": 2, "style": 5, "emoji": {"name": "🔗"}, "url": "https://example.com"})
        );
        let premium = PremiumButtonBuilder::new(test_id(77))
            .id(ComponentId::new(4))
            .build();
        assert_eq!(
            to_json(&premium),
            json!({"type": 2, "id": 4, "style": 6, "sku_id": "77"})
        );
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
    fn button_try_build_rejects_invalid_buttons() {
        expect_model_error(
            CustomButtonBuilder::new("i".repeat(101))
                .label("Go")
                .try_build(),
            "button custom_id must be 1-100 characters, got 101",
        );
        expect_model_error(
            CustomButtonBuilder::new("go")
                .label("l".repeat(81))
                .try_build(),
            "button label must be at most 80 characters, got 81",
        );
        expect_model_error(
            CustomButtonBuilder::new("go").try_build(),
            "button must have a label or emoji",
        );
        expect_model_error(
            CustomButtonBuilder::new("go")
                .label("Go")
                .style(ButtonStyle::LINK)
                .try_build(),
            "use LinkButton or PremiumButton",
        );
        expect_model_error(
            LinkButtonBuilder::new(format!("https://e.com/{}", "a".repeat(499)))
                .label("Docs")
                .try_build(),
            "button url must be 1-512 characters, got 513",
        );
        CustomButtonBuilder::new("go")
            .emoji_unicode("A")
            .try_build()
            .expect("emoji-only button is valid");
        // The deprecated adapter checks the same limits after converting.
        expect_model_error(
            ButtonBuilder::new()
                .label(&"l".repeat(81))
                .custom_id("go")
                .try_build(),
            "button label must be at most 80 characters, got 81",
        );
    }

    #[test]
    fn select_menu_kinds_keep_only_their_fields() {
        let string = SelectMenuBuilder::string("menu")
            .add_option(SelectOption::new("One", "one"))
            .default_channel(test_id(123))
            .channel_types([ChannelType::GUILD_TEXT]);
        assert!(string
            .validate()
            .unwrap_err()
            .to_string()
            .contains("does not take: channel_types, channel default value"));
        assert_eq!(
            to_json(&string.build()),
            json!({"type": 3, "custom_id": "menu", "options": [{"label": "One", "value": "one"}]})
        );

        let channel = SelectMenuBuilder::channel("menu")
            .add_option(SelectOption::new("Ignored", "ignored"))
            .channel_types([ChannelType::GUILD_TEXT, ChannelType::GUILD_VOICE])
            .build();
        assert_eq!(
            to_json(&channel),
            json!({"type": 8, "custom_id": "menu", "channel_types": [0, 2]})
        );

        let mentionable = SelectMenuBuilder::mentionable("menu")
            .placeholder("Pick a target")
            .default_user(test_id(42))
            .default_role(test_id(43))
            .required(false)
            .min_values(1)
            .max_values(3)
            .disabled(true)
            .build();
        assert_eq!(
            to_json(&mentionable),
            json!({
                "type": 7, "custom_id": "menu", "placeholder": "Pick a target",
                "min_values": 1, "max_values": 3, "required": false, "disabled": true,
                "default_values": [{"id": "42", "type": "user"}, {"id": "43", "type": "role"}]
            })
        );

        let user = SelectMenuBuilder::user("menu")
            .id(ComponentId::new(22))
            .default_values(vec![SelectDefaultValue::user(test_id(900))])
            .build();
        assert!(matches!(user, SelectMenu::User(_)));
        assert_eq!(
            to_json(&user),
            json!({"type": 5, "id": 22, "custom_id": "menu",
                   "default_values": [{"id": "900", "type": "user"}]})
        );
    }

    #[test]
    fn select_menu_try_build_rejects_invalid_menus() {
        let make = || {
            SelectMenuBuilder::string("menu")
                .placeholder("Pick")
                .add_option(SelectOption::new("One", "one"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid select"));

        expect_model_error(
            SelectMenuBuilder::string("")
                .add_option(SelectOption::new("One", "one"))
                .try_build(),
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
                .placeholder("p".repeat(151))
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
        expect_model_error(
            SelectMenuBuilder::user("menu")
                .default_user(test_id(1))
                .default_user(test_id(2))
                .try_build(),
            "select menu default_values must contain 1-1 items",
        );
    }

    #[test]
    fn action_row_try_build_enforces_component_limits() {
        let make =
            || ActionRowBuilder::new().add_button(CustomButtonBuilder::new("go").label("Go"));
        assert_eq!(make().build(), make().try_build().expect("valid row"));

        expect_model_error(
            ActionRowBuilder::new().try_build(),
            "action row components must contain 1-5 items, got 0",
        );
        let mut overfull = ActionRowBuilder::new();
        for index in 0..6 {
            overfull =
                overfull.add_button(CustomButtonBuilder::new(format!("go{index}")).label("Go"));
        }
        expect_model_error(
            overfull.try_build(),
            "action row components must contain 1-5 items, got 6",
        );
        expect_model_error(
            ActionRowBuilder::new()
                .add_button(CustomButtonBuilder::new("go").label("Go"))
                .add_button(CustomButtonBuilder::new("go").label("Again"))
                .try_build(),
            "components[1]: custom_id \"go\" is used more than once",
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
            "components v2 message components must contain at most 40 items",
        );
        // Nested components count too: one container with 40 children is 41.
        let mut container = ContainerBuilder::new();
        for _ in 0..40 {
            container = container.add_text_display(TextDisplayBuilder::new("x"));
        }
        expect_model_error(
            ComponentsV2Message::new()
                .add_container(container)
                .try_build(),
            "got 41",
        );
    }

    #[test]
    fn components_v2_message_preserves_order_and_raw_components() {
        let payload = ComponentsV2Message::new()
            .add_text_display(TextDisplayBuilder::new("Intro").id(ComponentId::new(1)))
            .add_action_row(
                ActionRowBuilder::new()
                    .add_button(CustomButtonBuilder::new("continue").label("Continue")),
            )
            .add_action_row(SelectMenuBuilder::user("who"))
            .add_component(RawComponent::from_json(r#"{"type":255,"marker":"raw"}"#).unwrap())
            .build();

        assert_eq!(
            to_json(&payload),
            json!([
                {"type": 10, "id": 1, "content": "Intro"},
                {"type": 1, "components": [
                    {"type": 2, "style": 1, "label": "Continue", "custom_id": "continue"}
                ]},
                {"type": 1, "components": [{"type": 5, "custom_id": "who"}]},
                {"type": 255, "marker": "raw"}
            ])
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
            .add_section(
                SectionBuilder::new()
                    .add_text_display(TextDisplayBuilder::new("section"))
                    .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png")),
            )
            .add_file(FileBuilder::new("attachment://file.txt"))
            .build();

        assert_eq!(
            to_json(&payload)
                .as_array()
                .unwrap()
                .iter()
                .map(|component| component["type"].as_u64())
                .collect::<Vec<_>>(),
            vec![
                Some(u64::from(ComponentType::CONTAINER.get())),
                Some(u64::from(ComponentType::MEDIA_GALLERY.get())),
                Some(u64::from(ComponentType::SEPARATOR.get())),
                Some(u64::from(ComponentType::SECTION.get())),
                Some(u64::from(ComponentType::FILE.get())),
            ]
        );
    }
}
