//! Application commands, command options, and command permissions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize, Serializer};

use super::{
    ApplicationCommandHandlerType, ApplicationId, ApplicationIntegrationType, ChannelId,
    ChannelType, CommandId, CommandOptionType, CommandPermissionType, CommandType,
    CommandVersionId, GenericId, GuildId, InteractionContextType, InvalidId, Permissions, RoleId,
    UserId,
};

/// A choice of a string, integer, or number command option. No `Default`:
/// `name` and `value` are required.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApplicationCommandOptionChoice {
    pub name: String,
    /// Localized `name` per locale key, 1-100 characters each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    /// The choice value: a string, integer, or double matching the option
    /// type.
    pub value: CommandOptionValue,
}

impl ApplicationCommandOptionChoice {
    /// A choice named `name` with `value` (`&str`/`String`, a signed or
    /// small unsigned integer, `f64`, or `bool`).
    pub fn new(name: impl Into<String>, value: impl Into<CommandOptionValue>) -> Self {
        Self {
            name: name.into(),
            name_localizations: None,
            value: value.into(),
        }
    }

    /// A choice whose value is any `Serialize` type that writes a JSON
    /// string, number, or boolean (for example a `u64` or a string newtype);
    /// fails for anything else.
    pub fn try_new(
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            name_localizations: None,
            value: CommandOptionValue::from_serialize(&value)?,
        })
    }
}

// [Decision Log]
// - 목적과 의도: 받은 커맨드 옵션 값(`options[].value`)과 선택지 값(`choices[].value`)을
//   `serde_json::Value` 대신 Discord가 문서화한 네 가지 JSON 타입으로 표현한다.
// - 기존 구현 및 제약 조건: 2.x는 둘 다 `Value`였다. Discord 문서: 옵션 값은 "string, integer,
//   double, or boolean", 선택지 값은 "string, integer, or double". INTEGER는 -2^53..2^53,
//   NUMBER는 double이다. 숫자 옵션도 JSON에 `5`처럼 소수점 없이 올 수 있고, 자동완성 중인 옵션은
//   사용자가 친 문자열이 그대로 온다. 사용자·채널·역할·첨부 옵션은 ID를 문자열로 보낸다.
// - 검토한 주요 대안: (1) `Value` 유지, (2) 옵션 타입별 enum(User(UserId), Channel(..) ..),
//   (3) JSON 타입 enum + 타입별 접근자.
// - 선택한 방식: (3) `CommandOptionValue { String, Integer, Number, Boolean }`, `as_f64`는
//   정수도 받는다, `as_id`는 문자열 ID를 파싱한다.
// - 다른 대안 대신 이 방식을 선택한 이유: (2)는 디코딩에 옵션 `type`이 먼저 필요하고(키 순서
//   보장 없음) 자동완성의 부분 입력 문자열을 표현하지 못한다. (3)은 와이어 그대로라 손실이
//   없고 값 하나에 트리 할당이 없다(문자열만 한 번).
// - 장점, 단점 및 영향: 공개 타입 변경(`Option<Value>` → `Option<CommandOptionValue>`,
//   선택지 `Value` → `CommandOptionValue`). 정수는 i64에 들어가야 한다(Discord 한도 ±2^53).
//   문자열·숫자·불 외의 JSON은 디코딩 오류다(전에는 그대로 담겼다).
/// The value of a received command option, or of an option choice, with
/// the JSON type Discord sent.
///
/// Discord documents option values as "string, integer, double, or
/// boolean" and choice values as "string, integer, or double". User,
/// channel, role, mentionable, and attachment options carry their ID as a
/// string ([`Self::as_id`]); the option being autocompleted carries the
/// partial text the user typed, also as a string.
///
/// ```
/// use discordrs::{CommandOptionValue, UserId};
///
/// let slots: CommandOptionValue = serde_json::from_str("6").unwrap();
/// assert_eq!(slots.as_i64(), Some(6));
/// assert_eq!(slots.as_f64(), Some(6.0));
///
/// let player: CommandOptionValue = serde_json::from_str(r#""80351110224678912""#).unwrap();
/// let player_id: Option<UserId> = player.as_id();
/// assert_eq!(player_id.map(UserId::get), Some(80351110224678912));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum CommandOptionValue {
    /// A JSON string: `STRING` options, snowflake options, and the
    /// autocompleted option's partial input.
    String(String),
    /// A JSON integer (`INTEGER` options; also a `NUMBER` option sent
    /// without a fraction).
    Integer(i64),
    /// A JSON number with a fraction or exponent (`NUMBER` options).
    Number(f64),
    /// A JSON boolean (`BOOLEAN` options).
    Boolean(bool),
}

