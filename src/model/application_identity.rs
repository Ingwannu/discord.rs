//! Application Identity Profile resource (Discord changelog 2026-08-26).
//!
//! Games claimed on Discord write player stats (rank, playtime, wins, custom
//! fields) to a user's Application Identity record, which Discord renders as
//! a Game Stats Widget on the user's profile.

use std::borrow::Cow;

use serde::de::{self, DeserializeOwned, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::error::DiscordError;

use super::UserId;

/// Serialized size cap for [`ApplicationIdentityProfileData`] in an update.
pub const MAX_PROFILE_DATA_BYTES: usize = 10 * 1024;
/// Maximum number of dynamic fields in one profile.
pub const MAX_PROFILE_DYNAMIC_FIELDS: usize = 30;
/// Maximum length of primary string values and dynamic field names.
pub const MAX_PROFILE_STRING_CHARS: usize = 100;
/// Maximum length of the external username.
pub const MAX_PROFILE_USERNAME_CHARS: usize = 1024;

/// One external account linked to a Discord user for an application.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIdentity {
    /// Discord user the identity belongs to; present in list responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// External account provider type (`NONE` for profile-only identities).
    pub provider_type: String,
    /// Provider-specific disambiguator; omitted when absent or empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// The user's ID in the external system.
    pub provider_issued_user_id: String,
}

/// Wrapped list returned by the application-identity lookup routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIdentities {
    #[serde(default)]
    pub identities: Vec<ApplicationIdentity>,
}

/// Profile stored on an Application Identity.
///
/// No `PartialEq`: [`Self::metadata`] is raw JSON, which has no structural
/// equality. Compare the typed fields, or the serialized JSON.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ApplicationIdentityProfile {
    /// The user's username in the external system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Arbitrary game-defined data that Discord stores but never reads, as
    /// the raw JSON text received. Decode it with [`Self::metadata_as`].
    ///
    /// This stays untyped on purpose: the shape belongs to the game, not to
    /// Discord, so any fixed Rust type would be wrong for someone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Box<RawValue>>,
    /// Game stats shown in the widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ApplicationIdentityProfileData>,
}

impl ApplicationIdentityProfile {
    /// Decodes [`Self::metadata`] as the game's own type `T`; `Ok(None)`
    /// when there is no metadata.
    pub fn metadata_as<T: DeserializeOwned>(&self) -> Result<Option<T>, serde_json::Error> {
        self.metadata
            .as_deref()
            .map(|raw| serde_json::from_str(raw.get()))
            .transpose()
    }
}

/// Game stats: pre-configured primary fields plus custom dynamic fields.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ApplicationIdentityProfileData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<PrimaryProfileData>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic: Vec<DynamicProfileField>,
}

/// Pre-configured stat fields a Game Stats Widget can show.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct PrimaryProfileData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank_image: Option<ProfileMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest_rank: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest_rank_image: Option<ProfileMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_played_character: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured_played_character_image: Option<ProfileMedia>,
    /// Total playtime in hours; decimals allowed (e.g. `69.41`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playtime_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_wins: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_period_wins: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_games: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_period_games: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_kills: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_period_kills: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_assists: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_period_assists: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_deaths: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_period_deaths: Option<u64>,
}

impl PrimaryProfileData {
    fn string_fields(&self) -> [(&'static str, Option<&str>); 4] {
        [
            ("season", self.season.as_deref()),
            ("rank_name", self.rank_name.as_deref()),
            ("highest_rank", self.highest_rank.as_deref()),
            (
                "featured_played_character",
                self.featured_played_character.as_deref(),
            ),
        ]
    }
}

/// Image or other media referenced by a profile field.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProfileMedia {
    pub url: String,
}

