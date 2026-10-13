//! Typed decoding of dispatch payloads straight from the `d` JSON text.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: `d`를 `Value` 트리로 만들지 않고 타입으로 바로 읽는다(기준선 핫스팟 ②:
//!   MESSAGE_CREATE 573 allocs 중 433이 Value 파싱·복제·디코딩이었다).
//! - 기존 구현 및 제약 조건: 디코더는 `Value`를 받아 `from_value(data.clone())`로 읽고 raw를
//!   남겼다. 일부 필드는 관대하게 읽었다(형식이 틀리면 `None`, 이벤트는 그대로 전달). 이 관대함은
//!   테스트로 고정된 동작이라 유지한다. `ci/api_drift.py`는 이 파일의 `"NAME" =>` 패턴으로
//!   디코딩하는 디스패치 이름을 센다.
//! - 검토한 주요 대안: (1) 관대한 필드도 엄격하게 읽기, (2) 관대한 필드만 `Value`로 읽은 뒤 변환,
//!   (3) 관대한 필드를 빌린 `RawValue`로 건너뛴 뒤 `T`로 시도(`Lenient<T>`).
//! - 선택한 방식: (3). 엄격한 payload는 모델 타입의 derive 디코더를, 관대한 필드가 있는 핫 payload
//!   (반응, PRESENCE_UPDATE, TYPING_START)는 비공개 wire 구조체를, 드물게 오는 관대한 payload는 빌린
//!   필드 맵(`Fields`, 아래 결정 기록)을 쓴다. 이름 표는 `dispatch_table!` 하나에서 디코더 분기와 함께
//!   만든다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 필드 하나가 틀려도 이벤트 전체가 DecodeFailed가
//!   되는 동작 변경이다. (2)는 문자열마다 할당이 생긴다. (3)은 할당 없이 같은 결과를 낸다.
//!   표를 매크로 하나에서 만들면 이름 인턴 표와 분기가 어긋날 수 없다.
//! - 장점, 단점 및 영향: `Lenient<T>`는 serde_json 텍스트 입력에서만 동작하므로 wire 구조체는
//!   비공개로 둔다. 관대한 필드는 값을 두 번 훑는다(작은 필드뿐이다). 회원·통합·스레드 생성처럼
//!   한 객체에 모델과 추가 키가 섞인 payload는 같은 텍스트를 두 번 읽는다(flatten은 버퍼링한다).
//! ```

use std::borrow::Cow;
use std::sync::Arc;

use serde::de::{DeserializeOwned, DeserializeSeed};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use serde_json::Value;
use tracing::warn;

use crate::model::{
    marker, normalize_presence, redacted, ApplicationCommandPermission, ApplicationId, ArcSeq,
    AuditLogEntry, BorrowedKey, ChannelId, ClientStatus, CommandPermissionsId, GuildCreatePayload,
    GuildId, GuildUpdate, Id, Lenient, LenientActivities, LenientSeq, Member, MessageId, Nullable,
    NullablePatch, PartialUser, Patch, Presence, PresenceStatus, ReactionType, Role, Skipped,
    SoundboardSound, UserId,
};
use crate::types::Emoji;

use super::*;

/// Builds the sorted table of known dispatch names and the decoder that
/// matches on them from one list, so the two cannot drift apart.
macro_rules! dispatch_table {
    ($data:ident; $($name:literal => $decode:expr,)*) => {
        /// Every dispatch name with a typed decoder, sorted (checked by a
        /// test) so a name can be interned with a binary search.
        pub(super) const KNOWN_DISPATCHES: &[&str] = &[$($name),*];

        /// Decodes `data` as the payload of dispatch `name`. Unknown names
        /// give [`EventKind::Unknown`].
        pub(super) fn decode_kind(
            name: &str,
            $data: &str,
        ) -> Result<EventKind, EventDecodeError> {
            Ok(match name {
                $($name => $decode,)*
                _ => EventKind::Unknown,
            })
        }
    };
}