impl CommandOptionValue {
    /// The string value, for `STRING` options and snowflake options.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// The integer value of an `INTEGER` option.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The value of a `NUMBER` option, including one Discord sent as a
    /// JSON integer.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Integer(value) => Some(integer_to_f64(*value)),
            _ => None,
        }
    }

    /// The value of a `BOOLEAN` option.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// The ID of a user, channel, role, mentionable, or attachment option:
    /// the string value parsed as a snowflake. `None` for other values or
    /// text that is not a valid ID.
    pub fn as_id<M: super::IdMarker>(&self) -> Option<super::Id<M>> {
        self.as_str()?.parse().ok()
    }

    /// Converts any `Serialize` value that writes a JSON string, number, or
    /// boolean. Scalars build no JSON tree (`serde_json::Value` holds them
    /// inline).
    pub(crate) fn from_serialize(value: &impl Serialize) -> Result<Self, serde_json::Error> {
        Self::deserialize(serde_json::to_value(value)?)
    }
}

/// Discord bounds integer option values to -2^53..2^53, which `f64`
/// represents exactly; outside that range the nearest `f64` is returned.
#[expect(
    clippy::as_conversions,
    reason = "i64 to f64 has no From impl; the documented range converts exactly"
)]
fn integer_to_f64(value: i64) -> f64 {
    value as f64
}

impl Serialize for CommandOptionValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Integer(value) => serializer.serialize_i64(*value),
            Self::Number(value) => serializer.serialize_f64(*value),
            Self::Boolean(value) => serializer.serialize_bool(*value),
        }
    }
}

impl<'de> Deserialize<'de> for CommandOptionValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;

        impl serde::de::Visitor<'_> for ValueVisitor {
            type Value = CommandOptionValue;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a string, integer, number, or boolean option value")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(CommandOptionValue::String(value.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(CommandOptionValue::String(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(CommandOptionValue::Integer(value))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .map(CommandOptionValue::Integer)
                    .map_err(|_| E::custom(format!("option value {value} is out of range")))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(CommandOptionValue::Number(value))
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(CommandOptionValue::Boolean(value))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl From<String> for CommandOptionValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for CommandOptionValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<&String> for CommandOptionValue {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl From<bool> for CommandOptionValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<f64> for CommandOptionValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<f32> for CommandOptionValue {
    fn from(value: f32) -> Self {
        Self::Number(f64::from(value))
    }
}

/// Integer types that fit `i64` without loss; `u64`/`usize` go through
/// [`ApplicationCommandOptionChoice::try_new`] or an explicit conversion.
macro_rules! option_value_from_integer {
    ($($integer:ty),*) => {
        $(
            impl From<$integer> for CommandOptionValue {
                fn from(value: $integer) -> Self {
                    Self::Integer(i64::from(value))
                }
            }
        )*
    };
}

option_value_from_integer!(i8, i16, i32, i64, u8, u16, u32);

/// An application command option (or subcommand / subcommand group).
///
/// No `Default`: `type` is required and has no default (2.x defaulted to
/// `0`, which is not an option type). Use [`ApplicationCommandOption::new`]
/// or [`CommandOptionBuilder`](crate::command::CommandOptionBuilder).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplicationCommandOption {
    #[serde(rename = "type")]
    pub kind: CommandOptionType,
    pub name: String,
    /// Localized `name` per locale key; same rules as `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    /// Localized `description` per locale key, 1-100 characters each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<bool>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(default)]
    pub choices: Vec<ApplicationCommandOptionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u16>,
    /// File types an `ATTACHMENT` option accepts: preset groups such as
    /// `image`, `video`, `audio`, or dot-prefixed extensions such as `.pdf`
    /// (max 10). Discord checks only the file name, not the contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,
    /// Channel types a `CHANNEL` option offers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_types: Option<Vec<ChannelType>>,
}

