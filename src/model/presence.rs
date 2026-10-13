//! Presences, activities, and presence updates.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::Emoji;

use super::{ActivityType, ApplicationId, BorrowedKey, StatusDisplayType, UserId};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Presence`.
pub struct Presence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// Partial user object carried by gateway presence payloads
    /// (GUILD_CREATE `presences[]` and GUILD_MEMBERS_CHUNK `presences[]`
    /// identify the user this way instead of `user_id`). Only `id` is
    /// guaranteed by Discord.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<PresenceUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PresenceStatus>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "activity_list"
    )]
    pub activities: Option<Vec<Activity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_status: Option<ClientStatus>,
}

/// Partial user object inside gateway presence payloads; Discord only
/// guarantees `id` here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresenceUser {
    pub id: UserId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ClientStatus`: the status per platform,
/// with a platform absent while the user is offline or invisible there.
pub struct ClientStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<PresenceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile: Option<PresenceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<PresenceStatus>,
    /// A virtual reality application session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vr: Option<PresenceStatus>,
}

/// A presence `status` string (Discord docs: Gateway Events › Update
/// Presence › Status Types, and the `client_status` of Presence Update).
///
/// An open vocabulary: the five documented values are associated
/// constants, and any other string Discord sends is kept and written back
/// unchanged. Known values decode without allocating (the presences of a
/// large `GUILD_CREATE` are mostly `online`/`idle`/`dnd`). No `Default`:
/// Discord defines none.
///
/// ```
/// use discordrs::PresenceStatus;
///
/// let status: PresenceStatus = serde_json::from_str(r#""dnd""#).unwrap();
/// assert_eq!(status, PresenceStatus::DND);
/// assert_eq!(status, "dnd");
/// assert!(status.is_known());
///
/// let future = PresenceStatus::new("streaming-soon");
/// assert!(!future.is_known());
/// assert_eq!(serde_json::to_string(&future).unwrap(), r#""streaming-soon""#);
/// ```
// [Decision Log]
// - 목적과 의도: 프레즌스 status를 어휘 문자열로 구분한다(설계 §2 "문자열은 어휘와 임의 텍스트를
//   구분"). 오타("onlin")를 상수로 막고 알려지지 않은 값은 보존한다.
// - 기존 구현 및 제약 조건: 2.x는 `Option<String>`/`String`이었다. GUILD_CREATE의
//   presences와 client_status마다 문자열을 할당했다. Discord는 새 값을 예고 없이 더할 수 있다.
// - 검토한 주요 대안: (1) 닫힌 enum + `Unknown(String)`, (2) `String` newtype, (3)
//   `Cow<'static, str>` newtype + 알려진 값 인터닝.
// - 선택한 방식: (3). 연관 상수는 `Cow::Borrowed`, 역직렬화는 알려진 값을 같은 정적 문자열로
//   바꾸고 나머지만 `Owned`로 둔다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 열린 판별값(§2)과 같은 이유로 알려진 값과
//   Unknown("online")이 갈라질 수 있다. (2)는 상수를 `const`로 만들 수 없고 알려진 값도 매번
//   할당한다. (3)은 크기가 `String`과 같고(24바이트, `Option`도 같음) 알려진 값은 할당이 없다.
// - 장점, 단점 및 영향: match 패턴으로 쓸 수 없어 `==`로 비교한다(`&str`과도 비교 가능).
//   `as_str()`이 원래 문자열을 돌려준다.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PresenceStatus(Cow<'static, str>);

impl PresenceStatus {
    /// Online.
    pub const ONLINE: Self = Self(Cow::Borrowed("online"));
    /// Do Not Disturb.
    pub const DND: Self = Self(Cow::Borrowed("dnd"));
    /// AFK.
    pub const IDLE: Self = Self(Cow::Borrowed("idle"));
    /// Invisible and shown as offline (sent in presence updates only).
    pub const INVISIBLE: Self = Self(Cow::Borrowed("invisible"));
    /// Offline.
    pub const OFFLINE: Self = Self(Cow::Borrowed("offline"));

