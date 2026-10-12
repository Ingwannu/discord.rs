//! Outgoing message components (3.0 phase 5, design §4).
//!
//! Every component a message can carry is a typed value here, built by the
//! [`crate::builders`] and serialized straight to JSON bytes: no
//! `serde_json::Value` is assembled on the way. The received side is
//! [`crate::model::MessageComponent`]; the two are separate because a sent
//! component has rules a received one does not (a link button has no
//! `custom_id`, a premium button has only a `sku_id`, a row holds buttons or
//! one select menu, never both).
//!
//! Known numeric tags (`type`, `style`) are written as numbers, `type` first,
//! by hand-written `Serialize` impls (the same layout as
//! [`super::ComponentEmbed`]); nothing relies on a string-tagged serde enum.
//! Anything this version cannot express goes through [`RawComponent`], which
//! is visible in every position that accepts it and is written verbatim.
//!
//! Validation (`validate()`) checks Discord's documented limits from
//! `builders/limits.rs`; the message types in [`super`] run it recursively
//! together with the message-wide rules (40 components, unique `id`s and
//! `custom_id`s, Components V2 only with `IS_COMPONENTS_V2`).
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 메시지 컴포넌트를 Value 없이 타입으로 들고, 규격이 금지하는 조합(링크 버튼의
//!   custom_id, 프리미엄 버튼의 label/url, 버튼·선택 혼합 행)을 컴파일 단계에서 막는다.
//! - 기존 구현 및 제약 조건: 2.x 빌더는 Vec<Value>를 들고 build()마다 to_value를 불렀다
//!   (create_message 성능 시나리오 313 allocs / 46,567 B). 범용 ButtonBuilder는 style로
//!   custom/link/premium을 구분해 잘못된 조합을 담았다. 숫자 type은 숫자로 나가야 하고(§4),
//!   Component Embed 생성기가 같은 방식(수동 Serialize, type 먼저)을 이미 쓴다.
//! - 검토한 주요 대안: (1) 받는 쪽 MessageComponent(평면 구조체 + Option 필드)를 보내는 데도
//!   사용, (2) serde 내부 태그 enum, (3) 위치별 데이터 enum + 수동 Serialize, (4) 빌더
//!   typestate로만 막고 모델은 Value 유지.
//! - 선택한 방식: (3). 최상위 Component, 컨테이너 자식 ContainerComponent, 행 ActionRow =
//!   ButtonRow | SelectRow, 버튼 Custom/Link/Premium, 선택 메뉴 다섯 구조체 + SelectMenu enum,
//!   섹션 액세서리 Button | Thumbnail, 각 위치의 Raw(RawValue).
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 모든 필드가 Option이라 잘못된 조합이 계속
//!   표현된다. (2)는 type이 문자열 태그가 되거나 숫자 태그를 위해 결국 수동 구현이 필요하다.
//!   (4)는 Value 트리를 남겨 할당 목표(≤64 allocs)를 맞출 수 없다. Raw는 Box<RawValue>라
//!   Value 트리 없이 그대로 쓴다.
//! - 장점, 단점 및 영향: 금지 조합은 컴파일되지 않는다(tests/ui). enum이 커서(가장 큰
//!   variant 기준) Vec 버퍼 바이트가 늘지만 Value 노드보다 작다. 새 컴포넌트·필드는
//!   #[non_exhaustive]로 추가할 수 있고, 그 전에는 Raw로 보낸다. 선택 메뉴의 required는
//!   모달 전용(메시지에서는 무시), disabled는 메시지 전용(모달에서는 오류)이라 모달 쪽 검증이
//!   disabled를 거부한다.
//! ```

use std::collections::HashSet;
use std::fmt;

use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::Serialize;
use serde_json::value::RawValue;
use serde_json::Value;

use crate::builders::limits;
use crate::command::validation::char_count;
use crate::error::DiscordError;
use crate::model::discriminant::{ButtonStyle, ChannelType, ComponentType, SeparatorSpacing};
use crate::model::{
    ChannelId, ComponentId, MentionableId, Nullable, Patch, RoleId, SelectDefaultValue, SkuId,
    UserId,
};
use crate::types::{Emoji, MediaGalleryItem, MediaInfo, SelectOption};

// ---------------------------------------------------------------------------
// Top level
// ---------------------------------------------------------------------------

/// A top-level component of a message.
///
/// Without the `IS_COMPONENTS_V2` message flag only [`Component::ActionRow`]
/// (and [`Component::Raw`]) may appear, at most 5 of them. With the flag every
/// variant may, up to 40 components counting nested ones.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Component {
    /// Action Row (type 1): buttons or one select menu.
    ActionRow(ActionRow),
    /// Section (type 9): text with a button or thumbnail beside it.
    Section(Section),
    /// Text Display (type 10).
    TextDisplay(TextDisplay),
    /// Media Gallery (type 12).
    MediaGallery(MediaGallery),
    /// File (type 13): an uploaded attachment shown as a file.
    File(FileComponent),
    /// Separator (type 14).
    Separator(Separator),
    /// Container (type 17).
    Container(Container),
    /// A component this version cannot express, written verbatim. Counts as
    /// one component in validation; its content is not checked.
    Raw(RawComponent),
}

impl Component {
    /// Whether this component needs the `IS_COMPONENTS_V2` message flag.
    /// Action rows and raw components work either way.
    pub fn requires_components_v2(&self) -> bool {
        !matches!(self, Self::ActionRow(_) | Self::Raw(_))
    }

    /// Checks this component and its children against Discord's limits.
    /// Message-wide rules (total count, unique ids) are checked by the
    /// message's own `validate()`.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    pub(crate) fn check<'a>(
        &'a self,
        path: &str,
        tally: &mut Tally<'a>,
    ) -> Result<(), DiscordError> {
        match self {
            Self::ActionRow(row) => row.check(path, tally),
            Self::Section(section) => section.check(path, tally),
            Self::TextDisplay(text) => text.check(path, tally),
            Self::MediaGallery(gallery) => gallery.check(path, tally),
            Self::File(file) => file.check(path, tally),
            Self::Separator(separator) => separator.check(path, tally),
            Self::Container(container) => container.check(path, tally),
            Self::Raw(_) => {
                tally.count();
                Ok(())
            }
        }
    }
}

impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ActionRow(row) => row.serialize(serializer),
            Self::Section(section) => section.serialize(serializer),
            Self::TextDisplay(text) => text.serialize(serializer),
            Self::MediaGallery(gallery) => gallery.serialize(serializer),
            Self::File(file) => file.serialize(serializer),
            Self::Separator(separator) => separator.serialize(serializer),
            Self::Container(container) => container.serialize(serializer),
            Self::Raw(raw) => raw.serialize(serializer),
        }
    }
}

/// A child of a [`Container`]: every top-level kind except another
/// container.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ContainerComponent {
    /// Action Row (type 1).
    ActionRow(ActionRow),
    /// Section (type 9).
    Section(Section),
    /// Text Display (type 10).
    TextDisplay(TextDisplay),
    /// Media Gallery (type 12).
    MediaGallery(MediaGallery),
    /// File (type 13).
    File(FileComponent),
    /// Separator (type 14).
    Separator(Separator),
    /// A child this version cannot express, written verbatim.
    Raw(RawComponent),
}

impl ContainerComponent {
    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        match self {
            Self::ActionRow(row) => row.check(path, tally),
            Self::Section(section) => section.check(path, tally),
            Self::TextDisplay(text) => text.check(path, tally),
            Self::MediaGallery(gallery) => gallery.check(path, tally),
            Self::File(file) => file.check(path, tally),
            Self::Separator(separator) => separator.check(path, tally),
            Self::Raw(_) => {
                tally.count();
                Ok(())
            }
        }
    }
}

impl Serialize for ContainerComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ActionRow(row) => row.serialize(serializer),
            Self::Section(section) => section.serialize(serializer),
            Self::TextDisplay(text) => text.serialize(serializer),
            Self::MediaGallery(gallery) => gallery.serialize(serializer),
            Self::File(file) => file.serialize(serializer),
            Self::Separator(separator) => separator.serialize(serializer),
            Self::Raw(raw) => raw.serialize(serializer),
        }
    }
}