dispatch_table! { data;
    "APPLICATION_COMMAND_PERMISSIONS_UPDATE" => EventKind::ApplicationCommandPermissionsUpdate(
        Box::new(decode_application_command_permissions_update_event(data)?),
    ),
    "AUTO_MODERATION_ACTION_EXECUTION" => {
        EventKind::AutoModerationActionExecution(Box::new(decode_auto_moderation_event(data)?))
    },
    "AUTO_MODERATION_RULE_CREATE" => {
        EventKind::AutoModerationRuleCreate(Box::new(decode_auto_moderation_event(data)?))
    },
    "AUTO_MODERATION_RULE_DELETE" => {
        EventKind::AutoModerationRuleDelete(Box::new(decode_auto_moderation_event(data)?))
    },
    "AUTO_MODERATION_RULE_UPDATE" => {
        EventKind::AutoModerationRuleUpdate(Box::new(decode_auto_moderation_event(data)?))
    },
    "CHANNEL_CREATE" => EventKind::ChannelCreate(Arc::new(typed(data)?)),
    "CHANNEL_DELETE" => EventKind::ChannelDelete(Box::new(typed(data)?)),
    "CHANNEL_INFO" => EventKind::ChannelInfo(decode_channel_info_event(data)?),
    "CHANNEL_PINS_UPDATE" => EventKind::ChannelPinsUpdate(decode_channel_pins_update_event(data)?),
    "CHANNEL_UPDATE" => EventKind::ChannelUpdate(Arc::new(typed(data)?)),
    "ENTITLEMENT_CREATE" => EventKind::EntitlementCreate(Box::new(typed(data)?)),
    "ENTITLEMENT_DELETE" => EventKind::EntitlementDelete(Box::new(typed(data)?)),
    "ENTITLEMENT_UPDATE" => EventKind::EntitlementUpdate(Box::new(typed(data)?)),
    "GUILD_AUDIT_LOG_ENTRY_CREATE" => {
        EventKind::GuildAuditLogEntryCreate(Box::new(decode_audit_log_entry_event(data)?))
    },
    "GUILD_BAN_ADD" => EventKind::GuildBanAdd(Box::new(typed(data)?)),
    "GUILD_BAN_REMOVE" => EventKind::GuildBanRemove(Box::new(typed(data)?)),
    "GUILD_CREATE" => EventKind::GuildCreate(GuildCreatePayload::from_json_str(data)?),
    "GUILD_DELETE" => EventKind::GuildDelete(typed(data)?),
    "GUILD_EMOJIS_UPDATE" => EventKind::GuildEmojisUpdate(decode_guild_emojis_update_event(data)?),
    "GUILD_INTEGRATIONS_UPDATE" => {
        EventKind::GuildIntegrationsUpdate(decode_guild_integrations_update_event(data)?)
    },
    "GUILD_MEMBERS_CHUNK" => EventKind::GuildMembersChunk(Box::new(decode_members_chunk(data)?)),
    "GUILD_MEMBER_ADD" => EventKind::MemberAdd(decode_member_event(data)?),
    "GUILD_MEMBER_REMOVE" => EventKind::MemberRemove(Box::new(typed(data)?)),
    "GUILD_MEMBER_UPDATE" => EventKind::MemberUpdate(Arc::new(typed(data)?)),
    "GUILD_ROLE_CREATE" => EventKind::RoleCreate(Box::new(typed(data)?)),
    "GUILD_ROLE_DELETE" => EventKind::RoleDelete(typed(data)?),
    "GUILD_ROLE_UPDATE" => EventKind::RoleUpdate(Box::new(typed(data)?)),
    "GUILD_SCHEDULED_EVENT_CREATE" => {
        EventKind::GuildScheduledEventCreate(Box::new(decode_scheduled_event(data)?))
    },
    "GUILD_SCHEDULED_EVENT_DELETE" => {
        EventKind::GuildScheduledEventDelete(Box::new(decode_scheduled_event(data)?))
    },
    "GUILD_SCHEDULED_EVENT_UPDATE" => {
        EventKind::GuildScheduledEventUpdate(Box::new(decode_scheduled_event(data)?))
    },
    "GUILD_SCHEDULED_EVENT_USER_ADD" => {
        EventKind::GuildScheduledEventUserAdd(Box::new(typed(data)?))
    },
    "GUILD_SCHEDULED_EVENT_USER_REMOVE" => {
        EventKind::GuildScheduledEventUserRemove(Box::new(typed(data)?))
    },
    "GUILD_SOUNDBOARD_SOUNDS_UPDATE" => {
        EventKind::GuildSoundboardSoundsUpdate(decode_soundboard_sounds_event(data)?)
    },
    "GUILD_SOUNDBOARD_SOUND_CREATE" => {
        EventKind::GuildSoundboardSoundCreate(Box::new(typed::<SoundboardSound>(data)?))
    },
    "GUILD_SOUNDBOARD_SOUND_DELETE" => EventKind::GuildSoundboardSoundDelete(typed(data)?),
    "GUILD_SOUNDBOARD_SOUND_UPDATE" => {
        EventKind::GuildSoundboardSoundUpdate(Box::new(typed::<SoundboardSound>(data)?))
    },
    "GUILD_STICKERS_UPDATE" => {
        EventKind::GuildStickersUpdate(decode_guild_stickers_update_event(data)?)
    },
    "GUILD_UPDATE" => EventKind::GuildUpdate(Box::new(decode_guild_update_event(data)?)),
    "INTEGRATION_CREATE" => EventKind::IntegrationCreate(Box::new(decode_integration_event(data)?)),
    "INTEGRATION_DELETE" => EventKind::IntegrationDelete(decode_integration_delete_event(data)?),
    "INTEGRATION_UPDATE" => EventKind::IntegrationUpdate(Box::new(decode_integration_event(data)?)),
    // `Interaction`'s Deserialize is Discord's wire format, read in one pass
    // (see `parsers::decode_interaction`).
    "INTERACTION_CREATE" => EventKind::InteractionCreate(Box::new(typed(data)?)),
    "INVITE_CREATE" => EventKind::InviteCreate(Box::new(decode_invite_event(data)?)),
    "INVITE_DELETE" => EventKind::InviteDelete(Box::new(decode_invite_event(data)?)),
    "MESSAGE_CREATE" => EventKind::MessageCreate(Arc::new(typed(data)?)),
    "MESSAGE_DELETE" => EventKind::MessageDelete(typed(data)?),
    "MESSAGE_DELETE_BULK" => EventKind::MessageDeleteBulk(decode_bulk_message_delete_event(data)?),
    "MESSAGE_POLL_VOTE_ADD" => EventKind::MessagePollVoteAdd(decode_poll_vote_event(data)?),
    "MESSAGE_POLL_VOTE_REMOVE" => EventKind::MessagePollVoteRemove(decode_poll_vote_event(data)?),
    "MESSAGE_REACTION_ADD" => EventKind::MessageReactionAdd(Box::new(decode_reaction_event(data)?)),
    "MESSAGE_REACTION_REMOVE" => {
        EventKind::MessageReactionRemove(Box::new(decode_reaction_event(data)?))
    },
    "MESSAGE_REACTION_REMOVE_ALL" => {
        EventKind::MessageReactionRemoveAll(decode_reaction_remove_all_event(data)?)
    },
    "MESSAGE_REACTION_REMOVE_EMOJI" => {
        EventKind::MessageReactionRemoveEmoji(Box::new(decode_reaction_remove_emoji_event(data)?))
    },
    "MESSAGE_UPDATE" => EventKind::MessageUpdate(Arc::new(typed(data)?)),
    "PRESENCE_UPDATE" => EventKind::PresenceUpdate(Box::new(decode_presence_update_event(data)?)),
    "RATE_LIMITED" => EventKind::RateLimited(Box::new(decode_rate_limited_event(data)?)),
    "READY" => EventKind::Ready(Box::new(typed(data)?)),
    "RESUMED" => EventKind::Resumed,
    "SOUNDBOARD_SOUNDS" => EventKind::SoundboardSounds(decode_soundboard_sounds_event(data)?),
    "STAGE_INSTANCE_CREATE" => EventKind::StageInstanceCreate(Box::new(typed(data)?)),
    "STAGE_INSTANCE_DELETE" => EventKind::StageInstanceDelete(Box::new(typed(data)?)),
    "STAGE_INSTANCE_UPDATE" => EventKind::StageInstanceUpdate(Box::new(typed(data)?)),
    "SUBSCRIPTION_CREATE" => EventKind::SubscriptionCreate(Box::new(typed(data)?)),
    "SUBSCRIPTION_DELETE" => EventKind::SubscriptionDelete(Box::new(typed(data)?)),
    "SUBSCRIPTION_UPDATE" => EventKind::SubscriptionUpdate(Box::new(typed(data)?)),
    "THREAD_CREATE" => EventKind::ThreadCreate(Box::new(decode_thread_event(data, true)?)),
    "THREAD_DELETE" => EventKind::ThreadDelete(Box::new(decode_thread_event(data, false)?)),
    "THREAD_LIST_SYNC" => EventKind::ThreadListSync(Box::new(decode_thread_list_sync_event(data)?)),
    "THREAD_MEMBERS_UPDATE" => {
        EventKind::ThreadMembersUpdate(Box::new(decode_thread_members_update_event(data)?))
    },
    "THREAD_MEMBER_UPDATE" => {
        EventKind::ThreadMemberUpdate(Box::new(decode_thread_member_update_event(data)?))
    },
    "THREAD_UPDATE" => EventKind::ThreadUpdate(Box::new(decode_thread_event(data, false)?)),
    "TYPING_START" => EventKind::TypingStart(decode_typing_start_event(data)?),
    "USER_UPDATE" => EventKind::UserUpdate(Box::new(typed(data)?)),
    "VOICE_CHANNEL_EFFECT_SEND" => {
        EventKind::VoiceChannelEffectSend(Box::new(decode_voice_channel_effect_event(data)?))
    },
    "VOICE_CHANNEL_START_TIME_UPDATE" => EventKind::VoiceChannelStartTimeUpdate(
        decode_voice_channel_start_time_update_event(data)?,
    ),
    "VOICE_CHANNEL_STATUS_UPDATE" => {
        EventKind::VoiceChannelStatusUpdate(decode_voice_channel_status_update_event(data)?)
    },
    "VOICE_SERVER_UPDATE" => EventKind::VoiceServerUpdate(Box::new(typed(data)?)),
    "VOICE_STATE_UPDATE" => EventKind::VoiceStateUpdate(Box::new(typed(data)?)),
    "WEBHOOKS_UPDATE" => EventKind::WebhooksUpdate(decode_webhooks_update_event(data)?),
}

