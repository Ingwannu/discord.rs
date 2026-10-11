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
//! - 선택한 방식: (3). 엄격한 payload는 모델 타입의 derive 디코더를, 관대한 필드가 있는 payload는
//!   비공개 wire 구조체를 쓴다. 이름 표는 `dispatch_table!` 하나에서 디코더 분기와 함께 만든다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 필드 하나가 틀려도 이벤트 전체가 DecodeFailed가
//!   되는 동작 변경이다. (2)는 문자열마다 할당이 생긴다. (3)은 할당 없이 같은 결과를 낸다.
//!   표를 매크로 하나에서 만들면 이름 인턴 표와 분기가 어긋날 수 없다.
//! - 장점, 단점 및 영향: `Lenient<T>`는 serde_json 텍스트 입력에서만 동작하므로 wire 구조체는
//!   비공개로 둔다. 관대한 필드는 값을 두 번 훑는다(작은 필드뿐이다). 회원·통합·스레드 생성처럼
//!   한 객체에 모델과 추가 키가 섞인 payload는 같은 텍스트를 두 번 읽는다(flatten은 버퍼링한다).
//! ```

use std::borrow::Cow;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use serde_json::Value;

use crate::model::{
    marker, ApplicationCommandPermission, ApplicationId, AuditLogActionType, AuditLogEntry,
    AuditLogEntryId, AutoModerationAction, AutoModerationEventType, AutoModerationRuleId,
    AutoModerationTriggerMetadata, AutoModerationTriggerType, ChannelId, ClientStatus,
    CommandPermissionsId, GenericId, GuildCreate, GuildId, GuildScheduledEventEntityMetadata, Id,
    IntegrationId, InviteTargetType, Member, MessageId, PollAnswerId, PresenceStatus, ReactionType,
    RoleId, ScheduledEventEntityType, ScheduledEventId, ScheduledEventPrivacyLevel,
    ScheduledEventStatus, SoundboardSound, SoundboardSoundId, Sticker, ThreadMember, User, UserId,
    VoiceChannelEffectAnimationType,
};
use crate::parsers::parse_interaction;
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
    "CHANNEL_CREATE" => EventKind::ChannelCreate(Box::new(typed(data)?)),
    "CHANNEL_DELETE" => EventKind::ChannelDelete(Box::new(typed(data)?)),
    "CHANNEL_INFO" => EventKind::ChannelInfo(decode_channel_info_event(data)?),
    "CHANNEL_PINS_UPDATE" => EventKind::ChannelPinsUpdate(decode_channel_pins_update_event(data)?),
    "CHANNEL_UPDATE" => EventKind::ChannelUpdate(Box::new(typed(data)?)),
    "ENTITLEMENT_CREATE" => EventKind::EntitlementCreate(Box::new(typed(data)?)),
    "ENTITLEMENT_DELETE" => EventKind::EntitlementDelete(Box::new(typed(data)?)),
    "ENTITLEMENT_UPDATE" => EventKind::EntitlementUpdate(Box::new(typed(data)?)),
    "GUILD_AUDIT_LOG_ENTRY_CREATE" => {
        EventKind::GuildAuditLogEntryCreate(Box::new(decode_audit_log_entry_event(data)?))
    },
    "GUILD_BAN_ADD" => EventKind::GuildBanAdd(Box::new(typed(data)?)),
    "GUILD_BAN_REMOVE" => EventKind::GuildBanRemove(Box::new(typed(data)?)),
    "GUILD_CREATE" => EventKind::GuildCreate(Box::new(GuildCreate::from_json_str(data)?)),
    "GUILD_DELETE" => EventKind::GuildDelete(typed(data)?),
    "GUILD_EMOJIS_UPDATE" => EventKind::GuildEmojisUpdate(decode_guild_emojis_update_event(data)?),
    "GUILD_INTEGRATIONS_UPDATE" => {
        EventKind::GuildIntegrationsUpdate(decode_guild_integrations_update_event(data)?)
    },
    "GUILD_MEMBERS_CHUNK" => EventKind::GuildMembersChunk(Box::new(typed(data)?)),
    "GUILD_MEMBER_ADD" => EventKind::MemberAdd(decode_member_event(data)?),
    "GUILD_MEMBER_REMOVE" => EventKind::MemberRemove(Box::new(typed(data)?)),
    "GUILD_MEMBER_UPDATE" => EventKind::MemberUpdate(decode_member_event(data)?),
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
    "GUILD_UPDATE" => EventKind::GuildUpdate(Box::new(typed(data)?)),
    "INTEGRATION_CREATE" => EventKind::IntegrationCreate(Box::new(decode_integration_event(data)?)),
    "INTEGRATION_DELETE" => EventKind::IntegrationDelete(decode_integration_delete_event(data)?),
    "INTEGRATION_UPDATE" => EventKind::IntegrationUpdate(Box::new(decode_integration_event(data)?)),
    "INTERACTION_CREATE" => {
        // The interaction parser still reads a `Value`; it will expose a
        // direct path from text later (model/parser series).
        let value: Value = serde_json::from_str(data)?;
        EventKind::InteractionCreate(Box::new(parse_interaction(&value)?))
    },
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