/// Lossless: every container child is also a valid top-level component.
impl From<ContainerComponent> for Component {
    fn from(component: ContainerComponent) -> Self {
        match component {
            ContainerComponent::ActionRow(row) => Self::ActionRow(row),
            ContainerComponent::Section(section) => Self::Section(section),
            ContainerComponent::TextDisplay(text) => Self::TextDisplay(text),
            ContainerComponent::MediaGallery(gallery) => Self::MediaGallery(gallery),
            ContainerComponent::File(file) => Self::File(file),
            ContainerComponent::Separator(separator) => Self::Separator(separator),
            ContainerComponent::Raw(raw) => Self::Raw(raw),
        }
    }
}

/// Fails for [`Component::Container`] (containers do not nest) and returns
/// it unchanged in the error.
impl TryFrom<Component> for ContainerComponent {
    type Error = Component;

    fn try_from(component: Component) -> Result<Self, Self::Error> {
        Ok(match component {
            Component::ActionRow(row) => Self::ActionRow(row),
            Component::Section(section) => Self::Section(section),
            Component::TextDisplay(text) => Self::TextDisplay(text),
            Component::MediaGallery(gallery) => Self::MediaGallery(gallery),
            Component::File(file) => Self::File(file),
            Component::Separator(separator) => Self::Separator(separator),
            Component::Raw(raw) => Self::Raw(raw),
            container @ Component::Container(_) => return Err(container),
        })
    }
}

/// `From` impls that put a typed component into both top-level and
/// container-child position.
macro_rules! component_from {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl From<$ty> for Component {
            fn from(value: $ty) -> Self {
                Self::$variant(value.into())
            }
        }
        impl From<$ty> for ContainerComponent {
            fn from(value: $ty) -> Self {
                Self::$variant(value.into())
            }
        }
    )*};
}

component_from! {
    ActionRow => ActionRow,
    ButtonRow => ActionRow,
    SelectRow => ActionRow,
    Section => Section,
    TextDisplay => TextDisplay,
    MediaGallery => MediaGallery,
    FileComponent => File,
    Separator => Separator,
    RawComponent => Raw,
}

impl From<Container> for Component {
    fn from(container: Container) -> Self {
        Self::Container(container)
    }
}

// ---------------------------------------------------------------------------
// Raw escape hatch
// ---------------------------------------------------------------------------

/// A component as JSON text, sent exactly as given.
///
/// The escape hatch for component types or fields this version does not
/// model. It holds a [`RawValue`], so sending it builds no `Value` tree and
/// the bytes go out unchanged. Only JSON syntax is checked when it is
/// created; Discord validates the rest.
#[derive(Clone)]
pub struct RawComponent(Box<RawValue>);

impl RawComponent {
    /// Wraps JSON text. Fails when `json` is not valid JSON.
    pub fn from_json(json: impl Into<String>) -> Result<Self, DiscordError> {
        Ok(Self(RawValue::from_string(json.into())?))
    }

    /// Encodes a `serde_json::Value` once, for code that already holds one.
    pub fn from_value(value: &Value) -> Result<Self, DiscordError> {
        Ok(Self(serde_json::value::to_raw_value(value)?))
    }

    /// The JSON text.
    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl fmt::Debug for RawComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RawComponent").field(&self.get()).finish()
    }
}

/// Compares the JSON text byte for byte (`{"a":1}` and `{ "a": 1 }` differ).
impl PartialEq for RawComponent {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Serialize for RawComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl TryFrom<Value> for RawComponent {
    type Error = DiscordError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::from_value(&value)
    }
}

impl TryFrom<&Value> for RawComponent {
    type Error = DiscordError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::from_value(value)
    }
}

// ---------------------------------------------------------------------------
// Action rows
// ---------------------------------------------------------------------------

/// Action Row (type 1). A row holds 1-5 buttons or exactly one select menu;
/// the two variants make a mixed row impossible to build.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ActionRow {
    /// A row of buttons.
    Buttons(ButtonRow),
    /// A row with a single select menu.
    Select(SelectRow),
}

impl ActionRow {
    /// Checks the row and its children against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        match self {
            Self::Buttons(row) => row.check(path, tally),
            Self::Select(row) => row.check(path, tally),
        }
    }
}

impl Serialize for ActionRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Buttons(row) => row.serialize(serializer),
            Self::Select(row) => row.serialize(serializer),
        }
    }
}

impl From<ButtonRow> for ActionRow {
    fn from(row: ButtonRow) -> Self {
        Self::Buttons(row)
    }
}

impl From<SelectRow> for ActionRow {
    fn from(row: SelectRow) -> Self {
        Self::Select(row)
    }
}

/// A select menu on its own row.
impl From<SelectMenu> for ActionRow {
    fn from(select: SelectMenu) -> Self {
        Self::Select(SelectRow::new(select))
    }
}

/// An Action Row of 1-5 buttons.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ButtonRow {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The buttons, left to right (serialized as `components`).
    pub buttons: Vec<Button>,
}

impl ButtonRow {
    /// An empty row; add 1-5 buttons.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a button.
    pub fn add_button(mut self, button: impl Into<Button>) -> Self {
        self.buttons.push(button.into());
        self
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        check_count(
            path,
            "action row components",
            self.buttons.len(),
            1,
            limits::MAX_ACTION_ROW_COMPONENTS,
        )?;
        for (index, button) in self.buttons.iter().enumerate() {
            button.check(&child_path(path, "components", index), tally)?;
        }
        Ok(())
    }
}

impl Serialize for ButtonRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::ACTION_ROW)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", &self.buttons)?;
        map.end()
    }
}

/// An Action Row holding one select menu.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelectRow {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The row's only component (serialized as a one-element `components`).
    pub select: SelectMenu,
}

impl SelectRow {
    /// A row with this select menu.
    pub fn new(select: impl Into<SelectMenu>) -> Self {
        Self {
            id: None,
            select: select.into(),
        }
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        self.select.check(&child_path(path, "components", 0), tally)
    }
}

impl Serialize for SelectRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::ACTION_ROW)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", std::slice::from_ref(&self.select))?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Button (type 2), split by what Discord allows each kind to carry.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Button {
    /// Primary, secondary, success, or danger button: sends an interaction
    /// with its `custom_id`.
    Custom(CustomButton),
    /// Link button (style 5): opens a URL, sends no interaction.
    Link(LinkButton),
    /// Premium button (style 6): opens the purchase flow for an SKU.
    Premium(PremiumButton),
}

impl Button {
    /// Checks the button against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        match self {
            Self::Custom(button) => button.check(path, tally),
            Self::Link(button) => button.check(path, tally),
            Self::Premium(button) => tally.component(button.id, path),
        }
    }
}

impl Serialize for Button {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Custom(button) => button.serialize(serializer),
            Self::Link(button) => button.serialize(serializer),
            Self::Premium(button) => button.serialize(serializer),
        }
    }
}

impl From<CustomButton> for Button {
    fn from(button: CustomButton) -> Self {
        Self::Custom(button)
    }
}

impl From<LinkButton> for Button {
    fn from(button: LinkButton) -> Self {
        Self::Link(button)
    }
}

impl From<PremiumButton> for Button {
    fn from(button: PremiumButton) -> Self {
        Self::Premium(button)
    }
}

/// A button that sends an interaction: styles 1-4 (primary, secondary,
/// success, danger). It has no `url` or `sku_id`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CustomButton {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// Button style. [`ButtonStyle::LINK`] and [`ButtonStyle::PREMIUM`]
    /// belong to [`LinkButton`] and [`PremiumButton`] and fail validation
    /// here; unknown styles pass.
    pub style: ButtonStyle,
    /// Identifier sent back in the interaction, 1-100 characters, unique
    /// within the message.
    pub custom_id: String,
    /// Button text, up to 80 characters. A button needs a label, an emoji,
    /// or both.
    pub label: Option<String>,
    /// Emoji shown on the button.
    pub emoji: Option<Emoji>,
    /// Whether the button is shown disabled.
    pub disabled: Option<bool>,
}

