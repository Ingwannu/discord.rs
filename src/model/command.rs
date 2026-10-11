//! Application commands, command options, and command permissions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize, Serializer};

use super::{
    ApplicationCommandHandlerType, ApplicationId, ApplicationIntegrationType, ChannelId, CommandId,
    CommandPermissionType, CommandVersionId, GenericId, GuildId, InteractionContextType, InvalidId,
    PermissionsBitField, RoleId, UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOptionChoice`.
pub struct ApplicationCommandOptionChoice {
    pub name: String,
    /// Localized `name` per locale key, 1-100 characters each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub value: serde_json::Value,
}

impl ApplicationCommandOptionChoice {
    /// Creates a `new` value.
    pub fn new(name: impl Into<String>, value: impl Serialize) -> Self {
        Self {
            name: name.into(),
            name_localizations: None,
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Creates a `try_new` value.
    pub fn try_new(
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            name_localizations: None,
            value: serde_json::to_value(value)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOption`.
pub struct ApplicationCommandOption {
    #[serde(rename = "type")]
    pub kind: u8,
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
    /// Channel types a `CHANNEL` option offers (raw channel type values).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_types: Option<Vec<u8>>,
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
    #[serde(rename = "type")]
    pub kind: u8,
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
    pub default_member_permissions: Option<PermissionsBitField>,
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
        match kind.get() {
            1 => Self::Role(id.cast()),
            2 => Self::User(id.cast()),
            3 => Self::Channel(id.cast()),
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
    /// Returns the command ID when Discord has assigned one.
    pub fn id_opt(&self) -> Option<CommandId> {
        self.id
    }

    /// Returns the creation timestamp once Discord has assigned an ID.
    pub fn created_at(&self) -> Option<u64> {
        self.id_opt().map(CommandId::timestamp)
    }
}