/// The dispatch name as stored in [`EventMetadata`]: a `'static` borrow for
/// every known name, an owned copy only for unknown ones.
pub(crate) fn intern_dispatch_name(name: &str) -> Cow<'static, str> {
    KNOWN_DISPATCHES
        .binary_search(&name)
        .ok()
        .and_then(|index| KNOWN_DISPATCHES.get(index))
        .map_or_else(
            || Cow::Owned(name.to_owned()),
            |known| Cow::Borrowed(*known),
        )
}

fn typed<'a, T: Deserialize<'a>>(data: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(data)
}

/// The top-level keys of a payload object, each with its value as borrowed
/// JSON text.
///
/// [Decision Log] Field map for low-volume lenient events
/// - Purpose: read the payloads whose fields are mostly lenient (auto
///   moderation, scheduled events, invites, thread syncs, ...) without one
///   derived wire struct per event.
/// - Constraint: every derived `Deserialize` struct is compiled for the
///   `&str` reader with its own field-name matcher and `visit_map`; the 26
///   wire structs here cost tens of thousands of LLVM IR lines while these
///   dispatches arrive a few times a day per guild. Hot dispatches
///   (MESSAGE_*, PRESENCE_UPDATE, GUILD_CREATE, GUILD_MEMBER_*, TYPING_START,
///   INTERACTION_CREATE, VOICE_STATE_UPDATE, reactions) keep derived structs.
/// - Alternatives: (1) keep the derived wire structs; (2) parse into a
///   `Value` and pick fields (the 2.x decoder; allocates every string);
///   (3) one non-generic map of borrowed `RawValue`s and a per-field
///   `from_str::<T>`.
/// - Chosen: (3). The only per-event code is the list of `from_str` calls,
///   each an instance the crate already has for the field's type.
/// - Trade-offs: one `Vec` allocation per event and a linear key search per
///   field (payloads have a few dozen keys at most). A key that appears twice
///   takes the last value, like the 2.x `Value` decoder, instead of failing as
///   a derived struct does. A payload that is a JSON array instead of an
///   object is now rejected (derived structs accepted fields by position).
struct Fields<'a> {
    entries: Vec<(Cow<'a, str>, &'a RawValue)>,
}

