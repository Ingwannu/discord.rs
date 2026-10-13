//! Gateway-only payloads: the shapes the event pipeline decodes a dispatch
//! into when the REST model does not fit.
//!
//! - [`GuildCreate`] holds GUILD_CREATE split into the units the cache stores
//!   (`Arc<Member>`, `Arc<Presence>`, channels with their guild filled in), so
//!   the cache shares them instead of copying a whole [`Guild`].
//!   [`GuildCreatePayload`] tells it apart from an unavailable guild, for
//!   which Discord sends only the id.
//! - [`MessageUpdate`] and [`PartialUser`] are MESSAGE_UPDATE's partial
//!   shape: every mutable field is a [`Patch`] / [`NullablePatch`], so an
//!   omitted field keeps the cached value, `null` clears it, and `[]` replaces
//!   it with an empty list.
//! - Element-by-element decoding (`Lenient`, `LenientActivities`,
//!   `LenientSeq`) keeps a GUILD_CREATE, GUILD_MEMBERS_CHUNK, or
//!   PRESENCE_UPDATE when one member, presence, activity, or voice state in
//!   it does not decode (decision log above `Skipped`).
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
use tracing::{debug, warn};

use crate::types::Emoji;

use super::presence::push_activity;
use super::{
    Activity, ApplicationId, Attachment, AvatarDecorationData, Channel, ChannelId, ChannelMention,
    ClientStatus, DefaultMessageNotificationLevel, DisplayNameStyles, Embed,
    ExplicitContentFilterLevel, Guild, GuildId, GuildIncidentsData, GuildNsfwLevel,
    GuildScheduledEvent, Member, Message, MessageActivity, MessageCall, MessageComponent,
    MessageId, MessageInteraction, MessageInteractionMetadata, MessageReference, MessageSnapshot,
    MessageType, MfaLevel, Nullable, NullablePatch, PartialApplication, Patch, Permissions, Poll,
    PremiumTier, Presence, PresenceStatus, PresenceUser, Reaction, ResolvedData, Role, RoleId,
    RoleSubscriptionData, SharedClientTheme, SoundboardSound, StageInstance, Sticker, StickerItem,
    User, UserCollectibles, UserId, UserPremiumType, UserPrimaryGuild, VerificationLevel,
    VoiceState, WebhookId,
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
/// Channels, threads, and roles are `Arc`s too, shared with the cache's
/// channel and role stores. [`guild`](Self::guild) is the guild itself with
/// those collections empty (as [`Guild::without_create_collections`] returns
/// it) and, since checkpoint C, without its roles, which are in
/// [`roles`](Self::roles); the cache fills them back in when it returns the
/// guild. Build one from a REST or hand-made [`Guild`] with
/// `GuildCreate::from(guild)`.
///
/// A member, presence, presence activity, or voice state that does not
/// decode (a shape this version does not know) is left out of its list and
/// logged with `tracing::warn!`, instead of failing the whole dispatch and
/// leaving the guild uncached. The guild's own fields, channels, threads,
/// and roles must decode.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct GuildCreate {
    /// The guild without its GUILD_CREATE-only collections and roles;
    /// shared with the cache's guild store.
    pub guild: Arc<Guild>,
    /// Roles; the cache's role store keeps these `Arc`s as they are.
    pub roles: Vec<Arc<Role>>,
    /// Channels, each with `guild_id` set; the cache keeps these `Arc`s.
    pub channels: Vec<Arc<Channel>>,
    /// Active threads, each with `guild_id` set; the cache keeps these
    /// `Arc`s.
    pub threads: Vec<Arc<Channel>>,
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
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
}

/// A GUILD_CREATE payload: a guild the bot can read, or one that is
/// unavailable.
///
/// Discord sends GUILD_CREATE when the bot joins a guild, while it lazily
/// loads the guilds READY listed, and when a guild becomes available again
/// after an outage. A guild still in an outage arrives as an unavailable
/// guild object, `{"id": "...", "unavailable": true}`, with no guild data;
/// it is never turned into a [`Guild`]. GUILD_DELETE with
/// `"unavailable": true` signals the same state for a guild that goes down
/// mid-session.
#[derive(Clone, Debug)]
pub enum GuildCreatePayload {
    /// A complete guild.
    Available(Box<GuildCreate>),
    /// A guild that is unavailable: only its id is known.
    Unavailable(UnavailableGuild),
}

impl GuildCreatePayload {
    /// The guild's id.
    pub fn id(&self) -> GuildId {
        match self {
            Self::Available(guild) => guild.id(),
            Self::Unavailable(guild) => guild.id,
        }
    }

    /// The guild, when it is available.
    pub fn available(&self) -> Option<&GuildCreate> {
        match self {
            Self::Available(guild) => Some(guild),
            Self::Unavailable(_) => None,
        }
    }

    /// Decodes a GUILD_CREATE `d` payload from JSON text, reading members
    /// and presences straight into `Arc`s.
    ///
    /// Only serde_json text works here: the keys other than `members` and
    /// `presences` are collected as borrowed [`RawValue`]s and handed to
    /// [`Guild`]'s own decoder, so the field list is not duplicated.
    ///
    /// When that fails, the text is decoded a second time with every
    /// member, presence (and each activity inside it), and voice state
    /// decoded on its own: the ones that do not decode are dropped and
    /// logged, and the rest of the guild is kept (decision log above
    /// `Skipped`). The guild's own fields, channels, threads, and roles stay
    /// strict.
    pub(crate) fn from_json_str(text: &str) -> Result<Self, serde_json::Error> {
        let strict_error = match Self::decode_json_str(text, None) {
            Ok(decoded) => return Ok(decoded),
            Err(error) => error,
        };
        let mut skipped = Skipped::default();
        let decoded = Self::decode_json_str(text, Some(&mut skipped))?;
        warn!(
            "GUILD_CREATE {} kept after dropping {skipped} that did not decode ({})",
            decoded.id(),
            redacted(&strict_error)
        );
        Ok(decoded)
    }

    /// One decoding pass; `lenient` collects what the lenient pass drops.
    fn decode_json_str(
        text: &str,
        lenient: Option<&mut Skipped>,
    ) -> Result<Self, serde_json::Error> {
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let decoded = deserializer.deserialize_map(GuildCreateVisitor { lenient })?;
        deserializer.end()?;
        Ok(decoded)
    }
}

impl From<GuildCreate> for GuildCreatePayload {
    fn from(guild: GuildCreate) -> Self {
        Self::Available(Box::new(guild))
    }
}

/// A guild Discord cannot serve right now (an outage): the unavailable
/// guild object of GUILD_CREATE, which carries nothing but the id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnavailableGuild {
    pub id: GuildId,
}

