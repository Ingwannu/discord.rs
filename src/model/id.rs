//! Typed Discord snowflake IDs.
//!
//! [`Id<M>`] is a nonzero 64-bit snowflake tagged with a marker type from
//! [`marker`], so a [`ChannelId`] cannot be passed where a [`GuildId`] is
//! expected. Wire values that look like IDs but are not nonzero snowflakes
//! (widget member IDs, the `"0"` install owner, upload indexes, component
//! and poll answer IDs) have dedicated types re-exported from here; see
//! [`WidgetUserId`], [`GuildInstallOwner`], [`UploadId`], [`ComponentId`],
//! and [`PollAnswerId`].

// [Decision Log]
// - 목적과 의도: ID를 Copy 가능한 8바이트 값으로 만들고, 종류가 다른 ID의 혼용과 0·공백·부호
//   같은 잘못된 와이어 값을 경계에서 거부한다(docs/design/3.0-types-events-requests.md §1).
// - 기존 구현 및 제약 조건: 2.x Snowflake는 String + Option<u64>라 복사마다 할당하고,
//   "abc"도 받아들이며, 종류 구분이 없다. 3단계 시리즈 1이 새 타입을 추가했고, 시리즈 2의
//   A 단계가 모든 사용처를 GenericId로 옮기고 옛 구조체를 지웠다(이름 `Snowflake`는
//   model.rs의 deprecated 별칭으로만 남는다). B 단계가 도메인별 마커로 좁혔고, 종류가
//   정해지지 않은 자리(감사 로그 target_id 등)만 GenericId로 남았다.
// - 검토한 주요 대안: (1) Snowflake를 u64 newtype으로만 교체, (2) Id<M>(u64),
//   (3) Id<M>(NonZeroU64) + PhantomData<fn() -> M>.
// - 선택한 방식: (3). 비교·해시·포맷은 숫자 값만 보고 마커 트레이트를 요구하지 않게 직접
//   구현한다. 생성은 실패 가능한 변환(TryFrom/FromStr)과 무실패 From<NonZeroU64>뿐이다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 종류 혼용을 못 막는다. (2)는 0을 표현할 수
//   있어 센티널 ID를 허용하고 Option<Id>가 16바이트가 된다. NonZeroU64는 Option<Id>도
//   8바이트로 만들고, fn() -> M은 마커와 무관하게 Send/Sync/Copy와 공변성을 준다.
// - 장점, 단점 및 영향: 오용이 컴파일 오류가 되고 ID 복사가 할당 없이 끝난다. 대신 Discord가
//   "0"을 유효 값으로 보내는 자리는 전용 타입이 필요하고(exceptions 모듈), From<u64>가 없어
//   리터럴 ID는 try_new/parse를 거쳐야 한다.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::str::FromStr;

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod exceptions;
pub mod marker;

pub use exceptions::{ComponentId, GuildInstallOwner, PollAnswerId, UploadId, WidgetUserId};
pub use marker::IdMarker;

/// Discord epoch (2015-01-01T00:00:00Z) in Unix milliseconds.
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// A nonzero Discord snowflake whose resource kind is the marker `M`.
///
/// Use the aliases ([`GuildId`], [`ChannelId`], [`UserId`], ...) rather than
/// naming markers directly. An `Id` is `Copy` and exactly eight bytes, and
/// so is `Option<Id<M>>`.
///
/// Construction is always explicit and checked. There is no `From<u64>`,
/// no `Default`, and no conversion between markers other than [`Id::cast`]
/// and [`Id::erase`]:
///
/// ```
/// use discordrs::{ChannelId, GuildId};
///
/// # fn main() -> Result<(), discordrs::InvalidId> {
/// let guild: GuildId = "81384788765712384".parse()?;
/// let channel = ChannelId::try_new(81384788765712384)?;
/// assert_eq!(guild.to_string(), "81384788765712384");
/// assert_eq!(format!("{channel:?}"), "ChannelId(81384788765712384)");
/// # Ok(())
/// # }
/// ```
///
/// # Wire format
///
/// Serializes as a decimal string (so it also works as a JSON object key).
/// Deserializes from a decimal string or an unsigned integer; zero,
/// negative numbers, floats, values above `u64::MAX`, whitespace, signs, and
/// any other non-digit character are rejected. Leading zeros are accepted
/// and dropped, so `"007"` reads as `7` and writes back as `"7"`.
#[repr(transparent)]
pub struct Id<M: IdMarker> {
    value: NonZeroU64,
    marker: PhantomData<fn() -> M>,
}

