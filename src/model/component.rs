//! Message components (legacy and Components V2) and their parts.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::Emoji;

use super::{
    AttachmentId, ButtonStyle, ChannelId, ChannelType, ComponentId, ComponentType, GenericId,
    RoleId, SeparatorSpacing, SkuId, TextInputStyle, UserId,
};

/// A single component attached to a received message, modal, or snapshot.
///
/// Discord components form a tree (action rows, sections, containers, and
/// labels nest child components). Rather than one struct per component
/// type, this is deliberately a *wide* struct: [`Self::kind`] carries the
/// component `type` and every known field across all component kinds
/// is optional (or defaults to empty). The tradeoff: the compiler cannot
/// guarantee which fields are populated for a given `kind`, but decoding
/// never fails when Discord ships new component types or fields, and one
/// type covers the whole tree recursively.
///
/// No `Default`: `type` is required and has no default. Build one with
/// [`MessageComponent::new`] and struct update syntax.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageComponent {
    /// Component type, such as [`ComponentType::BUTTON`].
    #[serde(rename = "type")]
    pub kind: ComponentType,
    /// Numeric component ID (32-bit, unique within the message). Not a
    /// snowflake; see [`ComponentId`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<ComponentId>,
    /// Developer-defined identifier (buttons, selects, text inputs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Raw `style`: a [`ButtonStyle`] on buttons and a [`TextInputStyle`] on
    /// text inputs. The meaning depends on [`Self::kind`], so this shared
    /// field keeps the wire number; [`Self::button_style`] and
    /// [`Self::text_input_style`] return the typed value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<u8>,
    /// Button label or label-component text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Description for label and thumbnail components.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Emoji shown on a button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    /// URL for link-style buttons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// SKU targeted by premium-style buttons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<SkuId>,
    /// Whether the interactive component is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// Whether the component must be filled in a modal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Choices of a string select menu.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<MessageSelectOption>,
    /// Channel types a channel select menu is limited to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_types: Vec<ChannelType>,
    /// Placeholder text for selects and text inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Pre-selected values of auto-populated select menus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_values: Vec<SelectDefaultValue>,
    /// Minimum number of select values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u64>,
    /// Maximum number of select values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u64>,
    /// Minimum text-input length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,
    /// Maximum text-input length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    /// `value`: a string for text inputs (pre-filled or submitted) and
    /// submitted radio groups, a boolean for submitted checkboxes. `None`
    /// when absent, and for a radio group submitted with nothing selected
    /// (`null`). Read it with [`Self::value_str`] / [`Self::value_bool`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<MessageComponentValue>,
    /// `values` of a submitted select menu, checkbox group, or file upload
    /// (selected option values, snowflakes, or attachment IDs). An empty
    /// list (nothing selected) is kept apart from an absent field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    /// Whether a checkbox starts checked (`default` in modal create
    /// payloads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    /// Child components (action rows, sections, containers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<MessageComponent>,
    /// Single child component of a label component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<Box<MessageComponent>>,
    /// Accessory (thumbnail or button) of a section component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<Box<MessageComponent>>,
    /// Markdown content of a text display component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Media of a thumbnail component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<UnfurledMediaItem>,
    /// Media of a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<UnfurledMediaItem>,
    /// File name reported for a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// File size in bytes reported for a file component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Items of a media gallery component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<MessageMediaGalleryItem>,
    /// Whether the media or container is a spoiler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
    /// Whether a separator renders a visual divider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divider: Option<bool>,
    /// Separator spacing size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<SeparatorSpacing>,
    /// Container accent color (RGB integer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<u64>,
}