impl UnavailableGuild {
    /// An unavailable guild with this id.
    pub fn new(id: GuildId) -> Self {
        Self { id }
    }
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
        fn unshared<T: Clone>(items: &[Arc<T>]) -> Vec<T> {
            items.iter().map(|item| (**item).clone()).collect()
        }
        Guild {
            roles: unshared(&self.roles),
            channels: unshared(&self.channels),
            threads: unshared(&self.threads),
            members: unshared(&self.members),
            voice_states: self.voice_states.clone(),
            presences: unshared(&self.presences),
            emojis: self.emojis.clone(),
            stickers: self.stickers.clone(),
            stage_instances: self.stage_instances.clone(),
            soundboard_sounds: self.soundboard_sounds.clone(),
            guild_scheduled_events: self.guild_scheduled_events.clone(),
            ..(*self.guild).clone()
        }
    }

    /// Splits `guild`'s collections out and normalizes them. The fast path
    /// passes the shared collections it decoded straight into `Arc`s;
    /// anything still inside `guild` (a REST or hand-made guild) is moved
    /// into `Arc`s here.
    fn assemble(mut guild: Guild, shared: SharedCollections) -> Self {
        fn into_shared<T>(items: Vec<T>) -> impl Iterator<Item = Arc<T>> {
            items.into_iter().map(Arc::new)
        }
        let guild_id = guild.id;
        let SharedCollections {
            mut roles,
            mut channels,
            mut threads,
            members,
            presences,
        } = shared;
        roles.extend(into_shared(std::mem::take(&mut guild.roles)));
        channels.extend(into_shared(std::mem::take(&mut guild.channels)));
        threads.extend(into_shared(std::mem::take(&mut guild.threads)));
        for channel in channels.iter_mut().chain(threads.iter_mut()) {
            if channel.guild_id.is_none() {
                // Unique at this point, so this never copies.
                Arc::make_mut(channel).guild_id = Some(guild_id);
            }
        }
        let mut voice_states = std::mem::take(&mut guild.voice_states);
        for voice_state in &mut voice_states {
            voice_state.guild_id.get_or_insert(guild_id);
        }
        Self {
            roles,
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
}

/// The GUILD_CREATE collections the cache shares, as the fast path decodes
/// them straight into `Arc`s.
#[derive(Default)]
struct SharedCollections {
    roles: Vec<Arc<Role>>,
    channels: Vec<Arc<Channel>>,
    threads: Vec<Arc<Channel>>,
    members: Vec<Arc<Member>>,
    presences: Vec<Arc<Presence>>,
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
        // Roles, channels, and threads are moved into `Arc`s by `assemble`.
        Self::assemble(
            guild,
            SharedCollections {
                members,
                presences,
                ..SharedCollections::default()
            },
        )
    }
}

/// Fills `user_id` from the partial `user` GUILD_CREATE and member chunks
/// identify presences with, and drops the spare capacity of `activities`.
///
/// A decoded list grows by doubling, so a single activity (440 bytes) leaves
/// three empty slots behind it. The presence is shared with the cache for as
/// long as the member stays online, where 2.x stored an exact-size clone:
/// keeping the slack cost 1.3 KB per cached presence, 33 MB in the 25-guild
/// storm scenario. Shrinking costs one reallocation per presence that has
/// activities, during a GUILD_CREATE that already allocates per member.
pub(crate) fn normalize_presence(presence: &mut Presence) {
    if presence.user_id.is_none() {
        presence.user_id = presence.user.as_ref().map(|user| user.id);
    }
    if let Some(activities) = &mut presence.activities {
        activities.shrink_to_fit();
    }
}

/// GUILD_MEMBERS_CHUNK `members`, each decoded straight into an `Arc` the
/// cache and the external backend store as they are.
pub(crate) fn shared_members<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Arc<Member>>, D::Error> {
    deserializer.deserialize_seq(ArcSeq::new(None))
}

/// GUILD_MEMBERS_CHUNK `presences`, each normalized ([`normalize_presence`])
/// and wrapped once, so the cache and the backend share it.
pub(crate) fn shared_presences<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<Arc<Presence>>>, D::Error> {
    struct Presences(Vec<Arc<Presence>>);

    impl<'de> Deserialize<'de> for Presences {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(ArcSeq::new(Some(normalize_presence)))
                .map(Presences)
        }
    }

    Option::<Presences>::deserialize(deserializer).map(|presences| presences.map(|list| list.0))
}

/// Number of top-level keys a GUILD_CREATE carries besides `members` and
/// `presences` (about 55 today); sizes the borrowed key list once.
const GUILD_CREATE_KEY_HINT: usize = 64;

struct GuildCreateVisitor<'s> {
    /// `Some` on the lenient pass: members, presences, and voice states are
    /// decoded one element at a time, and the dropped ones counted here.
    lenient: Option<&'s mut Skipped>,
}

impl<'de> Visitor<'de> for GuildCreateVisitor<'_> {
    type Value = GuildCreatePayload;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a GUILD_CREATE guild object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GuildCreatePayload, A::Error> {
        let mut lenient = self.lenient;
        let mut shared = SharedCollections::default();
        let mut voice_states = None;
        let mut rest: Vec<(Cow<'de, str>, &'de RawValue)> =
            Vec::with_capacity(GUILD_CREATE_KEY_HINT);
        while let Some(BorrowedKey(key)) = map.next_key()? {
            match (key.as_ref(), lenient.as_deref_mut()) {
                ("members", None) => shared.members = map.next_value_seed(ArcSeq::new(None))?,
                ("members", Some(skipped)) => {
                    shared.members = map.next_value_seed(LenientSeq::members(skipped))?;
                }
                ("presences", None) => {
                    shared.presences =
                        map.next_value_seed(ArcSeq::new(Some(normalize_presence)))?;
                }
                ("presences", Some(skipped)) => {
                    shared.presences = map.next_value_seed(LenientSeq::presences(skipped))?;
                }
                // Strict voice states are read by `Guild`'s decoder below.
                ("voice_states", Some(skipped)) => {
                    voice_states = Some(map.next_value_seed(LenientSeq::voice_states(skipped))?);
                }
                ("roles", _) => shared.roles = map.next_value_seed(ArcSeq::new(None))?,
                ("channels", _) => shared.channels = map.next_value_seed(ArcSeq::new(None))?,
                ("threads", _) => shared.threads = map.next_value_seed(ArcSeq::new(None))?,
                _ => rest.push((key, map.next_value()?)),
            }
        }
        // An unavailable guild object carries only `id`; reading it as a
        // `Guild` would fail on the missing `name` or, with defaults, make up
        // a guild that would overwrite the cached one.
        let field = |name: &str| {
            rest.iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
        };
        if field("unavailable").is_some_and(|value| value.get() == "true") {
            let id = field("id").ok_or_else(|| de::Error::missing_field("id"))?;
            let id = serde_json::from_str(id.get()).map_err(de::Error::custom)?;
            return Ok(GuildCreatePayload::Unavailable(UnavailableGuild { id }));
        }
        let rest = de::value::MapDeserializer::<_, serde_json::Error>::new(rest.into_iter());
        let mut guild = Guild::deserialize(rest).map_err(de::Error::custom)?;
        if let Some(voice_states) = voice_states {
            guild.voice_states = voice_states;
        }
        Ok(GuildCreate::assemble(guild, shared).into())
    }
}

/// A map key borrowed from the input when it has no escapes.
pub(crate) struct BorrowedKey<'de>(pub(crate) Cow<'de, str>);

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