/// A field read leniently: absent, `null`, or not a valid `T` reads as
/// `None` instead of failing the whole event, as the 2.x decoder did.
///
/// The value is skipped as a borrowed [`RawValue`] (no allocation) and then
/// decoded on its own, so this only works for serde_json text input; the
/// wire structs using it stay private for that reason.
pub(crate) struct Lenient<T>(pub(crate) Option<T>);

impl<T> Default for Lenient<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <&'de RawValue>::deserialize(deserializer)?;
        Ok(Self(serde_json::from_str(raw.get()).ok()))
    }
}

impl<T> Lenient<Vec<T>> {
    fn or_empty(self) -> Vec<T> {
        self.0.unwrap_or_default()
    }
}

/// `guild_id`, required, read beside a model decoded from the same object.
#[derive(Deserialize)]
struct RequiredGuildId {
    guild_id: GuildId,
}

/// `guild_id`, lenient, read beside a model decoded from the same object.
#[derive(Deserialize)]
struct OptionalGuildId {
    #[serde(default)]
    guild_id: Lenient<GuildId>,
}

fn decode_member_event(data: &str) -> Result<MemberEvent, EventDecodeError> {
    let RequiredGuildId { guild_id } = typed(data)?;
    Ok(MemberEvent {
        guild_id,
        member: Arc::new(typed::<Member>(data)?),
    })
}

fn decode_integration_event(data: &str) -> Result<IntegrationEvent, EventDecodeError> {
    let OptionalGuildId { guild_id } = typed(data)?;
    Ok(IntegrationEvent {
        integration: typed(data)?,
        guild_id: guild_id.0,
    })
}

fn decode_thread_event(data: &str, create: bool) -> Result<ThreadEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct NewlyCreated {
        #[serde(default)]
        newly_created: Lenient<bool>,
    }
    let newly_created = if create {
        typed::<NewlyCreated>(data)?.newly_created.0
    } else {
        None
    };
    Ok(ThreadEvent {
        thread: typed(data)?,
        newly_created,
    })
}

fn decode_bulk_message_delete_event(
    data: &str,
) -> Result<BulkMessageDeleteEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        ids: Vec<MessageId>,
        channel_id: ChannelId,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
    }
    let wire: Wire = typed(data)?;
    Ok(BulkMessageDeleteEvent {
        ids: wire.ids,
        channel_id: wire.channel_id,
        guild_id: wire.guild_id.0,
    })
}

fn decode_channel_pins_update_event(
    data: &str,
) -> Result<ChannelPinsUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        channel_id: ChannelId,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        last_pin_timestamp: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(ChannelPinsUpdateEvent {
        channel_id: wire.channel_id,
        guild_id: wire.guild_id.0,
        last_pin_timestamp: wire.last_pin_timestamp.0,
    })
}