// [Decision Log]
// - 목적과 의도: 넓은 구조체의 `type`·`spacing`을 열린 판별값으로 바꾸고, 종류마다 뜻이 다른
//   `style`도 타입으로 읽을 수 있게 한다(설계 §2, 시리즈 3).
// - 기존 구현 및 제약 조건: 2.x는 `kind: u8`(+ `#[serde(default)]`, 누락 시 0), `style:
//   Option<u64>`, 연관 상수 `MessageComponent::BUTTON: u8` 등을 두었다. `style`은 버튼이면
//   ButtonStyle, 텍스트 입력이면 TextInputStyle이다. 5단계에서 나가는 컴포넌트는 데이터 enum이
//   된다(설계 §4).
// - 검토한 주요 대안: `style`을 (1) Option<ButtonStyle>로 두기, (2) `kind`를 보고 직렬화하는
//   enum, (3) 와이어 숫자(u8) + 종류별 타입 접근자.
// - 선택한 방식: (3). `kind`는 ComponentType(필수, Default 없음), `spacing`은
//   SeparatorSpacing, 연관 상수는 지웠다(ComponentType 상수가 원본).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 텍스트 입력의 값을 버튼 이름으로 출력한다.
//   (2)는 감싸는 객체 문맥이 필요한 직렬화를 넓은 수신 구조체에 들여오고, 5단계에서 다시
//   바뀐다. (3)은 시리즈 2 B의 다형 ID(`CommandInteractionData::target_id`)와 같은 규칙이다.
// - 장점, 단점 및 영향: `style`만 숫자로 남는다(접근자로 보완). 256 이상의 `style`은 디코딩
//   오류가 된다(두 스타일 타입 모두 u8). `type` 누락도 이제 디코딩 오류다.
impl MessageComponent {
    /// A component of `kind` with every other field empty, for struct
    /// update syntax: `MessageComponent { custom_id: .., ..MessageComponent::new(kind) }`.
    pub fn new(kind: ComponentType) -> Self {
        Self {
            kind,
            id: None,
            custom_id: None,
            style: None,
            label: None,
            description: None,
            emoji: None,
            url: None,
            sku_id: None,
            disabled: None,
            required: None,
            options: Vec::new(),
            channel_types: Vec::new(),
            placeholder: None,
            default_values: Vec::new(),
            min_values: None,
            max_values: None,
            min_length: None,
            max_length: None,
            value: None,
            values: None,
            default: None,
            components: Vec::new(),
            component: None,
            accessory: None,
            content: None,
            media: None,
            file: None,
            name: None,
            size: None,
            items: Vec::new(),
            spoiler: None,
            divider: None,
            spacing: None,
            accent_color: None,
        }
    }

    /// The button style, if this is a [`ComponentType::BUTTON`] that has
    /// one.
    pub fn button_style(&self) -> Option<ButtonStyle> {
        match (self.kind, self.style) {
            (ComponentType::BUTTON, Some(style)) => Some(ButtonStyle::new(style)),
            _ => None,
        }
    }

    /// The string `value`: a text input's text or a submitted radio group's
    /// selected option. `None` when there is none or it is not a string.
    pub fn value_str(&self) -> Option<&str> {
        self.value.as_ref().and_then(MessageComponentValue::as_str)
    }

    /// The boolean `value` of a submitted checkbox (`true` when checked).
    /// `None` when there is none or it is not a boolean.
    pub fn value_bool(&self) -> Option<bool> {
        self.value.as_ref().and_then(MessageComponentValue::as_bool)
    }

    /// The text input style, if this is a [`ComponentType::TEXT_INPUT`] that
    /// has one.
    pub fn text_input_style(&self) -> Option<TextInputStyle> {
        match (self.kind, self.style) {
            (ComponentType::TEXT_INPUT, Some(style)) => Some(TextInputStyle::new(style)),
            _ => None,
        }
    }

    /// Depth-first iterator over this component and every nested child
    /// (child `components`, label `component`, and section `accessory`).
    pub fn iter(&self) -> MessageComponentIter<'_> {
        MessageComponentIter { stack: vec![self] }
    }
}

/// Depth-first iterator returned by [`MessageComponent::iter`].
pub struct MessageComponentIter<'a> {
    stack: Vec<&'a MessageComponent>,
}

impl<'a> Iterator for MessageComponentIter<'a> {
    type Item = &'a MessageComponent;

    fn next(&mut self) -> Option<Self::Item> {
        let component = self.stack.pop()?;
        if let Some(accessory) = component.accessory.as_deref() {
            self.stack.push(accessory);
        }
        if let Some(child) = component.component.as_deref() {
            self.stack.push(child);
        }
        for child in component.components.iter().rev() {
            self.stack.push(child);
        }
        Some(component)
    }
}