impl<M: IdMarker> Id<M> {
    /// Creates an ID from a raw snowflake, rejecting zero.
    pub const fn try_new(value: u64) -> Result<Self, InvalidId> {
        match NonZeroU64::new(value) {
            Some(value) => Ok(Self::from_nonzero(value)),
            None => Err(InvalidId::new(InvalidIdKind::Zero)),
        }
    }

    /// Creates an ID from a value that is already known to be nonzero.
    pub const fn from_nonzero(value: NonZeroU64) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    /// Returns the raw snowflake.
    pub const fn get(self) -> u64 {
        self.value.get()
    }

    /// Returns the raw snowflake as a `NonZeroU64`.
    pub const fn into_nonzero(self) -> NonZeroU64 {
        self.value
    }

    /// Returns the creation time encoded in the snowflake, in Unix
    /// milliseconds.
    pub const fn timestamp(self) -> u64 {
        // The top 42 bits are milliseconds since the Discord epoch. The
        // largest possible value, (u64::MAX >> 22) + epoch, is about 5.8e12,
        // far below u64::MAX, so this addition cannot overflow.
        (self.get() >> 22) + DISCORD_EPOCH_MS
    }

    /// Forgets the resource kind, e.g. to store mixed IDs together or to
    /// match an audit log `target_id`.
    pub const fn erase(self) -> GenericId {
        self.cast()
    }

    /// Reinterprets this ID as an ID of another resource kind.
    ///
    /// This is an assertion by the caller that the same snowflake names a
    /// resource of kind `N`, for example that a guild's `@everyone` role
    /// shares the guild's ID. It does not check that such a resource exists
    /// or that the kinds are related; a wrong cast compiles and then fails at
    /// Discord (usually as `404 Unknown ...`).
    pub const fn cast<N: IdMarker>(self) -> Id<N> {
        Id::from_nonzero(self.value)
    }
}

impl<M: IdMarker> Clone for Id<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: IdMarker> Copy for Id<M> {}

impl<M: IdMarker> PartialEq for Id<M> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<M: IdMarker> Eq for Id<M> {}

impl<M: IdMarker> PartialOrd for Id<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Orders numerically, which for snowflakes is also creation order (ties
/// broken by worker, process, and sequence bits).
impl<M: IdMarker> Ord for Id<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

/// Hashes only the numeric value, exactly like the `u64` it wraps.
impl<M: IdMarker> Hash for Id<M> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

/// Prints the alias name and the number, e.g. `GuildId(123)`.
impl<M: IdMarker> fmt::Debug for Id<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", M::DEBUG_NAME, self.value)
    }
}

/// Prints the canonical decimal form, the same text that is sent to Discord.
impl<M: IdMarker> fmt::Display for Id<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

impl<M: IdMarker> From<NonZeroU64> for Id<M> {
    fn from(value: NonZeroU64) -> Self {
        Self::from_nonzero(value)
    }
}

impl<M: IdMarker> From<Id<M>> for NonZeroU64 {
    fn from(id: Id<M>) -> Self {
        id.into_nonzero()
    }
}

impl<M: IdMarker> From<Id<M>> for u64 {
    fn from(id: Id<M>) -> Self {
        id.get()
    }
}

impl<M: IdMarker> TryFrom<u64> for Id<M> {
    type Error = InvalidId;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<M: IdMarker> FromStr for Id<M> {
    type Err = InvalidId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_decimal_u64(value).and_then(Self::try_new)
    }
}

impl<M: IdMarker> TryFrom<&str> for Id<M> {
    type Error = InvalidId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl<M: IdMarker> Serialize for Id<M> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // collect_str formats straight into the output for serde_json, and
        // serde_json's map-key serializer accepts strings, so the same impl
        // works for values and for `HashMap<GuildId, _>` keys.
        serializer.collect_str(&self.value)
    }
}