    const KNOWN: [Self; 5] = [
        Self::ONLINE,
        Self::DND,
        Self::IDLE,
        Self::INVISIBLE,
        Self::OFFLINE,
    ];

    /// Wraps any status string, including ones this version of discord.rs
    /// does not know. Known values are stored without an allocation.
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self::known(&value).unwrap_or(Self(Cow::Owned(value)))
    }

    fn known(value: &str) -> Option<Self> {
        Self::KNOWN
            .into_iter()
            .find(|known| known.as_str() == value)
    }

    /// The status string as sent on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is one of the associated constants.
    pub fn is_known(&self) -> bool {
        Self::known(self.as_str()).is_some()
    }
}

impl fmt::Debug for PresenceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for PresenceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Derefs to the status string, so `Option<PresenceStatus>::as_deref()`
/// gives `Option<&str>` like the 2.x `Option<String>` field did.
impl std::ops::Deref for PresenceStatus {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for PresenceStatus {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<str> for PresenceStatus {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for PresenceStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Serialize for PresenceStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PresenceStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StatusVisitor;

        impl Visitor<'_> for StatusVisitor {
            type Value = PresenceStatus;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a presence status string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PresenceStatus::known(value)
                    .unwrap_or_else(|| PresenceStatus(Cow::Owned(value.to_owned()))))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PresenceStatus::new(value))
            }
        }

        deserializer.deserialize_str(StatusVisitor)
    }
}

/// Unix timestamps (milliseconds) of when an activity started and ends.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActivityTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

/// The party of an activity.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActivityParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `[current_size, max_size]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<Vec<u64>>,
}

/// Images of an activity and their hover texts.
///
/// The image fields are asset strings (Discord docs: Activity Asset Image):
/// an application asset ID, or `mp:{image_id}` for an external image served
/// through the media proxy. They are user-specified and not sanitized.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActivityAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    /// URL opened when the large image is clicked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
    /// URL opened when the small image is clicked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_url: Option<String>,
    /// Banner shown on a game invite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_cover_image: Option<String>,
}

/// Rich Presence secrets for joining and spectating.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActivitySecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectate: Option<String>,
    /// The `match` secret (`match` is a Rust keyword). 2.x wrote and read a
    /// `match_secret` key, so Discord's value never arrived.
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_secret: Option<String>,
}

/// An activity of a user's presence, as Discord sends it in GUILD_CREATE,
/// PRESENCE_UPDATE, and GUILD_MEMBERS_CHUNK.
///
/// This is the received shape. A bot sets its own activity with
/// [`BotActivity`] in [`UpdatePresence`]: Discord lets bots set only
/// `name`, `state`, `type`, and `url`, and the received and sent shapes of
/// [`buttons`](Self::buttons) differ.
///
/// No `Default`: Discord requires `type` and defines no default for it
/// (2.x defaulted to `PLAYING`). Build one by hand (for tests or a
/// hand-made cache entry) from [`Activity::new`] with struct update syntax.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ActivityType,
    /// Stream URL, validated when `type` is `STREAMING`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Unix timestamp (milliseconds) of when the activity was added to the
    /// user's session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    /// Which field the member list shows in the user's status text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_display_type: Option<StatusDisplayType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// URL opened when the details text is clicked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details_url: Option<String>,
    /// The party status, or the text of a custom status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// URL opened when the state text is clicked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_url: Option<String>,
    /// The emoji of a custom status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<ActivityParty>,
    /// Boxed: custom statuses and games without Rich Presence have no
    /// images, and the cache keeps every activity of every online member;
    /// inline, the seven strings would add 160 bytes to each.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Box<ActivityAssets>>,
    /// Boxed for the same reason: rarely present in presences bots receive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Box<ActivitySecrets>>,
    /// Whether the activity is an instanced game session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<bool>,
    /// Activity flags OR'd together (Discord docs: Activity Flags):
    /// `INSTANCE` `1 << 0`, `JOIN` `1 << 1`, `SPECTATE` `1 << 2`,
    /// `JOIN_REQUEST` `1 << 3`, `SYNC` `1 << 4`, `PLAY` `1 << 5`,
    /// `PARTY_PRIVACY_FRIENDS` `1 << 6`, `PARTY_PRIVACY_VOICE_CHANNEL`
    /// `1 << 7`, `EMBEDDED` `1 << 8`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Labels of the custom Rich Presence buttons (at most two).
    ///
    /// Discord docs (Gateway Events › Activity Object › Activity Buttons):
    /// "When received over the gateway, the `buttons` field is an array of
    /// strings, which are the button labels. Bots cannot access a user's
    /// activity button URLs." Only the sending shape (Rich Presence over
    /// RPC or the Social SDK) is an array of `{label, url}` objects. Both
    /// shapes decode here, each to its label, so a presence written by
    /// 2.x (which modelled the objects) still reads. Written back as
    /// strings, the received shape.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "button_labels"
    )]
    pub buttons: Option<Vec<String>>,
}

