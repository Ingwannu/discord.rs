//! Gateway-only payloads: the shapes the event pipeline decodes a dispatch
//! into when the REST model does not fit.
//!
//! - [`GuildCreate`] holds GUILD_CREATE split into the units the cache stores
//!   (`Arc<Member>`, `Arc<Presence>`, channels with their guild filled in), so
//!   the cache shares them instead of copying a whole [`Guild`].
//! - [`MessageUpdate`] and [`PartialUser`] are MESSAGE_UPDATE's partial
//!   shape: every mutable field is a [`Patch`] / [`NullablePatch`], so an
//!   omitted field keeps the cached value, `null` clears it, and `[]` replaces
//!   it with an empty list.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 게이트웨이 수신 경로가 캐시와 같은 단위를 공유하고(GUILD_CREATE),
//!   부분 갱신을 키 집합 없이 타입으로 병합하게 한다(MESSAGE_UPDATE). 설계 문서
//!   `3.0-types-events-requests.md` §3 "4단계 실행 결정"의 체크포인트 A 계약이다.
//! - 기존 구현 및 제약 조건: GUILD_CREATE는 `Guild` 하나로 디코딩한 뒤 캐시가 멤버·presence·
//!   채널을 하나씩 복제했다. MESSAGE_UPDATE는 `Message`로 디코딩하고 raw `Value`의 키
//!   집합으로 생략과 지움을 구분했는데, raw는 이제 선택 사항이다. 공유 모델 파일(guild.rs,
//!   message.rs)은 다른 작업자가 동시에 고치므로 여기서는 건드리지 않는다.
//! - 검토한 주요 대안: GUILD_CREATE — (1) `Arc<Guild>`만 씌우기, (2) `Guild`를 디코딩한 뒤
//!   `Vec<Member>`를 `Vec<Arc<Member>>`로 옮기기, (3) members·presences만 `Arc`로 바로
//!   읽고 나머지 키는 빌린 `RawValue`로 모아 `Guild`의 derive 디코더에 넘기기.
//!   MESSAGE_UPDATE — (a) `Message` + 키 집합 유지, (b) 필드마다 `Patch`인 전용 DTO,
//!   (c) `Value` 패치를 그대로 병합.
//! - 선택한 방식: (3)과 (b). `PartialUser`는 author 안의 부분 사용자도 같은 규칙으로 병합한다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 캐시에 넣을 때 안쪽 `Vec<Member>`를 여전히
//!   복사한다. (2)는 1,000명 길드에서 `Vec<Member>`(728 B × 용량)를 키우는 재할당과 옮길 때의
//!   두 번째 벡터가 최대 힙을 늘린다. (3)은 `Guild` 필드 목록을 복제하지 않으면서(필드가 늘면
//!   그대로 따라온다) 원소를 바로 `Arc`에 넣는다. (a)는 raw 없이 동작하지 않고, (c)는 병합마다
//!   `Value`를 만든다. (b)는 생략·null·빈 배열이 타입에 드러나 테스트할 수 있다.
//! - 장점, 단점 및 영향: 빠른 GUILD_CREATE 경로는 serde_json 텍스트에서만 동작한다(빌린
//!   `RawValue`). 공개 경로는 `From<Guild>`로 같은 정규화를 거친다. `MessageUpdate`는
//!   `Message`의 필드를 그대로 따라야 하며, `apply_to`의 전체 구조 분해가 `Message`에 필드가
//!   추가되면 컴파일 오류로 알려 준다.
//! ```

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

use crate::types::Emoji;

use super::{
    ApplicationId, Attachment, AvatarDecorationData, Channel, ChannelId, ChannelMention, Embed,
    Guild, GuildId, Member, Message, MessageActivity, MessageCall, MessageComponent, MessageId,
    MessageInteraction, MessageInteractionMetadata, MessageReference, MessageSnapshot, MessageType,
    Nullable, NullablePatch, PartialApplication, Patch, Poll, Presence, Reaction, ResolvedData,
    RoleId, RoleSubscriptionData, SharedClientTheme, SoundboardSound, StageInstance, Sticker,
    StickerItem, User, UserCollectibles, UserId, UserPremiumType, UserPrimaryGuild, VoiceState,
    WebhookId,
};

