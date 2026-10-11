//! Presences, activities, and presence updates.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::Emoji;

use super::{ActivityType, ApplicationId, UserId};

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
    #[serde(skip_serializing_if = "Option::is_none")]
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
/// Typed Discord API object for `ClientStatus`.
pub struct ClientStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<PresenceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile: Option<PresenceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<PresenceStatus>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityTimestamps`.
pub struct ActivityTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityParty`.
pub struct ActivityParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityAssets`.
pub struct ActivityAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivitySecrets`.
pub struct ActivitySecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_secret: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityButton`.
pub struct ActivityButton {
    pub label: String,
    pub url: String,
}

/// A presence activity.
///
/// No `Default`: Discord requires `type` and defines no default for it
/// (2.x defaulted to `PLAYING`). Start from [`Activity::new`] and fill the
/// optional fields with struct update syntax.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ActivityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<ActivityParty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<ActivityAssets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<ActivitySecrets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<ActivityButton>>,
}

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
            details: None,
            state: None,
            emoji: None,
            party: None,
            assets: None,
            secrets: None,
            buttons: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UpdatePresence`.
pub struct UpdatePresence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default)]
    pub activities: Vec<Activity>,
    pub status: PresenceStatus,
    #[serde(default)]
    pub afk: bool,
}

impl UpdatePresence {
    /// Creates a `online_with_activity` value.
    pub fn online_with_activity(name: impl Into<String>) -> Self {
        Self {
            since: None,
            activities: vec![Activity::new(ActivityType::PLAYING, name)],
            status: PresenceStatus::ONLINE,
            afk: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use serde_json::json;

    use super::{ClientStatus, PresenceStatus};

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