/// The `value` of a received component, whose JSON type depends on the
/// component: text inputs and radio groups send a string, a submitted
/// checkbox sends a boolean.
///
/// Reads and writes the bare JSON value. Any other JSON type (a component
/// Discord adds later) is kept in [`Other`](Self::Other) and written back
/// unchanged, so decoding never fails on it.
///
/// ```
/// use discordrs::{MessageComponent, MessageComponentValue};
///
/// let checkbox: MessageComponent =
///     serde_json::from_str(r#"{"type": 23, "id": 2, "custom_id": "agree", "value": true}"#)
///         .unwrap();
/// assert_eq!(checkbox.value, Some(MessageComponentValue::Bool(true)));
/// assert_eq!(checkbox.value_bool(), Some(true));
/// assert_eq!(checkbox.value_str(), None);
/// ```
// [Decision Log]
// - 목적과 의도: 모달 제출 컴포넌트의 `value`를 문서대로 읽는다. 텍스트 입력·라디오 그룹은
//   문자열(라디오는 선택이 없으면 null), 체크박스는 bool이다(components/reference, 3714862).
// - 기존 구현 및 제약 조건: `MessageComponent`는 모든 종류의 필드를 담는 넓은 수신 구조체이고
//   디코딩이 실패하지 않는 것을 원칙으로 한다. 2.x는 `value: Option<String>`이라 체크박스 제출
//   `"value": true`가 들어 있는 페이로드 전체가 디코딩 오류였다.
// - 검토한 주요 대안: (1) `value`를 `serde_json::Value`로 두기, (2) 종류별 접근자만 두고 저장은
//   그대로, (3) JSON 타입별 enum(String/Bool) + 모르는 모양을 담는 Other + 접근자.
// - 선택한 방식: (3). `values`(선택 메뉴·체크박스 그룹·파일 업로드)는 별도 필드
//   `Option<Vec<String>>`로 둔다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 타입 정보를 잃고 사용자가 매번 꺼내야 한다.
//   (2)는 bool을 String에 담을 수 없어 저장이 불가능하다(`style`은 두 종류 모두 u8이라 원시 저장 +
//   접근자가 가능했던 것과 다르다). (3)은 와이어의 JSON 타입이 이미 종류를 구분하므로 종류를 다시
//   확인할 필요가 없고, Other로 넓은 구조체의 "디코딩은 실패하지 않는다" 원칙을 지킨다.
// - 장점, 단점 및 영향: 파괴적 변경(`value.as_deref()` → `value_str()`). Other는 `Box`로 두어
//   `Option<MessageComponentValue>` 크기가 `Option<String>`과 같다.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageComponentValue {
    /// A text input's text or a radio group's selected option value.
    String(String),
    /// A submitted checkbox: `true` when checked.
    Bool(bool),
    /// A JSON value of another type, kept as sent.
    Other(Box<serde_json::Value>),
}

impl MessageComponentValue {
    /// The string, if this is [`Self::String`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Bool(_) | Self::Other(_) => None,
        }
    }

    /// The boolean, if this is [`Self::Bool`].
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::String(_) | Self::Other(_) => None,
        }
    }
}

impl From<String> for MessageComponentValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for MessageComponentValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<bool> for MessageComponentValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl Serialize for MessageComponentValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for MessageComponentValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(value) => Self::String(value),
            serde_json::Value::Bool(value) => Self::Bool(value),
            other => Self::Other(Box::new(other)),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Option of a string select menu on a received message component.
pub struct MessageSelectOption {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

/// A pre-selected value of an auto-populated (user, role, mentionable, or
/// channel) select menu.
///
/// Reads and writes Discord's shape, `{"id": "...", "type": "user"}`. The
/// same type is used for received components ([`MessageComponent`]) and for
/// [`SelectMenuBuilder`](crate::builders::SelectMenuBuilder) defaults, so a
/// builder takes a [`UserId`], [`RoleId`], or [`ChannelId`] rather than a
/// string. A `type` this version does not know (or a missing one, read as
/// `""`) is kept in [`SelectDefaultValue::Unknown`] and written back as sent.
///
/// ```
/// use discordrs::{SelectDefaultValue, UserId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let user_id: UserId = "80351110224678912".parse()?;
/// let value = SelectDefaultValue::user(user_id);
/// assert_eq!(
///     serde_json::to_string(&value)?,
///     r#"{"id":"80351110224678912","type":"user"}"#
/// );
/// assert_eq!(serde_json::from_str::<SelectDefaultValue>(r#"{"id":"80351110224678912","type":"user"}"#)?, value);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "SelectDefaultValueWire")]
pub enum SelectDefaultValue {
    /// `"user"`.
    User(UserId),
    /// `"role"`.
    Role(RoleId),
    /// `"channel"`.
    Channel(ChannelId),
    /// A `type` this version of discord.rs does not know, kept as sent.
    Unknown {
        /// The raw `type` string.
        kind: String,
        /// The `id`, whose kind is unknown.
        id: GenericId,
    },
}

impl SelectDefaultValue {
    /// A `user` default value.
    pub const fn user(id: UserId) -> Self {
        Self::User(id)
    }