impl<'de, T: de::DeserializeOwned> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <&'de RawValue>::deserialize(deserializer)?;
        Ok(Self(serde_json::from_str(raw.get()).ok()))
    }
}

impl<T> Lenient<Vec<T>> {
    /// The list, or an empty one when the field was absent or invalid.
    pub(crate) fn or_empty(self) -> Vec<T> {
        self.0.unwrap_or_default()
    }
}

// [Decision Log] Element-by-element decoding of user-controlled collections
// - 목적과 의도: 원소 하나가 이 버전이 읽지 못하는 모양이어도 그 원소만 버리고 나머지를 지킨다.
//   라이브 Discord에서 활동 하나(`buttons` 문자열)가 GUILD_CREATE 30개 중 11개를 통째로
//   실패시켜 그 길드들이 캐시에 들어가지 못했고, PRESENCE_UPDATE는 `Lenient<Vec<Activity>>`라서
//   같은 활동 하나가 그 presence의 활동 전체를 조용히 지웠다. Discord 문서도 PRESENCE_UPDATE에
//   대해 "no fields are required, and the types of the fields are not validated"라고 적는다.
// - 기존 구현 및 제약 조건: GUILD_CREATE는 serde_json 텍스트에서 members·presences를
//   `ArcSeq`로 바로 `Arc`에 넣고(체크포인트 A), 나머지 키는 `Guild`의 derive 디코더가 읽는다.
//   스트리밍 디코더는 원소 중간에서 실패하면 이어서 읽을 수 없다. 성능 게이트는 할당 수·바이트로
//   GUILD_CREATE와 PRESENCE_UPDATE를 잰다(`ci/perf-gates.json`).
// - 검토한 주요 대안: (1) 모델을 필드마다 관대하게(모든 필드에 직접 쓴 방문자), (2) 원소를 늘
//   빌린 `RawValue`로 건너뛴 뒤 따로 디코딩, (3) 엄격한 빠른 경로를 먼저 돌리고 실패할 때만 같은
//   텍스트를 원소 단위로 다시 디코딩, (4) 원소를 `Value`로 읽은 뒤 변환.
// - 선택한 방식: 컬렉션 크기에 따라 나눈다. PRESENCE_UPDATE의 활동은 늘 원소 단위(2)로 읽는다
//   (활동 몇 개, 수백 바이트). GUILD_CREATE와 GUILD_MEMBERS_CHUNK는 (3): 엄격한 경로가 실패하면
//   members·presences(안의 활동까지)·voice_states를 원소 단위로 다시 읽고, 버린 원소 수와 엄격한
//   오류(값은 가린) 한 줄을 `warn!`으로 남긴다. 원소마다의 이유는 `debug!`. 길드 필수 필드와
//   channels·threads·roles는 엄격하게 둔다: 길드 구조라서 틀리면 모델 버그이고 조용히 빠지면 안 된다.
//   문서에 적힌 모양 차이(활동 `buttons`)는 모델이 직접 받는다(`presence.rs`).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 큰 손 디코더가 필요하고 문서에 없는 변화까지
//   예측해야 한다. (2)를 GUILD_CREATE에 늘 쓰면 1,000명 길드에서 모든 멤버·presence를 두 번
//   훑는다. (3)은 정상 경로 비용이 0이고(할당 게이트 그대로), 실패한 길드만 한 번 더 읽는다.
//   (4)는 원소마다 문자열을 할당한다. `RawValue`는 빌린 값이라 원소를 건너뛸 때 할당이 없다.
// - 장점, 단점 및 영향: 길드가 통째로 사라지는 대신 원소 몇 개가 빠지고 경고가 남는다(캐시에서
//   그 멤버·presence만 없다). 실패한 GUILD_CREATE는 두 번 디코딩한다. 관대한 경로는 serde_json
//   텍스트에서만 동작하므로 공개 모델의 `Deserialize`(`Value` 등 모든 입력)는 엄격한 그대로다.
//   로그에는 페이로드 값을 남기지 않는다(`redacted`: 오류 범주·위치와 디코더가 쓴 `expected` 절만).

/// What a lenient decode dropped, per kind of element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Skipped {
    pub(crate) members: usize,
    pub(crate) presences: usize,
    pub(crate) activities: usize,
    pub(crate) voice_states: usize,
}

impl fmt::Display for Skipped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} members, {} presences, {} activities, {} voice states",
            self.members, self.presences, self.activities, self.voice_states
        )
    }
}

/// Why `error` happened, for logs, with nothing taken from the payload.
///
/// serde's messages quote the offending value (`invalid type: string "Get
/// Valorant Tracker"`, ``integer `1234567890` ``, ``unknown variant `x` ``),
/// and in a presence or member that is user content: activity text,
/// nicknames, IDs. The reason is therefore rebuilt from parts that cannot
/// carry it: serde_json's error category and position, plus, for serde's
/// structured messages only, the clause the decoder wrote (the `expected`
/// part, or the field of `missing field`). Any other message (custom errors
/// may embed values) contributes nothing.
///
/// Holds as long as visitors' `expecting` texts and `missing_field` names
/// are static, as every one in this crate is.
pub(crate) fn redacted(error: &serde_json::Error) -> String {
    let kind = match error.classify() {
        serde_json::error::Category::Io => "I/O error",
        serde_json::error::Category::Syntax => "syntax error",
        serde_json::error::Category::Data => "data error",
        serde_json::error::Category::Eof => "unexpected end of input",
    };
    let (line, column) = (error.line(), error.column());
    match decoder_clause(&error.to_string()) {
        Some(clause) => format!("{kind}: {clause} at line {line} column {column}"),
        None => format!("{kind} at line {line} column {column}"),
    }
}

/// The part of a serde message that the decoder wrote, if the message has
/// one of serde's structured shapes.
fn decoder_clause(message: &str) -> Option<String> {
    /// serde's `invalid_type` / `invalid_value` / `invalid_length` /
    /// `unknown_variant` messages: `<prefix><value>, expected <expected>`.
    /// The value comes first, so the last `, expected ` is serde's own even
    /// when a quoted value contains the same text.
    const VALUE_THEN_EXPECTED: [&str; 4] = [
        "invalid type: ",
        "invalid value: ",
        "invalid length ",
        "unknown variant ",
    ];
    /// `missing_field` / `duplicate_field` name a field of the decoder.
    const FIELD: [&str; 2] = ["missing field `", "duplicate field `"];

    let mut message = message;
    // serde_json appends ` at line L column C`, once per nesting level (a
    // GUILD_CREATE field error is wrapped once more).
    while let Some(head) = without_position(message) {
        message = head;
    }
    if FIELD.iter().any(|prefix| message.starts_with(prefix)) {
        return Some(message.to_owned());
    }
    if VALUE_THEN_EXPECTED
        .iter()
        .any(|prefix| message.starts_with(prefix))
    {
        return message
            .rsplit_once(", expected ")
            .map(|(_, expected)| format!("expected {expected}"));
    }
    None
}