/// A custom stat field, tagged on the wire by an integer `type`.
///
/// Serde's internally tagged enums only support string tags, so the integer
/// tag is handled by hand. Unknown future types are kept verbatim in
/// [`DynamicProfileField::Unknown`] so reading and re-writing a profile never
/// drops a field this crate does not know yet.
///
/// Equality compares known fields by value and `Unknown` fields by their
/// JSON text, so the same unknown field formatted differently (spacing, key
/// order) compares unequal.
// [Decision Log]
// - 목적과 의도: 필드마다 `serde_json::Value` 트리를 만든 뒤 다시 디코딩하던 경로를 없애고,
//   모르는 `type`은 받은 그대로 보관한다.
// - 기존 구현 및 제약 조건: 태그가 정수라 serde 내부 태그 enum을 쓸 수 없고, `type`이 `name`/
//   `value`보다 뒤에 올 수 있어 한 번에 순서대로 읽을 수 없다. 모르는 타입은 손실 없이 다시
//   써야 한다(`validate`의 10 KB 계산도 그 텍스트를 쓴다).
// - 검토한 주요 대안: (1) `Value`로 버퍼링 후 `from_value`(기존), (2) 키 순서를 가정하지 않는
//   방문자에서 `value`만 `Box<RawValue>`로 버퍼링, (3) 필드 전체를 `Box<RawValue>`로 한 번
//   복사한 뒤 `type`만 읽는 헤더 파싱과 알려진 변형 파싱.
// - 선택한 방식: (3). `Unknown`은 그 `Box<RawValue>`를 그대로 가진다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 맵·문자열 노드를 모두 할당하고 다시 복제한다.
//   (2)는 알려진 타입의 할당이 가장 적지만 모르는 타입의 나머지 키를 보존하려면 결국 전체를
//   버퍼링해야 하고 방문자 코드가 길어진다. (3)은 할당 한 번(필드 텍스트 복사)과 할당 없는
//   헤더 스캔뿐이고, 기존 판정 규칙(정수가 아닌 `type`, 객체가 아닌 값은 `Unknown`)을 그대로
//   지킨다.
// - 장점, 단점 및 영향: 필드 텍스트를 두 번 훑는다. `RawValue`에 `PartialEq`가 없어 동등성을
//   직접 구현했고(`Unknown`은 텍스트 비교), `name()`은 `Unknown`에서 이름을 빌리지 못할 수
//   있어 `Cow`를 돌려준다. serde의 버퍼링 경로(`flatten`, `untagged`)에서는 `RawValue`가
//   디코딩되지 않으므로 이 타입을 그런 컨테이너에 넣지 않는다.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DynamicProfileField {
    /// `type: 1`
    String { name: String, value: String },
    /// `type: 2`
    Number { name: String, value: f64 },
    /// `type: 3`
    Media { name: String, value: ProfileMedia },
    /// A field type this crate does not model yet, preserved as the raw
    /// JSON text received.
    Unknown(Box<RawValue>),
}

impl PartialEq for DynamicProfileField {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::String { name, value },
                Self::String {
                    name: other_name,
                    value: other_value,
                },
            ) => name == other_name && value == other_value,
            (
                Self::Number { name, value },
                Self::Number {
                    name: other_name,
                    value: other_value,
                },
            ) => name == other_name && value == other_value,
            (
                Self::Media { name, value },
                Self::Media {
                    name: other_name,
                    value: other_value,
                },
            ) => name == other_name && value == other_value,
            (Self::Unknown(raw), Self::Unknown(other)) => raw.get() == other.get(),
            _ => false,
        }
    }
}

impl DynamicProfileField {
    const STRING: u64 = 1;
    const NUMBER: u64 = 2;
    const MEDIA: u64 = 3;

    /// Returns the field name. For an `Unknown` field it is read from the
    /// raw JSON (borrowed unless it contains escapes), and `None` when the
    /// field has no string `name`.
    pub fn name(&self) -> Option<Cow<'_, str>> {
        #[derive(Deserialize)]
        struct Named<'a> {
            #[serde(borrow)]
            name: Cow<'a, str>,
        }

        match self {
            Self::String { name, .. } | Self::Number { name, .. } | Self::Media { name, .. } => {
                Some(Cow::Borrowed(name))
            }
            Self::Unknown(raw) if is_object(raw) => serde_json::from_str::<Named<'_>>(raw.get())
                .ok()
                .map(|named| named.name),
            Self::Unknown(_) => None,
        }
    }
}