/// A GUILD_CREATE payload, split into the units the cache stores.
///
/// Discord sends the guild together with its channels, members, presences,
/// and other collections. The gateway decodes members and presences straight
/// into `Arc`s, so the cache keeps the same allocations the event holds
/// instead of copying them, and fills in what Discord leaves implicit before
/// anything is shared:
///
/// - every channel, thread, and voice state gets `guild_id` set to the guild;
/// - every presence gets `user_id` set from its partial `user`.
///
/// [`guild`](Self::guild) is the guild itself with those collections empty
/// (as [`Guild::without_create_collections`] returns it). Build one from a
/// REST or hand-made [`Guild`] with `GuildCreate::from(guild)`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct GuildCreate {
    /// The guild without its GUILD_CREATE-only collections; shared with the
    /// cache's guild store.
    pub guild: Arc<Guild>,
    /// Channels, each with `guild_id` set.
    pub channels: Vec<Channel>,
    /// Active threads, each with `guild_id` set.
    pub threads: Vec<Channel>,
    /// Members; the cache stores these `Arc`s as they are.
    pub members: Vec<Arc<Member>>,
    /// Presences, each with `user_id` set when Discord sent a `user`; the
    /// cache stores these `Arc`s as they are.
    pub presences: Vec<Arc<Presence>>,
    /// Voice states, each with `guild_id` set.
    pub voice_states: Vec<VoiceState>,
    /// Custom emojis.
    pub emojis: Vec<Emoji>,
    /// Custom stickers.
    pub stickers: Vec<Sticker>,
    /// Stage instances.
    pub stage_instances: Vec<StageInstance>,
    /// Soundboard sounds.
    pub soundboard_sounds: Vec<SoundboardSound>,
    /// Scheduled events, as [`Guild::guild_scheduled_events`] holds them.
    pub guild_scheduled_events: Vec<serde_json::Value>,
}

impl GuildCreate {
    /// The guild's id.
    pub fn id(&self) -> GuildId {
        self.guild.id
    }

    /// Rebuilds the full [`Guild`] with every collection put back, copying
    /// the shared members and presences. Meant for code written against the
    /// 2.x `guild_create(ctx, Guild)` hook; reading the fields directly costs
    /// nothing.
    pub fn to_guild(&self) -> Guild {
        Guild {
            channels: self.channels.clone(),
            threads: self.threads.clone(),
            members: self
                .members
                .iter()
                .map(|member| (**member).clone())
                .collect(),
            voice_states: self.voice_states.clone(),
            presences: self
                .presences
                .iter()
                .map(|presence| (**presence).clone())
                .collect(),
            emojis: self.emojis.clone(),
            stickers: self.stickers.clone(),
            stage_instances: self.stage_instances.clone(),
            soundboard_sounds: self.soundboard_sounds.clone(),
            guild_scheduled_events: self.guild_scheduled_events.clone(),
            ..(*self.guild).clone()
        }
    }