impl CustomButton {
    /// A button with this style and `custom_id`; give it a label, an emoji,
    /// or both.
    pub fn new(style: ButtonStyle, custom_id: impl Into<String>) -> Self {
        Self {
            id: None,
            style,
            custom_id: custom_id.into(),
            label: None,
            emoji: None,
            disabled: None,
        }
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        if matches!(self.style, ButtonStyle::LINK | ButtonStyle::PREMIUM) {
            return Err(at(
                path,
                format!(
                    "button style {} needs a url or sku_id; use LinkButton or PremiumButton",
                    self.style.get()
                ),
            ));
        }
        check_len(
            path,
            "button custom_id",
            &self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        tally.custom_id(&self.custom_id, path)?;
        check_label_or_emoji(path, self.label.as_deref(), self.emoji.as_ref())
    }
}

impl Serialize for CustomButton {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::BUTTON)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("style", &self.style)?;
        entry_some(&mut map, "label", &self.label)?;
        entry_some(&mut map, "emoji", &self.emoji)?;
        map.serialize_entry("custom_id", &self.custom_id)?;
        entry_some(&mut map, "disabled", &self.disabled)?;
        map.end()
    }
}

/// Link button (style 5): opens `url`. It has no `custom_id` and sends no
/// interaction.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LinkButton {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// Target URL, up to 512 characters.
    pub url: String,
    /// Button text, up to 80 characters. A button needs a label, an emoji,
    /// or both.
    pub label: Option<String>,
    /// Emoji shown on the button.
    pub emoji: Option<Emoji>,
    /// Whether the button is shown disabled.
    pub disabled: Option<bool>,
}

impl LinkButton {
    /// A link button to `url`; give it a label, an emoji, or both.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            url: url.into(),
            label: None,
            emoji: None,
            disabled: None,
        }
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        check_len(
            path,
            "button url",
            &self.url,
            1,
            limits::MAX_BUTTON_URL_LENGTH,
        )?;
        check_label_or_emoji(path, self.label.as_deref(), self.emoji.as_ref())
    }
}

impl Serialize for LinkButton {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::BUTTON)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("style", &ButtonStyle::LINK)?;
        entry_some(&mut map, "label", &self.label)?;
        entry_some(&mut map, "emoji", &self.emoji)?;
        map.serialize_entry("url", &self.url)?;
        entry_some(&mut map, "disabled", &self.disabled)?;
        map.end()
    }
}

/// Premium button (style 6): opens the purchase flow for `sku_id`. Discord
/// draws its label, emoji, and price, so it has none of those fields, and no
/// `custom_id` or `url`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PremiumButton {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The purchasable SKU.
    pub sku_id: SkuId,
    /// Whether the button is shown disabled.
    pub disabled: Option<bool>,
}

impl PremiumButton {
    /// A purchase button for `sku_id`.
    pub fn new(sku_id: SkuId) -> Self {
        Self {
            id: None,
            sku_id,
            disabled: None,
        }
    }
}

impl Serialize for PremiumButton {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::BUTTON)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("style", &ButtonStyle::PREMIUM)?;
        map.serialize_entry("sku_id", &self.sku_id)?;
        entry_some(&mut map, "disabled", &self.disabled)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// Select menus
// ---------------------------------------------------------------------------

/// One of the five select menus, for a message [`SelectRow`]. Modal labels
/// hold the five structs directly.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SelectMenu {
    /// String Select (type 3).
    String(StringSelect),
    /// User Select (type 5).
    User(UserSelect),
    /// Role Select (type 6).
    Role(RoleSelect),
    /// Mentionable Select (type 7).
    Mentionable(MentionableSelect),
    /// Channel Select (type 8).
    Channel(ChannelSelect),
}

impl SelectMenu {
    /// Checks the select menu against Discord's limits. `required` is not
    /// checked: Discord ignores it in messages.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        let common = self.common();
        tally.component(common.id, path)?;
        tally.custom_id(common.custom_id, path)?;
        match self {
            Self::String(select) => select.check_fields(path),
            Self::User(select) => select.check_fields(path),
            Self::Role(select) => select.check_fields(path),
            Self::Mentionable(select) => select.check_fields(path),
            Self::Channel(select) => select.check_fields(path),
        }
    }

    fn common(&self) -> CommonRef<'_> {
        match self {
            Self::String(select) => select.common(),
            Self::User(select) => select.common(),
            Self::Role(select) => select.common(),
            Self::Mentionable(select) => select.common(),
            Self::Channel(select) => select.common(),
        }
    }
}

impl Serialize for SelectMenu {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::String(select) => select.serialize(serializer),
            Self::User(select) => select.serialize(serializer),
            Self::Role(select) => select.serialize(serializer),
            Self::Mentionable(select) => select.serialize(serializer),
            Self::Channel(select) => select.serialize(serializer),
        }
    }
}

/// The fields all five select menus share, borrowed for validation.
struct CommonRef<'a> {
    id: Option<ComponentId>,
    custom_id: &'a str,
    placeholder: Option<&'a str>,
    min_values: Option<u8>,
    max_values: Option<u8>,
}

