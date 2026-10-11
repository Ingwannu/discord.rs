//! Discord's wire format for [`Interaction`], [`CommandInteractionData`],
//! and [`AuthorizingIntegrationOwners`].
//!
//! The decoder reads an interaction object once, field by field, straight
//! into the typed model: no `serde_json::Value` tree, no copy of `data` or
//! `resolved`. The interaction `type` decides how `data` is read, and JSON
//! does not order keys, so:
//!
//! - `type` before `data` (what Discord's gateway sends): `data` is decoded
//!   in place for the known types and kept as JSON text for unknown ones;
//! - `data` before `type` (any other producer, or a sorted
//!   `serde_json::Value`): `data` is kept as JSON text and decoded once the
//!   object ends. That costs one copy of the `data` text and a second scan,
//!   and needs a serde_json deserializer (text, slice, `Value`, or
//!   `RawValue`); like the rest of the crate's models, JSON is the only
//!   supported format.
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
//! - 선택한 방식: (3). 명령 data는 손으로 쓴 방문자가 명령 타입을 아직 모르거나 모르는
//!   타입일 때만 모르는 키를 RawValue로 모아, 알 수 없는 명령 타입이면 data를 다시 조립한다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 모든 인터랙션에 data 크기 복사와 두 번째
//!   스캔을 더한다. (2)는 알 수 없는 인터랙션 타입의 data를 아는 필드 모양으로 읽다가 실패할
//!   수 있고 모르는 키를 잃는다. 게이트웨이는 `type`을 `data`보다 먼저 보내므로 (3)의 흔한
//!   경로는 복사 없이 한 번 읽는다.
//! - 장점, 단점 및 영향: `Interaction`의 serde가 2.x의 외부 태그 형식에서 Discord 와이어
//!   형식으로 바뀐다. `data`가 먼저 오는 입력은 data 복사 1회와 재스캔이 든다. RawValue는
//!   serde의 Content 버퍼(untagged·flatten 안)를 통과하지 못한다. 알 수 없는 명령 타입의
//!   `raw_data`는 `type`이 먼저 온 경우 같은 내용의 다시 쓴 JSON이다(바이트 동일 아님).
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
    ApplicationId, AuthorizingIntegrationOwners, AutocompleteInteraction, ChannelId,
    ChatInputCommandInteraction, CommandId, CommandInteractionData, CommandInteractionOption,
    CommandType, ComponentInteraction, ComponentInteractionData, Entitlement, GenericId, GuildId,
    GuildInstallOwner, Interaction, InteractionContextData, InteractionContextType, InteractionId,
    InteractionResponseState, InteractionType, Member, MessageContextMenuInteraction,
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
    #[serde(other)]
    Other,
}

/// `data` as far as the decoder got before the object ended.
enum Data {
    Command(CommandData),
    Component(ComponentInteractionData),
    Modal(V2ModalSubmission, Option<ResolvedData>),
    /// Kept as text: an unknown interaction type, or `data` came before
    /// `type`.
    Raw(Box<RawValue>),
    /// `data` of a PING, which has none to keep.
    Ignored,
}