impl<'a> Fields<'a> {
    fn parse(data: &'a str) -> Result<Self, EventDecodeError> {
        Ok(serde_json::from_str(data)?)
    }

    fn get(&self, key: &str) -> Option<&'a RawValue> {
        self.entries
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
    }

    /// `key` as a `T`, or `None` when it is absent, `null`, or not a `T`.
    fn lenient<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key)
            .and_then(|value| serde_json::from_str(value.get()).ok())
    }

    /// [`Self::lenient`] for lists: empty instead of `None`.
    fn lenient_vec<T: DeserializeOwned>(&self, key: &str) -> Vec<T> {
        self.lenient(key).unwrap_or_default()
    }

    /// `key` as a `T`; absent or invalid fails the event.
    fn required<T: DeserializeOwned>(&self, key: &'static str) -> Result<T, EventDecodeError> {
        match self.get(key) {
            Some(value) => Ok(serde_json::from_str(value.get())?),
            None => Err(<serde_json::Error as serde::de::Error>::missing_field(key).into()),
        }
    }

    /// `key` as a [`Patch`]: absent is `Missing`; a value that is not a `T`
    /// (`null` included) fails the event, as the derived decoders do.
    fn patch<T: DeserializeOwned>(&self, key: &str) -> Result<Patch<T>, EventDecodeError> {
        // A `RawValue` is exactly one value, so no end check is needed (and
        // `from_str` would add one wrapper instance per type for it).
        match self.get(key) {
            Some(value) => Ok(Patch::Value(T::deserialize(
                &mut serde_json::Deserializer::from_str(value.get()),
            )?)),
            None => Ok(Patch::Missing),
        }
    }

    /// `key` as a [`NullablePatch`]: absent is `Missing`, `null` clears.
    fn nullable<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<NullablePatch<T>, EventDecodeError> {
        match self.get(key) {
            Some(value) => Ok(Patch::Value(Nullable::from(Option::<T>::deserialize(
                &mut serde_json::Deserializer::from_str(value.get()),
            )?))),
            None => Ok(Patch::Missing),
        }
    }

    /// `key` as raw JSON; absent and `null` are `None`.
    fn value(&self, key: &str) -> Result<Option<Value>, EventDecodeError> {
        match self.get(key) {
            Some(value) => Ok(serde_json::from_str(value.get())?),
            None => Ok(None),
        }
    }
}

