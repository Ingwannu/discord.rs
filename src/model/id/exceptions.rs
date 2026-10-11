//! Wire identifiers that must not become [`Id`](super::Id).
//!
//! These are the exceptions listed in docs/design/3.0-types-events-requests.md
//! §1: values that sit where an ID would but are either allowed to be `0`, are
//! not snowflakes at all, or are only meaningful inside one request or one
//! message. Each one gets its own small type instead of an `Id<M>` so that the
//! nonzero-snowflake guarantee of `Id` stays unconditional.
//!
//! Plain strings that the design keeps as documented `String` fields (voice
//! region, integration account and provider IDs, invite codes, activity
//! instance and location IDs, `custom_id`) deliberately have no type here.

// [Decision Log]
// - 목적과 의도: "0"이 정상 값인 자리나 지역 인덱스를 Id<M>에 억지로 넣지 않는다.
// - 기존 구현 및 제약 조건: 2.x는 이 값들을 Snowflake(String)나 u64로 받아 아무 검사도
//   하지 않았다. Id<M>은 0을 거부하므로 그대로 옮기면 위젯 목록이나 봇 DM 인터랙션
//   디코딩이 실패한다.
// - 검토한 주요 대안: (1) Option<Id>로 "0"을 None 처리, (2) GenericId에 센티널 허용,
//   (3) 의미별 전용 타입.
// - 선택한 방식: (3). id.rs 옆 하위 모듈에 두고 id에서 다시 내보낸다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 "값 없음"과 "봇 DM"을 구분하지 못하고
//   직렬화 왕복이 깨진다. (2)는 Id<M>의 불변식을 무너뜨린다. 전용 타입은 왜 이 값이 다른지를
//   타입 이름과 문서로 남긴다. 모두 "ID처럼 보이지만 Id가 아닌 값"이라 id 모듈 아래에 두면
//   찾기 쉽고, 파일을 분리하면 Id<M> 본문과 섞이지 않는다.
// - 장점, 단점 및 영향: 타입 수가 늘지만 각 타입은 작고 Copy(위젯 ID 제외)다. 시리즈 2의
//   A 단계에서 GuildWidgetMember.id, authorizing_integration_owners(구조체
//   AuthorizingIntegrationOwners), MessageComponent.id, 투표 answer_id를 옮겼다. UploadId를
//   담는 타입 요청 필드는 아직 없다(5단계 첨부 메타데이터 enum에서 쓴다).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{parse_decimal_u64, DecimalVisitor, DecimalWire, GuildId, InvalidId};

/// Member `id` from a guild widget (`GET /guilds/{guild.id}/widget.json`).
///
/// Discord anonymizes widget members "to prevent abuse", and its examples
/// show `"0"`. The value is opaque: it is not a [`UserId`](super::UserId),
/// cannot be turned into one, and is kept exactly as received.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WidgetUserId(String);

impl WidgetUserId {
    /// Wraps a widget member ID as received.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the ID text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the owned ID text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for WidgetUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Value under the `GUILD_INSTALL` (`"0"`) key of an interaction's
/// `authorizing_integration_owners`.
///
/// Discord sends the guild ID when the interaction came from a server and
/// the string `"0"` when it came from a DM with the app's bot user. `"0"` is
/// therefore a real value here, not a missing ID. (The `USER_INSTALL` value
/// is always a user ID and needs no special type.)
///
/// Reads the same spellings as [`Id`](super::Id) (decimal string or unsigned
/// integer, leading zeros dropped) and writes `"0"` or the guild ID string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GuildInstallOwner {
    /// Triggered from a DM with the app's bot user (wire value `"0"`).
    BotDm,
    /// Triggered from this guild.
    Guild(GuildId),
}

impl GuildInstallOwner {
    /// Returns the guild ID, or `None` for a bot DM.
    pub const fn guild_id(self) -> Option<GuildId> {
        match self {
            Self::BotDm => None,
            Self::Guild(guild_id) => Some(guild_id),
        }
    }

    /// Whether the interaction came from a DM with the app's bot user.
    pub const fn is_bot_dm(self) -> bool {
        matches!(self, Self::BotDm)
    }
}

impl DecimalWire for GuildInstallOwner {
    const EXPECTING: &'static str = "\"0\" or a guild ID as a decimal string or integer";

    fn from_wire_u64(value: u64) -> Result<Self, InvalidId> {
        if value == 0 {
            Ok(Self::BotDm)
        } else {
            GuildId::try_new(value).map(Self::Guild)
        }
    }
}

impl FromStr for GuildInstallOwner {
    type Err = InvalidId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_decimal_u64(value).and_then(Self::from_wire_u64)
    }
}

/// Prints the wire text: `0` for a bot DM, otherwise the guild ID.
impl fmt::Display for GuildInstallOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BotDm => f.write_str("0"),
            Self::Guild(guild_id) => fmt::Display::fmt(guild_id, f),
        }
    }
}

impl Serialize for GuildInstallOwner {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::BotDm => serializer.serialize_str("0"),
            Self::Guild(guild_id) => guild_id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for GuildInstallOwner {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DecimalVisitor::<Self>::new())
    }
}

/// Index of a file uploaded in the same multipart request.
///
/// For new attachments Discord requires the attachment `id` to equal the `n`
/// of the `files[n]` form part, so `0` is the normal first value. It names a
/// slot in one request, not a stored attachment; stored attachments use
/// [`AttachmentId`](super::AttachmentId). Serialized as a JSON integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UploadId(u32);

impl UploadId {
    /// Creates the index for `files[index]`.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the `files[n]` index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for UploadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Numeric `id` of a message or modal component (Components V2).
///
/// Discord documents it as a "32 bit integer", unique within one message and
/// generated sequentially when omitted. Sending `0` is allowed and is treated
/// as "not set" (Discord replaces it), so `0` is representable here rather
/// than rejected. This is unrelated to the string `custom_id`. Serialized as
/// a JSON integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(u32);

impl ComponentId {
    /// Wraps a component ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw component ID.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// `answer_id` of a poll answer, also used in the answer voters route
/// (`.../polls/{message.id}/answers/{answer_id}`).
///
/// Discord currently numbers answers from 1 and allows at most 10, but says
/// not to depend on that sequence, so any `u32` is accepted and none is
/// treated as special. Serialized as a JSON integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PollAnswerId(u32);

impl PollAnswerId {
    /// Wraps a poll answer ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the raw answer ID.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PollAnswerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}
