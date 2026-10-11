//! Discord's wire format for [`Interaction`], [`CommandInteractionData`],
//! and [`AuthorizingIntegrationOwners`].
//!
//! The decoder reads an interaction object once, field by field, straight
//! into the typed model: no `serde_json::Value` tree, no copy of `data` or
//! `resolved`. The interaction `type` decides how `data` is read, and JSON
//! does not order keys, so:
//!
//! - `type` before `data` (what Discord's gateway sends): `data` of a
//!   component, autocomplete, or modal submit interaction is decoded in
//!   place; `data` of an unknown interaction type is kept as JSON text;
//! - `data` of an application command is always kept as JSON text first and
//!   decoded once the object ends, because its variant depends on the
//!   command `type` inside it: an unknown command type keeps the bytes as
//!   received, whatever the key order;
//! - `data` before `type` (any other producer, or a sorted
//!   `serde_json::Value`): `data` is kept as JSON text and decoded once the
//!   object ends.
//!
//! Keeping `data` as text costs one copy of it and a second scan, and needs
//! a serde_json deserializer (text, slice, `Value`, or `RawValue`; not
//! serde's buffered `Content` inside `#[serde(flatten)]` or `untagged`).
//! Like the rest of the crate's models, JSON is the only supported format.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: INTERACTION_CREATE를 `&str`/`&RawValue`에서 바로 타입으로 디코딩한다.
//!   기준선(main 706b7ae)은 3.9 KB 페이로드에 decode_event 652 allocs였다(`data` 통째 복제 후
//!   `resolved` 재복제). 목표는 3배 이상 감소.
//! - 기존 구현 및 제약 조건: 파서는 `&Value`를 받아 member·data·resolved·options를 복제한 뒤
//!   `from_value`로 다시 디코딩했다. `data`의 모양은 `type`(명령은 `data.type`까지)에 달렸고
//!   키 순서는 보장되지 않는다. 알 수 없는 인터랙션·명령 타입은 data를 잃지 않아야 한다.
//!   런타임 작업자의 RawValue 봉투가 `d.get()`으로 이 경로를 부른다.
//! - 검토한 주요 대안: (1) `data`를 항상 RawValue로 잡고 끝에서 디코딩, (2) 모든 종류의 data
//!   필드를 합친 구조체 하나로 읽고 끝에서 나누기, (3) `type`이 먼저 오면 바로 디코딩하고
//!   아니면 (1)로 대체.
//! - 선택한 방식: (3). 단 애플리케이션 명령 data는 키 순서와 무관하게 항상 RawValue로 먼저
//!   잡고, 객체가 끝나면 명령 타입을 보고 타입 모델로 디코딩하거나 받은 바이트 그대로 둔다
//!   (2026-10-11 리뷰: 처음에는 모르는 키만 모아 다시 조립했는데, `type`이 먼저 오면 중첩된
//!   모르는 필드·명시적 null·빈 배열을 잃고, 객체 옵션 값이 있으면 Unknown 대신 오류가 났다).
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 모든 인터랙션에 data 크기 복사와 두 번째
//!   스캔을 더한다. (2)는 알 수 없는 인터랙션 타입의 data를 아는 필드 모양으로 읽다가 실패할
//!   수 있고 모르는 키를 잃는다. 게이트웨이는 `type`을 `data`보다 먼저 보내므로 (3)의 흔한
//!   경로는 복사 없이 한 번 읽는다.
//! - 장점, 단점 및 영향: `Interaction`의 serde가 2.x의 외부 태그 형식에서 Discord 와이어
//!   형식으로 바뀐다. 명령 인터랙션과 `data`가 먼저 오는 입력은 data 복사 1회(할당 1)와
//!   재스캔이 든다. RawValue는 serde의 Content 버퍼(untagged·flatten 안)를 통과하지 못한다.
//! ```

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::bitfield::Permissions;
use crate::model::{
    ApplicationId, AuthorizingIntegrationOwners, AutocompleteInteraction, Channel, ChannelId,
    ChatInputCommandInteraction, CommandInteractionData, CommandType, ComponentInteraction,
    ComponentInteractionData, Entitlement, GuildId, GuildInstallOwner, Interaction,
    InteractionContextData, InteractionContextType, InteractionGuild, InteractionId,
    InteractionResponseState, InteractionType, Member, Message, MessageContextMenuInteraction,
    ModalSubmitInteraction, PingInteraction, ResolvedData, User, UserContextMenuInteraction,
    UserId,
};
use crate::parsers::modal::{ModalDataRef, ModalDataSeed};
use crate::parsers::V2ModalSubmission;