/// Derived struct decoders also accept a JSON array, so the raw-field
/// readers check for an object first: only an object has keys.
fn is_object(raw: &RawValue) -> bool {
    raw.get().trim_start().starts_with('{')
}

impl Serialize for DynamicProfileField {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        fn tagged<S: Serializer, V: Serialize>(
            serializer: S,
            kind: u64,
            name: &str,
            value: &V,
        ) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(3))?;
            map.serialize_entry("type", &kind)?;
            map.serialize_entry("name", name)?;
            map.serialize_entry("value", value)?;
            map.end()
        }

        match self {
            Self::String { name, value } => tagged(serializer, Self::STRING, name, value),
            Self::Number { name, value } => tagged(serializer, Self::NUMBER, name, value),
            Self::Media { name, value } => tagged(serializer, Self::MEDIA, name, value),
            Self::Unknown(raw) => raw.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DynamicProfileField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Reads only `type`; serde skips the other keys without
        /// allocating. Any shape that is not an object with an integer
        /// `type` reads as `None`, which keeps the field `Unknown`.
        #[derive(Deserialize)]
        struct Tag {
            #[serde(rename = "type")]
            kind: Option<u64>,
        }

        #[derive(Deserialize)]
        struct Known<T> {
            name: String,
            value: T,
        }

        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let typed = |kind: &str| de::Error::custom(format!("invalid {kind} dynamic profile field"));
        let tag = if is_object(&raw) {
            serde_json::from_str::<Tag>(raw.get())
                .ok()
                .and_then(|tag| tag.kind)
        } else {
            None
        };
        match tag {
            Some(Self::STRING) => serde_json::from_str::<Known<String>>(raw.get())
                .map(|field| Self::String {
                    name: field.name,
                    value: field.value,
                })
                .map_err(|_| typed("string")),
            Some(Self::NUMBER) => serde_json::from_str::<Known<f64>>(raw.get())
                .map(|field| Self::Number {
                    name: field.name,
                    value: field.value,
                })
                .map_err(|_| typed("number")),
            Some(Self::MEDIA) => serde_json::from_str::<Known<ProfileMedia>>(raw.get())
                .map(|field| Self::Media {
                    name: field.name,
                    value: field.value,
                })
                .map_err(|_| typed("media")),
            _ => Ok(Self::Unknown(raw)),
        }
    }
}

/// JSON body for Update Application Identity Profile.
///
/// `data` is **fully replaced** on every update: fields left out of a sent
/// `data` are deleted. Leave `data` as `None` to keep the stored stats.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct UpdateApplicationIdentityProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ApplicationIdentityProfileData>,
}

impl UpdateApplicationIdentityProfile {
    /// Checks the documented limits: 10 KB serialized `data`, 30 dynamic
    /// fields, 100-character primary string values and dynamic field names,
    /// and a 1024-character username.
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(username) = &self.username {
            ensure_chars("profile username", username, MAX_PROFILE_USERNAME_CHARS)?;
        }
        let Some(data) = &self.data else {
            return Ok(());
        };
        if data.dynamic.len() > MAX_PROFILE_DYNAMIC_FIELDS {
            return Err(DiscordError::model(format!(
                "profile data may have at most {MAX_PROFILE_DYNAMIC_FIELDS} dynamic fields, got {}",
                data.dynamic.len()
            )));
        }
        if let Some(primary) = &data.primary {
            for (field, value) in primary.string_fields() {
                if let Some(value) = value {
                    ensure_chars(
                        &format!("profile primary {field}"),
                        value,
                        MAX_PROFILE_STRING_CHARS,
                    )?;
                }
            }
        }
        for field in &data.dynamic {
            if let Some(name) = field.name() {
                ensure_chars(
                    "profile dynamic field name",
                    &name,
                    MAX_PROFILE_STRING_CHARS,
                )?;
            }
            if let DynamicProfileField::String { value, .. } = field {
                ensure_chars(
                    "profile dynamic string value",
                    value,
                    MAX_PROFILE_STRING_CHARS,
                )?;
            }
        }
        let size = serde_json::to_vec(data)?.len();
        if size > MAX_PROFILE_DATA_BYTES {
            return Err(DiscordError::model(format!(
                "serialized profile data must be at most {MAX_PROFILE_DATA_BYTES} bytes, got {size}"
            )));
        }
        Ok(())
    }
}