/// Decoded command `data`, plus the keys the typed model does not know
/// when they may still be needed to rebuild `data` for an unknown command
/// type (see [`CommandDataSeed`]).
struct CommandData {
    data: CommandInteractionData,
    unknown_keys: Option<Vec<(String, Box<RawValue>)>>,
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
            InteractionType::APPLICATION_COMMAND => CommandDataSeed { keep_unknown: true }
                .deserialize(deserializer)
                .map(Data::Command),
            InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE => CommandDataSeed {
                keep_unknown: false,
            }
            .deserialize(deserializer)
            .map(Data::Command),
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
            channel_id: channel_id.flatten(),
            user,
            member,
            app_permissions: app_permissions.flatten(),
            locale: locale.flatten(),
            guild_locale: guild_locale.flatten(),
            entitlements: entitlements.flatten(),
            context: context.flatten(),
            authorizing_integration_owners: owners.flatten(),
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
        // `data` came before `type`: decode the kept text now.
        Some(Data::Raw(raw)) if is_typed(kind) => Some(decode_late(kind, raw)?),
        data => data,
    };
    let missing_data = || String::from("missing interaction.data");
    Ok(match kind {
        InteractionType::PING => Interaction::Ping(PingInteraction { context }),
        InteractionType::APPLICATION_COMMAND => match data {
            Some(Data::Command(command)) => command_interaction(context, command)?,
            Some(Data::Raw(raw)) => Interaction::Unknown {
                context,
                kind,
                raw_data: Some(raw),
            },
            _ => return Err(missing_data()),
        },
        InteractionType::APPLICATION_COMMAND_AUTOCOMPLETE => match data {
            Some(Data::Command(command)) => Interaction::Autocomplete(AutocompleteInteraction {
                context,
                data: command.data,
            }),
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

/// Decodes `data` kept as text because it came before `type`. An
/// application command of an unknown command type keeps that text as its
/// `raw_data` unchanged.
fn decode_late(kind: InteractionType, raw: Box<RawValue>) -> Result<Data, String> {
    let invalid = |error: serde_json::Error| format!("invalid interaction.data: {error}");
    if kind != InteractionType::APPLICATION_COMMAND {
        return DataSeed { kind }.deserialize(&*raw).map_err(invalid);
    }
    let command = CommandDataSeed {
        keep_unknown: false,
    }
    .deserialize(&*raw)
    .map_err(invalid)?;
    Ok(if is_known_command_type(command.data.kind) {
        Data::Command(command)
    } else {
        Data::Raw(raw)
    })
}

/// The command types with their own variant. Discord treats an absent
/// command `type` as `CHAT_INPUT`.
fn is_known_command_type(kind: Option<CommandType>) -> bool {
    matches!(
        kind.unwrap_or(CommandType::CHAT_INPUT),
        CommandType::CHAT_INPUT | CommandType::USER | CommandType::MESSAGE
    )
}

fn command_interaction(
    context: InteractionContextData,
    command: CommandData,
) -> Result<Interaction, String> {
    let CommandData { data, unknown_keys } = command;
    Ok(match data.kind.unwrap_or(CommandType::CHAT_INPUT) {
        CommandType::CHAT_INPUT => {
            Interaction::ChatInputCommand(ChatInputCommandInteraction { context, data })
        }
        CommandType::USER => {
            Interaction::UserContextMenu(UserContextMenuInteraction { context, data })
        }
        CommandType::MESSAGE => {
            Interaction::MessageContextMenu(MessageContextMenuInteraction { context, data })
        }
        _ => Interaction::Unknown {
            context,
            kind: InteractionType::APPLICATION_COMMAND,
            raw_data: Some(
                rebuild_command_data(&data, unknown_keys.unwrap_or_default())
                    .map_err(|error| format!("invalid interaction.data: {error}"))?,
            ),
        },
    })
}

/// Writes `data` of an unknown command type back as one JSON object: the
/// documented keys from their decoded values, then the other keys as sent.
fn rebuild_command_data(
    data: &CommandInteractionData,
    unknown_keys: Vec<(String, Box<RawValue>)>,
) -> Result<Box<RawValue>, serde_json::Error> {
    let mut text = serde_json::to_string(data)?;
    if !unknown_keys.is_empty() {
        // `text` is a JSON object; reopen it to append the kept keys.
        text.truncate(text.len().saturating_sub(1));
        for (key, value) in &unknown_keys {
            if text.len() > 1 {
                text.push(',');
            }
            text.push_str(&serde_json::to_string(key)?);
            text.push(':');
            text.push_str(value.get());
        }
        text.push('}');
    }
    RawValue::from_string(text)
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
            channel_id,
            user,
            member,
            app_permissions,
            locale,
            guild_locale,
            entitlements,
            context: context_type,
            authorizing_integration_owners,
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
        entry_if_some(&mut map, "channel_id", channel_id)?;
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
// Command data
// ---------------------------------------------------------------------------

impl<'de> Deserialize<'de> for CommandInteractionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CommandDataSeed {
            keep_unknown: false,
        }
        .deserialize(deserializer)
        .map(|command| command.data)
    }
}

/// Reads command `data`. With `keep_unknown`, keys the model does not know
/// are kept as JSON text while the command type is unknown or not yet read,
/// so `data` of an unknown command type can be rebuilt without loss. Keys
/// after a known `type` are skipped without allocating; Discord sends
/// `type` first.
struct CommandDataSeed {
    keep_unknown: bool,
}

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

/// A command `data` key: a known field, or the name of another key (only
/// copied when the input cannot lend it).
enum CommandKey<'de> {
    Id,
    Name,
    Type,
    Options,
    Resolved,
    TargetId,
    GuildId,
    Other(Cow<'de, str>),
}

impl<'de> CommandKey<'de> {
    fn from_name(name: Cow<'de, str>) -> Self {
        match &*name {
            "id" => Self::Id,
            "name" => Self::Name,
            "type" => Self::Type,
            "options" => Self::Options,
            "resolved" => Self::Resolved,
            "target_id" => Self::TargetId,
            "guild_id" => Self::GuildId,
            _ => Self::Other(name),
        }
    }
}

impl<'de> Deserialize<'de> for CommandKey<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;

        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = CommandKey<'de>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a field name")
            }

            fn visit_borrowed_str<E: de::Error>(self, name: &'de str) -> Result<Self::Value, E> {
                Ok(CommandKey::from_name(Cow::Borrowed(name)))
            }

            fn visit_str<E: de::Error>(self, name: &str) -> Result<Self::Value, E> {
                // Only unknown keys keep their name; known ones match here
                // without a copy.
                Ok(match CommandKey::from_name(Cow::Borrowed(name)) {
                    CommandKey::Other(_) => CommandKey::Other(Cow::Owned(name.to_owned())),
                    CommandKey::Id => CommandKey::Id,
                    CommandKey::Name => CommandKey::Name,
                    CommandKey::Type => CommandKey::Type,
                    CommandKey::Options => CommandKey::Options,
                    CommandKey::Resolved => CommandKey::Resolved,
                    CommandKey::TargetId => CommandKey::TargetId,
                    CommandKey::GuildId => CommandKey::GuildId,
                })
            }

            fn visit_string<E: de::Error>(self, name: String) -> Result<Self::Value, E> {
                Ok(CommandKey::from_name(Cow::Owned(name)))
            }
        }

        deserializer.deserialize_identifier(KeyVisitor)
    }
}