// ---------------------------------------------------------------------------
// Interaction
// ---------------------------------------------------------------------------

impl<'de> Deserialize<'de> for Interaction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(InteractionVisitor)
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum InteractionField {
    Id,
    ApplicationId,
    Type,
    Data,
    Token,
    GuildId,
    ChannelId,
    User,
    Member,
    AppPermissions,
    Locale,
    GuildLocale,
    Entitlements,
    Context,
    AuthorizingIntegrationOwners,
    Guild,
    Channel,
    Message,
    AttachmentSizeLimit,
    Version,
    // Undocumented keys, such as the deprecated `entitlement_sku_ids`.
    #[serde(other)]
    Other,
}

/// `data` as far as the decoder got before the object ended.
enum Data {
    /// Application command `data` of a command type with its own variant.
    Command(CommandKind, CommandInteractionData),
    /// Autocomplete `data` (any command type).
    Autocomplete(CommandInteractionData),
    Component(ComponentInteractionData),
    Modal(V2ModalSubmission, Option<ResolvedData>),
    /// Kept as text: an unknown interaction type, or `data` came before
    /// `type`.
    Raw(Box<RawValue>),
    /// `data` of a PING, which has none to keep.
    Ignored,
}

/// Reads `data` the way interaction `type` says.
struct DataSeed {
    kind: InteractionType,
}

impl<'de> DeserializeSeed<'de> for DataSeed {
    type Value = Data;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Data, D::Error> {
        match self.kind {
            InteractionType::PING => IgnoredAny::deserialize(deserializer).map(|_| Data::Ignored),
            // The variant depends on the command type inside `data`, and an
            // unknown command type must keep `data` exactly as sent, so the
            // text is kept and decoded when the object ends.
            InteractionType::APPLICATION_COMMAND => {
                Box::<RawValue>::deserialize(deserializer).map(Data::Raw)
            }
            InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE => {
                CommandInteractionData::deserialize(deserializer).map(Data::Autocomplete)
            }
            InteractionType::MESSAGE_COMPONENT => {
                ComponentInteractionData::deserialize(deserializer).map(Data::Component)
            }
            InteractionType::MODAL_SUBMIT => ModalDataSeed {
                keep_resolved: true,
            }
            .deserialize(deserializer)
            .map(|(submission, resolved)| Data::Modal(submission, resolved)),
            _ => Box::<RawValue>::deserialize(deserializer).map(Data::Raw),
        }
    }
}

struct InteractionVisitor;