fn decode_guild_emojis_update_event(
    data: &str,
) -> Result<GuildEmojisUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        guild_id: GuildId,
        #[serde(default)]
        emojis: Lenient<Vec<Emoji>>,
    }
    let wire: Wire = typed(data)?;
    Ok(GuildEmojisUpdateEvent {
        guild_id: wire.guild_id,
        emojis: wire.emojis.or_empty(),
    })
}

fn decode_guild_integrations_update_event(
    data: &str,
) -> Result<GuildIntegrationsUpdateEvent, EventDecodeError> {
    let OptionalGuildId { guild_id } = typed(data)?;
    Ok(GuildIntegrationsUpdateEvent {
        guild_id: guild_id.0,
    })
}

fn decode_webhooks_update_event(data: &str) -> Result<WebhooksUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
    }
    let wire: Wire = typed(data)?;
    Ok(WebhooksUpdateEvent {
        guild_id: wire.guild_id.0,
        channel_id: wire.channel_id.0,
    })
}

fn decode_reaction_remove_all_event(
    data: &str,
) -> Result<ReactionRemoveAllEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        message_id: Lenient<MessageId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
    }
    let wire: Wire = typed(data)?;
    Ok(ReactionRemoveAllEvent {
        channel_id: wire.channel_id.0,
        message_id: wire.message_id.0,
        guild_id: wire.guild_id.0,
    })
}

fn decode_reaction_remove_emoji_event(
    data: &str,
) -> Result<ReactionRemoveEmojiEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        message_id: Lenient<MessageId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        emoji: Lenient<Emoji>,
    }
    let wire: Wire = typed(data)?;
    Ok(ReactionRemoveEmojiEvent {
        channel_id: wire.channel_id.0,
        message_id: wire.message_id.0,
        guild_id: wire.guild_id.0,
        emoji: wire.emoji.0,
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
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        channel_ids: Lenient<Vec<ChannelId>>,
        #[serde(default)]
        threads: Lenient<Vec<Channel>>,
        #[serde(default)]
        members: Lenient<Vec<ThreadMember>>,
    }
    let wire: Wire = typed(data)?;
    Ok(ThreadListSyncEvent {
        guild_id: wire.guild_id.0,
        channel_ids: wire.channel_ids.or_empty(),
        threads: wire.threads.or_empty(),
        members: wire.members.or_empty(),
    })
}

fn decode_thread_member_update_event(
    data: &str,
) -> Result<ThreadMemberUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        id: Lenient<ChannelId>,
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        join_timestamp: Lenient<String>,
        #[serde(default)]
        flags: Lenient<u64>,
    }
    let wire: Wire = typed(data)?;
    Ok(ThreadMemberUpdateEvent {
        guild_id: wire.guild_id.0,
        thread_id: wire.id.0,
        user_id: wire.user_id.0,
        join_timestamp: wire.join_timestamp.0,
        flags: wire.flags.0,
    })
}

fn decode_thread_members_update_event(
    data: &str,
) -> Result<ThreadMembersUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        id: Lenient<ChannelId>,
        #[serde(default)]
        added_members: Lenient<Vec<ThreadMember>>,
        #[serde(default)]
        removed_member_ids: Lenient<Vec<UserId>>,
        #[serde(default)]
        member_count: Lenient<u64>,
    }
    let wire: Wire = typed(data)?;
    Ok(ThreadMembersUpdateEvent {
        guild_id: wire.guild_id.0,
        thread_id: wire.id.0,
        added_members: wire.added_members.0,
        removed_member_ids: wire.removed_member_ids.0,
        member_count: wire.member_count.0,
    })
}

fn decode_guild_stickers_update_event(
    data: &str,
) -> Result<GuildStickersUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        stickers: Lenient<Vec<Sticker>>,
    }
    let wire: Wire = typed(data)?;
    Ok(GuildStickersUpdateEvent {
        guild_id: wire.guild_id.0,
        stickers: wire.stickers.or_empty(),
    })
}

fn decode_integration_delete_event(data: &str) -> Result<IntegrationDeleteEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        id: Lenient<IntegrationId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        application_id: Lenient<ApplicationId>,
    }
    let wire: Wire = typed(data)?;
    Ok(IntegrationDeleteEvent {
        id: wire.id.0,
        guild_id: wire.guild_id.0,
        application_id: wire.application_id.0,
    })
}