impl<'de> DeserializeSeed<'de> for CommandDataSeed {
    type Value = CommandData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<CommandData, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for CommandDataSeed {
    type Value = CommandData;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an application command data object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<CommandData, A::Error> {
        let mut id: Option<Option<CommandId>> = None;
        let mut name: Option<Option<String>> = None;
        let mut kind: Option<Option<CommandType>> = None;
        let mut options: Option<Option<Vec<CommandInteractionOption>>> = None;
        let mut resolved: Option<Option<ResolvedData>> = None;
        let mut target_id: Option<Option<GenericId>> = None;
        let mut guild_id: Option<Option<GuildId>> = None;
        let mut unknown_keys = Vec::new();

        while let Some(key) = map.next_key::<CommandKey<'de>>()? {
            match key {
                CommandKey::Id => set_once(&mut id, "id", &mut map)?,
                CommandKey::Name => set_once(&mut name, "name", &mut map)?,
                CommandKey::Type => {
                    set_once(&mut kind, "type", &mut map)?;
                    if is_known_command_type(kind.flatten()) {
                        unknown_keys = Vec::new();
                    }
                }
                CommandKey::Options => set_once(&mut options, "options", &mut map)?,
                CommandKey::Resolved => set_once(&mut resolved, "resolved", &mut map)?,
                CommandKey::TargetId => set_once(&mut target_id, "target_id", &mut map)?,
                CommandKey::GuildId => set_once(&mut guild_id, "guild_id", &mut map)?,
                CommandKey::Other(key) => {
                    let known_type = kind.is_some_and(is_known_command_type);
                    if self.keep_unknown && !known_type {
                        unknown_keys.push((key.into_owned(), map.next_value()?));
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
            }
        }

        let data = CommandInteractionData {
            id: id.flatten(),
            name: name.flatten(),
            kind: kind.flatten(),
            options: options.flatten().unwrap_or_default(),
            resolved: resolved.flatten(),
            target_id: target_id.flatten(),
            guild_id: guild_id.flatten(),
        };
        let unknown_keys =
            (self.keep_unknown && !is_known_command_type(data.kind)).then_some(unknown_keys);
        Ok(CommandData { data, unknown_keys })
    }
}

// ---------------------------------------------------------------------------
// authorizing_integration_owners
// ---------------------------------------------------------------------------

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
    fn unknown_command_types_keep_every_data_key() {
        let data = json!({
            "type": 99,
            "id": "5",
            "name": "launch",
            "options": [{ "type": 3, "name": "o", "value": "v" }],
            "future_key": { "nested": [true] },
            "guild_id": "7"
        });
        // `type` first inside `data` (Discord's order): the documented keys
        // are re-encoded and the others kept as sent.
        let type_first = format!(
            r#"{{{CONTEXT},"type":2,"data":{{"type":99,"future_key":{{"nested":[true]}},"id":"5","name":"launch","options":[{{"type":3,"name":"o","value":"v"}}],"guild_id":"7"}}}}"#
        );
        // `type` last, so the unknown key comes before the decoder knows
        // the command type.
        let type_last = format!(
            r#"{{{CONTEXT},"type":2,"data":{{"future_key":{{"nested":[true]}},"id":"5","name":"launch","options":[{{"type":3,"name":"o","value":"v"}}],"guild_id":"7","type":99}}}}"#
        );
        let data_first = format!(r#"{{"data":{data},{CONTEXT},"type":2}}"#);
        for payload in [type_first, type_last, data_first] {
            let interaction = decode_interaction(&payload).unwrap();
            let Interaction::Unknown { kind, .. } = &interaction else {
                panic!("{interaction:?}");
            };
            assert_eq!(*kind, InteractionType::APPLICATION_COMMAND);
            assert_eq!(raw_json(&interaction), data, "{payload}");
        }

        // A known command type drops the unknown keys.
        let known = decode_interaction(&format!(
            r#"{{{CONTEXT},"type":2,"data":{{"future_key":1,"type":1,"name":"ping"}}}}"#
        ))
        .unwrap();
        let Interaction::ChatInputCommand(command) = known else {
            panic!("{known:?}");
        };
        assert_eq!(command.data.kind, Some(CommandType::CHAT_INPUT));
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