    /// Splits `guild`'s collections out and normalizes them; members and
    /// presences arrive already wrapped (and presences normalized).
    fn assemble(
        mut guild: Guild,
        members: Vec<Arc<Member>>,
        presences: Vec<Arc<Presence>>,
    ) -> Self {
        let guild_id = guild.id;
        let mut channels = std::mem::take(&mut guild.channels);
        let mut threads = std::mem::take(&mut guild.threads);
        let mut voice_states = std::mem::take(&mut guild.voice_states);
        for channel in channels.iter_mut().chain(threads.iter_mut()) {
            channel.guild_id.get_or_insert(guild_id);
        }
        for voice_state in &mut voice_states {
            voice_state.guild_id.get_or_insert(guild_id);
        }
        Self {
            channels,
            threads,
            members,
            presences,
            voice_states,
            emojis: std::mem::take(&mut guild.emojis),
            stickers: std::mem::take(&mut guild.stickers),
            stage_instances: std::mem::take(&mut guild.stage_instances),
            soundboard_sounds: std::mem::take(&mut guild.soundboard_sounds),
            guild_scheduled_events: std::mem::take(&mut guild.guild_scheduled_events),
            // Leftovers in a hand-made guild (the fast path never fills
            // these) must not survive into the cached copy.
            guild: Arc::new(Guild {
                members: Vec::new(),
                presences: Vec::new(),
                ..guild
            }),
        }
    }

    /// Decodes a GUILD_CREATE `d` payload from JSON text, reading members
    /// and presences straight into `Arc`s.
    ///
    /// Only serde_json text works here: the keys other than `members` and
    /// `presences` are collected as borrowed [`RawValue`]s and handed to
    /// [`Guild`]'s own decoder, so the field list is not duplicated.
    pub(crate) fn from_json_str(text: &str) -> Result<Self, serde_json::Error> {
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let decoded = deserializer.deserialize_map(GuildCreateVisitor)?;
        deserializer.end()?;
        Ok(decoded)
    }
}

impl From<Guild> for GuildCreate {
    fn from(mut guild: Guild) -> Self {
        let members = std::mem::take(&mut guild.members)
            .into_iter()
            .map(Arc::new)
            .collect();
        let presences = std::mem::take(&mut guild.presences)
            .into_iter()
            .map(|mut presence| {
                normalize_presence(&mut presence);
                Arc::new(presence)
            })
            .collect();
        Self::assemble(guild, members, presences)
    }
}

/// Fills `user_id` from the partial `user` GUILD_CREATE and member chunks
/// identify presences with.
pub(crate) fn normalize_presence(presence: &mut Presence) {
    if presence.user_id.is_none() {
        presence.user_id = presence.user.as_ref().map(|user| user.id);
    }
}

/// Number of top-level keys a GUILD_CREATE carries besides `members` and
/// `presences` (about 55 today); sizes the borrowed key list once.
const GUILD_CREATE_KEY_HINT: usize = 64;

struct GuildCreateVisitor;

impl<'de> Visitor<'de> for GuildCreateVisitor {
    type Value = GuildCreate;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a GUILD_CREATE guild object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GuildCreate, A::Error> {
        let mut members = Vec::new();
        let mut presences = Vec::new();
        let mut rest: Vec<(Cow<'de, str>, &'de RawValue)> =
            Vec::with_capacity(GUILD_CREATE_KEY_HINT);
        while let Some(BorrowedKey(key)) = map.next_key()? {
            match key.as_ref() {
                "members" => members = map.next_value_seed(ArcSeq::<Member>::new(None))?,
                "presences" => {
                    presences = map.next_value_seed(ArcSeq::new(Some(normalize_presence)))?;
                }
                _ => rest.push((key, map.next_value()?)),
            }
        }
        let rest = de::value::MapDeserializer::<_, serde_json::Error>::new(rest.into_iter());
        let guild = Guild::deserialize(rest).map_err(de::Error::custom)?;
        Ok(GuildCreate::assemble(guild, members, presences))
    }
}

/// A map key borrowed from the input when it has no escapes.
struct BorrowedKey<'de>(Cow<'de, str>);

impl<'de> Deserialize<'de> for BorrowedKey<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;

        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = Cow<'de, str>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map key")
            }

            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Cow::Borrowed(value))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Cow::Owned(value.to_owned()))
            }
        }

        deserializer.deserialize_str(KeyVisitor).map(BorrowedKey)
    }
}

/// Reads a JSON array straight into `Vec<Arc<T>>`: each element is decoded
/// and wrapped once, optionally normalized while it is still unique.
pub(crate) struct ArcSeq<T> {
    normalize: Option<fn(&mut T)>,
    marker: PhantomData<fn() -> T>,
}