impl<'de> Visitor<'de> for InteractionVisitor {
    type Value = Interaction;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an interaction object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Interaction, A::Error> {
        let mut id: Option<InteractionId> = None;
        let mut application_id: Option<ApplicationId> = None;
        let mut kind: Option<InteractionType> = None;
        let mut data: Option<Data> = None;
        let mut token: Option<String> = None;
        let mut guild_id: Option<Option<GuildId>> = None;
        let mut channel_id: Option<Option<ChannelId>> = None;
        let mut user: Option<Option<User>> = None;
        let mut member: Option<Option<Member>> = None;
        let mut app_permissions: Option<Option<Permissions>> = None;
        let mut locale: Option<Option<String>> = None;
        let mut guild_locale: Option<Option<String>> = None;
        let mut entitlements: Option<Option<Vec<Entitlement>>> = None;
        let mut context: Option<Option<InteractionContextType>> = None;
        let mut owners: Option<Option<AuthorizingIntegrationOwners>> = None;
        let mut guild: Option<Option<Box<InteractionGuild>>> = None;
        let mut channel: Option<Option<Box<Channel>>> = None;
        let mut message: Option<Option<Box<Message>>> = None;
        let mut attachment_size_limit: Option<Option<u64>> = None;
        let mut version: Option<Option<u8>> = None;

        while let Some(field) = map.next_key::<InteractionField>()? {
            match field {
                InteractionField::Id => set_once(&mut id, "id", &mut map)?,
                InteractionField::ApplicationId => {
                    set_once(&mut application_id, "application_id", &mut map)?;
                }
                InteractionField::Type => set_once(&mut kind, "type", &mut map)?,
                InteractionField::Data => {
                    if data.is_some() {
                        return Err(de::Error::duplicate_field("data"));
                    }
                    data = Some(match kind {
                        Some(kind) => map.next_value_seed(DataSeed { kind }).map_err(|error| {
                            de::Error::custom(format_args!("invalid interaction.data: {error}"))
                        })?,
                        // `type` has not been read yet: keep the text and
                        // decode it once the object ends.
                        None => Data::Raw(map.next_value()?),
                    });
                }
                InteractionField::Token => set_once(&mut token, "token", &mut map)?,
                InteractionField::GuildId => set_once(&mut guild_id, "guild_id", &mut map)?,
                InteractionField::ChannelId => {
                    set_once(&mut channel_id, "channel_id", &mut map)?;
                }
                InteractionField::User => set_once(&mut user, "user", &mut map)?,
                InteractionField::Member => set_once(&mut member, "member", &mut map)?,
                InteractionField::AppPermissions => {
                    set_once(&mut app_permissions, "app_permissions", &mut map)?;
                }
                InteractionField::Locale => set_once(&mut locale, "locale", &mut map)?,
                InteractionField::GuildLocale => {
                    set_once(&mut guild_locale, "guild_locale", &mut map)?;
                }
                InteractionField::Entitlements => {
                    set_once(&mut entitlements, "entitlements", &mut map)?;
                }
                InteractionField::Context => set_once(&mut context, "context", &mut map)?,
                InteractionField::AuthorizingIntegrationOwners => {
                    set_once(&mut owners, "authorizing_integration_owners", &mut map)?;
                }
                InteractionField::Guild => set_once(&mut guild, "guild", &mut map)?,
                InteractionField::Channel => set_once(&mut channel, "channel", &mut map)?,
                InteractionField::Message => set_once(&mut message, "message", &mut map)?,
                InteractionField::AttachmentSizeLimit => {
                    set_once(
                        &mut attachment_size_limit,
                        "attachment_size_limit",
                        &mut map,
                    )?;
                }
                InteractionField::Version => set_once(&mut version, "version", &mut map)?,
                InteractionField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }

        let kind = kind.ok_or_else(|| de::Error::custom("missing or invalid interaction.type"))?;
        let member = member.flatten();
        // Discord sends `member` in guilds and `user` elsewhere; `user` is
        // filled from `member.user` so handlers find the invoker in one place.
        let user = user
            .flatten()
            .or_else(|| member.as_ref().and_then(|member| member.user.clone()));
        let context = InteractionContextData {
            id: id.ok_or_else(|| de::Error::custom("missing or invalid interaction.id"))?,
            application_id: application_id.ok_or_else(|| {
                de::Error::custom("missing or invalid interaction.application_id")
            })?,
            token: token
                .ok_or_else(|| de::Error::custom("missing or invalid interaction.token"))?,
            guild_id: guild_id.flatten(),
            guild: guild.flatten(),
            channel_id: channel_id.flatten(),
            channel: channel.flatten(),
            user,
            member,
            app_permissions: app_permissions.flatten(),
            locale: locale.flatten(),
            guild_locale: guild_locale.flatten(),
            entitlements: entitlements.flatten(),
            context: context.flatten(),
            authorizing_integration_owners: owners.flatten(),
            message: message.flatten(),
            attachment_size_limit: attachment_size_limit.flatten(),
            version: version.flatten(),
            response_state: InteractionResponseState::new(),
        };
        build_interaction(context, kind, data).map_err(de::Error::custom)
    }
}

/// Reads the next value into `slot`, rejecting a second occurrence of the
/// key like a serde derive does.
fn set_once<'de, T, A>(
    slot: &mut Option<T>,
    field: &'static str,
    map: &mut A,
) -> Result<(), A::Error>
where
    T: Deserialize<'de>,
    A: MapAccess<'de>,
{
    if slot.is_some() {
        return Err(de::Error::duplicate_field(field));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}

/// Picks the variant once the whole object is read. Errors are plain text
/// so the caller can wrap them in its deserializer's error type.
fn build_interaction(
    context: InteractionContextData,
    kind: InteractionType,
    data: Option<Data>,
) -> Result<Interaction, String> {
    let data = match data {
        // `data` kept as text (an application command, or `data` came
        // before `type`): decode it now.
        Some(Data::Raw(raw)) if is_typed(kind) => Some(decode_late(kind, raw)?),
        data => data,
    };
    let missing_data = || String::from("missing interaction.data");
    Ok(match kind {
        InteractionType::PING => Interaction::Ping(PingInteraction { context }),
        InteractionType::APPLICATION_COMMAND => match data {
            Some(Data::Command(command_kind, data)) => {
                command_interaction(context, command_kind, data)
            }
            Some(Data::Raw(raw)) => Interaction::Unknown {
                context,
                kind,
                raw_data: Some(raw),
            },
            _ => return Err(missing_data()),
        },
        InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE => match data {
            Some(Data::Autocomplete(data)) => {
                Interaction::Autocomplete(AutocompleteInteraction { context, data })
            }
            _ => return Err(missing_data()),
        },
        InteractionType::MESSAGE_COMPONENT => match data {
            Some(Data::Component(data)) => {
                Interaction::Component(ComponentInteraction { context, data })
            }
            _ => return Err(missing_data()),
        },
        InteractionType::MODAL_SUBMIT => match data {
            Some(Data::Modal(submission, resolved)) => {
                Interaction::ModalSubmit(ModalSubmitInteraction {
                    context,
                    submission,
                    resolved,
                })
            }
            _ => return Err(missing_data()),
        },
        kind => Interaction::Unknown {
            context,
            kind,
            raw_data: match data {
                Some(Data::Raw(raw)) if raw.get() != "null" => Some(raw),
                _ => None,
            },
        },
    })
}

/// Interaction types whose `data` has a typed model.
fn is_typed(kind: InteractionType) -> bool {
    matches!(
        kind,
        InteractionType::PING
            | InteractionType::APPLICATION_COMMAND
            | InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE
            | InteractionType::MESSAGE_COMPONENT
            | InteractionType::MODAL_SUBMIT
    )
}

/// Decodes `data` kept as text. An application command of an unknown
/// command type keeps that text as its `raw_data`, byte for byte, and is
/// never forced through the typed model: a future command type may use
/// shapes it rejects (an object option value, say).
fn decode_late(kind: InteractionType, raw: Box<RawValue>) -> Result<Data, String> {
    let invalid = |error: serde_json::Error| format!("invalid interaction.data: {error}");
    if kind != InteractionType::APPLICATION_COMMAND {
        return DataSeed { kind }.deserialize(&*raw).map_err(invalid);
    }
    match CommandInteractionData::deserialize(&*raw) {
        Ok(data) => Ok(match CommandKind::of(data.kind) {
            Some(command_kind) => Data::Command(command_kind, data),
            None => Data::Raw(raw),
        }),
        // The typed model failed: read only `type`, and keep the text when
        // the command type is one this version does not type.
        Err(error) => match command_type_of(&raw) {
            Some(kind) if CommandKind::of(kind).is_none() => Ok(Data::Raw(raw)),
            _ => Err(invalid(error)),
        },
    }
}

/// The command `type` of command `data`, skipping every other key without
/// allocating; `None` when `data` is not an object with a readable `type`,
/// `Some(None)` when it has no `type`.
fn command_type_of(raw: &RawValue) -> Option<Option<CommandType>> {
    #[derive(Deserialize)]
    struct CommandHeader {
        #[serde(rename = "type")]
        kind: Option<CommandType>,
    }
    CommandHeader::deserialize(raw)
        .ok()
        .map(|header| header.kind)
}

/// The command types with their own [`Interaction`] variant.
#[derive(Clone, Copy)]
enum CommandKind {
    ChatInput,
    User,
    Message,
}

impl CommandKind {
    /// `None` for a command type this version does not type. Discord treats
    /// an absent command `type` as `CHAT_INPUT`.
    fn of(kind: Option<CommandType>) -> Option<Self> {
        match kind.unwrap_or(CommandType::CHAT_INPUT) {
            CommandType::CHAT_INPUT => Some(Self::ChatInput),
            CommandType::USER => Some(Self::User),
            CommandType::MESSAGE => Some(Self::Message),
            _ => None,
        }
    }
}

fn command_interaction(
    context: InteractionContextData,
    command_kind: CommandKind,
    data: CommandInteractionData,
) -> Interaction {
    match command_kind {
        CommandKind::ChatInput => {
            Interaction::ChatInputCommand(ChatInputCommandInteraction { context, data })
        }
        CommandKind::User => {
            Interaction::UserContextMenu(UserContextMenuInteraction { context, data })
        }
        CommandKind::Message => {
            Interaction::MessageContextMenu(MessageContextMenuInteraction { context, data })
        }
    }
}

/// Writes the interaction as Discord sends it: the context fields, `type`,
/// and `data`. `user` is left out when `member.user` carries the same user,
/// as on the wire (the decoder copies it for convenience).
impl Serialize for Interaction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (context, kind, data) = match self {
            Interaction::Ping(interaction) => (&interaction.context, InteractionType::PING, None),
            Interaction::ChatInputCommand(interaction) => (
                &interaction.context,
                InteractionType::APPLICATION_COMMAND,
                Some(DataRef::Command(&interaction.data)),
            ),
            Interaction::UserContextMenu(interaction) => (
                &interaction.context,
                InteractionType::APPLICATION_COMMAND,
                Some(DataRef::Command(&interaction.data)),
            ),
            Interaction::MessageContextMenu(interaction) => (
                &interaction.context,
                InteractionType::APPLICATION_COMMAND,
                Some(DataRef::Command(&interaction.data)),
            ),
            Interaction::Autocomplete(interaction) => (
                &interaction.context,
                InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE,
                Some(DataRef::Command(&interaction.data)),
            ),
            Interaction::Component(interaction) => (
                &interaction.context,
                InteractionType::MESSAGE_COMPONENT,
                Some(DataRef::Component(&interaction.data)),
            ),
            Interaction::ModalSubmit(interaction) => (
                &interaction.context,
                InteractionType::MODAL_SUBMIT,
                Some(DataRef::Modal(ModalDataRef {
                    submission: &interaction.submission,
                    resolved: interaction.resolved.as_ref(),
                })),
            ),
            Interaction::Unknown {
                context,
                kind,
                raw_data,
            } => (context, *kind, raw_data.as_deref().map(DataRef::Raw)),
        };

        let mut map = serializer.serialize_map(None)?;
        let InteractionContextData {
            id,
            application_id,
            token,
            guild_id,
            guild,
            channel_id,
            channel,
            user,
            member,
            app_permissions,
            locale,
            guild_locale,
            entitlements,
            context: context_type,
            authorizing_integration_owners,
            message,
            attachment_size_limit,
            version,
            response_state: _,
        } = context;
        map.serialize_entry("id", id)?;
        map.serialize_entry("application_id", application_id)?;
        map.serialize_entry("type", &kind)?;
        if let Some(data) = &data {
            map.serialize_entry("data", data)?;
        }
        map.serialize_entry("token", token)?;
        entry_if_some(&mut map, "guild_id", guild_id)?;
        entry_if_some(&mut map, "guild", guild)?;
        entry_if_some(&mut map, "channel_id", channel_id)?;
        entry_if_some(&mut map, "channel", channel)?;
        let member_user = member.as_ref().and_then(|member| member.user.as_ref());
        if member_user.is_none() {
            entry_if_some(&mut map, "user", user)?;
        }
        entry_if_some(&mut map, "member", member)?;
        entry_if_some(&mut map, "app_permissions", app_permissions)?;
        entry_if_some(&mut map, "locale", locale)?;
        entry_if_some(&mut map, "guild_locale", guild_locale)?;
        entry_if_some(&mut map, "entitlements", entitlements)?;
        entry_if_some(&mut map, "context", context_type)?;
        entry_if_some(
            &mut map,
            "authorizing_integration_owners",
            authorizing_integration_owners,
        )?;
        entry_if_some(&mut map, "message", message)?;
        entry_if_some(&mut map, "attachment_size_limit", attachment_size_limit)?;
        entry_if_some(&mut map, "version", version)?;
        map.end()
    }
}