impl CommonRef<'_> {
    /// The context-free limits every select menu shares. `defaults` is the
    /// number of `default_values` (auto-populated selects only).
    fn check(&self, path: &str, defaults: Option<usize>) -> Result<(), DiscordError> {
        check_len(
            path,
            "select menu custom_id",
            self.custom_id,
            1,
            limits::MAX_CUSTOM_ID_LENGTH,
        )?;
        if let Some(placeholder) = self.placeholder {
            check_max_len(
                path,
                "select menu placeholder",
                placeholder,
                limits::MAX_SELECT_PLACEHOLDER_LENGTH,
            )?;
        }
        if let Some(min) = self.min_values {
            if min > limits::MAX_SELECT_VALUES {
                return Err(at(
                    path,
                    format!(
                        "select menu min_values must be at most {}, got {min}",
                        limits::MAX_SELECT_VALUES
                    ),
                ));
            }
        }
        if let Some(max) = self.max_values {
            if !(1..=limits::MAX_SELECT_VALUES).contains(&max) {
                return Err(at(
                    path,
                    format!(
                        "select menu max_values must be 1-{}, got {max}",
                        limits::MAX_SELECT_VALUES
                    ),
                ));
            }
        }
        // An omitted bound is Discord's default (1), so `min_values: 2`
        // alone is already out of range.
        let min = self.min_values.unwrap_or(limits::DEFAULT_SELECT_VALUES);
        let max = self.max_values.unwrap_or(limits::DEFAULT_SELECT_VALUES);
        if min > max {
            let defaulted = |value: Option<u8>| {
                if value.is_none() {
                    ", the default when omitted"
                } else {
                    ""
                }
            };
            return Err(at(
                path,
                format!(
                    "select menu min_values ({min}{}) must not exceed max_values ({max}{})",
                    defaulted(self.min_values),
                    defaulted(self.max_values),
                ),
            ));
        }
        // "number of default values must be in the range defined by
        // `min_values` and `max_values`" (both default to 1).
        if let Some(count) = defaults.filter(|count| *count > 0) {
            let min = usize::from(min);
            let max = usize::from(max);
            if count < min || count > max {
                return Err(at(
                    path,
                    format!(
                        "select menu default_values must contain {min}-{max} items \
                         (min_values-max_values), got {count}"
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// Defines one select menu struct: the shared fields, `new`, `validate`,
/// and a `Serialize` impl that writes `type` first, then the shared fields,
/// then the per-kind ones.
macro_rules! select_menu {
    (
        $(#[$meta:meta])*
        $name:ident, $variant:ident, $kind:expr;
        $( $(#[$field_meta:meta])* $field:ident : $field_ty:ty = $field_init:expr ),* $(,)?
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        #[non_exhaustive]
        pub struct $name {
            /// Optional component identifier, unique within the message.
            pub id: Option<ComponentId>,
            /// Identifier sent back in the interaction, 1-100 characters.
            pub custom_id: String,
            /// Text shown when nothing is selected, up to 150 characters.
            pub placeholder: Option<String>,
            /// Fewest items to choose, 0-25 (Discord's default: 1).
            pub min_values: Option<u8>,
            /// Most items to choose, 1-25 (Discord's default: 1).
            pub max_values: Option<u8>,
            /// Whether an answer is required. Modal only: Discord ignores it
            /// in messages and defaults it to `true` in modals.
            pub required: Option<bool>,
            /// Whether the menu is shown disabled. Message only: Discord
            /// rejects a modal with a disabled component.
            pub disabled: Option<bool>,
            $( $(#[$field_meta])* pub $field: $field_ty, )*
        }

        impl $name {
            /// A select menu with this `custom_id`.
            pub fn new(custom_id: impl Into<String>) -> Self {
                Self {
                    id: None,
                    custom_id: custom_id.into(),
                    placeholder: None,
                    min_values: None,
                    max_values: None,
                    required: None,
                    disabled: None,
                    $( $field: $field_init, )*
                }
            }

            /// Checks the context-free limits (custom_id, placeholder,
            /// options, min/max values, default count). Neither `required`
            /// nor `disabled` is checked here: whether each is allowed
            /// depends on whether the menu is in a message or a modal.
            pub fn validate(&self) -> Result<(), DiscordError> {
                self.check_fields("")
            }

            fn common(&self) -> CommonRef<'_> {
                CommonRef {
                    id: self.id,
                    custom_id: &self.custom_id,
                    placeholder: self.placeholder.as_deref(),
                    min_values: self.min_values,
                    max_values: self.max_values,
                }
            }

            fn serialize_common<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
                map.serialize_entry("type", &$kind)?;
                entry_some(map, "id", &self.id)?;
                map.serialize_entry("custom_id", &self.custom_id)?;
                entry_some(map, "placeholder", &self.placeholder)?;
                entry_some(map, "min_values", &self.min_values)?;
                entry_some(map, "max_values", &self.max_values)?;
                entry_some(map, "required", &self.required)?;
                entry_some(map, "disabled", &self.disabled)
            }
        }

        impl From<$name> for SelectMenu {
            fn from(select: $name) -> Self {
                Self::$variant(select)
            }
        }

        /// The select menu on its own row.
        impl From<$name> for ActionRow {
            fn from(select: $name) -> Self {
                Self::Select(SelectRow::new(select))
            }
        }
    };
}

select_menu! {
    /// String Select (type 3): 1-25 developer-defined options.
    StringSelect, String, ComponentType::STRING_SELECT;
    /// The choices, 1-25.
    options: Vec<SelectOption> = Vec::new(),
}

impl StringSelect {
    fn check_fields(&self, path: &str) -> Result<(), DiscordError> {
        self.common().check(path, None)?;
        check_count(
            path,
            "select menu options",
            self.options.len(),
            1,
            limits::MAX_SELECT_OPTIONS,
        )?;
        for option in &self.options {
            let label = &option.label;
            check_len(
                path,
                &format!("select menu option \"{label}\" label"),
                label,
                1,
                limits::MAX_SELECT_OPTION_TEXT_LENGTH,
            )?;
            check_len(
                path,
                &format!("select menu option \"{label}\" value"),
                &option.value,
                1,
                limits::MAX_SELECT_OPTION_TEXT_LENGTH,
            )?;
            if let Some(description) = &option.description {
                check_max_len(
                    path,
                    &format!("select menu option \"{label}\" description"),
                    description,
                    limits::MAX_SELECT_OPTION_TEXT_LENGTH,
                )?;
            }
        }
        Ok(())
    }
}

impl Serialize for StringSelect {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        self.serialize_common(&mut map)?;
        map.serialize_entry("options", &self.options)?;
        map.end()
    }
}

select_menu! {
    /// User Select (type 5): picks users.
    UserSelect, User, ComponentType::USER_SELECT;
    /// Users selected when the menu is shown.
    default_values: Vec<UserId> = Vec::new(),
}

select_menu! {
    /// Role Select (type 6): picks roles.
    RoleSelect, Role, ComponentType::ROLE_SELECT;
    /// Roles selected when the menu is shown.
    default_values: Vec<RoleId> = Vec::new(),
}

select_menu! {
    /// Mentionable Select (type 7): picks users and roles.
    MentionableSelect, Mentionable, ComponentType::MENTIONABLE_SELECT;
    /// Users and roles selected when the menu is shown.
    default_values: Vec<MentionableId> = Vec::new(),
}

select_menu! {
    /// Channel Select (type 8): picks channels.
    ChannelSelect, Channel, ComponentType::CHANNEL_SELECT;
    /// Channels selected when the menu is shown.
    default_values: Vec<ChannelId> = Vec::new(),
    /// Channel types to offer; `None` offers every type.
    channel_types: Option<Vec<ChannelType>> = None,
}

/// `check_fields` and `Serialize` for the auto-populated selects, which
/// differ only in the id type of `default_values`.
macro_rules! auto_select {
    ($($name:ident),*) => {$(
        impl $name {
            fn check_fields(&self, path: &str) -> Result<(), DiscordError> {
                self.common().check(path, Some(self.default_values.len()))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                self.serialize_common(&mut map)?;
                if !self.default_values.is_empty() {
                    map.serialize_entry("default_values", &DefaultValues(&self.default_values))?;
                }
                auto_select!(@extra self, map, $name);
                map.end()
            }
        }
    )*};
    (@extra $self:ident, $map:ident, ChannelSelect) => {
        entry_some(&mut $map, "channel_types", &$self.channel_types)?;
    };
    (@extra $self:ident, $map:ident, $other:ident) => {};
}

auto_select!(UserSelect, RoleSelect, MentionableSelect, ChannelSelect);

/// `default_values` written as `{"id": ..., "type": "user" | "role" |
/// "channel"}` objects, borrowing the typed ids.
struct DefaultValues<'a, T>(&'a [T]);

/// The id types an auto-populated select's `default_values` can hold.
trait DefaultValueId: Copy {
    fn to_default_value(self) -> SelectDefaultValue;
}

impl DefaultValueId for UserId {
    fn to_default_value(self) -> SelectDefaultValue {
        SelectDefaultValue::User(self)
    }
}

impl DefaultValueId for RoleId {
    fn to_default_value(self) -> SelectDefaultValue {
        SelectDefaultValue::Role(self)
    }
}

impl DefaultValueId for ChannelId {
    fn to_default_value(self) -> SelectDefaultValue {
        SelectDefaultValue::Channel(self)
    }
}

impl DefaultValueId for MentionableId {
    fn to_default_value(self) -> SelectDefaultValue {
        SelectDefaultValue::from(self)
    }
}

impl<T: DefaultValueId> Serialize for DefaultValues<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for id in self.0 {
            seq.serialize_element(&id.to_default_value())?;
        }
        seq.end()
    }
}

// ---------------------------------------------------------------------------
// Layout and content components (Components V2)
// ---------------------------------------------------------------------------

/// Section (type 9): 1-3 text displays with an accessory beside them.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The section's text, 1-3 text displays.
    pub components: Vec<TextDisplay>,
    /// The button or thumbnail shown beside the text.
    pub accessory: SectionAccessory,
}

impl Section {
    /// A section with this accessory; add 1-3 text displays.
    pub fn new(accessory: impl Into<SectionAccessory>) -> Self {
        Self {
            id: None,
            components: Vec::new(),
            accessory: accessory.into(),
        }
    }

    /// Checks the section and its children against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        check_count(
            path,
            "section components",
            self.components.len(),
            1,
            limits::MAX_SECTION_COMPONENTS,
        )?;
        for (index, text) in self.components.iter().enumerate() {
            text.check(&child_path(path, "components", index), tally)?;
        }
        let accessory_path = field_path(path, "accessory");
        match &self.accessory {
            SectionAccessory::Button(button) => button.check(&accessory_path, tally),
            SectionAccessory::Thumbnail(thumbnail) => thumbnail.check(&accessory_path, tally),
        }
    }
}

impl Serialize for Section {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::SECTION)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("components", &self.components)?;
        map.serialize_entry("accessory", &self.accessory)?;
        map.end()
    }
}

/// The accessory of a [`Section`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SectionAccessory {
    /// A button of any kind.
    Button(Button),
    /// A thumbnail image.
    Thumbnail(Thumbnail),
}