impl ApplicationCommandOption {
    /// An option of `kind` with every optional field unset.
    pub fn new(
        kind: CommandOptionType,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            name_localizations: None,
            description: description.into(),
            description_localizations: None,
            required: None,
            autocomplete: None,
            options: Vec::new(),
            choices: Vec::new(),
            min_value: None,
            max_value: None,
            min_length: None,
            max_length: None,
            file_types: None,
            channel_types: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommand`.
pub struct ApplicationCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CommandId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    /// Command type. Discord documents `type` as optional with
    /// [`CommandType::CHAT_INPUT`] as its default; [`Self::command_type`]
    /// applies that default.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<CommandType>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types: Option<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<ApplicationCommandHandlerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<CommandVersionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
}

/// Who an application command permission applies to: the permission's wire
/// `id` and `type` read together, with Discord's two guild-relative
/// constants spelled out.
///
/// Discord encodes "everyone" as a role permission whose `id` is the guild
/// ID, and "all channels" as a channel permission whose `id` is the guild
/// ID minus one. Neither can be recognised from the permission alone, so a
/// lone [`ApplicationCommandPermission`] decodes them as plain
/// [`Role`](Self::Role) / [`Channel`](Self::Channel) targets.
/// [`GuildApplicationCommandPermissions`] and the
/// `APPLICATION_COMMAND_PERMISSIONS_UPDATE` event know the guild and resolve
/// them to [`Everyone`](Self::Everyone) / [`AllChannels`](Self::AllChannels)
/// (see [`Self::in_guild`]). Both spellings serialize to the same JSON.
///
/// ```
/// use discordrs::model::{CommandPermissionTarget, GuildApplicationCommandPermissions};
/// use discordrs::GuildId;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let permissions: GuildApplicationCommandPermissions = serde_json::from_str(
///     r#"{
///         "id": "1001", "application_id": "1000", "guild_id": "500",
///         "permissions": [
///             {"id": "500", "type": 1, "permission": false},
///             {"id": "499", "type": 3, "permission": false}
///         ]
///     }"#,
/// )?;
/// let guild_id: GuildId = "500".parse()?;
/// assert_eq!(permissions.permissions[0].target, CommandPermissionTarget::Everyone(guild_id));
/// assert_eq!(permissions.permissions[1].target, CommandPermissionTarget::AllChannels(guild_id));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandPermissionTarget {
    /// `type: 1`: a role.
    Role(RoleId),
    /// `type: 2`: a user.
    User(UserId),
    /// `type: 3`: a channel.
    Channel(ChannelId),
    /// Every member of the guild: `type: 1` with the guild's ID (the
    /// `@everyone` role).
    Everyone(GuildId),
    /// Every channel of the guild: `type: 3` with the guild's ID minus one.
    AllChannels(GuildId),
    /// A `type` this version of discord.rs does not know, kept as sent.
    ///
    /// [`CommandPermissionTarget::from_wire`] never produces this for the
    /// three known types.
    Unknown {
        /// The raw permission `type`.
        kind: CommandPermissionType,
        /// The permission `id`, whose kind is unknown.
        id: GenericId,
    },
}

impl CommandPermissionTarget {
    /// Interprets a wire `id` according to the permission `type`, without
    /// guild context (so never [`Everyone`](Self::Everyone) or
    /// [`AllChannels`](Self::AllChannels); see [`Self::in_guild`]).
    pub const fn from_wire(kind: CommandPermissionType, id: GenericId) -> Self {
        match kind {
            CommandPermissionType::ROLE => Self::Role(id.cast()),
            CommandPermissionType::USER => Self::User(id.cast()),
            CommandPermissionType::CHANNEL => Self::Channel(id.cast()),
            _ => Self::Unknown { kind, id },
        }
    }

    /// Resolves the guild-relative constants of `guild_id`: its `@everyone`
    /// role becomes [`Everyone`](Self::Everyone) and the channel ID
    /// `guild_id - 1` becomes [`AllChannels`](Self::AllChannels). Other
    /// targets are returned unchanged.
    pub const fn in_guild(self, guild_id: GuildId) -> Self {
        match self {
            Self::Role(role_id) if role_id.get() == guild_id.get() => Self::Everyone(guild_id),
            // A guild ID is nonzero, so `- 1` cannot underflow.
            Self::Channel(channel_id) if channel_id.get() == guild_id.get() - 1 => {
                Self::AllChannels(guild_id)
            }
            other => other,
        }
    }

    /// The wire `type`.
    pub const fn kind(self) -> CommandPermissionType {
        match self {
            Self::Role(_) | Self::Everyone(_) => CommandPermissionType::ROLE,
            Self::User(_) => CommandPermissionType::USER,
            Self::Channel(_) | Self::AllChannels(_) => CommandPermissionType::CHANNEL,
            Self::Unknown { kind, .. } => kind,
        }
    }