fn entry_if_some<M: SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Option<T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(key, value),
        None => Ok(()),
    }
}

enum DataRef<'a> {
    Command(&'a CommandInteractionData),
    Component(&'a ComponentInteractionData),
    Modal(ModalDataRef<'a>),
    Raw(&'a RawValue),
}

impl Serialize for DataRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            DataRef::Command(data) => data.serialize(serializer),
            DataRef::Component(data) => data.serialize(serializer),
            DataRef::Modal(data) => data.serialize(serializer),
            DataRef::Raw(raw) => raw.serialize(serializer),
        }
    }
}

// ---------------------------------------------------------------------------
// authorizing_integration_owners
// ---------------------------------------------------------------------------

/// An object key, borrowed from the input when the deserializer can lend it
/// (serde's own `Cow<str>` always copies).
struct KeyName<'de>(Cow<'de, str>);

impl<'de> Deserialize<'de> for KeyName<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyNameVisitor;

        impl<'de> Visitor<'de> for KeyNameVisitor {
            type Value = KeyName<'de>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object key")
            }

            fn visit_borrowed_str<E: de::Error>(self, key: &'de str) -> Result<Self::Value, E> {
                Ok(KeyName(Cow::Borrowed(key)))
            }

            fn visit_str<E: de::Error>(self, key: &str) -> Result<Self::Value, E> {
                Ok(KeyName(Cow::Owned(key.to_owned())))
            }

            fn visit_string<E: de::Error>(self, key: String) -> Result<Self::Value, E> {
                Ok(KeyName(Cow::Owned(key)))
            }
        }

        deserializer.deserialize_identifier(KeyNameVisitor)
    }
}