fn decode_scheduled_event(data: &str) -> Result<ScheduledEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        id: Lenient<ScheduledEventId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        creator_id: Lenient<UserId>,
        #[serde(default)]
        name: Lenient<String>,
        #[serde(default)]
        description: Lenient<String>,
        #[serde(default)]
        scheduled_start_time: Lenient<String>,
        #[serde(default)]
        scheduled_end_time: Lenient<String>,
        #[serde(default)]
        privacy_level: Lenient<ScheduledEventPrivacyLevel>,
        #[serde(default)]
        status: Lenient<ScheduledEventStatus>,
        #[serde(default)]
        entity_type: Lenient<ScheduledEventEntityType>,
        #[serde(default)]
        entity_id: Lenient<GenericId>,
        #[serde(default)]
        entity_metadata: Lenient<GuildScheduledEventEntityMetadata>,
        #[serde(default)]
        user_count: Lenient<u64>,
        #[serde(default)]
        image: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(ScheduledEvent {
        id: wire.id.0,
        guild_id: wire.guild_id.0,
        channel_id: wire.channel_id.0,
        creator_id: wire.creator_id.0,
        name: wire.name.0,
        description: wire.description.0,
        scheduled_start_time: wire.scheduled_start_time.0,
        scheduled_end_time: wire.scheduled_end_time.0,
        privacy_level: wire.privacy_level.0,
        status: wire.status.0,
        entity_type: wire.entity_type.0,
        entity_id: wire.entity_id.0,
        entity_metadata: wire.entity_metadata.0,
        user_count: wire.user_count.0,
        image: wire.image.0,
    })
}

fn decode_soundboard_sounds_event(data: &str) -> Result<SoundboardSoundsEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        guild_id: GuildId,
        #[serde(default)]
        soundboard_sounds: Lenient<Vec<SoundboardSound>>,
    }
    let wire: Wire = typed(data)?;
    Ok(SoundboardSoundsEvent {
        guild_id: wire.guild_id,
        soundboard_sounds: wire.soundboard_sounds.or_empty(),
    })
}

fn decode_poll_vote_event(data: &str) -> Result<PollVoteEvent, EventDecodeError> {
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
        answer_id: Lenient<PollAnswerId>,
    }
    let wire: Wire = typed(data)?;
    Ok(PollVoteEvent {
        user_id: wire.user_id.0,
        channel_id: wire.channel_id.0,
        message_id: wire.message_id.0,
        guild_id: wire.guild_id.0,
        answer_id: wire.answer_id.0,
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

fn decode_presence_update_event(data: &str) -> Result<PresenceUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct UserWire {
        id: UserId,
        #[serde(default)]
        username: Lenient<String>,
    }
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        user: Lenient<UserWire>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        status: Lenient<String>,
        #[serde(default)]
        activities: Lenient<Vec<Activity>>,
        #[serde(default)]
        client_status: Lenient<ClientStatus>,
    }
    let wire: Wire = typed(data)?;
    let user = wire.user.0.map(|user| PresenceUpdateUser {
        id: user.id,
        username: user.username.0,
    });
    Ok(PresenceUpdateEvent {
        user_id: user.as_ref().map(|user| user.id),
        user,
        guild_id: wire.guild_id.0,
        status: wire.status.0.map(PresenceStatus::new),
        activities: wire.activities.or_empty(),
        client_status: wire.client_status.0,
    })
}