impl Serialize for SectionAccessory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Button(button) => button.serialize(serializer),
            Self::Thumbnail(thumbnail) => thumbnail.serialize(serializer),
        }
    }
}

impl From<Button> for SectionAccessory {
    fn from(button: Button) -> Self {
        Self::Button(button)
    }
}

impl From<CustomButton> for SectionAccessory {
    fn from(button: CustomButton) -> Self {
        Self::Button(button.into())
    }
}

impl From<LinkButton> for SectionAccessory {
    fn from(button: LinkButton) -> Self {
        Self::Button(button.into())
    }
}

impl From<PremiumButton> for SectionAccessory {
    fn from(button: PremiumButton) -> Self {
        Self::Button(button.into())
    }
}

impl From<Thumbnail> for SectionAccessory {
    fn from(thumbnail: Thumbnail) -> Self {
        Self::Thumbnail(thumbnail)
    }
}

/// Text Display (type 10): Markdown text. Usable at the top level of a
/// Components V2 message, in a container or section, and at the top level of
/// a modal.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextDisplay {
    /// Optional component identifier, unique within the message or modal.
    pub id: Option<ComponentId>,
    /// Markdown text, 1-4000 characters.
    pub content: String,
}

impl TextDisplay {
    /// A text display with this Markdown content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: None,
            content: content.into(),
        }
    }

    /// Checks the content length (1-4000 characters).
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_len(
            "",
            "text display content",
            &self.content,
            1,
            limits::MAX_TEXT_DISPLAY_LENGTH,
        )
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        check_len(
            path,
            "text display content",
            &self.content,
            1,
            limits::MAX_TEXT_DISPLAY_LENGTH,
        )
    }
}

impl Serialize for TextDisplay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::TEXT_DISPLAY)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("content", &self.content)?;
        map.end()
    }
}

/// Thumbnail (type 11): a small image, used as a [`Section`] accessory.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Thumbnail {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The image: an `https://` URL or `attachment://<filename>`.
    pub media: MediaInfo,
    /// Alt text, up to 1024 characters.
    pub description: Option<String>,
    /// Whether the image is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl Thumbnail {
    /// A thumbnail of the image at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            media: MediaInfo { url: url.into() },
            description: None,
            spoiler: None,
        }
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        if let Some(description) = &self.description {
            check_max_len(
                path,
                "thumbnail description",
                description,
                limits::MAX_MEDIA_DESCRIPTION_LENGTH,
            )?;
        }
        Ok(())
    }
}

impl Serialize for Thumbnail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::THUMBNAIL)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("media", &self.media)?;
        entry_some(&mut map, "description", &self.description)?;
        entry_some(&mut map, "spoiler", &self.spoiler)?;
        map.end()
    }
}

/// Media Gallery (type 12): 1-10 images or videos.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MediaGallery {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The gallery items, 1-10.
    pub items: Vec<MediaGalleryItem>,
}

impl MediaGallery {
    /// An empty gallery; add 1-10 items.
    pub fn new() -> Self {
        Self::default()
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        check_count(
            path,
            "media gallery items",
            self.items.len(),
            1,
            limits::MAX_MEDIA_GALLERY_ITEMS,
        )?;
        for (index, item) in self.items.iter().enumerate() {
            if let Some(description) = &item.description {
                check_max_len(
                    path,
                    &format!("media gallery item {index} description"),
                    description,
                    limits::MAX_MEDIA_DESCRIPTION_LENGTH,
                )?;
            }
        }
        Ok(())
    }
}

impl Serialize for MediaGallery {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::MEDIA_GALLERY)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("items", &self.items)?;
        map.end()
    }
}

/// File (type 13): shows a file uploaded with the same message. `file.url`
/// must be `attachment://<filename>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileComponent {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// The attachment reference, `attachment://<filename>`.
    pub file: MediaInfo,
    /// Whether the file is blurred as a spoiler.
    pub spoiler: Option<bool>,
}

impl FileComponent {
    /// A file component for `attachment://<filename>`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            file: MediaInfo { url: url.into() },
            spoiler: None,
        }
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        // "only supports attachment references using the
        // `attachment://<filename>` syntax" (File Structure).
        if !self.file.url.starts_with(ATTACHMENT_SCHEME) {
            return Err(at(
                path,
                format!(
                    "file component url must use {ATTACHMENT_SCHEME}<filename>, got {:?}",
                    self.file.url
                ),
            ));
        }
        Ok(())
    }
}

impl Serialize for FileComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::FILE)?;
        entry_some(&mut map, "id", &self.id)?;
        map.serialize_entry("file", &self.file)?;
        entry_some(&mut map, "spoiler", &self.spoiler)?;
        map.end()
    }
}

/// The URL scheme a [`FileComponent`] must use.
const ATTACHMENT_SCHEME: &str = "attachment://";

/// Separator (type 14): vertical padding, with or without a divider line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Separator {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// Whether a divider line is shown (Discord's default: `true`).
    pub divider: Option<bool>,
    /// Padding size, [`SeparatorSpacing::SMALL`] (Discord's default) or
    /// [`SeparatorSpacing::LARGE`]; other values fail validation.
    pub spacing: Option<SeparatorSpacing>,
}

impl Separator {
    /// A separator with Discord's defaults.
    pub fn new() -> Self {
        Self::default()
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        match self.spacing {
            None | Some(SeparatorSpacing::SMALL | SeparatorSpacing::LARGE) => Ok(()),
            Some(spacing) => Err(at(
                path,
                format!(
                    "separator spacing must be 1 (small) or 2 (large), got {}",
                    spacing.get()
                ),
            )),
        }
    }
}

impl Serialize for Separator {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::SEPARATOR)?;
        entry_some(&mut map, "id", &self.id)?;
        entry_some(&mut map, "divider", &self.divider)?;
        entry_some(&mut map, "spacing", &self.spacing)?;
        map.end()
    }
}

/// Container (type 17): a box around other components, with an optional
/// accent colour. Containers do not nest.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Container {
    /// Optional component identifier, unique within the message.
    pub id: Option<ComponentId>,
    /// Accent bar colour as RGB, `0x000000`-`0xFFFFFF`.
    pub accent_color: Option<u32>,
    /// Whether the container is blurred as a spoiler.
    pub spoiler: Option<bool>,
    /// The children, in display order.
    pub components: Vec<ContainerComponent>,
}

impl Container {
    /// An empty container; add at least one child.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks the container and its children against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("", &mut Tally::default())
    }

    fn check<'a>(&'a self, path: &str, tally: &mut Tally<'a>) -> Result<(), DiscordError> {
        tally.component(self.id, path)?;
        if let Some(color) = self.accent_color {
            if color > limits::MAX_ACCENT_COLOR {
                return Err(at(
                    path,
                    format!(
                        "container accent_color must be at most 0x{:06X}, got 0x{color:X}",
                        limits::MAX_ACCENT_COLOR
                    ),
                ));
            }
        }
        check_count(
            path,
            "container components",
            self.components.len(),
            1,
            limits::MAX_COMPONENTS_V2_TOTAL,
        )?;
        for (index, component) in self.components.iter().enumerate() {
            component.check(&child_path(path, "components", index), tally)?;
        }
        Ok(())
    }
}

impl Serialize for Container {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &ComponentType::CONTAINER)?;
        entry_some(&mut map, "id", &self.id)?;
        entry_some(&mut map, "accent_color", &self.accent_color)?;
        entry_some(&mut map, "spoiler", &self.spoiler)?;
        map.serialize_entry("components", &self.components)?;
        map.end()
    }
}

/// `validate()` for the leaf and row types, which only have the private
/// `check` so far.
macro_rules! validate_via_check {
    ($($ty:ty: $what:literal),* $(,)?) => {$(
        impl $ty {
            #[doc = concat!("Checks the ", $what, " against Discord's limits.")]
            pub fn validate(&self) -> Result<(), DiscordError> {
                self.check("", &mut Tally::default())
            }
        }
    )*};
}

validate_via_check! {
    ButtonRow: "row and its buttons",
    SelectRow: "row and its select menu",
    CustomButton: "button",
    LinkButton: "button",
    Thumbnail: "thumbnail",
    MediaGallery: "gallery",
    FileComponent: "file component",
    Separator: "separator",
}