// [Decision Log]
// - 목적과 의도: 라이브 Discord에서 GUILD_CREATE 30개 중 11개가 `invalid type: string "Get Valorant
//   Tracker", expected struct ActivityButton`으로 디코딩에 실패했다. 받은 활동의 `buttons`는 문서대로
//   라벨 문자열 배열이다. 이 모양을 받고, 받는 모양과 보내는 모양이 다른 점을 타입에 드러낸다.
// - 기존 구현 및 제약 조건: 2.x부터 `Activity` 하나가 받은 presence와 `UpdatePresence`(보내기)를 모두
//   맡았고 `buttons: Option<Vec<ActivityButton {label, url}>>`, 곧 보내는 모양이었다. 봇은
//   `name`, `state`, `type`, `url`만 설정할 수 있다(문서 Activity Structure의 Info). 3.0.0은 아직
//   배포되지 않아 공개 타입을 바꿀 수 있다.
// - 검토한 주요 대안: (1) `ActivityButton`을 유지하고 문자열이면 `url: None`으로 읽기, (2) 라벨
//   `Vec<String>`으로 바꾸고 `Activity`를 보내기에도 그대로 쓰기, (3) 받은 `Activity`는 라벨
//   `Vec<String>`, 보내기는 봇이 설정할 수 있는 네 필드만 가진 `BotActivity`로 분리.
// - 선택한 방식: (3). 받을 때는 문자열과 `{label, url}` 객체를 모두 라벨로 읽는다(객체는 2.x가 저장한
//   presence나 RPC 모양). 그 밖의 모양은 오류이고, 게이트웨이 경로가 그 활동 하나만 버린다
//   (`gateway_payload.rs`의 요소 단위 디코딩).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 봇이 결코 받지 못하는 URL 필드를 남기고 `ActivityButton`을
//   보내기 타입처럼 보이게 한다. (2)는 보내기 API가 봇이 설정할 수 없는 필드(버튼, 타임스탬프, 에셋)를
//   계속 노출한다. (3)은 twilight(`MinimalActivity`)·serenity(`ActivityData`)와 같은 분리이고, 각
//   타입이 문서의 한쪽 모양만 표현한다.
// - 장점, 단점 및 영향: `ActivityButton`이 없어지고 `UpdatePresence::activities`가
//   `Vec<BotActivity>`가 된다(3.0의 다른 깨지는 변경과 함께 이전 안내에 적는다). 받은 버튼 URL은
//   봇에게 오지 않으므로 잃는 정보가 없다. 버튼 라벨은 객체 모양에서도 `label`만 남긴다.

impl Activity {
    /// An activity of `kind` named `name`, with every optional field unset.
    pub fn new(kind: ActivityType, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            url: None,
            created_at: None,
            timestamps: None,
            application_id: None,
            status_display_type: None,
            details: None,
            details_url: None,
            state: None,
            state_url: None,
            emoji: None,
            party: None,
            assets: None,
            secrets: None,
            instance: None,
            flags: None,
            buttons: None,
        }
    }
}