fn decode_application_command_permissions_update_event(
    data: &str,
) -> Result<ApplicationCommandPermissionsUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        id: Lenient<Id<marker::Generic>>,
        #[serde(default)]
        application_id: Lenient<ApplicationId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        permissions: Lenient<Vec<ApplicationCommandPermission>>,
    }
    let wire: Wire = typed(data)?;
    let application_id = wire.application_id.0;
    let guild_id = wire.guild_id.0;
    let id = wire.id.0.map(|id| match application_id {
        Some(application_id) => CommandPermissionsId::from_wire(id, application_id),
        None => CommandPermissionsId::Command(id.cast()),
    });
    let mut permissions = wire.permissions.or_empty();
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
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        emoji: Lenient<Emoji>,
        #[serde(default)]
        animation_type: Lenient<VoiceChannelEffectAnimationType>,
        #[serde(default)]
        animation_id: Lenient<u64>,
        #[serde(default)]
        sound_id: Lenient<SoundboardSoundId>,
        #[serde(default)]
        sound_volume: Lenient<f64>,
    }
    let wire: Wire = typed(data)?;
    Ok(VoiceChannelEffectEvent {
        channel_id: wire.channel_id.0,
        guild_id: wire.guild_id.0,
        user_id: wire.user_id.0,
        emoji: wire.emoji.0,
        animation_type: wire.animation_type.0,
        animation_id: wire.animation_id.0,
        sound_id: wire.sound_id.0,
        sound_volume: wire.sound_volume.0,
    })
}

fn decode_voice_channel_start_time_update_event(
    data: &str,
) -> Result<VoiceChannelStartTimeUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        voice_channel_start_time: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(VoiceChannelStartTimeUpdateEvent {
        channel_id: wire.channel_id.0,
        guild_id: wire.guild_id.0,
        voice_channel_start_time: wire.voice_channel_start_time.0,
    })
}

fn decode_voice_channel_status_update_event(
    data: &str,
) -> Result<VoiceChannelStatusUpdateEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        status: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(VoiceChannelStatusUpdateEvent {
        channel_id: wire.channel_id.0,
        guild_id: wire.guild_id.0,
        status: wire.status.0,
    })
}

fn decode_channel_info_event(data: &str) -> Result<ChannelInfoEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire<'a> {
        guild_id: GuildId,
        #[serde(default, borrow)]
        channels: Option<&'a RawValue>,
    }
    #[derive(Deserialize)]
    struct ChannelWire {
        id: ChannelId,
        #[serde(default)]
        status: Lenient<String>,
        #[serde(default)]
        voice_start_time: Lenient<String>,
    }
    let wire: Wire<'_> = typed(data)?;
    // Like the 2.x decoder: a `channels` value that is not an array is
    // ignored, but every entry of an array must carry a valid `id`.
    let entries: Vec<&RawValue> = wire
        .channels
        .and_then(|channels| serde_json::from_str(channels.get()).ok())
        .unwrap_or_default();
    let channels = entries
        .into_iter()
        .map(|entry| {
            let channel: ChannelWire = typed(entry.get())?;
            Ok(ChannelInfoChannel {
                id: channel.id,
                status: channel.status.0,
                voice_start_time: channel.voice_start_time.0,
            })
        })
        .collect::<Result<Vec<_>, EventDecodeError>>()?;
    Ok(ChannelInfoEvent {
        guild_id: wire.guild_id,
        channels,
    })
}

fn decode_rate_limited_event(data: &str) -> Result<RateLimitedEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        opcode: Lenient<u64>,
        #[serde(default)]
        retry_after: Lenient<f64>,
        #[serde(default)]
        meta: Option<Value>,
    }
    let wire: Wire = typed(data)?;
    Ok(RateLimitedEvent {
        opcode: wire.opcode.0,
        retry_after: wire.retry_after.0,
        meta: wire.meta,
    })
}

fn decode_invite_event(data: &str) -> Result<InviteEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        code: Lenient<String>,
        #[serde(default)]
        inviter: Lenient<User>,
        #[serde(default)]
        uses: Lenient<u64>,
        #[serde(default)]
        max_uses: Lenient<u64>,
        #[serde(default)]
        max_age: Lenient<u64>,
        #[serde(default)]
        temporary: Lenient<bool>,
        #[serde(default)]
        created_at: Lenient<String>,
        #[serde(default)]
        expires_at: Lenient<String>,
        #[serde(default)]
        target_type: Lenient<InviteTargetType>,
        #[serde(default)]
        target_user: Lenient<User>,
    }
    let wire: Wire = typed(data)?;
    Ok(InviteEvent {
        guild_id: wire.guild_id.0,
        channel_id: wire.channel_id.0,
        code: wire.code.0,
        inviter: wire.inviter.0,
        uses: wire.uses.0,
        max_uses: wire.max_uses.0,
        max_age: wire.max_age.0,
        temporary: wire.temporary.0,
        created_at: wire.created_at.0,
        expires_at: wire.expires_at.0,
        target_type: wire.target_type.0,
        target_user: wire.target_user.0,
    })
}