impl<'de> Deserialize<'de> for Fields<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldsVisitor;

        impl<'de> serde::de::Visitor<'de> for FieldsVisitor {
            type Value = Fields<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(BorrowedKey(key)) = map.next_key()? {
                    entries.push((key, map.next_value()?));
                }
                Ok(Fields { entries })
            }
        }

        deserializer.deserialize_map(FieldsVisitor)
    }
}

/// `guild_id`, required, read beside a model decoded from the same object.
#[derive(Deserialize)]
struct RequiredGuildId {
    guild_id: GuildId,
}

fn decode_guild_update_event(data: &str) -> Result<GuildUpdate, EventDecodeError> {
    // Low volume: the field map instead of a derived decoder for 34 fields.
    let fields = Fields::parse(data)?;
    let mut update = GuildUpdate::new(fields.required("id")?);
    update.name = fields.patch("name")?;
    update.owner_id = fields.nullable("owner_id")?;
    update.icon = fields.nullable("icon")?;
    update.splash = fields.nullable("splash")?;
    update.banner = fields.nullable("banner")?;
    update.description = fields.nullable("description")?;
    update.features = fields.patch("features")?;
    update.premium_tier = fields.nullable("premium_tier")?;
    update.max_presences = fields.nullable("max_presences")?;
    update.max_members = fields.nullable("max_members")?;
    update.vanity_url_code = fields.nullable("vanity_url_code")?;
    update.system_channel_id = fields.nullable("system_channel_id")?;
    update.rules_channel_id = fields.nullable("rules_channel_id")?;
    update.preferred_locale = fields.nullable("preferred_locale")?;
    update.afk_channel_id = fields.nullable("afk_channel_id")?;
    update.afk_timeout = fields.nullable("afk_timeout")?;
    update.verification_level = fields.nullable("verification_level")?;
    update.default_message_notifications = fields.nullable("default_message_notifications")?;
    update.explicit_content_filter = fields.nullable("explicit_content_filter")?;
    update.mfa_level = fields.nullable("mfa_level")?;
    update.application_id = fields.nullable("application_id")?;
    update.system_channel_flags = fields.nullable("system_channel_flags")?;
    update.public_updates_channel_id = fields.nullable("public_updates_channel_id")?;
    update.nsfw_level = fields.nullable("nsfw_level")?;
    update.premium_subscription_count = fields.nullable("premium_subscription_count")?;
    update.premium_progress_bar_enabled = fields.nullable("premium_progress_bar_enabled")?;
    update.safety_alerts_channel_id = fields.nullable("safety_alerts_channel_id")?;
    update.approximate_member_count = fields.nullable("approximate_member_count")?;
    update.approximate_presence_count = fields.nullable("approximate_presence_count")?;
    update.incidents_data = fields.nullable("incidents_data")?;
    if let Some(roles) = fields.get("roles") {
        // Straight into the `Arc`s the role store keeps.
        let roles =
            serde_json::Deserializer::from_str(roles.get())
                .deserialize_seq(ArcSeq::<Role>::new(None))?;
        update.roles = Patch::Value(roles);
    }
    update.emojis = fields.patch("emojis")?;
    update.stickers = fields.patch("stickers")?;
    Ok(update)
}

fn decode_member_event(data: &str) -> Result<MemberEvent, EventDecodeError> {
    let RequiredGuildId { guild_id } = typed(data)?;
    Ok(MemberEvent {
        guild_id,
        member: Arc::new(typed::<Member>(data)?),
    })
}

fn decode_integration_event(data: &str) -> Result<IntegrationEvent, EventDecodeError> {
    Ok(IntegrationEvent {
        integration: typed(data)?,
        guild_id: Fields::parse(data)?.lenient("guild_id"),
    })
}

fn decode_thread_event(data: &str, create: bool) -> Result<ThreadEvent, EventDecodeError> {
    let newly_created = if create {
        Fields::parse(data)?.lenient("newly_created")
    } else {
        None
    };
    Ok(ThreadEvent {
        thread: Arc::new(typed(data)?),
        newly_created,
    })
}

fn decode_bulk_message_delete_event(
    data: &str,
) -> Result<BulkMessageDeleteEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(BulkMessageDeleteEvent {
        ids: fields.required("ids")?,
        channel_id: fields.required("channel_id")?,
        guild_id: fields.lenient("guild_id"),
    })
}

fn decode_channel_pins_update_event(
    data: &str,
) -> Result<ChannelPinsUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ChannelPinsUpdateEvent {
        channel_id: fields.required("channel_id")?,
        guild_id: fields.lenient("guild_id"),
        last_pin_timestamp: fields.lenient("last_pin_timestamp"),
    })
}

fn decode_guild_emojis_update_event(
    data: &str,
) -> Result<GuildEmojisUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(GuildEmojisUpdateEvent {
        guild_id: fields.required("guild_id")?,
        emojis: fields.lenient_vec("emojis"),
    })
}