/// `buttons` of a received activity: label strings (the gateway shape) or
/// `{label, url}` objects (the sending shape), each read as its label.
fn button_labels<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    /// One element: a label, or an object whose `label` is kept.
    struct ButtonLabel(String);

    impl<'de> Deserialize<'de> for ButtonLabel {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct LabelVisitor;

            impl<'de> Visitor<'de> for LabelVisitor {
                type Value = ButtonLabel;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("a button label or a {label, url} button object")
                }

                fn visit_str<E: de::Error>(self, label: &str) -> Result<ButtonLabel, E> {
                    Ok(ButtonLabel(label.to_owned()))
                }

                fn visit_string<E: de::Error>(self, label: String) -> Result<ButtonLabel, E> {
                    Ok(ButtonLabel(label))
                }

                fn visit_map<A: de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<ButtonLabel, A::Error> {
                    let mut label = None;
                    while let Some(BorrowedKey(key)) = map.next_key()? {
                        if key == "label" {
                            label = Some(map.next_value::<String>()?);
                        } else {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                    label
                        .map(ButtonLabel)
                        .ok_or_else(|| de::Error::missing_field("label"))
                }
            }

            deserializer.deserialize_any(LabelVisitor)
        }
    }

    let Some(buttons) = Option::<Vec<ButtonLabel>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    // `ButtonLabel` has the layout of `String`, so collecting reuses the
    // buffer.
    Ok(Some(buttons.into_iter().map(|button| button.0).collect()))
}

/// `activities` of a presence, grown as [`push_activity`] does.
fn activity_list<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<Activity>>, D::Error> {
    struct Activities;

    impl<'de> Visitor<'de> for Activities {
        type Value = Vec<Activity>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an array of activities")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<Activity>, A::Error> {
            let mut activities = Vec::new();
            while let Some(activity) = seq.next_element()? {
                push_activity(&mut activities, activity);
            }
            Ok(activities)
        }
    }

    struct List(Vec<Activity>);

    impl<'de> Deserialize<'de> for List {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_seq(Activities).map(List)
        }
    }

    Option::<List>::deserialize(deserializer).map(|list| list.map(|list| list.0))
}

/// Appends `activity`, growing `activities` to 2, 4, 8, ... slots instead
/// of the four the standard library reserves first.
///
/// A presence carries one or two activities (often a custom status and a
/// game), and the cache keeps it for as long as the member is online. With
/// Vec's first reservation of four slots (an activity is 352 bytes)
/// every list was reallocated by
/// [`normalize_presence`](super::normalize_presence). Two slots first make
/// the two-activity list one exact allocation and leave a single activity
/// one spare slot, which `normalize_presence` gives back as before. Growing
/// from one slot was measured too: it saves those bytes but costs a second
/// allocation for every two-activity list decoded without normalizing (a
/// REST `Guild`).
pub(crate) fn push_activity(activities: &mut Vec<Activity>, activity: Activity) {
    const FIRST_SLOTS: usize = 2;
    if activities.len() == activities.capacity() {
        activities.reserve_exact(activities.len().max(FIRST_SLOTS));
    }
    activities.push(activity);
}