impl<T> ArcSeq<T> {
    pub(crate) fn new(normalize: Option<fn(&mut T)>) -> Self {
        Self {
            normalize,
            marker: PhantomData,
        }
    }
}

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for ArcSeq<T> {
    type Value = Vec<Arc<T>>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for ArcSeq<T> {
    type Value = Vec<Arc<T>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(mut item) = seq.next_element::<T>()? {
            if let Some(normalize) = self.normalize {
                normalize(&mut item);
            }
            items.push(Arc::new(item));
        }
        Ok(items)
    }
}

/// MESSAGE_UPDATE: the changed fields of a message.
///
/// Discord may send only some fields, so each mutable field says whether it
/// was sent:
///
/// | Field shape | Omitted | `null` | Value |
/// |---|---|---|---|
/// | [`Patch<T>`] (fields that are `T` on [`Message`]) | keep | decode error | replace |
/// | [`NullablePatch<T>`] (fields that are `Option<T>` on [`Message`]) | keep | clear | replace |
///
/// An empty array is an ordinary value: `"embeds": []` replaces the embeds
/// with none. `false` and `0` are values too. Apply an update to a complete
/// message with [`apply_to`](Self::apply_to); an update alone is never turned
/// into a full [`Message`], because the omitted fields are unknown.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MessageUpdate {
    pub id: MessageId,
    pub channel_id: ChannelId,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub guild_id: NullablePatch<GuildId>,
    /// The author, merged field by field into the cached author with the
    /// same id (see [`PartialUser`]).
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub author: NullablePatch<PartialUser>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub member: NullablePatch<Member>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub content: Patch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub timestamp: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub edited_timestamp: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub mentions: Patch<Vec<User>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub mention_roles: Patch<Vec<RoleId>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub attachments: Patch<Vec<Attachment>>,
    #[serde(rename = "type", default, skip_serializing_if = "Patch::is_missing")]
    pub kind: NullablePatch<MessageType>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub pinned: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub tts: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub flags: NullablePatch<u64>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub webhook_id: NullablePatch<WebhookId>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub application_id: NullablePatch<ApplicationId>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub application: NullablePatch<PartialApplication>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub embeds: Patch<Vec<Embed>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub reactions: Patch<Vec<Reaction>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub activity: NullablePatch<MessageActivity>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub mention_everyone: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub mention_channels: NullablePatch<Vec<ChannelMention>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub sticker_items: Patch<Vec<StickerItem>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub stickers: Patch<Vec<Sticker>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub position: NullablePatch<u64>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub role_subscription_data: NullablePatch<RoleSubscriptionData>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub message_reference: NullablePatch<MessageReference>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub message_snapshots: Patch<Vec<MessageSnapshot>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub referenced_message: NullablePatch<Box<Message>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub interaction_metadata: NullablePatch<MessageInteractionMetadata>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub interaction: NullablePatch<MessageInteraction>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub resolved: NullablePatch<ResolvedData>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub components: NullablePatch<Vec<MessageComponent>>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub poll: NullablePatch<Poll>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub call: NullablePatch<MessageCall>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub shared_client_theme: NullablePatch<SharedClientTheme>,
}

/// Replaces `target` with a clone of a present value.
///
/// Plain assignment, not `clone_from`: reusing the old buffer would keep a
/// cached message's larger string and list capacities alive after an edit
/// shrinks them.
fn apply_patch<T: Clone>(target: &mut T, patch: &Patch<T>) {
    if let Patch::Value(value) = patch {
        *target = value.clone();
    }
}

/// Clears `target` on `null` and replaces it with a clone of a present value.
fn apply_nullable<T: Clone>(target: &mut Option<T>, patch: &NullablePatch<T>) {
    match patch {
        Patch::Missing => {}
        Patch::Value(Nullable::Null) => *target = None,
        Patch::Value(Nullable::Value(value)) => *target = Some(value.clone()),
    }
}