impl<'de, M: IdMarker> Deserialize<'de> for Id<M> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DecimalVisitor::<Self>::new())
    }
}

impl<M: IdMarker> DecimalWire for Id<M> {
    const EXPECTING: &'static str = "a nonzero Discord snowflake as a decimal string or integer";

    fn from_wire_u64(value: u64) -> Result<Self, InvalidId> {
        Self::try_new(value)
    }
}

/// Why a value was rejected as a Discord ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InvalidIdKind {
    /// The value was zero. Snowflakes are never zero; wire fields where
    /// Discord sends `"0"` on purpose have dedicated types instead.
    Zero,
    /// The string was empty.
    Empty,
    /// The string contained something other than ASCII digits: a sign,
    /// whitespace, a decimal point, an exponent, or any other character.
    InvalidDigit,
    /// The number does not fit in 64 bits.
    Overflow,
}

/// Error returned when a value is not a valid Discord ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidId {
    kind: InvalidIdKind,
}

impl InvalidId {
    const fn new(kind: InvalidIdKind) -> Self {
        Self { kind }
    }

    /// Returns the reason the value was rejected.
    pub const fn kind(&self) -> InvalidIdKind {
        self.kind
    }
}

/// Describes the reason without echoing the input, so untrusted text never
/// ends up in logs through this error.
impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            InvalidIdKind::Zero => "Discord ID must not be zero",
            InvalidIdKind::Empty => "Discord ID must not be empty",
            InvalidIdKind::InvalidDigit => {
                "Discord ID must contain only ASCII digits (no sign, whitespace, or decimal point)"
            }
            InvalidIdKind::Overflow => "Discord ID does not fit in 64 bits",
        })
    }
}

impl std::error::Error for InvalidId {}

/// Lets application code validate IDs with `?` in functions returning
/// [`DiscordError`](crate::DiscordError). Maps to `DiscordError::Model` with
/// the reason only, so the rejected input is not copied into the error.
impl From<InvalidId> for crate::error::DiscordError {
    fn from(error: InvalidId) -> Self {
        Self::model(error.to_string())
    }
}

/// Parses a decimal string the way Discord writes snowflakes: one or more
/// ASCII digits and nothing else. Zero is allowed here because some wire
/// types (such as [`GuildInstallOwner`]) give it a meaning; [`Id`] rejects it
/// afterwards.
///
/// `u64::from_str` is not used directly because it accepts a leading `+`.
fn parse_decimal_u64(value: &str) -> Result<u64, InvalidId> {
    if value.is_empty() {
        return Err(InvalidId::new(InvalidIdKind::Empty));
    }
    let mut parsed: u64 = 0;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(InvalidId::new(InvalidIdKind::InvalidDigit));
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|shifted| shifted.checked_add(u64::from(byte - b'0')))
            .ok_or(InvalidId::new(InvalidIdKind::Overflow))?;
    }
    Ok(parsed)
}

/// A type read from Discord's "decimal string or unsigned integer" ID
/// encoding. Shared by [`Id`] and the sibling exception types so they accept
/// and reject exactly the same spellings.
trait DecimalWire: Sized {
    /// Text for serde's "expected ..." messages.
    const EXPECTING: &'static str;

    /// Builds the value from the parsed number; zero handling is up to the
    /// type.
    fn from_wire_u64(value: u64) -> Result<Self, InvalidId>;
}

struct DecimalVisitor<T>(PhantomData<fn() -> T>);

impl<T> DecimalVisitor<T> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: DecimalWire> Visitor<'_> for DecimalVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(T::EXPECTING)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_wire_u64(value).map_err(E::custom)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // serde_json only calls visit_i64 for negative numbers, but other
        // formats may report small positive integers as signed.
        match u64::try_from(value) {
            Ok(value) => self.visit_u64(value),
            Err(_) => Err(E::invalid_value(Unexpected::Signed(value), &self)),
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_decimal_u64(value)
            .and_then(T::from_wire_u64)
            .map_err(E::custom)
    }
}