// ---------------------------------------------------------------------------
// Message-wide validation
// ---------------------------------------------------------------------------

/// Which component rules a message body is checked under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComponentsMode {
    /// `IS_COMPONENTS_V2` is not set: only action rows (and raw components)
    /// at the top level, at most 5.
    Legacy,
    /// `IS_COMPONENTS_V2` is set: any component, at most 40 counting nested
    /// ones.
    V2,
    /// An edit that does not set the flags: the message keeps whatever mode
    /// it has, which the body cannot see. Only the rules both modes share
    /// are checked (each component's own limits, unique ids, at most 40).
    Unknown,
}

/// Checks a message's `components` array: the per-component limits, plus
/// the message-wide rules of `mode`. Non-zero `id`s and `custom_id`s must be
/// unique within the message.
pub(crate) fn check_message_components(
    components: &[Component],
    mode: ComponentsMode,
) -> Result<(), DiscordError> {
    let mut tally = Tally::default();
    if mode == ComponentsMode::Legacy {
        if let Some((index, _)) = components
            .iter()
            .enumerate()
            .find(|(_, component)| component.requires_components_v2())
        {
            return Err(DiscordError::model(format!(
                "components[{index}]: layout and content components need the \
                 IS_COMPONENTS_V2 message flag"
            )));
        }
        check_count(
            "",
            "message action rows",
            components.len(),
            0,
            limits::MAX_LEGACY_ACTION_ROWS,
        )?;
    }
    for (index, component) in components.iter().enumerate() {
        component.check(&child_path("", "components", index), &mut tally)?;
    }
    // The legacy cap (5 rows of at most 5) is far below 40, so 40 holds in
    // every mode.
    if tally.components > limits::MAX_COMPONENTS_V2_TOTAL {
        return Err(DiscordError::model(format!(
            "components v2 message components must contain at most {} items \
             (nested components included), got {}",
            limits::MAX_COMPONENTS_V2_TOTAL,
            tally.components
        )));
    }
    Ok(())
}

/// Counters filled while walking one message's components.
#[derive(Default)]
pub(crate) struct Tally<'a> {
    components: usize,
    ids: HashSet<ComponentId>,
    custom_ids: HashSet<&'a str>,
}

impl<'a> Tally<'a> {
    fn count(&mut self) {
        self.components = self.components.saturating_add(1);
    }

    /// Counts one component and records its `id`. `ComponentId` 0 is
    /// Discord's "unset" value (Discord assigns one), so it may repeat.
    fn component(&mut self, id: Option<ComponentId>, path: &str) -> Result<(), DiscordError> {
        self.count();
        match id {
            Some(id) if id.get() != 0 && !self.ids.insert(id) => Err(at(
                path,
                format!("component id {} is used more than once", id.get()),
            )),
            _ => Ok(()),
        }
    }

