//! Button, action row, and select menu builders, and [`ComponentsV2Message`].
//!
//! Each builder holds the typed request model from [`crate::request`], so
//! `build()` returns data that serializes straight to JSON bytes. `build()`
//! does not check limits; `try_build()` checks them and returns the same
//! type. No builder drops a value its model cannot hold: such a value either
//! has no setter on that builder or fails a conversion.
//!
//! The 2.x [`ButtonBuilder`] described custom, link, and premium buttons with
//! one set of optional fields; it is deprecated in favour of
//! [`CustomButtonBuilder`], [`LinkButtonBuilder`], and
//! [`PremiumButtonBuilder`], and converts to a [`Button`] only through a
//! fallible conversion. [`ActionRowBuilder`] builds a row of buttons; a
//! select menu row is the select builder itself, passed wherever an action
//! row is accepted (`impl Into<ActionRow>`), so a row mixing buttons and a
//! select menu does not compile. [`SelectMenuBuilder`]'s constructors return
//! one builder per select kind ([`StringSelectBuilder`],
//! [`UserSelectBuilder`], [`RoleSelectBuilder`],
//! [`MentionableSelectBuilder`], [`ChannelSelectBuilder`]), each with only the
//! setters its kind takes.

use crate::error::DiscordError;
use crate::model::{
    ButtonStyle, ChannelId, ChannelType, ComponentId, MentionableId, RoleId, SkuId, UserId,
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

/// The 2.x entry point for select menus: each constructor returns the
/// builder for one kind, which has only the setters that kind takes.
///
/// ```
/// use discordrs::builders::SelectMenuBuilder;
/// use discordrs::SelectOption;
///
/// let pick = SelectMenuBuilder::string("topic").add_option(SelectOption::new("Billing", "billing"));
/// let who = SelectMenuBuilder::mentionable("notify").max_values(2);
/// ```
///
/// A user select has no `default_role`, a role select no `add_option`, a
/// string select no `channel_types`: a field the kind cannot carry does not
/// compile, so nothing is dropped between the builder and the wire.
///
/// ```text
/// [Decision Log]
/// - 목적과 의도: 선택 메뉴 종류가 담을 수 없는 값(사용자 선택의 역할 기본값, 문자열 외 종류의
///   options, 채널 외 종류의 channel_types)이 조용히 사라지지 않게 한다.
/// - 기존 구현 및 제약 조건: 2.x SelectMenuBuilder 하나가 다섯 종류를 만들고 build()가 그런
///   필드를 버렸다. 5단계 A 첫 구현은 validate()로만 보고해, 부모의 try_build()는 이미 버려진 뒤라
///   통과했다(astra 리뷰 P2). 2.x 호출 모양(SelectMenuBuilder::string(..).add_option(..))은 유지한다.
///   C의 모달 레이블도 선택 메뉴를 받는다.
/// - 검토한 주요 대안: (1) build()를 Result로 바꾸고 TryFrom 변환(ButtonBuilder 방식), (2) 종류별
///   빌더로 나눠 표현 자체를 막기, (3) 빌더에 오류를 쌓아 두고 부모 검증에서 꺼내기.
/// - 선택한 방식: (2). SelectMenuBuilder는 생성자 모음이고 string/user/role/mentionable/channel이
///   StringSelectBuilder 등 종류별 빌더를 돌려준다.
/// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 add_action_row(select)와 모달 레이블 같은 무실패
///   경로마다 `?`를 요구하고, 잘못된 값이 여전히 실행 시점에만 드러난다. (3)은 타입 모델로
///   바뀌는 순간 오류 정보가 사라져 같은 구멍이 남는다. (2)는 유효한 2.x 체인은 그대로 컴파일되고
///   잘못된 체인은 컴파일되지 않는다(tests/ui/select_menu_wrong_kind_fields.rs).
/// - 장점, 단점 및 영향: build()가 무손실·무실패다. 대신 SelectMenuBuilder를 타입으로 쓰던 코드
///   (매개변수·반환 타입)와 범용 default_value(SelectDefaultValue)/default_values(Vec<..>) 호출은
///   종류별 타입과 타입 있는 id(default_user 등, default_values(ids))로 옮겨야 한다.
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct SelectMenuBuilder;

impl SelectMenuBuilder {
    /// A string select (type 3) of developer-defined options.
    pub fn string(custom_id: impl Into<String>) -> StringSelectBuilder {
        StringSelectBuilder::new(custom_id)
    }

    /// A user select (type 5).
    pub fn user(custom_id: impl Into<String>) -> UserSelectBuilder {
        UserSelectBuilder::new(custom_id)
    }

    /// A role select (type 6).
    pub fn role(custom_id: impl Into<String>) -> RoleSelectBuilder {
        RoleSelectBuilder::new(custom_id)
    }

    /// A mentionable (user and role) select (type 7).
    pub fn mentionable(custom_id: impl Into<String>) -> MentionableSelectBuilder {
        MentionableSelectBuilder::new(custom_id)
    }

    /// A channel select (type 8).
    pub fn channel(custom_id: impl Into<String>) -> ChannelSelectBuilder {
        ChannelSelectBuilder::new(custom_id)
    }
}

/// One select builder per kind: the shared setters, `build()` /
/// `validate()` / `try_build()`, and the conversions into every position a
/// select menu can take.
macro_rules! select_builder {
    ($(#[$meta:meta])* $builder:ident => $model:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug)]
        pub struct $builder {
            inner: $model,
        }

        impl $builder {
            /// A select menu with this `custom_id`.
            pub fn new(custom_id: impl Into<String>) -> Self {
                Self {
                    inner: $model::new(custom_id),
                }
            }

            /// Sets the component `id`.
            pub fn id(mut self, id: ComponentId) -> Self {
                self.inner.id = Some(id);
                self
            }

            /// Sets the placeholder (up to 150 characters).
            pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
                self.inner.placeholder = Some(placeholder.into());
                self
            }

            /// Sets the fewest items to choose (0-25; Discord's default: 1).
            pub fn min_values(mut self, min: u8) -> Self {
                self.inner.min_values = Some(min);
                self
            }

            /// Sets the most items to choose (1-25; Discord's default: 1).
            pub fn max_values(mut self, max: u8) -> Self {
                self.inner.max_values = Some(max);
                self
            }

            /// Whether an answer is required (modals only; ignored in
            /// messages).
            pub fn required(mut self, required: bool) -> Self {
                self.inner.required = Some(required);
                self
            }

            /// Shows the menu disabled (messages only; modals reject it).
            pub fn disabled(mut self, disabled: bool) -> Self {
                self.inner.disabled = Some(disabled);
                self
            }

            /// The select menu, without checking limits.
            pub fn build(self) -> $model {
                self.inner
            }

            /// Checks the select menu against Discord's limits.
            pub fn validate(&self) -> Result<(), DiscordError> {
                self.inner.validate()
            }

            /// [`Self::build`] after [`Self::validate`].
            pub fn try_build(self) -> Result<$model, DiscordError> {
                self.validate()?;
                Ok(self.inner)
            }
        }

        impl From<$builder> for $model {
            fn from(builder: $builder) -> Self {
                builder.build()
            }
        }

        impl From<$builder> for SelectMenu {
            fn from(builder: $builder) -> Self {
                builder.build().into()
            }
        }

        /// The select menu on its own row.
        impl From<$builder> for ActionRow {
            fn from(builder: $builder) -> Self {
                builder.build().into()
            }
        }

        /// The select menu on its own row.
        impl From<$builder> for Component {
            fn from(builder: $builder) -> Self {
                Self::ActionRow(builder.build().into())
            }
        }
    };
}

select_builder! {
    /// Builds a [`StringSelect`] (type 3): 1-25 developer-defined options.
    StringSelectBuilder => StringSelect
}

impl StringSelectBuilder {
    /// Appends an option.
    pub fn add_option(mut self, option: SelectOption) -> Self {
        self.inner.options.push(option);
        self
    }

    /// Appends options.
    pub fn add_options(mut self, options: Vec<SelectOption>) -> Self {
        self.inner.options.extend(options);
        self
    }
}

select_builder! {
    /// Builds a [`UserSelect`] (type 5).
    UserSelectBuilder => UserSelect
}

impl UserSelectBuilder {
    /// Pre-selects a user.
    pub fn default_user(mut self, id: UserId) -> Self {
        self.inner.default_values.push(id);
        self
    }

    /// Pre-selects these users.
    pub fn default_values(mut self, ids: impl IntoIterator<Item = UserId>) -> Self {
        self.inner.default_values.extend(ids);
        self
    }
}

select_builder! {
    /// Builds a [`RoleSelect`] (type 6).
    RoleSelectBuilder => RoleSelect
}

impl RoleSelectBuilder {
    /// Pre-selects a role.
    pub fn default_role(mut self, id: RoleId) -> Self {
        self.inner.default_values.push(id);
        self
    }

    /// Pre-selects these roles.
    pub fn default_values(mut self, ids: impl IntoIterator<Item = RoleId>) -> Self {
        self.inner.default_values.extend(ids);
        self
    }
}

select_builder! {
    /// Builds a [`MentionableSelect`] (type 7): users and roles.
    MentionableSelectBuilder => MentionableSelect
}

impl MentionableSelectBuilder {
    /// Pre-selects a user.
    pub fn default_user(self, id: UserId) -> Self {
        self.default_mentionable(MentionableId::User(id))
    }

    /// Pre-selects a role.
    pub fn default_role(self, id: RoleId) -> Self {
        self.default_mentionable(MentionableId::Role(id))
    }

    /// Pre-selects a user or role.
    pub fn default_mentionable(mut self, id: MentionableId) -> Self {
        self.inner.default_values.push(id);
        self
    }

    /// Pre-selects these users and roles (`UserId`, `RoleId`, or
    /// `MentionableId`).
    pub fn default_values<I>(mut self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<MentionableId>,
    {
        self.inner
            .default_values
            .extend(ids.into_iter().map(Into::into));
        self
    }
}

select_builder! {
    /// Builds a [`ChannelSelect`] (type 8).
    ChannelSelectBuilder => ChannelSelect
}

impl ChannelSelectBuilder {
    /// Pre-selects a channel.
    pub fn default_channel(mut self, id: ChannelId) -> Self {
        self.inner.default_values.push(id);
        self
    }

    /// Pre-selects these channels.
    pub fn default_values(mut self, ids: impl IntoIterator<Item = ChannelId>) -> Self {
        self.inner.default_values.extend(ids);
        self
    }

    /// Restricts the menu to these channel types.
    pub fn channel_types(mut self, channel_types: impl IntoIterator<Item = ChannelType>) -> Self {
        self.inner.channel_types = Some(channel_types.into_iter().collect());
        self
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
        LinkButtonBuilder, PremiumButtonBuilder, SelectMenuBuilder,
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
    fn split_builders_cover_every_setter() {
        use crate::request::{ActionRow, Component, SectionAccessory};

        let custom = CustomButtonBuilder::new("c")
            .emoji_unicode("✅")
            .disabled(false)
            .id(ComponentId::new(2));
        custom.validate().expect("emoji is enough");
        assert_eq!(
            to_json(&custom.clone().try_build().unwrap()),
            json!({"type": 2, "id": 2, "style": 1, "emoji": {"name": "✅"}, "custom_id": "c", "disabled": false})
        );
        let link = LinkButtonBuilder::new("https://e.co")
            .label("L")
            .emoji(Emoji::unicode("🔗"))
            .disabled(true)
            .id(ComponentId::new(3));
        assert_eq!(link.clone().try_build().unwrap(), link.clone().build());
        let premium = PremiumButtonBuilder::new(test_id(5)).disabled(false);
        premium.validate().expect("no limits");
        assert_eq!(
            premium.clone().try_build().unwrap(),
            premium.clone().build()
        );
        for accessory in [
            SectionAccessory::from(custom.clone()),
            SectionAccessory::from(link.clone()),
            SectionAccessory::from(premium.clone()),
        ] {
            assert!(matches!(accessory, SectionAccessory::Button(_)));
        }
        assert!(matches!(Button::from(premium), Button::Premium(_)));
        let row = ActionRowBuilder::new().add_button(custom).add_button(link);
        assert!(matches!(
            ActionRow::from(row.clone()),
            ActionRow::Buttons(_)
        ));
        assert!(matches!(Component::from(row), Component::ActionRow(_)));
    }

    #[test]
    fn deprecated_builder_covers_every_setter() {
        let builder = ButtonBuilder::default()
            .label("Go")
            .emoji(Emoji::unicode("👍"))
            .emoji_unicode("👋")
            .disabled(true)
            .custom_id("go");
        builder.validate().expect("valid custom button");
        assert_eq!(
            to_json(&builder.try_build().unwrap()),
            json!({"type": 2, "style": 1, "label": "Go", "emoji": {"name": "👋"}, "custom_id": "go", "disabled": true})
        );
        let link = ButtonBuilder::new()
            .label("Docs")
            .style(ButtonStyle::SECONDARY)
            .url("https://e.co");
        assert!(matches!(link.try_build().unwrap(), Button::Link(_)));
        assert!(ButtonBuilder::new()
            .custom_id("x")
            .validate()
            .unwrap_err()
            .to_string()
            .contains("label or emoji"));
        assert!(ButtonBuilder::new()
            .label("x")
            .try_build()
            .unwrap_err()
            .to_string()
            .contains("custom_id is required"));
    }

    #[test]
    fn select_builders_convert_into_every_position() {
        use crate::request::{ActionRow, Component};

        let role = SelectMenuBuilder::role("r")
            .default_role(test_id(1))
            .default_values([test_id(2)])
            .max_values(2)
            .required(true);
        assert_eq!(
            to_json(&role.clone().build()),
            json!({"type": 6, "custom_id": "r", "max_values": 2, "required": true,
                   "default_values": [{"id": "1", "type": "role"}, {"id": "2", "type": "role"}]})
        );
        assert!(matches!(
            SelectMenu::from(role.clone()),
            SelectMenu::Role(_)
        ));
        assert!(matches!(
            ActionRow::from(role.clone()),
            ActionRow::Select(_)
        ));
        assert!(matches!(
            Component::from(role.clone()),
            Component::ActionRow(ActionRow::Select(_))
        ));
        assert_eq!(crate::request::RoleSelect::from(role.clone()), role.build());
        assert!(ComponentsV2Message::default().build().is_empty());
    }

    #[test]
    fn select_builders_carry_their_kind_fields_through_parents() {
        use crate::builders::MessageBuilder;

        let string = SelectMenuBuilder::string("pick").add_options(vec![
            SelectOption::new("One", "one"),
            SelectOption::new("Two", "two"),
        ]);
        assert_eq!(
            to_json(&string.clone().build()),
            json!({"type": 3, "custom_id": "pick", "options": [
                {"label": "One", "value": "one"}, {"label": "Two", "value": "two"}
            ]})
        );
        let channel = SelectMenuBuilder::channel("where")
            .channel_types([ChannelType::GUILD_TEXT, ChannelType::GUILD_VOICE])
            .default_channel(test_id(9))
            .default_values([test_id(10)])
            .max_values(2);
        assert_eq!(
            to_json(&channel.clone().build()),
            json!({"type": 8, "custom_id": "where", "max_values": 2, "channel_types": [0, 2],
                   "default_values": [{"id": "9", "type": "channel"}, {"id": "10", "type": "channel"}]})
        );
        let mentionable = SelectMenuBuilder::mentionable("notify")
            .placeholder("Pick a target")
            .default_user(test_id(42))
            .default_role(test_id(43))
            .default_mentionable(crate::model::MentionableId::User(test_id(44)))
            .default_values([test_id::<crate::model::marker::Role>(45)])
            .required(false)
            .min_values(1)
            .max_values(4)
            .disabled(true)
            .id(ComponentId::new(3));
        assert_eq!(
            to_json(&mentionable.clone().build()),
            json!({
                "type": 7, "id": 3, "custom_id": "notify", "placeholder": "Pick a target",
                "min_values": 1, "max_values": 4, "required": false, "disabled": true,
                "default_values": [
                    {"id": "42", "type": "user"}, {"id": "43", "type": "role"},
                    {"id": "44", "type": "user"}, {"id": "45", "type": "role"}
                ]
            })
        );
        let user = SelectMenuBuilder::user("who")
            .default_user(test_id(900))
            .default_values([test_id(901)])
            .max_values(2);

        // Every kind keeps its defaults through a parent's try_build: the
        // message serializes exactly what the builders were given.
        let message = MessageBuilder::new()
            .add_component(string)
            .add_component(channel)
            .add_component(mentionable)
            .add_component(user)
            .try_build()
            .expect("valid legacy message");
        let value = to_json(&message);
        let defaults =
            |row: usize| value["components"][row]["components"][0]["default_values"].clone();
        assert_eq!(defaults(1).as_array().map(Vec::len), Some(2));
        assert_eq!(defaults(2).as_array().map(Vec::len), Some(4));
        assert_eq!(
            defaults(3),
            json!([{"id": "900", "type": "user"}, {"id": "901", "type": "user"}])
        );

        // Too many defaults for the bounds fails the parent too, with the
        // path of the select.
        let crowded = MessageBuilder::new().add_component(
            SelectMenuBuilder::user("u")
                .default_user(test_id(1))
                .default_user(test_id(2)),
        );
        expect_model_error(
            crowded.try_build(),
            "components[0].components[0]: select menu default_values must contain 1-1 items",
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