/// An activity a bot sets for itself in [`UpdatePresence`]: Discord lets
/// bot users set only `name`, `state`, `type`, and `url` (Discord docs:
/// Activity Structure). Received activities are [`Activity`].
///
/// ```
/// use discordrs::{ActivityType, BotActivity, PresenceStatus, UpdatePresence};
///
/// let streaming = BotActivity {
///     url: Some("https://twitch.tv/discord".to_string()),
///     ..BotActivity::new(ActivityType::STREAMING, "Rocket League")
/// };
/// assert_eq!(
///     serde_json::to_value(&streaming).unwrap(),
///     serde_json::json!({
///         "name": "Rocket League",
///         "type": 1,
///         "url": "https://twitch.tv/discord"
///     })
/// );
///
/// let presence = UpdatePresence {
///     since: None,
///     activities: vec![BotActivity::custom("Handling tickets")],
///     status: PresenceStatus::ONLINE,
///     afk: false,
/// };
/// assert_eq!(presence.activities[0].kind, ActivityType::CUSTOM);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotActivity {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ActivityType,
    /// Custom status text (`CUSTOM`), or the party status line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Stream URL for `STREAMING`; only Twitch and YouTube URLs work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl BotActivity {
    /// An activity of `kind` named `name`, without `state` or `url`.
    pub fn new(kind: ActivityType, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            state: None,
            url: None,
        }
    }

    /// A custom status showing `state`. Discord requires a `name` and does
    /// not display it; this sets `"Custom Status"`.
    pub fn custom(state: impl Into<String>) -> Self {
        Self {
            state: Some(state.into()),
            ..Self::new(ActivityType::CUSTOM, "Custom Status")
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UpdatePresence` (gateway opcode 3, and the
/// `presence` of IDENTIFY).
pub struct UpdatePresence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default)]
    pub activities: Vec<BotActivity>,
    pub status: PresenceStatus,
    #[serde(default)]
    pub afk: bool,
}

impl UpdatePresence {
    /// Online, playing `name`.
    pub fn online_with_activity(name: impl Into<String>) -> Self {
        Self {
            since: None,
            activities: vec![BotActivity::new(ActivityType::PLAYING, name)],
            status: PresenceStatus::ONLINE,
            afk: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use serde_json::json;

    use super::{
        push_activity, Activity, ActivityType, BotActivity, ClientStatus, Presence, PresenceStatus,
        StatusDisplayType, UpdatePresence,
    };

    /// Activities shaped like the ones live Discord sent on 2026-10-13 that
    /// failed 11 of 30 GUILD_CREATE dispatches: `buttons` holds the labels.
    fn received_activities() -> serde_json::Value {
        json!([
            {
                "type": 0,
                "name": "VALORANT",
                "application_id": "700136079562375258",
                "details": "Competitive",
                "state": "In a match",
                "created_at": 1791796520000_u64,
                "timestamps": { "start": 1791792900000_u64 },
                "buttons": ["Get Valorant Tracker"],
                "assets": { "large_image": "mp:external/abc/https/example.png", "large_text": "Ascent" },
                "flags": 1,
                "id": "e3a1c0de",
                "session_id": "7c2d1c0f8e5b4a3fa1b2c3d4e5f60718"
            },
            {
                "type": 0,
                "name": "Medal",
                "buttons": ["Follow on Medal", "Hosting"],
                "created_at": 1791796530000_u64
            }
        ])
    }

    #[test]
    fn received_buttons_are_label_strings() {
        let activities: Vec<Activity> = serde_json::from_value(received_activities()).unwrap();
        assert_eq!(
            activities[0].buttons.as_deref(),
            Some(&["Get Valorant Tracker".to_string()][..])
        );
        assert_eq!(
            activities[1].buttons.as_deref(),
            Some(&["Follow on Medal".to_string(), "Hosting".to_string()][..])
        );
        assert_eq!(activities[0].flags, Some(1));

        // Round trip: written back in the received shape.
        let written = serde_json::to_value(&activities[1]).unwrap();
        assert_eq!(written["buttons"], json!(["Follow on Medal", "Hosting"]));
        let again: Activity = serde_json::from_value(written).unwrap();
        assert_eq!(again.buttons, activities[1].buttons);
    }

    #[test]
    fn button_objects_and_missing_or_null_buttons_still_decode() {
        // The sending shape (2.x stored presences, RPC payloads) keeps the
        // label; the URL is not something bots receive.
        let activity: Activity = serde_json::from_value(json!({
            "type": 0,
            "name": "Game",
            "buttons": [
                { "label": "Join", "url": "https://example.com/join" },
                "Watch",
                { "url": "https://example.com/x", "label": "Escaped \"label\"" }
            ]
        }))
        .unwrap();
        assert_eq!(
            activity.buttons.unwrap(),
            ["Join", "Watch", "Escaped \"label\""]
        );

        for missing in [
            json!({ "type": 0, "name": "Game" }),
            json!({ "type": 0, "name": "Game", "buttons": null }),
        ] {
            let activity: Activity = serde_json::from_value(missing).unwrap();
            assert_eq!(activity.buttons, None);
        }

        // Shapes Discord does not document stay errors; the gateway drops
        // just that activity (`LenientActivities`).
        for invalid in [
            json!([1]),
            json!([{ "url": "https://example.com" }]),
            json!("Join"),
        ] {
            let result = serde_json::from_value::<Activity>(
                json!({ "type": 0, "name": "Game", "buttons": invalid }),
            );
            assert!(result.is_err(), "{invalid}");
        }

        // The same from serde_json text, where strings arrive borrowed.
        let text = r#"{"type":0,"name":"Game","buttons":["A",{"label":"B","url":"u"}]}"#;
        let activity: Activity = serde_json::from_str(text).unwrap();
        assert_eq!(activity.buttons.unwrap(), ["A", "B"]);
    }

    #[test]
    fn documented_activity_fields_decode() {
        let activity: Activity = serde_json::from_value(json!({
            "name": "Spotify",
            "type": 2,
            "url": null,
            "details": null,
            "status_display_type": 2,
            "details_url": "https://example.com/track",
            "state": "Rick Astley",
            "state_url": "https://example.com/artist",
            "emoji": { "name": "🎵" },
            "party": { "id": "spotify:1", "size": [1, 2] },
            "assets": {
                "large_image": "spotify:ab67",
                "large_url": "https://example.com/album",
                "small_url": "https://example.com/small",
                "invite_cover_image": "mp:cover"
            },
            "secrets": { "join": "j", "spectate": "s", "match": "m" },
            "instance": true,
            "flags": 48
        }))
        .unwrap();
        assert_eq!(activity.kind, ActivityType::LISTENING);
        assert_eq!(activity.url, None);
        assert_eq!(
            activity.status_display_type,
            Some(StatusDisplayType::DETAILS)
        );
        assert_eq!(
            activity.details_url.as_deref(),
            Some("https://example.com/track")
        );
        assert_eq!(
            activity.state_url.as_deref(),
            Some("https://example.com/artist")
        );
        let assets = activity.assets.as_ref().unwrap();
        assert_eq!(
            assets.large_url.as_deref(),
            Some("https://example.com/album")
        );
        assert_eq!(
            assets.small_url.as_deref(),
            Some("https://example.com/small")
        );
        assert_eq!(assets.invite_cover_image.as_deref(), Some("mp:cover"));
        // `match`, not the 2.x `match_secret` key.
        let secrets = activity.secrets.as_ref().unwrap();
        assert_eq!(secrets.match_secret.as_deref(), Some("m"));
        assert_eq!(activity.instance, Some(true));
        assert_eq!(activity.flags, Some(48));

        let written = serde_json::to_value(&activity).unwrap();
        assert_eq!(
            written["secrets"],
            json!({ "join": "j", "spectate": "s", "match": "m" })
        );
        assert_eq!(written["status_display_type"], json!(2));
        assert!(written.get("url").is_none());

        let client: ClientStatus =
            serde_json::from_value(json!({ "vr": "online", "web": "idle" })).unwrap();
        assert_eq!(client.vr, Some(PresenceStatus::ONLINE));
        assert_eq!(client.web, Some(PresenceStatus::IDLE));
    }

    #[test]
    fn activities_stay_no_larger_than_before_the_documented_fields() {
        // 440 bytes before `status_display_type`, `details_url`,
        // `state_url`, `instance`, `flags`, and the three asset fields were
        // added; boxing `assets` and `secrets` pays for them. The cache
        // keeps every activity of every online member.
        let size = std::mem::size_of::<Activity>();
        assert!(size <= 440, "Activity is {size} bytes");
    }

    #[test]
    fn presence_activities_start_with_two_slots() {
        let presence: Presence = serde_json::from_value(json!({
            "user": { "id": "1" },
            "activities": received_activities()
        }))
        .unwrap();
        let activities = presence.activities.unwrap();
        assert_eq!((activities.len(), activities.capacity()), (2, 2));

        let one: Presence = serde_json::from_str(
            r#"{"user":{"id":"1"},"activities":[{"type":4,"name":"Custom Status"}]}"#,
        )
        .unwrap();
        let one = one.activities.unwrap();
        // `normalize_presence` returns the spare slot on the gateway path.
        assert_eq!((one.len(), one.capacity()), (1, 2));

        let mut grown = Vec::new();
        let capacities: Vec<usize> = (0..5)
            .map(|_| {
                push_activity(&mut grown, Activity::new(ActivityType::PLAYING, "g"));
                grown.capacity()
            })
            .collect();
        assert_eq!(capacities, [2, 2, 4, 4, 8]);

        let null: Presence = serde_json::from_value(json!({ "activities": null })).unwrap();
        assert!(null.activities.is_none());
        let empty: Presence = serde_json::from_value(json!({ "activities": [] })).unwrap();
        assert_eq!(empty.activities.map(|list| list.len()), Some(0));
    }

    #[test]
    fn bots_send_only_the_fields_they_may_set() {
        let presence = UpdatePresence {
            since: None,
            activities: vec![
                BotActivity::new(ActivityType::PLAYING, "with the API"),
                BotActivity::custom("Handling tickets"),
            ],
            status: PresenceStatus::DND,
            afk: false,
        };
        assert_eq!(
            serde_json::to_value(&presence).unwrap(),
            json!({
                "activities": [
                    { "name": "with the API", "type": 0 },
                    { "name": "Custom Status", "type": 4, "state": "Handling tickets" }
                ],
                "status": "dnd",
                "afk": false
            })
        );
        let read: UpdatePresence =
            serde_json::from_value(serde_json::to_value(&presence).unwrap()).unwrap();
        assert_eq!(read.activities, presence.activities);
        assert_eq!(
            UpdatePresence::online_with_activity("x").activities,
            [BotActivity::new(ActivityType::PLAYING, "x")]
        );
    }

    #[test]
    fn known_statuses_decode_to_the_static_constants() {
        for (wire, constant) in [
            ("online", PresenceStatus::ONLINE),
            ("dnd", PresenceStatus::DND),
            ("idle", PresenceStatus::IDLE),
            ("invisible", PresenceStatus::INVISIBLE),
            ("offline", PresenceStatus::OFFLINE),
        ] {
            let decoded: PresenceStatus = match serde_json::from_value(json!(wire)) {
                Ok(status) => status,
                Err(error) => panic!("{wire} must decode: {error}"),
            };
            assert_eq!(decoded, constant);
            assert!(decoded.is_known());
            // Interned: no owned copy of a documented value.
            assert!(matches!(decoded.0, Cow::Borrowed(_)), "{wire}");
            assert!(matches!(PresenceStatus::new(wire).0, Cow::Borrowed(_)));
            assert_eq!(serde_json::to_value(&decoded).ok(), Some(json!(wire)));
        }
    }

    #[test]
    fn unknown_statuses_round_trip_and_non_strings_fail() {
        let status: PresenceStatus = match serde_json::from_value(json!("away-on-mars")) {
            Ok(status) => status,
            Err(error) => panic!("unknown statuses must decode: {error}"),
        };
        assert!(!status.is_known());
        assert_eq!(status, "away-on-mars");
        assert_eq!(status.as_str(), "away-on-mars");
        assert_eq!(status.to_string(), "away-on-mars");
        assert_eq!(format!("{status:?}"), "\"away-on-mars\"");
        assert_eq!(
            serde_json::to_value(&status).ok(),
            Some(json!("away-on-mars"))
        );
        assert!(serde_json::from_value::<PresenceStatus>(json!(1)).is_err());
        assert!(serde_json::from_value::<PresenceStatus>(json!(null)).is_err());

        let client: ClientStatus =
            match serde_json::from_value(json!({ "desktop": "online", "mobile": "idle" })) {
                Ok(client) => client,
                Err(error) => panic!("client status must decode: {error}"),
            };
        assert_eq!(client.desktop, Some(PresenceStatus::ONLINE));
        assert_eq!(client.mobile.as_deref(), Some("idle"));
        assert_eq!(client.web, None);
    }
}