    /// "Multiple components on the same message must not share the same
    /// `custom_id`" (components/reference, Custom ID).
    fn custom_id(&mut self, custom_id: &'a str, path: &str) -> Result<(), DiscordError> {
        if self.custom_ids.insert(custom_id) {
            Ok(())
        } else {
            Err(at(
                path,
                format!("custom_id {custom_id:?} is used more than once"),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers shared by the request modules
// ---------------------------------------------------------------------------

/// A validation error located at `path` (`components[2].accessory`), or
/// unlocated when `path` is empty.
pub(crate) fn at(path: &str, message: String) -> DiscordError {
    if path.is_empty() {
        DiscordError::model(message)
    } else {
        DiscordError::model(format!("{path}: {message}"))
    }
}

/// `path.key[index]`, or `key[index]` at the root.
pub(crate) fn child_path(path: &str, key: &str, index: usize) -> String {
    if path.is_empty() {
        format!("{key}[{index}]")
    } else {
        format!("{path}.{key}[{index}]")
    }
}

/// `path.key`, or `key` at the root.
pub(crate) fn field_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// `field` is `min..=max` characters (Discord counts codepoints).
pub(crate) fn check_len(
    path: &str,
    field: &str,
    value: &str,
    min: usize,
    max: usize,
) -> Result<(), DiscordError> {
    let count = char_count(value);
    if count < min || count > max {
        return Err(at(
            path,
            format!("{field} must be {min}-{max} characters, got {count}"),
        ));
    }
    Ok(())
}

/// `field` is at most `max` characters.
pub(crate) fn check_max_len(
    path: &str,
    field: &str,
    value: &str,
    max: usize,
) -> Result<(), DiscordError> {
    let count = char_count(value);
    if count > max {
        return Err(at(
            path,
            format!("{field} must be at most {max} characters, got {count}"),
        ));
    }
    Ok(())
}

/// A collection holds `min..=max` items.
pub(crate) fn check_count(
    path: &str,
    field: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), DiscordError> {
    if actual < min || actual > max {
        let message = if min == 0 {
            format!("{field} must contain at most {max} items, got {actual}")
        } else {
            format!("{field} must contain {min}-{max} items, got {actual}")
        };
        return Err(at(path, message));
    }
    Ok(())
}

/// The label rules shared by custom and link buttons: up to 80 characters,
/// and a label, an emoji, or both.
fn check_label_or_emoji(
    path: &str,
    label: Option<&str>,
    emoji: Option<&Emoji>,
) -> Result<(), DiscordError> {
    if let Some(label) = label {
        check_max_len(path, "button label", label, limits::MAX_BUTTON_LABEL_LENGTH)?;
    }
    if label.is_none() && emoji.is_none() {
        return Err(at(path, "button must have a label or emoji".to_owned()));
    }
    Ok(())
}

/// Writes `key` when the value is present.
pub(crate) fn entry_some<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Option<T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(key, value),
        None => Ok(()),
    }
}

/// Writes `key` unless the patch omits it: `null` or the value.
pub(crate) fn entry_patch<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Patch<Nullable<T>>,
) -> Result<(), M::Error> {
    match value {
        Patch::Missing => Ok(()),
        Patch::Value(value) => map.serialize_entry(key, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::test_id;
    use serde_json::json;

    /// The validation message without the `Model error: ` prefix.
    fn message(result: Result<(), DiscordError>) -> String {
        let error = result.expect_err("expected a validation error").to_string();
        error
            .strip_prefix("Model error: ")
            .unwrap_or(&error)
            .to_owned()
    }

    fn error(select: impl Into<SelectMenu>) -> String {
        message(select.into().validate())
    }

    fn custom(custom_id: &str) -> CustomButton {
        let mut button = CustomButton::new(ButtonStyle::PRIMARY, custom_id);
        button.label = Some("Go".to_owned());
        button
    }

    fn link() -> LinkButton {
        let mut button = LinkButton::new("https://example.com");
        button.label = Some("Docs".to_owned());
        button
    }

    fn string_select(custom_id: &str) -> StringSelect {
        let mut select = StringSelect::new(custom_id);
        select.options = vec![SelectOption::new("One", "1")];
        select
    }

    fn section_with(accessory: impl Into<SectionAccessory>) -> Section {
        let mut section = Section::new(accessory);
        section.components = vec![TextDisplay::new("text")];
        section
    }

    fn with_id<T>(mut value: T, set: impl FnOnce(&mut T)) -> T {
        set(&mut value);
        value
    }

    #[test]
    fn select_bounds_use_discords_defaults_when_omitted() {
        let mut string = StringSelect::new("menu");
        string.options = vec![SelectOption::new("a", "a"), SelectOption::new("b", "b")];
        string.min_values = Some(2);
        assert_eq!(
            error(string.clone()),
            "select menu min_values (2) must not exceed max_values (1, the default when omitted)"
        );
        string.max_values = Some(2);
        SelectMenu::from(string).validate().expect("2..=2");

        let mut user = UserSelect::new("who");
        user.max_values = Some(3);
        SelectMenu::from(user.clone())
            .validate()
            .expect("default min 1 <= 3");
        user.min_values = Some(4);
        assert_eq!(
            error(user),
            "select menu min_values (4) must not exceed max_values (3)"
        );
        // Only an omitted min can be the defaulted side.
        let mut role = RoleSelect::new("r");
        role.max_values = Some(1);
        role.min_values = None;
        SelectMenu::from(role).validate().expect("1..=1");
    }

    #[test]
    fn select_menus_check_their_shared_limits() {
        assert_eq!(
            error(string_select("")),
            "select menu custom_id must be 1-100 characters, got 0"
        );
        let mut placeholder = string_select("p");
        placeholder.placeholder = Some("p".repeat(151));
        assert_eq!(
            error(placeholder),
            "select menu placeholder must be at most 150 characters, got 151"
        );
        let mut min = UserSelect::new("u");
        min.min_values = Some(26);
        min.max_values = Some(25);
        assert_eq!(
            error(min),
            "select menu min_values must be at most 25, got 26"
        );
        let mut max = RoleSelect::new("r");
        max.max_values = Some(0);
        assert_eq!(error(max), "select menu max_values must be 1-25, got 0");
        let mut defaults = ChannelSelect::new("c");
        defaults.default_values = vec![test_id(1), test_id(2)];
        assert_eq!(
            error(defaults.clone()),
            "select menu default_values must contain 1-1 items (min_values-max_values), got 2"
        );
        defaults.max_values = Some(2);
        SelectMenu::from(defaults)
            .validate()
            .expect("two defaults allowed");
        let mut too_few = MentionableSelect::new("m");
        too_few.min_values = Some(2);
        too_few.max_values = Some(3);
        too_few.default_values = vec![MentionableId::User(test_id(1))];
        assert_eq!(
            error(too_few),
            "select menu default_values must contain 2-3 items (min_values-max_values), got 1"
        );
        // Each struct's own validate() is the same check.
        assert!(StringSelect::new("x").validate().is_err());
        UserSelect::new("x").validate().expect("no options needed");
        RoleSelect::new("x").validate().expect("valid");
        MentionableSelect::new("x").validate().expect("valid");
        ChannelSelect::new("x").validate().expect("valid");
    }

    #[test]
    fn string_select_checks_its_options() {
        assert_eq!(
            error(StringSelect::new("s")),
            "select menu options must contain 1-25 items, got 0"
        );
        let mut many = StringSelect::new("s");
        many.options = (0..26)
            .map(|index| SelectOption::new(&index.to_string(), "v"))
            .collect();
        assert_eq!(
            error(many),
            "select menu options must contain 1-25 items, got 26"
        );
        let mut label = StringSelect::new("s");
        label.options = vec![SelectOption::new("", "v")];
        assert_eq!(
            error(label),
            "select menu option \"\" label must be 1-100 characters, got 0"
        );
        let mut value = StringSelect::new("s");
        value.options = vec![SelectOption::new("L", "")];
        assert_eq!(
            error(value),
            "select menu option \"L\" value must be 1-100 characters, got 0"
        );
        let mut description = StringSelect::new("s");
        description.options = vec![SelectOption::new("L", "v").description(&"d".repeat(101))];
        assert_eq!(
            error(description),
            "select menu option \"L\" description must be at most 100 characters, got 101"
        );
    }

    #[test]
    fn buttons_check_their_kind_rules() {
        let button = |button: CustomButton| message(Button::from(button).validate());
        let mut link_style = custom("a");
        link_style.style = ButtonStyle::LINK;
        assert_eq!(
            button(link_style),
            "button style 5 needs a url or sku_id; use LinkButton or PremiumButton"
        );
        let mut premium_style = custom("a");
        premium_style.style = ButtonStyle::PREMIUM;
        assert!(button(premium_style).contains("button style 6"));
        assert_eq!(
            button(custom(&"i".repeat(101))),
            "button custom_id must be 1-100 characters, got 101"
        );
        let mut unlabeled = custom("a");
        unlabeled.label = None;
        assert_eq!(
            button(unlabeled.clone()),
            "button must have a label or emoji"
        );
        unlabeled.emoji = Some(Emoji::unicode("👍"));
        unlabeled.validate().expect("emoji-only button");
        let mut long_label = custom("a");
        long_label.label = Some("l".repeat(81));
        assert_eq!(
            button(long_label),
            "button label must be at most 80 characters, got 81"
        );
        // Unknown styles pass; Discord decides.
        let mut future = custom("a");
        future.style = ButtonStyle::new(9);
        future.validate().expect("unknown style passes");

        let mut empty_url = link();
        empty_url.url = String::new();
        assert_eq!(
            message(empty_url.validate()),
            "button url must be 1-512 characters, got 0"
        );
        let mut no_label = link();
        no_label.label = None;
        assert_eq!(
            message(no_label.validate()),
            "button must have a label or emoji"
        );
        link().validate().expect("valid link");
        Button::from(PremiumButton::new(test_id(1)))
            .validate()
            .expect("premium buttons have no limits");
    }

    #[test]
    fn rows_count_and_deduplicate_their_children() {
        assert_eq!(
            message(ButtonRow::new().validate()),
            "action row components must contain 1-5 items, got 0"
        );
        let full = (0..6).fold(ButtonRow::new(), |row, index| {
            row.add_button(custom(&format!("b{index}")))
        });
        assert_eq!(
            message(full.validate()),
            "action row components must contain 1-5 items, got 6"
        );
        let duplicate = ButtonRow::new()
            .add_button(custom("same"))
            .add_button(custom("same"));
        assert_eq!(
            message(duplicate.validate()),
            "components[1]: custom_id \"same\" is used more than once"
        );
        let ids = ButtonRow::new()
            .add_button(with_id(custom("a"), |b| b.id = Some(ComponentId::new(3))))
            .add_button(with_id(link(), |b| b.id = Some(ComponentId::new(3))));
        assert_eq!(
            message(ids.validate()),
            "components[1]: component id 3 is used more than once"
        );
        // Id 0 means "unset" and may repeat.
        ButtonRow::new()
            .add_button(with_id(custom("a"), |b| b.id = Some(ComponentId::new(0))))
            .add_button(with_id(link(), |b| b.id = Some(ComponentId::new(0))))
            .validate()
            .expect("id 0 repeats");
        let mut select_row = SelectRow::new(string_select("s"));
        select_row.validate().expect("valid select row");
        select_row.select = SelectMenu::from(StringSelect::new("s"));
        assert_eq!(
            message(select_row.validate()),
            "components[0]: select menu options must contain 1-25 items, got 0"
        );
        ActionRow::from(ButtonRow::new().add_button(link()))
            .validate()
            .expect("valid row");
        ActionRow::from(SelectMenu::from(UserSelect::new("u")))
            .validate()
            .expect("valid select row");
        assert!(
            ActionRow::from(UserSelect::new("u"))
                == ActionRow::from(SelectRow::new(UserSelect::new("u")))
        );
    }

    #[test]
    fn layout_components_check_their_limits() {
        let thumbnail = Thumbnail::new("https://e.co/t.png");
        assert_eq!(
            message(Section::new(thumbnail.clone()).validate()),
            "section components must contain 1-3 items, got 0"
        );
        let mut crowded = section_with(thumbnail.clone());
        crowded.components = vec![TextDisplay::new("a"); 4];
        assert_eq!(
            message(crowded.validate()),
            "section components must contain 1-3 items, got 4"
        );
        let mut empty_text = section_with(thumbnail.clone());
        empty_text.components = vec![TextDisplay::new("")];
        assert_eq!(
            message(empty_text.validate()),
            "components[0]: text display content must be 1-4000 characters, got 0"
        );
        let mut long_alt = thumbnail.clone();
        long_alt.description = Some("d".repeat(1025));
        assert_eq!(
            message(section_with(long_alt.clone()).validate()),
            "accessory: thumbnail description must be at most 1024 characters, got 1025"
        );
        assert_eq!(
            message(long_alt.validate()),
            "thumbnail description must be at most 1024 characters, got 1025"
        );
        let mut bad_button = custom("x");
        bad_button.label = None;
        assert_eq!(
            message(section_with(bad_button).validate()),
            "accessory: button must have a label or emoji"
        );
        section_with(link()).validate().expect("link accessory");
        section_with(PremiumButton::new(test_id(1)))
            .validate()
            .expect("premium accessory");
        section_with(Button::from(link()))
            .validate()
            .expect("button accessory");

        assert_eq!(
            message(TextDisplay::new("c".repeat(4001)).validate()),
            "text display content must be 1-4000 characters, got 4001"
        );
        assert_eq!(
            message(MediaGallery::new().validate()),
            "media gallery items must contain 1-10 items, got 0"
        );
        let mut gallery = MediaGallery::new();
        gallery.items =
            vec![MediaGalleryItem::new("https://e.co/a.png").description(&"d".repeat(1025))];
        assert_eq!(
            message(gallery.validate()),
            "media gallery item 0 description must be at most 1024 characters, got 1025"
        );
        gallery.items = vec![MediaGalleryItem::new("https://e.co/a.png"); 11];
        assert_eq!(
            message(gallery.validate()),
            "media gallery items must contain 1-10 items, got 11"
        );
        assert_eq!(
            message(FileComponent::new("https://e.co/a.zip").validate()),
            "file component url must use attachment://<filename>, got \"https://e.co/a.zip\""
        );
        FileComponent::new("attachment://a.zip")
            .validate()
            .expect("attachment reference");
        let mut separator = Separator::new();
        separator.spacing = Some(SeparatorSpacing::new(0));
        assert_eq!(
            message(separator.validate()),
            "separator spacing must be 1 (small) or 2 (large), got 0"
        );
        separator.spacing = Some(SeparatorSpacing::LARGE);
        separator.validate().expect("large spacing");
    }

    #[test]
    fn containers_check_colour_children_and_nesting() {
        assert_eq!(
            message(Container::new().validate()),
            "container components must contain 1-40 items, got 0"
        );
        let mut container = Container::new();
        container.components = vec![ContainerComponent::from(TextDisplay::new("x"))];
        container.accent_color = Some(0x0100_0000);
        assert_eq!(
            message(container.validate()),
            "container accent_color must be at most 0xFFFFFF, got 0x1000000"
        );
        container.accent_color = Some(0xFF_FFFF);
        container.validate().expect("max colour");
        container.components = vec![
            ContainerComponent::from(ActionRow::from(ButtonRow::new().add_button(link()))),
            ContainerComponent::from(section_with(link())),
            ContainerComponent::from(with_id(MediaGallery::new(), |g| {
                g.items = vec![MediaGalleryItem::new("https://e.co/a.png")];
            })),
            ContainerComponent::from(FileComponent::new("attachment://a")),
            ContainerComponent::from(Separator::new()),
            ContainerComponent::from(RawComponent::from_json("{}").unwrap()),
            ContainerComponent::from(SelectRow::new(UserSelect::new("u"))),
            ContainerComponent::from(ButtonRow::new().add_button(custom("c"))),
        ];
        container.validate().expect("every child kind");
        container
            .components
            .push(ContainerComponent::from(TextDisplay::new("")));
        assert_eq!(
            message(container.validate()),
            "components[8]: text display content must be 1-4000 characters, got 0"
        );

        // Containers do not nest; the conversion returns the container.
        let inner = Component::from(Container::new());
        assert!(matches!(
            ContainerComponent::try_from(inner),
            Err(Component::Container(_))
        ));
        for component in [
            Component::from(TextDisplay::new("t")),
            Component::from(ButtonRow::new()),
            Component::from(section_with(link())),
            Component::from(MediaGallery::new()),
            Component::from(FileComponent::new("attachment://a")),
            Component::from(Separator::new()),
            Component::from(RawComponent::from_json("1").unwrap()),
        ] {
            let child = ContainerComponent::try_from(component.clone()).expect("a container child");
            assert_eq!(Component::from(child), component);
        }
    }

    #[test]
    fn components_v2_requirement_by_kind() {
        assert!(!Component::from(ButtonRow::new()).requires_components_v2());
        assert!(!Component::from(RawComponent::from_json("{}").unwrap()).requires_components_v2());
        for component in [
            Component::from(TextDisplay::new("t")),
            Component::from(section_with(link())),
            Component::from(MediaGallery::new()),
            Component::from(FileComponent::new("attachment://a")),
            Component::from(Separator::new()),
            Component::from(Container::new()),
        ] {
            assert!(component.requires_components_v2(), "{component:?}");
        }
        Component::from(RawComponent::from_json("{}").unwrap())
            .validate()
            .expect("raw is not checked");
        Component::from(with_id(Container::new(), |c| {
            c.components = vec![ContainerComponent::from(TextDisplay::new("x"))];
        }))
        .validate()
        .expect("valid container component");
    }

    #[test]
    fn message_components_follow_the_mode() {
        let text = || Component::from(TextDisplay::new("t"));
        let row = || Component::from(ButtonRow::new().add_button(link()));
        assert_eq!(
            message(check_message_components(
                &[row(), text()],
                ComponentsMode::Legacy
            )),
            "components[1]: layout and content components need the IS_COMPONENTS_V2 message flag"
        );
        assert_eq!(
            message(check_message_components(
                &vec![row(); 6],
                ComponentsMode::Legacy
            )),
            "message action rows must contain at most 5 items, got 6"
        );
        check_message_components(&vec![row(); 5], ComponentsMode::Legacy).expect("five rows");
        check_message_components(&[row(), text()], ComponentsMode::V2).expect("v2");
        check_message_components(&[row(), text()], ComponentsMode::Unknown).expect("unknown");
        for mode in [ComponentsMode::V2, ComponentsMode::Unknown] {
            assert_eq!(
                message(check_message_components(&vec![text(); 41], mode)),
                "components v2 message components must contain at most 40 items \
                 (nested components included), got 41"
            );
        }
        // Buttons inside rows count toward 40.
        let five = (0..5).fold(ButtonRow::new(), |row, index| {
            row.add_button(custom(&format!("{index}")))
        });
        let rows: Vec<Component> = (0..7)
            .map(|row| {
                Component::from((0..5).fold(ButtonRow::new(), |r, index| {
                    r.add_button(custom(&format!("{row}-{index}")))
                }))
            })
            .collect();
        assert!(message(check_message_components(&rows, ComponentsMode::V2)).contains("got 42"));
        check_message_components(&[Component::from(five)], ComponentsMode::V2).expect("six");
    }

    #[test]
    fn raw_components_keep_their_text() {
        assert!(RawComponent::from_json("{not json").is_err());
        let from_text = RawComponent::from_json(r#"{"type":1}"#).unwrap();
        let value = json!({"type": 1});
        let from_value = RawComponent::from_value(&value).unwrap();
        assert_eq!(from_text, from_value);
        assert_eq!(RawComponent::try_from(&value).unwrap(), from_text);
        assert_eq!(RawComponent::try_from(value).unwrap(), from_text);
        assert_eq!(from_text.get(), r#"{"type":1}"#);
        assert_eq!(format!("{from_text:?}"), r#"RawComponent("{\"type\":1}")"#);
        assert_ne!(
            RawComponent::from_json(r#"{ "type": 1 }"#).unwrap(),
            from_text,
            "compared as text"
        );
        assert_eq!(
            serde_json::to_string(&Component::from(from_text)).unwrap(),
            r#"{"type":1}"#
        );
    }

    #[test]
    fn helpers_build_paths_and_messages() {
        assert_eq!(child_path("", "components", 2), "components[2]");
        assert_eq!(child_path("a", "items", 0), "a.items[0]");
        assert_eq!(field_path("", "accessory"), "accessory");
        assert_eq!(field_path("a", "accessory"), "a.accessory");
        assert_eq!(
            message(check_count("p", "things", 3, 0, 2)),
            "p: things must contain at most 2 items, got 3"
        );
        assert_eq!(
            message(check_count("", "things", 0, 1, 2)),
            "things must contain 1-2 items, got 0"
        );
        check_count("", "things", 1, 1, 2).expect("in range");
        assert_eq!(
            message(check_max_len("p", "text", "abc", 2)),
            "p: text must be at most 2 characters, got 3"
        );
        check_len("", "text", "é", 1, 1).expect("codepoints, not bytes");
        assert_eq!(
            message(check_len("", "text", "", 1, 2)),
            "text must be 1-2 characters, got 0"
        );
    }
}
