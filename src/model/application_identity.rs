//! Application Identity Profile resource (Discord changelog 2026-08-26).
//!
//! Games claimed on Discord write player stats (rank, playtime, wins, custom
//! fields) to a user's Application Identity record, which Discord renders as
//! a Game Stats Widget on the user's profile.

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DiscordError;

use super::GenericId;

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
    pub user_id: Option<GenericId>,
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
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ApplicationIdentityProfile {
    /// The user's username in the external system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Arbitrary game-defined data that Discord stores but never reads.
    ///
    /// This stays untyped on purpose: the shape belongs to the game, not to
    /// Discord, so any fixed Rust type would be wrong for someone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Game stats shown in the widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ApplicationIdentityProfileData>,
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
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DynamicProfileField {
    /// `type: 1`
    String { name: String, value: String },
    /// `type: 2`
    Number { name: String, value: f64 },
    /// `type: 3`
    Media { name: String, value: ProfileMedia },
    /// A field type this crate does not model yet, preserved as received.
    Unknown(Value),
}

impl DynamicProfileField {
    const STRING: u64 = 1;
    const NUMBER: u64 = 2;
    const MEDIA: u64 = 3;

    /// Returns the field name, when the field type is known.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::String { name, .. } | Self::Number { name, .. } | Self::Media { name, .. } => {
                Some(name)
            }
            Self::Unknown(raw) => raw.get("name").and_then(Value::as_str),
        }
    }
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
        #[derive(Deserialize)]
        struct Known<T> {
            name: String,
            value: T,
        }

        let raw = Value::deserialize(deserializer)?;
        let typed = |kind: &str| de::Error::custom(format!("invalid {kind} dynamic profile field"));
        match raw.get("type").and_then(Value::as_u64) {
            Some(Self::STRING) => serde_json::from_value::<Known<String>>(raw)
                .map(|field| Self::String {
                    name: field.name,
                    value: field.value,
                })
                .map_err(|_| typed("string")),
            Some(Self::NUMBER) => serde_json::from_value::<Known<f64>>(raw)
                .map(|field| Self::Number {
                    name: field.name,
                    value: field.value,
                })
                .map_err(|_| typed("number")),
            Some(Self::MEDIA) => serde_json::from_value::<Known<ProfileMedia>>(raw)
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
                ensure_chars("profile dynamic field name", name, MAX_PROFILE_STRING_CHARS)?;
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