    /// A `role` default value.
    pub const fn role(id: RoleId) -> Self {
        Self::Role(id)
    }

    /// A `channel` default value.
    pub const fn channel(id: ChannelId) -> Self {
        Self::Channel(id)
    }

    /// The wire `id`.
    pub const fn id(&self) -> GenericId {
        match self {
            Self::User(id) => id.erase(),
            Self::Role(id) => id.erase(),
            Self::Channel(id) => id.erase(),
            Self::Unknown { id, .. } => *id,
        }
    }

    /// The wire `type`: `"user"`, `"role"`, `"channel"`, or the unknown
    /// string as sent.
    pub fn kind(&self) -> &str {
        match self {
            Self::User(_) => "user",
            Self::Role(_) => "role",
            Self::Channel(_) => "channel",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl From<MentionableId> for SelectDefaultValue {
    fn from(id: MentionableId) -> Self {
        match id {
            MentionableId::User(id) => Self::User(id),
            MentionableId::Role(id) => Self::Role(id),
        }
    }
}

impl Serialize for SelectDefaultValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            id: GenericId,
            #[serde(rename = "type")]
            kind: &'a str,
        }

        Wire {
            id: self.id(),
            kind: self.kind(),
        }
        .serialize(serializer)
    }
}

/// The wire shape of [`SelectDefaultValue`], unchanged from 2.x.
#[derive(Deserialize)]
struct SelectDefaultValueWire {
    id: GenericId,
    #[serde(rename = "type", default)]
    kind: String,
}

impl From<SelectDefaultValueWire> for SelectDefaultValue {
    fn from(wire: SelectDefaultValueWire) -> Self {
        match wire.kind.as_str() {
            "user" => Self::User(wire.id.cast()),
            "role" => Self::Role(wire.id.cast()),
            "channel" => Self::Channel(wire.id.cast()),
            _ => Self::Unknown {
                kind: wire.kind,
                id: wire.id,
            },
        }
    }
}

/// The 2.x name of [`SelectDefaultValue`] for received components.
#[deprecated(
    since = "3.0.0",
    note = "received and builder select defaults are one type now: use SelectDefaultValue"
)]
pub type MessageSelectDefaultValue = SelectDefaultValue;

/// The ID of something a mentionable select or `MENTIONABLE` command option
/// can pick: a user or a role.
///
/// A mentionable option value is a bare snowflake; which kind it is shows
/// only in the interaction's resolved data, so
/// [`ResolvedData::mentionable`](super::ResolvedData::mentionable) builds
/// this from a value and the resolved maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MentionableId {
    /// A user.
    User(UserId),
    /// A role.
    Role(RoleId),
}

impl MentionableId {
    /// The snowflake with its kind erased.
    pub const fn id(self) -> GenericId {
        match self {
            Self::User(id) => id.erase(),
            Self::Role(id) => id.erase(),
        }
    }
}

impl From<UserId> for MentionableId {
    fn from(id: UserId) -> Self {
        Self::User(id)
    }
}

impl From<RoleId> for MentionableId {
    fn from(id: RoleId) -> Self {
        Self::Role(id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Media reference attached to a Components V2 component.
pub struct UnfurledMediaItem {
    #[serde(default)]
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<AttachmentId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Single item of a media gallery component on a received message.
pub struct MessageMediaGalleryItem {
    #[serde(default)]
    pub media: UnfurledMediaItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
}