fn decode_auto_moderation_event(data: &str) -> Result<AutoModerationEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        id: Lenient<AutoModerationRuleId>,
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        name: Lenient<String>,
        #[serde(default)]
        creator_id: Lenient<UserId>,
        #[serde(default)]
        event_type: Lenient<AutoModerationEventType>,
        #[serde(default)]
        trigger_type: Lenient<AutoModerationTriggerType>,
        #[serde(default)]
        trigger_metadata: Lenient<AutoModerationTriggerMetadata>,
        #[serde(default)]
        actions: Lenient<Vec<AutoModerationAction>>,
        #[serde(default)]
        enabled: Lenient<bool>,
        #[serde(default)]
        exempt_roles: Lenient<Vec<RoleId>>,
        #[serde(default)]
        exempt_channels: Lenient<Vec<ChannelId>>,
        #[serde(default)]
        action: Lenient<AutoModerationAction>,
        #[serde(default)]
        rule_id: Lenient<AutoModerationRuleId>,
        #[serde(default)]
        rule_trigger_type: Lenient<AutoModerationTriggerType>,
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        channel_id: Lenient<ChannelId>,
        #[serde(default)]
        message_id: Lenient<MessageId>,
        #[serde(default)]
        alert_system_message_id: Lenient<MessageId>,
        #[serde(default)]
        content: Lenient<String>,
        #[serde(default)]
        matched_keyword: Lenient<String>,
        #[serde(default)]
        matched_content: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(AutoModerationEvent {
        id: wire.id.0,
        guild_id: wire.guild_id.0,
        name: wire.name.0,
        creator_id: wire.creator_id.0,
        event_type: wire.event_type.0,
        trigger_type: wire.trigger_type.0,
        trigger_metadata: wire.trigger_metadata.0,
        actions: wire.actions.or_empty(),
        enabled: wire.enabled.0,
        exempt_roles: wire.exempt_roles.or_empty(),
        exempt_channels: wire.exempt_channels.or_empty(),
        action: wire.action.0,
        rule_id: wire.rule_id.0,
        rule_trigger_type: wire.rule_trigger_type.0,
        user_id: wire.user_id.0,
        channel_id: wire.channel_id.0,
        message_id: wire.message_id.0,
        alert_system_message_id: wire.alert_system_message_id.0,
        content: wire.content.0,
        matched_keyword: wire.matched_keyword.0,
        matched_content: wire.matched_content.0,
    })
}

fn decode_audit_log_entry_event(data: &str) -> Result<AuditLogEntryEvent, EventDecodeError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(default)]
        guild_id: Lenient<GuildId>,
        #[serde(default)]
        id: Lenient<AuditLogEntryId>,
        #[serde(default)]
        user_id: Lenient<UserId>,
        #[serde(default)]
        target_id: Lenient<GenericId>,
        #[serde(default)]
        action_type: Lenient<AuditLogActionType>,
        #[serde(default)]
        changes: Lenient<Vec<Value>>,
        #[serde(default)]
        options: Option<Value>,
        #[serde(default)]
        reason: Lenient<String>,
    }
    let wire: Wire = typed(data)?;
    Ok(AuditLogEntryEvent {
        guild_id: wire.guild_id.0,
        // The entry is the same object; a payload it does not fit leaves
        // only the individual fields below.
        entry: typed::<AuditLogEntry>(data).ok(),
        id: wire.id.0,
        user_id: wire.user_id.0,
        target_id: wire.target_id.0,
        action_type: wire.action_type.0,
        changes: wire.changes.0,
        options: wire.options,
        reason: wire.reason.0,
    })
}