/// `message` without a trailing ` at line L column C`.
fn without_position(message: &str) -> Option<&str> {
    let (head, position) = message.rsplit_once(" at line ")?;
    let (line, column) = position.split_once(" column ")?;
    let number = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    (number(line) && number(column)).then_some(head)
}

/// Visits a JSON array in serde_json text and hands each element to the
/// closure as borrowed text, so it can be decoded on its own: an element
/// that fails no longer fails the array.
struct EachElement<F>(F);

impl<'de, F: FnMut(&'de RawValue)> DeserializeSeed<'de> for EachElement<F> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, F: FnMut(&'de RawValue)> Visitor<'de> for EachElement<F> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<(), A::Error> {
        while let Some(element) = seq.next_element()? {
            (self.0)(element);
        }
        Ok(())
    }
}

/// Decodes one array element. A [`RawValue`] is exactly one value, so no
/// end check is needed (see `Fields::patch` in `event/decode.rs`).
fn element<T: de::DeserializeOwned>(raw: &RawValue) -> Result<T, serde_json::Error> {
    T::deserialize(&mut serde_json::Deserializer::from_str(raw.get()))
}

/// A presence's `activities` read one activity at a time: an activity that
/// does not decode is dropped and counted instead of failing the presence
/// (2.x and the 3.0 PRESENCE_UPDATE decoder dropped every activity of the
/// presence then). Absent, `null`, or not an array reads as `None`, as a
/// [`Lenient`] field does. serde_json text only.
#[derive(Default)]
pub(crate) struct LenientActivities {
    pub(crate) activities: Option<Vec<Activity>>,
    pub(crate) skipped: usize,
}

impl<'de> Deserialize<'de> for LenientActivities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Reads the elements from the enclosing deserializer instead of
        // skipping the whole array as one `RawValue` first: a second
        // deserializer over that text allocates its own stack for skipping
        // nested objects, one allocation per PRESENCE_UPDATE.
        struct ActivitiesVisitor;

        impl<'de> Visitor<'de> for ActivitiesVisitor {
            type Value = LenientActivities;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("presence activities")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut activities = Vec::new();
                let mut skipped = 0;
                while let Some(raw) = seq.next_element::<&'de RawValue>()? {
                    match element(raw) {
                        Ok(activity) => push_activity(&mut activities, activity),
                        Err(error) => {
                            skipped += 1;
                            debug!(
                                "dropped a presence activity that did not decode: {}",
                                redacted(&error)
                            );
                        }
                    }
                }
                Ok(LenientActivities {
                    activities: Some(activities),
                    skipped,
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                while map
                    .next_entry::<de::IgnoredAny, de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(LenientActivities::default())
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }

            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }

            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }

            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }

            fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(LenientActivities::default())
            }
        }

        deserializer.deserialize_any(ActivitiesVisitor)
    }
}

/// A GUILD_CREATE or GUILD_MEMBERS_CHUNK presence read leniently, the way
/// PRESENCE_UPDATE is: fields of the wrong type read as `None` and
/// activities are decoded one at a time.
#[derive(Deserialize)]
struct PresenceElement {
    #[serde(default)]
    user_id: Lenient<UserId>,
    #[serde(default)]
    user: Lenient<PresenceUser>,
    #[serde(default)]
    status: Lenient<PresenceStatus>,
    #[serde(default)]
    activities: LenientActivities,
    #[serde(default)]
    client_status: Lenient<ClientStatus>,
}

fn member_element(raw: &RawValue, skipped: &mut Skipped) -> Option<Arc<Member>> {
    match element(raw) {
        Ok(member) => Some(Arc::new(member)),
        Err(error) => {
            skipped.members += 1;
            debug!("dropped a member that did not decode: {}", redacted(&error));
            None
        }
    }
}

/// A presence, or `None` (counted) when it is not an object or names no
/// user: the cache keys presences by user.
fn presence_element(raw: &RawValue, skipped: &mut Skipped) -> Option<Arc<Presence>> {
    let element: PresenceElement = match element(raw) {
        Ok(element) => element,
        Err(error) => {
            skipped.presences += 1;
            debug!(
                "dropped a presence that did not decode: {}",
                redacted(&error)
            );
            return None;
        }
    };
    skipped.activities += element.activities.skipped;
    let mut presence = Presence {
        user_id: element.user_id.0,
        user: element.user.0,
        status: element.status.0,
        activities: element.activities.activities,
        client_status: element.client_status.0,
    };
    normalize_presence(&mut presence);
    if presence.user_id.is_none() {
        skipped.presences += 1;
        debug!("dropped a presence whose user id did not decode");
        return None;
    }
    Some(Arc::new(presence))
}

fn voice_state_element(raw: &RawValue, skipped: &mut Skipped) -> Option<VoiceState> {
    match element(raw) {
        Ok(voice_state) => Some(voice_state),
        Err(error) => {
            skipped.voice_states += 1;
            debug!(
                "dropped a voice state that did not decode: {}",
                redacted(&error)
            );
            None
        }
    }
}

/// One collection of a lenient pass: each element goes through `decode`,
/// which returns `None` (and counts it in `skipped`) for one it drops.
pub(crate) struct LenientSeq<'s, T> {
    skipped: &'s mut Skipped,
    decode: fn(&RawValue, &mut Skipped) -> Option<T>,
}

impl<'s> LenientSeq<'s, Arc<Member>> {
    pub(crate) fn members(skipped: &'s mut Skipped) -> Self {
        Self {
            skipped,
            decode: member_element,
        }
    }
}

impl<'s> LenientSeq<'s, Arc<Presence>> {
    pub(crate) fn presences(skipped: &'s mut Skipped) -> Self {
        Self {
            skipped,
            decode: presence_element,
        }
    }
}

impl<'s> LenientSeq<'s, VoiceState> {
    fn voice_states(skipped: &'s mut Skipped) -> Self {
        Self {
            skipped,
            decode: voice_state_element,
        }
    }
}

impl<'de, T> DeserializeSeed<'de> for LenientSeq<'_, T> {
    type Value = Vec<T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Vec<T>, D::Error> {
        let Self { skipped, decode } = self;
        let mut items = Vec::new();
        EachElement(|raw: &RawValue| items.extend(decode(raw, skipped)))
            .deserialize(deserializer)?;
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
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub guild_id: NullablePatch<GuildId>,
    /// The author, merged field by field into the cached author with the
    /// same id (see [`PartialUser`]).
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub author: NullablePatch<PartialUser>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub member: NullablePatch<Member>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub content: Patch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub timestamp: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub edited_timestamp: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub mentions: Patch<Vec<User>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub mention_roles: Patch<Vec<RoleId>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub attachments: Patch<Vec<Attachment>>,
    #[serde(
        rename = "type",
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub kind: NullablePatch<MessageType>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub pinned: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub tts: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub flags: NullablePatch<u64>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub webhook_id: NullablePatch<WebhookId>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub application_id: NullablePatch<ApplicationId>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub application: NullablePatch<PartialApplication>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub embeds: Patch<Vec<Embed>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub reactions: Patch<Vec<Reaction>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub activity: NullablePatch<MessageActivity>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub mention_everyone: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub mention_channels: NullablePatch<Vec<ChannelMention>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub sticker_items: Patch<Vec<StickerItem>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub stickers: Patch<Vec<Sticker>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub position: NullablePatch<u64>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub role_subscription_data: NullablePatch<RoleSubscriptionData>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub message_reference: NullablePatch<MessageReference>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub message_snapshots: Patch<Vec<MessageSnapshot>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub referenced_message: NullablePatch<Box<Message>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub interaction_metadata: NullablePatch<MessageInteractionMetadata>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub interaction: NullablePatch<MessageInteraction>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub resolved: NullablePatch<ResolvedData>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub components: NullablePatch<Vec<MessageComponent>>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub poll: NullablePatch<Poll>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub call: NullablePatch<MessageCall>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub shared_client_theme: NullablePatch<SharedClientTheme>,
}