impl MessageUpdate {
    /// An update of message `id` in `channel_id` that changes nothing; set
    /// the fields to send. Mostly useful in tests and custom backends.
    pub fn new(id: MessageId, channel_id: ChannelId) -> Self {
        Self {
            id,
            channel_id,
            guild_id: Patch::Missing,
            author: Patch::Missing,
            member: Patch::Missing,
            content: Patch::Missing,
            timestamp: Patch::Missing,
            edited_timestamp: Patch::Missing,
            mentions: Patch::Missing,
            mention_roles: Patch::Missing,
            attachments: Patch::Missing,
            kind: Patch::Missing,
            pinned: Patch::Missing,
            tts: Patch::Missing,
            flags: Patch::Missing,
            webhook_id: Patch::Missing,
            application_id: Patch::Missing,
            application: Patch::Missing,
            embeds: Patch::Missing,
            reactions: Patch::Missing,
            activity: Patch::Missing,
            mention_everyone: Patch::Missing,
            mention_channels: Patch::Missing,
            sticker_items: Patch::Missing,
            stickers: Patch::Missing,
            position: Patch::Missing,
            role_subscription_data: Patch::Missing,
            message_reference: Patch::Missing,
            message_snapshots: Patch::Missing,
            referenced_message: Patch::Missing,
            interaction_metadata: Patch::Missing,
            interaction: Patch::Missing,
            resolved: Patch::Missing,
            components: Patch::Missing,
            poll: Patch::Missing,
            call: Patch::Missing,
            shared_client_theme: Patch::Missing,
        }
    }

    /// Whether this update is about `message` (same id and channel).
    pub fn targets(&self, message: &Message) -> bool {
        self.id == message.id && self.channel_id == message.channel_id
    }