impl Serialize for AuthorizingIntegrationOwners {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_if_some(&mut map, "0", &self.guild_install)?;
        entry_if_some(&mut map, "1", &self.user_install)?;
        for (key, value) in &self.other {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for AuthorizingIntegrationOwners {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OwnersVisitor;

        impl<'de> Visitor<'de> for OwnersVisitor {
            type Value = AuthorizingIntegrationOwners;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an authorizing_integration_owners object")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<AuthorizingIntegrationOwners, A::Error> {
                let mut guild_install: Option<Option<GuildInstallOwner>> = None;
                let mut user_install: Option<Option<UserId>> = None;
                let mut other = BTreeMap::new();
                while let Some(KeyName(key)) = map.next_key::<KeyName<'de>>()? {
                    match &*key {
                        "0" => set_once(&mut guild_install, "0", &mut map)?,
                        "1" => set_once(&mut user_install, "1", &mut map)?,
                        _ => {
                            let value: Box<RawValue> = map.next_value()?;
                            if other.insert(key.into_owned(), value).is_some() {
                                return Err(de::Error::custom(
                                    "duplicate authorizing_integration_owners key",
                                ));
                            }
                        }
                    }
                }
                Ok(AuthorizingIntegrationOwners {
                    guild_install: guild_install.flatten(),
                    user_install: user_install.flatten(),
                    other,
                })
            }
        }