/// `deserialize_with` for a [`Patch`] field: a present key is read as `T`
/// (so `null` is an error unless `T` accepts it); an omitted key falls back
/// to the field's `#[serde(default)]`, which is `Missing`.
///
/// The same semantics as `Patch`'s own `Deserialize`, but `T` is read
/// directly instead of through `deserialize_newtype_struct`. With
/// serde_json's `raw_value` feature (which the gateway needs) that newtype
/// path also compiles a raw-value branch and the `Patch` fallback visitors
/// for every field type, about 56k LLVM IR lines of code that never runs
/// for these two types.
fn present<'de, D, T>(deserializer: D) -> Result<Patch<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Patch::Value)
}

/// `deserialize_with` for a [`NullablePatch`] field: `null` is
/// `Value(Null)`, a value is `Value(Value(_))`, an omitted key is `Missing`
/// (from `#[serde(default)]`). See [`present`].
fn present_nullable<'de, D, T>(deserializer: D) -> Result<NullablePatch<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(|value| Patch::Value(Nullable::from(value)))
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
        self.apply_with_author(message, None)
    }

    /// [`Self::apply_to`], with `shared_author` (when given) taking the place
    /// of merging `author`: the cache passes the user it has just patched in
    /// its user store, so the message and the store share one allocation
    /// instead of each copying the shared user before patching it.
    pub(crate) fn apply_with_author(
        &self,
        message: &mut Message,
        shared_author: Option<Arc<User>>,
    ) -> bool {
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
        match (author, shared_author) {
            (Patch::Missing, _) => {}
            (Patch::Value(Nullable::Null), _) => *target_author = None,
            (Patch::Value(Nullable::Value(partial)), Some(shared)) if shared.id == partial.id => {
                *target_author = Some(shared);
            }
            (Patch::Value(Nullable::Value(partial)), _) => partial.merge_into(target_author),
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

/// GUILD_UPDATE: the guild fields Discord sent.
///
/// Discord sends the whole guild object, but some of its fields are optional
/// and the GUILD_CREATE-only fields (`joined_at`, `large`, `member_count`,
/// the member, channel, and presence lists, ...) are never part of it, so an
/// update decoded as a whole [`Guild`] cleared everything it did not carry.
/// Every field follows the [`MessageUpdate`] table instead: omitted keeps,
/// `null` clears where the field is nullable, a value replaces. `roles`,
/// `emojis`, and `stickers` are full lists when sent; the cache keeps them
/// in its role, emoji, and sticker stores.
///
/// Decoded from the gateway's text only (like [`GuildCreatePayload`]); build
/// one from a REST [`Guild`] with `GuildUpdate::from(guild)`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct GuildUpdate {
    pub id: GuildId,
    pub name: Patch<String>,
    pub owner_id: NullablePatch<UserId>,
    pub icon: NullablePatch<String>,
    pub splash: NullablePatch<String>,
    pub banner: NullablePatch<String>,
    pub description: NullablePatch<String>,
    pub features: Patch<Vec<String>>,
    pub premium_tier: NullablePatch<PremiumTier>,
    pub max_presences: NullablePatch<u64>,
    pub max_members: NullablePatch<u64>,
    pub vanity_url_code: NullablePatch<String>,
    pub system_channel_id: NullablePatch<ChannelId>,
    pub rules_channel_id: NullablePatch<ChannelId>,
    pub preferred_locale: NullablePatch<String>,
    pub afk_channel_id: NullablePatch<ChannelId>,
    pub afk_timeout: NullablePatch<u64>,
    pub verification_level: NullablePatch<VerificationLevel>,
    pub default_message_notifications: NullablePatch<DefaultMessageNotificationLevel>,
    pub explicit_content_filter: NullablePatch<ExplicitContentFilterLevel>,
    pub mfa_level: NullablePatch<MfaLevel>,
    pub application_id: NullablePatch<ApplicationId>,
    pub system_channel_flags: NullablePatch<u64>,
    pub public_updates_channel_id: NullablePatch<ChannelId>,
    pub nsfw_level: NullablePatch<GuildNsfwLevel>,
    pub premium_subscription_count: NullablePatch<u64>,
    pub premium_progress_bar_enabled: NullablePatch<bool>,
    pub safety_alerts_channel_id: NullablePatch<ChannelId>,
    pub approximate_member_count: NullablePatch<u64>,
    pub approximate_presence_count: NullablePatch<u64>,
    pub incidents_data: NullablePatch<GuildIncidentsData>,
    /// Roles; the cache's role store keeps these `Arc`s as they are.
    pub roles: Patch<Vec<Arc<Role>>>,
    pub emojis: Patch<Vec<Emoji>>,
    pub stickers: Patch<Vec<Sticker>>,
}

impl GuildUpdate {
    /// An update for guild `id` with no field sent.
    pub fn new(id: GuildId) -> Self {
        Self {
            id,
            name: Patch::Missing,
            owner_id: Patch::Missing,
            icon: Patch::Missing,
            splash: Patch::Missing,
            banner: Patch::Missing,
            description: Patch::Missing,
            features: Patch::Missing,
            premium_tier: Patch::Missing,
            max_presences: Patch::Missing,
            max_members: Patch::Missing,
            vanity_url_code: Patch::Missing,
            system_channel_id: Patch::Missing,
            rules_channel_id: Patch::Missing,
            preferred_locale: Patch::Missing,
            afk_channel_id: Patch::Missing,
            afk_timeout: Patch::Missing,
            verification_level: Patch::Missing,
            default_message_notifications: Patch::Missing,
            explicit_content_filter: Patch::Missing,
            mfa_level: Patch::Missing,
            application_id: Patch::Missing,
            system_channel_flags: Patch::Missing,
            public_updates_channel_id: Patch::Missing,
            nsfw_level: Patch::Missing,
            premium_subscription_count: Patch::Missing,
            premium_progress_bar_enabled: Patch::Missing,
            safety_alerts_channel_id: Patch::Missing,
            approximate_member_count: Patch::Missing,
            approximate_presence_count: Patch::Missing,
            incidents_data: Patch::Missing,
            roles: Patch::Missing,
            emojis: Patch::Missing,
            stickers: Patch::Missing,
        }
    }

    /// Applies every sent field, `roles`, `emojis`, and `stickers` included,
    /// to `guild`, which must have the same id; returns `false` and leaves it
    /// unchanged otherwise.
    pub fn apply_to(&self, guild: &mut Guild) -> bool {
        self.apply(guild, true)
    }

    /// [`Self::apply_to`] without the lists the cache keeps in their own
    /// stores (roles, emojis, stickers): its guilds hold none of them.
    #[cfg(any(feature = "cache", test))]
    pub(crate) fn apply_fields(&self, guild: &mut Guild) -> bool {
        self.apply(guild, false)
    }

    fn apply(&self, guild: &mut Guild, lists: bool) -> bool {
        if self.id != guild.id {
            return false;
        }
        // Both destructurings are exhaustive, as in `MessageUpdate::apply_to`.
        let GuildUpdate {
            id: _,
            name,
            owner_id,
            icon,
            splash,
            banner,
            description,
            features,
            premium_tier,
            max_presences,
            max_members,
            vanity_url_code,
            system_channel_id,
            rules_channel_id,
            preferred_locale,
            afk_channel_id,
            afk_timeout,
            verification_level,
            default_message_notifications,
            explicit_content_filter,
            mfa_level,
            application_id,
            system_channel_flags,
            public_updates_channel_id,
            nsfw_level,
            premium_subscription_count,
            premium_progress_bar_enabled,
            safety_alerts_channel_id,
            approximate_member_count,
            approximate_presence_count,
            incidents_data,
            roles,
            emojis,
            stickers,
        } = self;
        let Guild {
            id: _,
            name: target_name,
            owner_id: target_owner_id,
            icon: target_icon,
            splash: target_splash,
            banner: target_banner,
            description: target_description,
            features: target_features,
            premium_tier: target_premium_tier,
            max_presences: target_max_presences,
            max_members: target_max_members,
            vanity_url_code: target_vanity_url_code,
            system_channel_id: target_system_channel_id,
            rules_channel_id: target_rules_channel_id,
            preferred_locale: target_preferred_locale,
            afk_channel_id: target_afk_channel_id,
            afk_timeout: target_afk_timeout,
            verification_level: target_verification_level,
            default_message_notifications: target_default_message_notifications,
            explicit_content_filter: target_explicit_content_filter,
            mfa_level: target_mfa_level,
            application_id: target_application_id,
            system_channel_flags: target_system_channel_flags,
            public_updates_channel_id: target_public_updates_channel_id,
            nsfw_level: target_nsfw_level,
            premium_subscription_count: target_premium_subscription_count,
            premium_progress_bar_enabled: target_premium_progress_bar_enabled,
            safety_alerts_channel_id: target_safety_alerts_channel_id,
            approximate_member_count: target_approximate_member_count,
            approximate_presence_count: target_approximate_presence_count,
            incidents_data: target_incidents_data,
            roles: target_roles,
            emojis: target_emojis,
            stickers: target_stickers,
            // GUILD_CREATE only: never part of GUILD_UPDATE.
            unavailable: _,
            member_count: _,
            joined_at: _,
            large: _,
            channels: _,
            threads: _,
            members: _,
            voice_states: _,
            presences: _,
            stage_instances: _,
            soundboard_sounds: _,
            guild_scheduled_events: _,
        } = guild;
        apply_patch(target_name, name);
        apply_nullable(target_owner_id, owner_id);
        apply_nullable(target_icon, icon);
        apply_nullable(target_splash, splash);
        apply_nullable(target_banner, banner);
        apply_nullable(target_description, description);
        apply_patch(target_features, features);
        apply_nullable(target_premium_tier, premium_tier);
        apply_nullable(target_max_presences, max_presences);
        apply_nullable(target_max_members, max_members);
        apply_nullable(target_vanity_url_code, vanity_url_code);
        apply_nullable(target_system_channel_id, system_channel_id);
        apply_nullable(target_rules_channel_id, rules_channel_id);
        apply_nullable(target_preferred_locale, preferred_locale);
        apply_nullable(target_afk_channel_id, afk_channel_id);
        apply_nullable(target_afk_timeout, afk_timeout);
        apply_nullable(target_verification_level, verification_level);
        apply_nullable(
            target_default_message_notifications,
            default_message_notifications,
        );
        apply_nullable(target_explicit_content_filter, explicit_content_filter);
        apply_nullable(target_mfa_level, mfa_level);
        apply_nullable(target_application_id, application_id);
        apply_nullable(target_system_channel_flags, system_channel_flags);
        apply_nullable(target_public_updates_channel_id, public_updates_channel_id);
        apply_nullable(target_nsfw_level, nsfw_level);
        apply_nullable(
            target_premium_subscription_count,
            premium_subscription_count,
        );
        apply_nullable(
            target_premium_progress_bar_enabled,
            premium_progress_bar_enabled,
        );
        apply_nullable(target_safety_alerts_channel_id, safety_alerts_channel_id);
        apply_nullable(target_approximate_member_count, approximate_member_count);
        apply_nullable(
            target_approximate_presence_count,
            approximate_presence_count,
        );
        apply_nullable(target_incidents_data, incidents_data);
        if lists {
            if let Patch::Value(roles) = roles {
                *target_roles = roles.iter().map(|role| (**role).clone()).collect();
            }
            apply_patch(target_emojis, emojis);
            apply_patch(target_stickers, stickers);
        }
        true
    }
}

impl From<Guild> for GuildUpdate {
    /// Every field of `guild` as sent: applying it replaces them all (an
    /// absent optional value clears), as the GUILD_UPDATE handling of 2.x
    /// did; GUILD_CREATE-only fields are not part of an update.
    fn from(guild: Guild) -> Self {
        Self {
            id: guild.id,
            name: Patch::Value(guild.name),
            owner_id: Patch::Value(Nullable::from(guild.owner_id)),
            icon: Patch::Value(Nullable::from(guild.icon)),
            splash: Patch::Value(Nullable::from(guild.splash)),
            banner: Patch::Value(Nullable::from(guild.banner)),
            description: Patch::Value(Nullable::from(guild.description)),
            features: Patch::Value(guild.features),
            premium_tier: Patch::Value(Nullable::from(guild.premium_tier)),
            max_presences: Patch::Value(Nullable::from(guild.max_presences)),
            max_members: Patch::Value(Nullable::from(guild.max_members)),
            vanity_url_code: Patch::Value(Nullable::from(guild.vanity_url_code)),
            system_channel_id: Patch::Value(Nullable::from(guild.system_channel_id)),
            rules_channel_id: Patch::Value(Nullable::from(guild.rules_channel_id)),
            preferred_locale: Patch::Value(Nullable::from(guild.preferred_locale)),
            afk_channel_id: Patch::Value(Nullable::from(guild.afk_channel_id)),
            afk_timeout: Patch::Value(Nullable::from(guild.afk_timeout)),
            verification_level: Patch::Value(Nullable::from(guild.verification_level)),
            default_message_notifications: Patch::Value(Nullable::from(
                guild.default_message_notifications,
            )),
            explicit_content_filter: Patch::Value(Nullable::from(guild.explicit_content_filter)),
            mfa_level: Patch::Value(Nullable::from(guild.mfa_level)),
            application_id: Patch::Value(Nullable::from(guild.application_id)),
            system_channel_flags: Patch::Value(Nullable::from(guild.system_channel_flags)),
            public_updates_channel_id: Patch::Value(Nullable::from(
                guild.public_updates_channel_id,
            )),
            nsfw_level: Patch::Value(Nullable::from(guild.nsfw_level)),
            premium_subscription_count: Patch::Value(Nullable::from(
                guild.premium_subscription_count,
            )),
            premium_progress_bar_enabled: Patch::Value(Nullable::from(
                guild.premium_progress_bar_enabled,
            )),
            safety_alerts_channel_id: Patch::Value(Nullable::from(guild.safety_alerts_channel_id)),
            approximate_member_count: Patch::Value(Nullable::from(guild.approximate_member_count)),
            approximate_presence_count: Patch::Value(Nullable::from(
                guild.approximate_presence_count,
            )),
            incidents_data: Patch::Value(Nullable::from(guild.incidents_data)),
            roles: Patch::Value(guild.roles.into_iter().map(Arc::new).collect()),
            emojis: Patch::Value(guild.emojis),
            stickers: Patch::Value(guild.stickers),
        }
    }
}

/// GUILD_MEMBER_UPDATE: the member fields Discord sent.
///
/// `guild_id`, `user`, and `roles` are always sent; the other fields follow
/// the [`MessageUpdate`] table (omitted keeps, `null` clears where the field
/// is nullable), mirroring [`Member`]. The cache merges an update into the
/// member it holds and never stores one on its own: a member it does not
/// hold stays uncached, because the omitted fields are unknown.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct MemberUpdate {
    pub guild_id: GuildId,
    /// The member's user, sent in full; shared with the cache's user store.
    #[serde(serialize_with = "super::shared::arc::serialize")]
    pub user: Arc<User>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub roles: Patch<Vec<RoleId>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub nick: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub joined_at: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub permissions: NullablePatch<Permissions>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub deaf: NullablePatch<bool>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub mute: NullablePatch<bool>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub avatar: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub banner: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub premium_since: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub pending: NullablePatch<bool>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub communication_disabled_until: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub avatar_decoration_data: NullablePatch<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub collectibles: NullablePatch<UserCollectibles>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub flags: NullablePatch<u64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub display_name_styles: NullablePatch<Box<DisplayNameStyles>>,
}