    /// Applies every sent field to `message`, which must be the complete
    /// message this update is about; returns `false` and leaves it unchanged
    /// otherwise (see [`targets`](Self::targets)).
    ///
    /// Present values are cloned, so the update stays usable (it is shared
    /// through `Arc<Event>`).
    pub fn apply_to(&self, message: &mut Message) -> bool {
        if !self.targets(message) {
            return false;
        }
        // Both destructurings are exhaustive on purpose: a field added to
        // `Message` (or to this type) fails to compile here until the merge
        // handles it, so an update can never silently drop a field again.
        let MessageUpdate {
            id: _,
            channel_id: _,
            guild_id,
            author,
            member,
            content,
            timestamp,
            edited_timestamp,
            mentions,
            mention_roles,
            attachments,
            kind,
            pinned,
            tts,
            flags,
            webhook_id,
            application_id,
            application,
            embeds,
            reactions,
            activity,
            mention_everyone,
            mention_channels,
            sticker_items,
            stickers,
            position,
            role_subscription_data,
            message_reference,
            message_snapshots,
            referenced_message,
            interaction_metadata,
            interaction,
            resolved,
            components,
            poll,
            call,
            shared_client_theme,
        } = self;
        let Message {
            id: _,
            channel_id: _,
            guild_id: target_guild_id,
            author: target_author,
            member: target_member,
            content: target_content,
            timestamp: target_timestamp,
            edited_timestamp: target_edited_timestamp,
            mentions: target_mentions,
            mention_roles: target_mention_roles,
            attachments: target_attachments,
            kind: target_kind,
            pinned: target_pinned,
            tts: target_tts,
            flags: target_flags,
            webhook_id: target_webhook_id,
            application_id: target_application_id,
            application: target_application,
            embeds: target_embeds,
            reactions: target_reactions,
            activity: target_activity,
            mention_everyone: target_mention_everyone,
            mention_channels: target_mention_channels,
            sticker_items: target_sticker_items,
            stickers: target_stickers,
            position: target_position,
            role_subscription_data: target_role_subscription_data,
            message_reference: target_message_reference,
            message_snapshots: target_message_snapshots,
            referenced_message: target_referenced_message,
            interaction_metadata: target_interaction_metadata,
            interaction: target_interaction,
            resolved: target_resolved,
            components: target_components,
            poll: target_poll,
            call: target_call,
            shared_client_theme: target_shared_client_theme,
        } = message;
        apply_nullable(target_guild_id, guild_id);
        match author {
            Patch::Missing => {}
            Patch::Value(Nullable::Null) => *target_author = None,
            Patch::Value(Nullable::Value(partial)) => partial.merge_into(target_author),
        }
        apply_nullable(target_member, member);
        apply_patch(target_content, content);
        apply_nullable(target_timestamp, timestamp);
        apply_nullable(target_edited_timestamp, edited_timestamp);
        apply_patch(target_mentions, mentions);
        apply_patch(target_mention_roles, mention_roles);
        apply_patch(target_attachments, attachments);
        apply_nullable(target_kind, kind);
        apply_nullable(target_pinned, pinned);
        apply_nullable(target_tts, tts);
        apply_nullable(target_flags, flags);
        apply_nullable(target_webhook_id, webhook_id);
        apply_nullable(target_application_id, application_id);
        apply_nullable(target_application, application);
        apply_patch(target_embeds, embeds);
        apply_patch(target_reactions, reactions);
        apply_nullable(target_activity, activity);
        apply_nullable(target_mention_everyone, mention_everyone);
        apply_nullable(target_mention_channels, mention_channels);
        apply_patch(target_sticker_items, sticker_items);
        apply_patch(target_stickers, stickers);
        apply_nullable(target_position, position);
        apply_nullable(target_role_subscription_data, role_subscription_data);
        apply_nullable(target_message_reference, message_reference);
        apply_patch(target_message_snapshots, message_snapshots);
        apply_nullable(target_referenced_message, referenced_message);
        apply_nullable(target_interaction_metadata, interaction_metadata);
        apply_nullable(target_interaction, interaction);
        apply_nullable(target_resolved, resolved);
        apply_nullable(target_components, components);
        apply_nullable(target_poll, poll);
        apply_nullable(target_call, call);
        apply_nullable(target_shared_client_theme, shared_client_theme);
        true
    }

    /// The author as a complete [`User`], when the update carries one with
    /// a `username` (see [`PartialUser::to_user`]).
    pub fn author_user(&self) -> Option<User> {
        match &self.author {
            Patch::Value(Nullable::Value(author)) => author.to_user(),
            _ => None,
        }
    }
}

/// A user object that may carry only some fields (MESSAGE_UPDATE's
/// `author`). `id` is always present; the other fields follow the
/// [`MessageUpdate`] table, mirroring [`User`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PartialUser {
    pub id: UserId,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub username: Patch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub global_name: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub discriminator: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub avatar: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub bot: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub system: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub mfa_enabled: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub banner: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub accent_color: NullablePatch<u64>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub locale: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub verified: NullablePatch<bool>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub email: NullablePatch<String>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub flags: NullablePatch<u64>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub premium_type: NullablePatch<UserPremiumType>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub public_flags: NullablePatch<u64>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub avatar_decoration_data: NullablePatch<AvatarDecorationData>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub collectibles: NullablePatch<UserCollectibles>,
    #[serde(default, skip_serializing_if = "Patch::is_missing")]
    pub primary_guild: NullablePatch<UserPrimaryGuild>,
}

impl PartialUser {
    /// A partial user with only `id` sent.
    pub fn new(id: UserId) -> Self {
        Self {
            id,
            username: Patch::Missing,
            global_name: Patch::Missing,
            discriminator: Patch::Missing,
            avatar: Patch::Missing,
            bot: Patch::Missing,
            system: Patch::Missing,
            mfa_enabled: Patch::Missing,
            banner: Patch::Missing,
            accent_color: Patch::Missing,
            locale: Patch::Missing,
            verified: Patch::Missing,
            email: Patch::Missing,
            flags: Patch::Missing,
            premium_type: Patch::Missing,
            public_flags: Patch::Missing,
            avatar_decoration_data: Patch::Missing,
            collectibles: Patch::Missing,
            primary_guild: Patch::Missing,
        }
    }