/// Optional body for Delete Application Identity and query for the
/// external-ID lookup: disambiguates identities that share a provider type
/// and provider-issued user ID.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIdentityProviderId {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

fn ensure_chars(field: &str, value: &str, max: usize) -> Result<(), DiscordError> {
    let count = value.chars().count();
    if count > max {
        return Err(DiscordError::model(format!(
            "{field} must be at most {max} characters, got {count}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use serde::Deserialize;
    use serde_json::{json, Value};

    use super::{
        ApplicationIdentityProfile, ApplicationIdentityProfileData, DynamicProfileField,
        UpdateApplicationIdentityProfile,
    };

    /// resources/application-identity-profile.mdx — Example Application
    /// Identity Profile Object.
    const PROFILE: &str = r#"{
      "username": "johndoe123",
      "metadata": null,
      "data": {
        "primary": {
          "season": "Season 3",
          "rank_name": "Silver",
          "rank_image": {"url": "https://example.com/assets/rank-images/silver.png"},
          "highest_rank": "Platinum",
          "highest_rank_image": {"url": "https://example.com/assets/rank-images/platinum.png"},
          "featured_played_character": "John Doe",
          "featured_played_character_image": {"url": "https://example.com/assets/character-images/john-doe.png"},
          "playtime_hours": 69.41,
          "total_wins": 57,
          "current_period_wins": 8,
          "total_games": 100,
          "current_period_games": 10,
          "total_kills": 253,
          "current_period_kills": 35,
          "total_assists": 478,
          "current_period_assists": 68,
          "total_deaths": 561,
          "current_period_deaths": 21
        },
        "dynamic": [
          {
            "type": 1,
            "name": "my_string",
            "value": "hello"
          },
          {
            "type": 2,
            "name": "my_number",
            "value": 123.45
          },
          {
            "type": 3,
            "name": "my_media",
            "value": {"url": "https://example.com/some-media.png"}
          }
        ]
      }
    }"#;

    fn check_documented(profile: &ApplicationIdentityProfile) {
        assert_eq!(profile.username.as_deref(), Some("johndoe123"));
        assert!(profile.metadata.is_none());
        let data = profile.data.as_ref().unwrap();
        assert_eq!(
            data.primary.as_ref().unwrap().rank_name.as_deref(),
            Some("Silver")
        );
        assert_eq!(
            data.dynamic,
            [
                DynamicProfileField::String {
                    name: "my_string".to_string(),
                    value: "hello".to_string(),
                },
                DynamicProfileField::Number {
                    name: "my_number".to_string(),
                    value: 123.45,
                },
                DynamicProfileField::Media {
                    name: "my_media".to_string(),
                    value: super::ProfileMedia {
                        url: "https://example.com/some-media.png".to_string(),
                    },
                },
            ]
        );
    }

    #[test]
    fn documented_profile_decodes_from_text_and_value_and_writes_back() {
        let from_text: ApplicationIdentityProfile = serde_json::from_str(PROFILE).unwrap();
        check_documented(&from_text);
        let wire: Value = serde_json::from_str(PROFILE).unwrap();
        let from_value: ApplicationIdentityProfile = serde_json::from_value(wire.clone()).unwrap();
        check_documented(&from_value);

        // Everything comes back except the explicit `"metadata": null`.
        let mut expected = wire;
        if let Some(profile) = expected.as_object_mut() {
            profile.remove("metadata");
        }
        assert_eq!(serde_json::to_value(&from_text).unwrap(), expected);
        assert_eq!(serde_json::to_value(&from_value).unwrap(), expected);
    }

    #[test]
    fn game_metadata_stays_raw_and_decodes_on_demand() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct GameMetadata {
            region: String,
            seasons: Vec<u32>,
        }

        let text = r#"{"username": "p1", "metadata": {"region": "kr", "seasons": [1, 2]}}"#;
        let from_text: ApplicationIdentityProfile = serde_json::from_str(text).unwrap();
        // Text input keeps the received bytes.
        assert_eq!(
            from_text.metadata.as_deref().map(|raw| raw.get()),
            Some(r#"{"region": "kr", "seasons": [1, 2]}"#)
        );
        let from_value: ApplicationIdentityProfile =
            serde_json::from_value(serde_json::from_str(text).unwrap()).unwrap();
        for profile in [&from_text, &from_value] {
            assert_eq!(
                profile.metadata_as::<GameMetadata>().unwrap(),
                Some(GameMetadata {
                    region: "kr".to_string(),
                    seasons: vec![1, 2],
                })
            );
            assert_eq!(
                serde_json::to_value(profile).unwrap()["metadata"],
                json!({ "region": "kr", "seasons": [1, 2] })
            );
        }
        assert!(from_text.metadata_as::<Vec<u8>>().is_err());
        assert_eq!(
            ApplicationIdentityProfile::default()
                .metadata_as::<GameMetadata>()
                .unwrap(),
            None
        );
    }

    #[test]
    fn unknown_dynamic_fields_keep_their_text() {
        let text = r#"[
            {"value": [1, 2], "type": 42, "name": "Future", "extra": {"k": true}},
            {"type": "1", "name": "StringTag", "value": "x"},
            [2, "Array", 1.5],
            "plain"
        ]"#;
        let fields: Vec<DynamicProfileField> = serde_json::from_str(text).unwrap();
        assert!(fields
            .iter()
            .all(|field| matches!(field, DynamicProfileField::Unknown(_))));
        let DynamicProfileField::Unknown(raw) = &fields[0] else {
            unreachable!("checked above");
        };
        assert_eq!(
            raw.get(),
            r#"{"value": [1, 2], "type": 42, "name": "Future", "extra": {"k": true}}"#
        );
        assert_eq!(fields[0].name(), Some(Cow::Borrowed("Future")));
        assert_eq!(fields[1].name().as_deref(), Some("StringTag"));
        // Only an object has a name; an array does not.
        assert_eq!(fields[2].name(), None);
        assert_eq!(fields[3].name(), None);

        let wire: Value = serde_json::from_str(text).unwrap();
        assert_eq!(serde_json::to_value(&fields).unwrap(), wire);
        let from_value: Vec<DynamicProfileField> = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&from_value).unwrap(), wire);
    }

    #[test]
    fn known_tags_decode_in_any_key_order_and_reject_bad_values() {
        let field: DynamicProfileField =
            serde_json::from_str(r#"{"value": 3.5, "name": "KDA", "type": 2}"#).unwrap();
        assert_eq!(
            field,
            DynamicProfileField::Number {
                name: "KDA".to_string(),
                value: 3.5,
            }
        );
        assert!(serde_json::from_str::<DynamicProfileField>(
            r#"{"type": 3, "name": "Badge", "value": "not media"}"#
        )
        .is_err());
        assert!(serde_json::from_value::<DynamicProfileField>(
            json!({ "type": 1, "name": "Main" })
        )
        .is_err());
    }

    #[test]
    fn unknown_fields_compare_by_text_and_an_escaped_name_is_owned() {
        let compact: DynamicProfileField =
            serde_json::from_str(r#"{"type":9,"name":"Caf\u00e9"}"#).unwrap();
        let same: DynamicProfileField =
            serde_json::from_str(r#"{"type":9,"name":"Caf\u00e9"}"#).unwrap();
        let spaced: DynamicProfileField =
            serde_json::from_str(r#"{"type": 9, "name": "Caf\u00e9"}"#).unwrap();
        assert_eq!(compact, same);
        assert_ne!(compact, spaced);
        assert!(matches!(compact.name(), Some(Cow::Owned(name)) if name == "Café"));
    }

    #[test]
    fn validate_checks_unknown_field_names() {
        let long = "n".repeat(101);
        let field: DynamicProfileField =
            serde_json::from_value(json!({ "type": 77, "name": long, "value": 1 })).unwrap();
        let update = UpdateApplicationIdentityProfile {
            username: None,
            data: Some(ApplicationIdentityProfileData {
                primary: None,
                dynamic: vec![field],
            }),
        };
        assert!(update.validate().is_err());
    }
}