fn decode_guild_integrations_update_event(
    data: &str,
) -> Result<GuildIntegrationsUpdateEvent, EventDecodeError> {
    Ok(GuildIntegrationsUpdateEvent {
        guild_id: Fields::parse(data)?.lenient("guild_id"),
    })
}

fn decode_webhooks_update_event(data: &str) -> Result<WebhooksUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(WebhooksUpdateEvent {
        guild_id: fields.lenient("guild_id"),
        channel_id: fields.lenient("channel_id"),
    })
}

fn decode_reaction_remove_all_event(
    data: &str,
) -> Result<ReactionRemoveAllEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ReactionRemoveAllEvent {
        channel_id: fields.lenient("channel_id"),
        message_id: fields.lenient("message_id"),
        guild_id: fields.lenient("guild_id"),
    })
}

fn decode_reaction_remove_emoji_event(
    data: &str,
) -> Result<ReactionRemoveEmojiEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ReactionRemoveEmojiEvent {
        channel_id: fields.lenient("channel_id"),
        message_id: fields.lenient("message_id"),
        guild_id: fields.lenient("guild_id"),
        emoji: fields.lenient("emoji"),
    })
}

fn decode_typing_start_event(data: &str) -> Result<TypingStartEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        timestamp: Lenient<u64>,
    }
    let wire: Wire = typed(data)?;
    Ok(TypingStartEvent {
        channel_id: wire.channel_id.0,
        guild_id: wire.guild_id.0,
        user_id: wire.user_id.0,
        timestamp: wire.timestamp.0,
    })
}

fn decode_thread_list_sync_event(data: &str) -> Result<ThreadListSyncEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ThreadListSyncEvent {
        guild_id: fields.lenient("guild_id"),
        channel_ids: fields.lenient_vec("channel_ids"),
        threads: fields.lenient_vec("threads"),
        members: fields.lenient_vec("members"),
    })
}

fn decode_thread_member_update_event(
    data: &str,
) -> Result<ThreadMemberUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ThreadMemberUpdateEvent {
        guild_id: fields.lenient("guild_id"),
        thread_id: fields.lenient("id"),
        user_id: fields.lenient("user_id"),
        join_timestamp: fields.lenient("join_timestamp"),
        flags: fields.lenient("flags"),
    })
}

fn decode_thread_members_update_event(
    data: &str,
) -> Result<ThreadMembersUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ThreadMembersUpdateEvent {
        guild_id: fields.lenient("guild_id"),
        thread_id: fields.lenient("id"),
        added_members: fields.lenient("added_members"),
        removed_member_ids: fields.lenient("removed_member_ids"),
        member_count: fields.lenient("member_count"),
    })
}

fn decode_guild_stickers_update_event(
    data: &str,
) -> Result<GuildStickersUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(GuildStickersUpdateEvent {
        guild_id: fields.lenient("guild_id"),
        stickers: fields.lenient_vec("stickers"),
    })
}

fn decode_integration_delete_event(data: &str) -> Result<IntegrationDeleteEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(IntegrationDeleteEvent {
        id: fields.lenient("id"),
        guild_id: fields.lenient("guild_id"),
        application_id: fields.lenient("application_id"),
    })
}

fn decode_scheduled_event(data: &str) -> Result<ScheduledEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(ScheduledEvent {
        id: fields.lenient("id"),
        guild_id: fields.lenient("guild_id"),
        channel_id: fields.lenient("channel_id"),
        creator_id: fields.lenient("creator_id"),
        name: fields.lenient("name"),
        description: fields.lenient("description"),
        scheduled_start_time: fields.lenient("scheduled_start_time"),
        scheduled_end_time: fields.lenient("scheduled_end_time"),
        privacy_level: fields.lenient("privacy_level"),
        status: fields.lenient("status"),
        entity_type: fields.lenient("entity_type"),
        entity_id: fields.lenient("entity_id"),
        entity_metadata: fields.lenient("entity_metadata"),
        user_count: fields.lenient("user_count"),
        image: fields.lenient("image"),
    })
}

fn decode_soundboard_sounds_event(data: &str) -> Result<SoundboardSoundsEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(SoundboardSoundsEvent {
        guild_id: fields.required("guild_id")?,
        soundboard_sounds: fields.lenient_vec("soundboard_sounds"),
    })
}

fn decode_poll_vote_event(data: &str) -> Result<PollVoteEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(PollVoteEvent {
        user_id: fields.lenient("user_id"),
        channel_id: fields.lenient("channel_id"),
        message_id: fields.lenient("message_id"),
        guild_id: fields.lenient("guild_id"),
        answer_id: fields.lenient("answer_id"),
    })
}