    /// Applies every sent field to `user`, which must have the same id;
    /// returns `false` and leaves it unchanged otherwise.
    pub fn apply_to(&self, user: &mut User) -> bool {
        if self.id != user.id {
            return false;
        }
        // Exhaustive for the same reason as `MessageUpdate::apply_to`.
        let PartialUser {
            id: _,
            username,
            global_name,
            discriminator,
            avatar,
            bot,
            system,
            mfa_enabled,
            banner,
            accent_color,
            locale,
            verified,
            email,
            flags,
            premium_type,
            public_flags,
            avatar_decoration_data,
            collectibles,
            primary_guild,
        } = self;
        let User {
            id: _,
            username: target_username,
            global_name: target_global_name,
            discriminator: target_discriminator,
            avatar: target_avatar,
            bot: target_bot,
            system: target_system,
            mfa_enabled: target_mfa_enabled,
            banner: target_banner,
            accent_color: target_accent_color,
            locale: target_locale,
            verified: target_verified,
            email: target_email,
            flags: target_flags,
            premium_type: target_premium_type,
            public_flags: target_public_flags,
            avatar_decoration_data: target_avatar_decoration_data,
            collectibles: target_collectibles,
            primary_guild: target_primary_guild,
        } = user;
        apply_patch(target_username, username);
        apply_nullable(target_global_name, global_name);
        apply_nullable(target_discriminator, discriminator);
        apply_nullable(target_avatar, avatar);
        apply_nullable(target_bot, bot);
        apply_nullable(target_system, system);
        apply_nullable(target_mfa_enabled, mfa_enabled);
        apply_nullable(target_banner, banner);
        apply_nullable(target_accent_color, accent_color);
        apply_nullable(target_locale, locale);
        apply_nullable(target_verified, verified);
        apply_nullable(target_email, email);
        apply_nullable(target_flags, flags);
        apply_nullable(target_premium_type, premium_type);
        apply_nullable(target_public_flags, public_flags);
        apply_nullable(target_avatar_decoration_data, avatar_decoration_data);
        apply_nullable(target_collectibles, collectibles);
        apply_nullable(target_primary_guild, primary_guild);
        true
    }

    /// A complete [`User`] when `username` (the only required field besides
    /// `id`) was sent: omitted optional fields are `None`, exactly as when a
    /// full user object omits them. `None` otherwise; a user is never made up
    /// from an id alone.
    pub fn to_user(&self) -> Option<User> {
        let Patch::Value(username) = &self.username else {
            return None;
        };
        let mut user = User {
            id: self.id,
            username: username.clone(),
            global_name: None,
            discriminator: None,
            avatar: None,
            bot: None,
            system: None,
            mfa_enabled: None,
            banner: None,
            accent_color: None,
            locale: None,
            verified: None,
            email: None,
            flags: None,
            premium_type: None,
            public_flags: None,
            avatar_decoration_data: None,
            collectibles: None,
            primary_guild: None,
        };
        self.apply_to(&mut user);
        Some(user)
    }

    /// Merges into an optional slot (a message's `author`): the same user is
    /// patched in place, a different or missing one is replaced only when
    /// this partial is complete, and otherwise the slot is left as it is.
    pub(crate) fn merge_into(&self, slot: &mut Option<User>) {
        if let Some(user) = slot.as_mut() {
            if self.apply_to(user) {
                return;
            }
        }
        if let Some(user) = self.to_user() {
            *slot = Some(user);
        }
    }
}

#[cfg(test)]
#[path = "gateway_payload_tests.rs"]
mod tests;