    /// The wire `id`.
    ///
    /// Fails only for [`AllChannels`](Self::AllChannels) of guild ID `1`,
    /// whose constant would be the invalid ID `0`.
    pub const fn id(self) -> Result<GenericId, InvalidId> {
        match self {
            Self::Role(id) => Ok(id.erase()),
            Self::User(id) => Ok(id.erase()),
            Self::Channel(id) => Ok(id.erase()),
            Self::Everyone(guild_id) => Ok(guild_id.erase()),
            Self::AllChannels(guild_id) => GenericId::try_new(guild_id.get() - 1),
            Self::Unknown { id, .. } => Ok(id),
        }
    }
}

/// A single application command permission overwrite.
///
/// Reads and writes Discord's shape, `{"id", "type", "permission"}`; `id`
/// and `type` together become [`Self::target`].
// [Decision Log]
// - 목적과 의도: 커맨드 권한의 `id`는 `type`에 따라 역할·사용자·채널 ID이고, 길드 ID(모든
//   멤버)와 길드 ID - 1(모든 채널)이라는 센티널이 있다. 이를 타입으로 드러내 잘못된 ID 종류와
//   센티널 계산 실수를 막는다(설계 §1 다형 ID, 시리즈 2 결정 "AllChannels는 전용 enum").
// - 기존 구현 및 제약 조건: A 단계는 `id: GenericId` + `kind: u8` + ROLE/USER/CHANNEL u8
//   상수였다. 센티널은 권한 하나만 보고는 알 수 없고 감싸는 객체의 guild_id가 필요하다.
//   와이어 형태는 바뀌면 안 된다.
// - 검토한 주요 대안: (1) Everyone/AllChannels를 데이터 없는 변형으로 두고 직렬화 때 길드
//   문맥을 요구, (2) 변형이 GuildId를 품어 직렬화는 문맥 없이, 역직렬화만 감싸는 객체에서
//   해석, (3) 센티널을 표현하지 않고 헬퍼 함수만 제공.
// - 선택한 방식: (2). 단독 권한은 Role/Channel로 읽고, GuildApplicationCommandPermissions와
//   APPLICATION_COMMAND_PERMISSIONS_UPDATE가 `in_guild`로 해석한다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 수정 요청 본문(길드 ID가 경로에만 있음)을
//   직렬화할 수 없다. (3)은 설계 결정(전용 enum)을 따르지 않고 `guild_id - 1` 계산을 사용자에게
//   남긴다.
// - 장점, 단점 및 영향: `AllChannels`는 길드 ID 1에서 ID 0이 되어 직렬화 오류를 낸다(패닉 없음).
//   같은 와이어 값이 문맥에 따라 Role 또는 Everyone으로 읽힌다(직렬화 결과는 같다).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "ApplicationCommandPermissionWire")]
pub struct ApplicationCommandPermission {
    /// The role, user, channel, or guild-wide constant the permission
    /// applies to (wire `id` and `type`).
    pub target: CommandPermissionTarget,
    /// Whether this target is allowed to use the command.
    pub permission: bool,
}

impl ApplicationCommandPermission {
    /// Creates a role permission overwrite.
    pub const fn role(role_id: RoleId, permission: bool) -> Self {
        Self::new(CommandPermissionTarget::Role(role_id), permission)
    }

    /// Creates a user permission overwrite.
    pub const fn user(user_id: UserId, permission: bool) -> Self {
        Self::new(CommandPermissionTarget::User(user_id), permission)
    }

    /// Creates a channel permission overwrite.
    pub const fn channel(channel_id: ChannelId, permission: bool) -> Self {
        Self::new(CommandPermissionTarget::Channel(channel_id), permission)
    }

    /// Creates a permission for every member of `guild_id` (its `@everyone`
    /// role).
    pub const fn everyone(guild_id: GuildId, permission: bool) -> Self {
        Self::new(CommandPermissionTarget::Everyone(guild_id), permission)
    }

    /// Creates a permission for every channel of `guild_id`.
    pub const fn all_channels(guild_id: GuildId, permission: bool) -> Self {
        Self::new(CommandPermissionTarget::AllChannels(guild_id), permission)
    }

    /// Creates a permission overwrite for any target.
    pub const fn new(target: CommandPermissionTarget, permission: bool) -> Self {
        Self { target, permission }
    }

    /// Resolves the guild-relative constants of `guild_id` in the target
    /// (see [`CommandPermissionTarget::in_guild`]).
    pub const fn in_guild(self, guild_id: GuildId) -> Self {
        Self::new(self.target.in_guild(guild_id), self.permission)
    }
}

/// Fails only for [`CommandPermissionTarget::AllChannels`] of guild ID `1`
/// (see [`CommandPermissionTarget::id`]).
impl Serialize for ApplicationCommandPermission {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let id = self.target.id().map_err(serde::ser::Error::custom)?;
        ApplicationCommandPermissionWire {
            id,
            kind: self.target.kind(),
            permission: self.permission,
        }
        .serialize(serializer)
    }
}