fn decode_reaction_event(data: &str) -> Result<ReactionEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        message_id: Lenient<MessageId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        member: Lenient<Member>,
        #[serde(default)]
        message_author_id: Lenient<UserId>,
        #[serde(default)]
        emoji: Lenient<Emoji>,
        #[serde(default)]
        burst: Lenient<bool>,
        #[serde(default)]
        burst_colors: Lenient<Vec<String>>,
        #[serde(rename = "type", default)]
        reaction_type: Lenient<ReactionType>,
    }
    let wire: Wire = typed(data)?;
    Ok(ReactionEvent {
        user_id: wire.user_id.0,
        channel_id: wire.channel_id.0,
        message_id: wire.message_id.0,
        guild_id: wire.guild_id.0,
        member: wire.member.0,
        message_author_id: wire.message_author_id.0,
        emoji: wire.emoji.0,
        burst: wire.burst.0,
        burst_colors: wire.burst_colors.or_empty(),
        reaction_type: wire.reaction_type.0,
    })
}

/// GUILD_MEMBERS_CHUNK with the derived decoder; when that fails, again
/// with each member and presence decoded on its own, as GUILD_CREATE does
/// (`GuildCreatePayload::from_json_str`): a member or presence this version
/// cannot read is dropped and logged instead of failing the chunk. The
/// members it delivers still reach the cache, and `dropped_members` lets
/// `fetch_members` report the loss instead of a short list as complete.
fn decode_members_chunk(data: &str) -> Result<GuildMembersChunkPayload, EventDecodeError> {
    /// The chunk's fields, with the two collections kept as text for the
    /// element-by-element pass.
    #[derive(Deserialize)]
    struct Wire<'a> {
        guild_id: GuildId,
        #[serde(borrow, default)]
        members: Option<&'a RawValue>,
        chunk_index: u64,
        chunk_count: u64,
        #[serde(default)]
        not_found: Vec<UserId>,
        #[serde(borrow, default)]
        presences: Option<&'a RawValue>,
        #[serde(default)]
        nonce: Option<String>,
    }

    let strict_error = match typed(data) {
        Ok(chunk) => return Ok(chunk),
        Err(error) => error,
    };
    let wire: Wire<'_> = typed(data)?;
    let mut skipped = Skipped::default();
    let members = match wire.members {
        Some(raw) => LenientSeq::members(&mut skipped)
            .deserialize(&mut serde_json::Deserializer::from_str(raw.get()))?,
        None => Vec::new(),
    };
    let presences = match wire.presences {
        Some(raw) => Some(
            LenientSeq::presences(&mut skipped)
                .deserialize(&mut serde_json::Deserializer::from_str(raw.get()))?,
        ),
        None => None,
    };
    warn!(
        "GUILD_MEMBERS_CHUNK {} ({}/{}) kept after dropping {skipped} that did not decode ({})",
        wire.guild_id,
        wire.chunk_index,
        wire.chunk_count,
        redacted(&strict_error)
    );
    Ok(GuildMembersChunkPayload {
        guild_id: wire.guild_id,
        members,
        chunk_index: wire.chunk_index,
        chunk_count: wire.chunk_count,
        not_found: wire.not_found,
        presences,
        nonce: wire.nonce,
        dropped_members: skipped.members,
    })
}

fn decode_presence_update_event(data: &str) -> Result<PresenceUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        user: Lenient<PartialUser>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        status: Lenient<String>,
        /// One activity at a time: an activity this version cannot read
        /// is dropped (logged at debug level) instead of the whole list.
        #[serde(default)]
        activities: LenientActivities,
        #[serde(default)]
        client_status: Lenient<ClientStatus>,
    }
    let wire: Wire = typed(data)?;
    let user = wire.user.0;
    let mut presence = Presence {
        user_id: user.as_ref().map(|user| user.id),
        user: None,
        status: wire.status.0.map(PresenceStatus::new),
        activities: wire
            .activities
            .activities
            .filter(|activities| !activities.is_empty()),
        client_status: wire.client_status.0,
    };
    // The cache keeps this presence while the member is online: drop the
    // decoder's spare `activities` capacity first, as GUILD_CREATE does.
    normalize_presence(&mut presence);
    Ok(PresenceUpdateEvent {
        guild_id: wire.guild_id.0,
        user,
        presence: Arc::new(presence),
    })
}

fn decode_application_command_permissions_update_event(
    data: &str,
) -> Result<ApplicationCommandPermissionsUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    let application_id: Option<ApplicationId> = fields.lenient("application_id");
    let guild_id: Option<GuildId> = fields.lenient("guild_id");
    let id = fields
        .lenient::<Id<marker::Generic>>("id")
        .map(|id| match application_id {
            Some(application_id) => CommandPermissionsId::from_wire(id, application_id),
            None => CommandPermissionsId::Command(id.cast()),
        });
    let mut permissions: Vec<ApplicationCommandPermission> = fields.lenient_vec("permissions");
    if let Some(guild_id) = guild_id {
        for permission in &mut permissions {
            *permission = permission.in_guild(guild_id);
        }
    }
    Ok(ApplicationCommandPermissionsUpdateEvent {
        id,
        application_id,
        guild_id,
        permissions,
    })
}