/// Hand-written instead of derived: a derived decoder also compiles the
/// positional (JSON array) form, dead for a gateway object, and one
/// `deserialize_with` wrapper per field. This one reads the map only and
/// calls `next_value::<T>` / `next_value::<Option<T>>` directly, the instances
/// [`Member`]'s own decoder already has. Same rules as the derive: a field
/// sent twice is an error, unknown fields are skipped, `guild_id` and `user`
/// are required.
impl<'de> Deserialize<'de> for MemberUpdate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MemberUpdateVisitor)
    }
}

struct MemberUpdateVisitor;

impl<'de> Visitor<'de> for MemberUpdateVisitor {
    type Value = MemberUpdate;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a GUILD_MEMBER_UPDATE object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MemberUpdate, A::Error> {
        fn sent<T, E: de::Error>(
            slot: &mut Patch<T>,
            name: &'static str,
            value: Patch<T>,
        ) -> Result<(), E> {
            if !slot.is_missing() {
                return Err(E::duplicate_field(name));
            }
            *slot = value;
            Ok(())
        }
        let mut guild_id: Option<GuildId> = None;
        let mut user: Option<User> = None;
        let mut roles: Patch<Vec<RoleId>> = Patch::Missing;
        let mut nick: NullablePatch<String> = Patch::Missing;
        let mut joined_at: NullablePatch<String> = Patch::Missing;
        let mut permissions: NullablePatch<Permissions> = Patch::Missing;
        let mut deaf: NullablePatch<bool> = Patch::Missing;
        let mut mute: NullablePatch<bool> = Patch::Missing;
        let mut avatar: NullablePatch<String> = Patch::Missing;
        let mut banner: NullablePatch<String> = Patch::Missing;
        let mut premium_since: NullablePatch<String> = Patch::Missing;
        let mut pending: NullablePatch<bool> = Patch::Missing;
        let mut communication_disabled_until: NullablePatch<String> = Patch::Missing;
        let mut avatar_decoration_data: NullablePatch<AvatarDecorationData> = Patch::Missing;
        let mut collectibles: NullablePatch<UserCollectibles> = Patch::Missing;
        let mut flags: NullablePatch<u64> = Patch::Missing;
        let mut display_name_styles: NullablePatch<Box<DisplayNameStyles>> = Patch::Missing;
        while let Some(BorrowedKey(key)) = map.next_key()? {
            match key.as_ref() {
                "guild_id" => {
                    if guild_id.is_some() {
                        return Err(de::Error::duplicate_field("guild_id"));
                    }
                    guild_id = Some(map.next_value()?);
                }
                "user" => {
                    if user.is_some() {
                        return Err(de::Error::duplicate_field("user"));
                    }
                    user = Some(map.next_value()?);
                }
                "roles" => sent(&mut roles, "roles", Patch::Value(map.next_value()?))?,
                "nick" => sent(
                    &mut nick,
                    "nick",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "joined_at" => sent(
                    &mut joined_at,
                    "joined_at",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "permissions" => sent(
                    &mut permissions,
                    "permissions",
                    Patch::Value(Nullable::from(map.next_value::<Option<Permissions>>()?)),
                )?,
                "deaf" => sent(
                    &mut deaf,
                    "deaf",
                    Patch::Value(Nullable::from(map.next_value::<Option<bool>>()?)),
                )?,
                "mute" => sent(
                    &mut mute,
                    "mute",
                    Patch::Value(Nullable::from(map.next_value::<Option<bool>>()?)),
                )?,
                "avatar" => sent(
                    &mut avatar,
                    "avatar",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "banner" => sent(
                    &mut banner,
                    "banner",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "premium_since" => sent(
                    &mut premium_since,
                    "premium_since",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "pending" => sent(
                    &mut pending,
                    "pending",
                    Patch::Value(Nullable::from(map.next_value::<Option<bool>>()?)),
                )?,
                "communication_disabled_until" => sent(
                    &mut communication_disabled_until,
                    "communication_disabled_until",
                    Patch::Value(Nullable::from(map.next_value::<Option<String>>()?)),
                )?,
                "avatar_decoration_data" => sent(
                    &mut avatar_decoration_data,
                    "avatar_decoration_data",
                    Patch::Value(Nullable::from(
                        map.next_value::<Option<AvatarDecorationData>>()?,
                    )),
                )?,
                "collectibles" => sent(
                    &mut collectibles,
                    "collectibles",
                    Patch::Value(Nullable::from(
                        map.next_value::<Option<UserCollectibles>>()?,
                    )),
                )?,
                "flags" => sent(
                    &mut flags,
                    "flags",
                    Patch::Value(Nullable::from(map.next_value::<Option<u64>>()?)),
                )?,
                "display_name_styles" => sent(
                    &mut display_name_styles,
                    "display_name_styles",
                    Patch::Value(Nullable::from(
                        map.next_value::<Option<Box<DisplayNameStyles>>>()?,
                    )),
                )?,
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(MemberUpdate {
            guild_id: guild_id.ok_or_else(|| de::Error::missing_field("guild_id"))?,
            user: Arc::new(user.ok_or_else(|| de::Error::missing_field("user"))?),
            roles,
            nick,
            joined_at,
            permissions,
            deaf,
            mute,
            avatar,
            banner,
            premium_since,
            pending,
            communication_disabled_until,
            avatar_decoration_data,
            collectibles,
            flags,
            display_name_styles,
        })
    }
}

impl MemberUpdate {
    /// An update for `user` in `guild_id` with no other field sent.
    pub fn new(guild_id: GuildId, user: impl Into<Arc<User>>) -> Self {
        Self {
            guild_id,
            user: user.into(),
            roles: Patch::Missing,
            nick: Patch::Missing,
            joined_at: Patch::Missing,
            permissions: Patch::Missing,
            deaf: Patch::Missing,
            mute: Patch::Missing,
            avatar: Patch::Missing,
            banner: Patch::Missing,
            premium_since: Patch::Missing,
            pending: Patch::Missing,
            communication_disabled_until: Patch::Missing,
            avatar_decoration_data: Patch::Missing,
            collectibles: Patch::Missing,
            flags: Patch::Missing,
            display_name_styles: Patch::Missing,
        }
    }

    /// The id of the member's user.
    pub fn user_id(&self) -> UserId {
        self.user.id
    }

    /// Applies every sent field to `member`, whose user must be this
    /// update's user (or unknown); returns `false` and leaves it unchanged
    /// otherwise. `member.user` becomes the update's (shared) user.
    pub fn apply_to(&self, member: &mut Member) -> bool {
        if member
            .user
            .as_ref()
            .is_some_and(|user| user.id != self.user.id)
        {
            return false;
        }
        // Exhaustive for the same reason as `MessageUpdate::apply_to`.
        let MemberUpdate {
            guild_id: _,
            user,
            roles,
            nick,
            joined_at,
            permissions,
            deaf,
            mute,
            avatar,
            banner,
            premium_since,
            pending,
            communication_disabled_until,
            avatar_decoration_data,
            collectibles,
            flags,
            display_name_styles,
        } = self;
        let Member {
            user: target_user,
            roles: target_roles,
            nick: target_nick,
            joined_at: target_joined_at,
            permissions: target_permissions,
            deaf: target_deaf,
            mute: target_mute,
            avatar: target_avatar,
            banner: target_banner,
            premium_since: target_premium_since,
            pending: target_pending,
            communication_disabled_until: target_communication_disabled_until,
            avatar_decoration_data: target_avatar_decoration_data,
            collectibles: target_collectibles,
            flags: target_flags,
            display_name_styles: target_display_name_styles,
        } = member;
        *target_user = Some(Arc::clone(user));
        apply_patch(target_roles, roles);
        apply_nullable(target_nick, nick);
        apply_nullable(target_joined_at, joined_at);
        apply_nullable(target_permissions, permissions);
        apply_nullable(target_deaf, deaf);
        apply_nullable(target_mute, mute);
        apply_nullable(target_avatar, avatar);
        apply_nullable(target_banner, banner);
        apply_nullable(target_premium_since, premium_since);
        apply_nullable(target_pending, pending);
        apply_nullable(
            target_communication_disabled_until,
            communication_disabled_until,
        );
        apply_nullable(target_avatar_decoration_data, avatar_decoration_data);
        apply_nullable(target_collectibles, collectibles);
        apply_nullable(target_flags, flags);
        apply_nullable(target_display_name_styles, display_name_styles);
        true
    }
}

/// A user object that may carry only some fields (MESSAGE_UPDATE's
/// `author`). `id` is always present; the other fields follow the
/// [`MessageUpdate`] table, mirroring [`User`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PartialUser {
    pub id: UserId,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present"
    )]
    pub username: Patch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub global_name: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub discriminator: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub avatar: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub bot: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub system: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub mfa_enabled: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub banner: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub accent_color: NullablePatch<u64>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub locale: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub verified: NullablePatch<bool>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub email: NullablePatch<String>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub flags: NullablePatch<u64>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub premium_type: NullablePatch<UserPremiumType>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub public_flags: NullablePatch<u64>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub avatar_decoration_data: NullablePatch<AvatarDecorationData>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub collectibles: NullablePatch<UserCollectibles>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub primary_guild: NullablePatch<UserPrimaryGuild>,
    #[serde(
        default,
        skip_serializing_if = "Patch::is_missing",
        deserialize_with = "present_nullable"
    )]
    pub display_name_styles: NullablePatch<Box<DisplayNameStyles>>,
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
            display_name_styles: Patch::Missing,
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
            display_name_styles,
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
            display_name_styles: target_display_name_styles,
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
        apply_nullable(target_display_name_styles, display_name_styles);
        true
    }

    /// Whether any field besides `id` was sent. PRESENCE_UPDATE usually
    /// sends `{"id": ...}` alone; the cache skips such a patch instead of
    /// taking its write lock or copying a shared user for nothing.
    #[cfg(feature = "cache")]
    pub(crate) fn carries_changes(&self) -> bool {
        // Exhaustive for the same reason as `apply_to`.
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
            display_name_styles,
        } = self;
        !(username.is_missing()
            && global_name.is_missing()
            && discriminator.is_missing()
            && avatar.is_missing()
            && bot.is_missing()
            && system.is_missing()
            && mfa_enabled.is_missing()
            && banner.is_missing()
            && accent_color.is_missing()
            && locale.is_missing()
            && verified.is_missing()
            && email.is_missing()
            && flags.is_missing()
            && premium_type.is_missing()
            && public_flags.is_missing()
            && avatar_decoration_data.is_missing()
            && collectibles.is_missing()
            && primary_guild.is_missing()
            && display_name_styles.is_missing())
    }

    /// Whether [`Self::to_user`] would return a user (`username` was sent),
    /// without building it.
    #[cfg(feature = "cache")]
    pub(crate) fn is_complete(&self) -> bool {
        !self.username.is_missing()
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
            display_name_styles: None,
        };
        self.apply_to(&mut user);
        Some(user)
    }

    /// Merges into an optional slot (a message's `author`): the same user is
    /// patched in place, a different or missing one is replaced only when
    /// this partial is complete, and otherwise the slot is left as it is.
    ///
    /// The slot's user may be shared (with the cache's user store, or an
    /// earlier event); it is patched copy-on-write.
    pub(crate) fn merge_into(&self, slot: &mut Option<Arc<User>>) {
        if let Some(user) = slot.as_mut() {
            if user.id == self.id {
                self.apply_to(Arc::make_mut(user));
                return;
            }
        }
        if let Some(user) = self.to_user() {
            *slot = Some(Arc::new(user));
        }
    }
}

#[cfg(test)]
#[path = "gateway_payload_tests.rs"]
mod tests;