/// ID of a guild.
pub type GuildId = Id<marker::Guild>;
/// ID of a channel, including threads.
pub type ChannelId = Id<marker::Channel>;
/// ID of a user. Members and presences are addressed by `(GuildId, UserId)`.
pub type UserId = Id<marker::User>;
/// ID of a role. A guild's `@everyone` role has the guild's ID.
pub type RoleId = Id<marker::Role>;
/// ID of a message.
pub type MessageId = Id<marker::Message>;
/// ID of an application.
pub type ApplicationId = Id<marker::Application>;
/// ID of an application command.
pub type CommandId = Id<marker::Command>;
/// Version ID of an application command, which changes on every update.
pub type CommandVersionId = Id<marker::CommandVersion>;
/// ID of an interaction.
pub type InteractionId = Id<marker::Interaction>;
/// ID of a webhook.
pub type WebhookId = Id<marker::Webhook>;
/// ID of a custom emoji.
pub type EmojiId = Id<marker::Emoji>;
/// ID of a stored attachment. Upload indexes in request bodies are
/// [`UploadId`], not this type.
pub type AttachmentId = Id<marker::Attachment>;
/// ID of a sticker.
pub type StickerId = Id<marker::Sticker>;
/// ID of a sticker pack.
pub type StickerPackId = Id<marker::StickerPack>;
/// ID of a soundboard sound.
pub type SoundboardSoundId = Id<marker::SoundboardSound>;
/// ID of a guild scheduled event.
pub type ScheduledEventId = Id<marker::ScheduledEvent>;
/// ID of a stage instance.
pub type StageInstanceId = Id<marker::StageInstance>;
/// ID of a guild integration.
pub type IntegrationId = Id<marker::Integration>;
/// ID of an audit log entry.
pub type AuditLogEntryId = Id<marker::AuditLogEntry>;
/// ID of an auto moderation rule.
pub type AutoModerationRuleId = Id<marker::AutoModerationRule>;
/// ID of a forum or media channel tag.
pub type ForumTagId = Id<marker::ForumTag>;
/// ID of an SKU.
pub type SkuId = Id<marker::Sku>;
/// ID of an entitlement.
pub type EntitlementId = Id<marker::Entitlement>;
/// ID of a subscription.
pub type SubscriptionId = Id<marker::Subscription>;
/// ID of a promotion.
pub type PromotionId = Id<marker::Promotion>;
/// ID of a role subscription listing.
pub type RoleSubscriptionListingId = Id<marker::RoleSubscriptionListing>;
/// ID of a Social SDK lobby.
pub type LobbyId = Id<marker::Lobby>;
/// ID of an activity launch.
pub type ActivityLaunchId = Id<marker::ActivityLaunch>;
/// ID of a developer team.
pub type TeamId = Id<marker::Team>;
/// ID of a guild onboarding prompt.
pub type OnboardingPromptId = Id<marker::OnboardingPrompt>;
/// ID of a guild onboarding prompt option.
pub type OnboardingOptionId = Id<marker::OnboardingOption>;
/// ID whose resource kind is not statically known, such as an audit log
/// `target_id`.
pub type GenericId = Id<marker::Generic>;

impl GuildId {
    /// The ID of this guild's `@everyone` role, which Discord gives the
    /// guild's own ID.
    ///
    /// ```
    /// use discordrs::GuildId;
    ///
    /// # fn main() -> Result<(), discordrs::InvalidId> {
    /// let guild_id: GuildId = "81384788765712384".parse()?;
    /// assert_eq!(guild_id.everyone_role().get(), guild_id.get());
    /// # Ok(())
    /// # }
    /// ```
    pub const fn everyone_role(self) -> RoleId {
        self.cast()
    }
}

/// Builds an ID from a test literal, inferring the marker from context.
///
/// Test-only: it panics on zero, which production code must never do (use
/// [`Id::try_new`] or `parse` there). Kept generic over the marker so that
/// narrowing a field from `GenericId` to a typed ID does not touch the
/// fixtures that fill it.
#[cfg(test)]
#[track_caller]
pub(crate) fn test_id<M: IdMarker>(value: u64) -> Id<M> {
    match Id::try_new(value) {
        Ok(id) => id,
        Err(error) => panic!("test fixture ID {value}: {error}"),
    }
}

#[cfg(test)]
mod tests;