        deserializer.deserialize_map(OwnersVisitor)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::value::RawValue;
    use serde_json::{json, Value};

    use crate::model::{CommandType, Interaction, InteractionType};
    use crate::parsers::decode_interaction;

    fn raw_json(interaction: &Interaction) -> Value {
        match interaction {
            Interaction::Unknown {
                raw_data: Some(raw),
                ..
            } => serde_json::from_str(raw.get()).unwrap(),
            other => panic!("expected raw data, got {other:?}"),
        }
    }

    const CONTEXT: &str = r#""id":"1","application_id":"2","token":"t""#;

    #[test]
    fn unknown_interaction_types_keep_data_in_either_key_order() {
        let data = r#"{"future":[1,{"x":null}],"name":"n"}"#;
        for payload in [
            format!(r#"{{{CONTEXT},"type":42,"data":{data}}}"#),
            format!(r#"{{"data":{data},{CONTEXT},"type":42}}"#),
        ] {
            let interaction = decode_interaction(&payload).unwrap();
            let Interaction::Unknown { kind, raw_data, .. } = &interaction else {
                panic!("{interaction:?}");
            };
            assert_eq!(*kind, InteractionType::new(42));
            // Kept byte for byte.
            assert_eq!(raw_data.as_deref().map(|raw| raw.get()), Some(data));
            let written = serde_json::to_value(&interaction).unwrap();
            assert_eq!(written["type"], json!(42));
            assert_eq!(
                written["data"],
                serde_json::from_str::<Value>(data).unwrap()
            );
        }
        let without_data = decode_interaction(&format!(r#"{{{CONTEXT},"type":42}}"#)).unwrap();
        assert!(matches!(
            without_data,
            Interaction::Unknown { raw_data: None, .. }
        ));
    }

    #[test]
    fn unknown_command_types_keep_data_byte_for_byte_in_any_key_order() {
        // Nested unknown fields, explicit nulls, empty arrays, and an
        // option whose value is an object (which `CommandOptionValue`
        // rejects): none of it may be lost or turned into an error.
        let data_type_first = r#"{"type":99,"id":"5","name":"launch","resolved":{"future":{"x":1},"users":{}},"options":[{"type":3,"name":"o","value":{"k":[1,null]}},{"type":42,"name":"p","value":null,"options":[]}],"future_key":{"nested":[true,null]},"guild_id":null,"empty":[]}"#;
        let data_type_last = r#"{"id":"5","options":[{"type":3,"name":"o","value":{"k":[1,null]}}],"resolved":{"future":{"x":1}},"empty":[],"type":99}"#;
        for data in [data_type_first, data_type_last] {
            for payload in [
                format!(r#"{{{CONTEXT},"type":2,"data":{data}}}"#),
                format!(r#"{{"data":{data},{CONTEXT},"type":2}}"#),
                format!(r#"{{"type":2,"data":{data},{CONTEXT}}}"#),
            ] {
                let interaction = decode_interaction(&payload).unwrap();
                let Interaction::Unknown { kind, raw_data, .. } = &interaction else {
                    panic!("{payload}: {interaction:?}");
                };
                assert_eq!(*kind, InteractionType::APPLICATION_COMMAND);
                assert_eq!(
                    raw_data.as_deref().map(RawValue::get),
                    Some(data),
                    "{payload}"
                );
                // Written back unchanged, and read back the same way.
                let written = serde_json::to_string(&interaction).unwrap();
                assert!(written.contains(data), "{written}");
                let again = decode_interaction(&written).unwrap();
                assert_eq!(serde_json::to_string(&again).unwrap(), written);
            }
        }

        // The same through a `Value` (sorted keys, `data` first).
        let value: Value = serde_json::from_str(&format!(
            r#"{{{CONTEXT},"type":2,"data":{data_type_first}}}"#
        ))
        .unwrap();
        let from_value = crate::parsers::parse_interaction(&value).unwrap();
        assert_eq!(
            raw_json(&from_value),
            serde_json::from_str::<Value>(data_type_first).unwrap()
        );
    }

    #[test]
    fn known_command_types_decode_typed_in_any_key_order() {
        let data = r#"{"future_key":1,"options":[{"type":4,"name":"n","value":3}],"type":1,"name":"ping"}"#;
        for payload in [
            format!(r#"{{{CONTEXT},"type":2,"data":{data}}}"#),
            format!(r#"{{"data":{data},{CONTEXT},"type":2}}"#),
        ] {
            let interaction = decode_interaction(&payload).unwrap();
            let Interaction::ChatInputCommand(command) = interaction else {
                panic!("{interaction:?}");
            };
            assert_eq!(command.data.kind, Some(CommandType::CHAT_INPUT));
            assert_eq!(
                command.data.options[0]
                    .value
                    .as_ref()
                    .and_then(|v| v.as_i64()),
                Some(3)
            );
        }
        // A known command type is held to the typed model: an object option
        // value is an error, not an unknown interaction.
        let error = decode_interaction(&format!(
            r#"{{{CONTEXT},"type":2,"data":{{"type":1,"name":"x","options":[{{"type":3,"name":"o","value":{{}}}}]}}}}"#
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid interaction.data"), "{error}");
        // A command type that is not a number is an error too.
        assert!(decode_interaction(&format!(
            r#"{{{CONTEXT},"type":2,"data":{{"type":"1","name":"x"}}}}"#
        ))
        .is_err());
    }

    #[test]
    fn decoder_errors_name_the_field() {
        let error = |payload: &str| decode_interaction(payload).unwrap_err().to_string();
        assert!(error(r#"{"id":"1","application_id":"2","token":"t"}"#)
            .contains("missing or invalid interaction.type"));
        assert!(error(r#"{"application_id":"2","token":"t","type":1}"#)
            .contains("missing or invalid interaction.id"));
        assert!(error(r#"{"id":"1","token":"t","type":1}"#)
            .contains("missing or invalid interaction.application_id"));
        assert!(error(r#"{"id":"1","application_id":"2","type":1}"#)
            .contains("missing or invalid interaction.token"));
        assert!(error(&format!(r#"{{{CONTEXT},"type":2}}"#)).contains("missing interaction.data"));
        assert!(error(&format!(r#"{{{CONTEXT},"type":1,"type":1}}"#)).contains("duplicate field"));
        assert!(error(&format!(
            r#"{{{CONTEXT},"type":2,"data":{{"name":"a","name":"b"}}}}"#
        ))
        .contains("duplicate field"));
        assert!(error(&format!(r#"{{{CONTEXT},"type":2,"data":null}}"#))
            .contains("invalid interaction.data"));
        // Data read late reports the same way.
        assert!(error(&format!(
            r#"{{"data":{{"component_type":2}},{CONTEXT},"type":3}}"#
        ))
        .contains("invalid interaction.data: missing field `custom_id`"));
        assert!(error(&format!(
            r#"{{{CONTEXT},"type":2,"authorizing_integration_owners":{{"7":1,"7":2}},"data":{{}}}}"#
        ))
        .contains("duplicate authorizing_integration_owners key"));
    }

    #[test]
    fn ping_data_is_ignored_and_the_invoker_is_written_once() {
        let ping =
            decode_interaction(&format!(r#"{{{CONTEXT},"type":1,"data":{{"x":1}}}}"#)).unwrap();
        assert!(matches!(ping, Interaction::Ping(_)));

        let guild = decode_interaction(&format!(
            r#"{{{CONTEXT},"type":1,"member":{{"user":{{"id":"9","username":"m"}},"roles":[],"joined_at":"2024-01-01T00:00:00+00:00"}}}}"#
        ))
        .unwrap();
        assert_eq!(
            guild.context().user.as_ref().map(|user| user.id.get()),
            Some(9)
        );
        let written = serde_json::to_value(&guild).unwrap();
        assert!(written.get("user").is_none(), "{written}");
        assert_eq!(written["member"]["user"]["id"], json!("9"));

        let dm = decode_interaction(&format!(
            r#"{{{CONTEXT},"type":1,"user":{{"id":"8","username":"u"}},"context":1}}"#
        ))
        .unwrap();
        let written = serde_json::to_value(&dm).unwrap();
        assert_eq!(written["user"]["id"], json!("8"));
        assert_eq!(written["context"], json!(1));
    }

    #[test]
    fn modal_submit_lookups_resolve_each_submitted_value() {
        let interaction = decode_interaction(&format!(
            r#"{{{CONTEXT},"type":5,"data":{{"custom_id":"m","components":[
                {{"type":18,"component":{{"type":19,"custom_id":"files","values":["11","12","13"]}}}},
                {{"type":18,"component":{{"type":7,"custom_id":"who","values":["21","22"]}}}},
                {{"type":18,"component":{{"type":4,"custom_id":"text","value":"21"}}}}
            ],"resolved":{{
                "attachments":{{
                    "11":{{"id":"11","filename":"a.png","size":1,"url":"https://cdn/a","proxy_url":"https://media/a"}},
                    "13":{{"id":"13","filename":"c.png","size":3,"url":"https://cdn/c","proxy_url":"https://media/c"}}
                }},
                "users":{{"21":{{"id":"21","username":"u"}}}},
                "roles":{{"22":{{"id":"22","name":"r","color":0,"hoist":false,"position":1,"permissions":"0","managed":false,"mentionable":false}}}}
            }}}}}}"#
        ))
        .unwrap();
        let Interaction::ModalSubmit(modal) = interaction else {
            panic!("{interaction:?}");
        };
        // "12" was not resolved, so it is skipped; the order is the upload
        // order.
        let files: Vec<_> = modal
            .get_attachments("files")
            .map(|file| file.filename.as_str())
            .collect();
        assert_eq!(files, ["a.png", "c.png"]);
        let users: Vec<_> = modal.get_users("who").map(|user| user.id.get()).collect();
        assert_eq!(users, [21]);
        let roles: Vec<_> = modal.get_roles("who").map(|role| role.id.get()).collect();
        assert_eq!(roles, [22]);
        // Text inputs and unknown IDs resolve to nothing.
        assert_eq!(modal.get_users("text").count(), 0);
        assert_eq!(modal.get_attachments("missing").count(), 0);
        assert_eq!(modal.get_channels("who").count(), 0);
        assert!(modal.resolved().is_some());

        let written = serde_json::to_value(Interaction::ModalSubmit(modal)).unwrap();
        assert_eq!(
            written["data"]["resolved"]["attachments"]["11"]["filename"],
            json!("a.png")
        );
        assert_eq!(
            written["data"]["components"][0]["values"],
            json!(["11", "12", "13"])
        );
    }
}