/// The wire shape of [`ApplicationCommandPermission`], unchanged from 2.x.
#[derive(Serialize, Deserialize)]
struct ApplicationCommandPermissionWire {
    id: GenericId,
    #[serde(rename = "type")]
    kind: CommandPermissionType,
    permission: bool,
}

impl From<ApplicationCommandPermissionWire> for ApplicationCommandPermission {
    fn from(wire: ApplicationCommandPermissionWire) -> Self {
        Self::new(
            CommandPermissionTarget::from_wire(wire.kind, wire.id),
            wire.permission,
        )
    }
}

/// Which commands a [`GuildApplicationCommandPermissions`] entry covers:
/// one command, or every command of the application (wire `id` equal to the
/// application ID).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandPermissionsId {
    /// Permissions of one command.
    Command(CommandId),
    /// Application-wide permissions that apply to every command without
    /// its own entry.
    Application(ApplicationId),
}

impl CommandPermissionsId {
    /// Interprets a wire `id` given the entry's `application_id`.
    pub const fn from_wire(id: GenericId, application_id: ApplicationId) -> Self {
        if id.get() == application_id.get() {
            Self::Application(application_id)
        } else {
            Self::Command(id.cast())
        }
    }

    /// The wire `id`.
    pub const fn id(self) -> GenericId {
        match self {
            Self::Command(id) => id.erase(),
            Self::Application(id) => id.erase(),
        }
    }
}

/// Permissions returned for one application command (or the whole
/// application) in one guild.
///
/// Reads and writes Discord's shape. On decode, `id` is compared with
/// `application_id` to tell command and application-wide entries apart, and
/// every permission target is resolved against `guild_id`
/// ([`CommandPermissionTarget::in_guild`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "GuildApplicationCommandPermissionsWire")]
pub struct GuildApplicationCommandPermissions {
    /// The command, or the application for application-wide permissions.
    pub id: CommandPermissionsId,
    /// Application ID that owns the command.
    pub application_id: ApplicationId,
    /// Guild ID where the permissions apply.
    pub guild_id: GuildId,
    /// Permission overwrites attached to the command.
    pub permissions: Vec<ApplicationCommandPermission>,
}

impl Serialize for GuildApplicationCommandPermissions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            id: GenericId,
            application_id: ApplicationId,
            guild_id: GuildId,
            permissions: &'a [ApplicationCommandPermission],
        }

        Wire {
            id: self.id.id(),
            application_id: self.application_id,
            guild_id: self.guild_id,
            permissions: &self.permissions,
        }
        .serialize(serializer)
    }
}

/// The wire shape of [`GuildApplicationCommandPermissions`], unchanged from
/// 2.x.
#[derive(Deserialize)]
struct GuildApplicationCommandPermissionsWire {
    id: GenericId,
    application_id: ApplicationId,
    guild_id: GuildId,
    #[serde(default)]
    permissions: Vec<ApplicationCommandPermission>,
}

impl From<GuildApplicationCommandPermissionsWire> for GuildApplicationCommandPermissions {
    fn from(wire: GuildApplicationCommandPermissionsWire) -> Self {
        let guild_id = wire.guild_id;
        Self {
            id: CommandPermissionsId::from_wire(wire.id, wire.application_id),
            application_id: wire.application_id,
            guild_id,
            permissions: wire
                .permissions
                .into_iter()
                .map(|permission| permission.in_guild(guild_id))
                .collect(),
        }
    }
}

/// Request body for editing a guild application command's permissions.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EditApplicationCommandPermissions {
    /// Permission overwrites to replace on the command.
    pub permissions: Vec<ApplicationCommandPermission>,
}

impl EditApplicationCommandPermissions {
    /// Creates a permission edit body from an iterator of overwrites.
    pub fn new(permissions: impl IntoIterator<Item = ApplicationCommandPermission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }
}

impl ApplicationCommand {
    /// The command type, applying Discord's documented default
    /// ([`CommandType::CHAT_INPUT`]) when `type` is absent.
    pub fn command_type(&self) -> CommandType {
        self.kind.unwrap_or(CommandType::CHAT_INPUT)
    }

    /// Returns the command ID when Discord has assigned one.
    pub fn id_opt(&self) -> Option<CommandId> {
        self.id
    }

    /// Returns the creation timestamp once Discord has assigned an ID.
    pub fn created_at(&self) -> Option<u64> {
        self.id_opt().map(CommandId::timestamp)
    }
}