fn decode_voice_channel_effect_event(
    data: &str,
) -> Result<VoiceChannelEffectEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(VoiceChannelEffectEvent {
        channel_id: fields.lenient("channel_id"),
        guild_id: fields.lenient("guild_id"),
        user_id: fields.lenient("user_id"),
        emoji: fields.lenient("emoji"),
        animation_type: fields.lenient("animation_type"),
        animation_id: fields.lenient("animation_id"),
        sound_id: fields.lenient("sound_id"),
        sound_volume: fields.lenient("sound_volume"),
    })
}

fn decode_voice_channel_start_time_update_event(
    data: &str,
) -> Result<VoiceChannelStartTimeUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(VoiceChannelStartTimeUpdateEvent {
        channel_id: fields.lenient("channel_id"),
        guild_id: fields.lenient("guild_id"),
        voice_channel_start_time: fields.lenient("voice_channel_start_time"),
    })
}

fn decode_voice_channel_status_update_event(
    data: &str,
) -> Result<VoiceChannelStatusUpdateEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(VoiceChannelStatusUpdateEvent {
        channel_id: fields.lenient("channel_id"),
        guild_id: fields.lenient("guild_id"),
        status: fields.lenient("status"),
    })
}

fn decode_channel_info_event(data: &str) -> Result<ChannelInfoEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    // Like the 2.x decoder: a `channels` value that is not an array is
    // ignored, but every entry of an array must carry a valid `id`.
    let entries: Vec<&RawValue> = fields
        .get("channels")
        .and_then(|channels| serde_json::from_str(channels.get()).ok())
        .unwrap_or_default();
    let channels = entries
        .into_iter()
        .map(|entry| {
            let channel = Fields::parse(entry.get())?;
            Ok(ChannelInfoChannel {
                id: channel.required("id")?,
                status: channel.lenient("status"),
                voice_start_time: channel.lenient("voice_start_time"),
            })
        })
        .collect::<Result<Vec<_>, EventDecodeError>>()?;
    Ok(ChannelInfoEvent {
        guild_id: fields.required("guild_id")?,
        channels,
    })
}

fn decode_rate_limited_event(data: &str) -> Result<RateLimitedEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(RateLimitedEvent {
        opcode: fields.lenient("opcode"),
        retry_after: fields.lenient("retry_after"),
        meta: fields.value("meta")?,
    })
}

fn decode_invite_event(data: &str) -> Result<InviteEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(InviteEvent {
        guild_id: fields.lenient("guild_id"),
        channel_id: fields.lenient("channel_id"),
        code: fields.lenient("code"),
        inviter: fields.lenient("inviter"),
        uses: fields.lenient("uses"),
        max_uses: fields.lenient("max_uses"),
        max_age: fields.lenient("max_age"),
        temporary: fields.lenient("temporary"),
        created_at: fields.lenient("created_at"),
        expires_at: fields.lenient("expires_at"),
        target_type: fields.lenient("target_type"),
        target_user: fields.lenient("target_user"),
    })
}

fn decode_auto_moderation_event(data: &str) -> Result<AutoModerationEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(AutoModerationEvent {
        id: fields.lenient("id"),
        guild_id: fields.lenient("guild_id"),
        name: fields.lenient("name"),
        creator_id: fields.lenient("creator_id"),
        event_type: fields.lenient("event_type"),
        trigger_type: fields.lenient("trigger_type"),
        trigger_metadata: fields.lenient("trigger_metadata"),
        actions: fields.lenient_vec("actions"),
        enabled: fields.lenient("enabled"),
        exempt_roles: fields.lenient_vec("exempt_roles"),
        exempt_channels: fields.lenient_vec("exempt_channels"),
        action: fields.lenient("action"),
        rule_id: fields.lenient("rule_id"),
        rule_trigger_type: fields.lenient("rule_trigger_type"),
        user_id: fields.lenient("user_id"),
        channel_id: fields.lenient("channel_id"),
        message_id: fields.lenient("message_id"),
        alert_system_message_id: fields.lenient("alert_system_message_id"),
        content: fields.lenient("content"),
        matched_keyword: fields.lenient("matched_keyword"),
        matched_content: fields.lenient("matched_content"),
    })
}

fn decode_audit_log_entry_event(data: &str) -> Result<AuditLogEntryEvent, EventDecodeError> {
    let fields = Fields::parse(data)?;
    Ok(AuditLogEntryEvent {
        guild_id: fields.lenient("guild_id"),
        // The entry is the same object; a payload it does not fit leaves
        // only the individual fields below.
        entry: typed::<AuditLogEntry>(data).ok(),
        id: fields.lenient("id"),
        user_id: fields.lenient("user_id"),
        target_id: fields.lenient("target_id"),
        action_type: fields.lenient("action_type"),
        changes: fields.lenient("changes"),
        options: fields.value("options")?,
        reason: fields.lenient("reason"),
    })
}
