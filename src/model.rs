use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::parsers::V2ModalSubmission;
use crate::types::Emoji;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
/// Typed Discord API object for `Snowflake`.
pub struct Snowflake {
    raw: String,
    numeric: Option<u64>,
}

impl Snowflake {
    /// Discord epoch: January 1, 2015 00:00:00 UTC in milliseconds.
    const DISCORD_EPOCH: u64 = 1_420_070_400_000;

    /// Creates or returns `new` data.
    pub fn new(value: impl Into<String>) -> Self {
        let raw = value.into();
        let numeric = raw.parse().ok();
        Self { raw, numeric }
    }

    /// Creates or returns `try_new` data.
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let snowflake = Self::new(value);
        if snowflake.is_valid() {
            Ok(snowflake)
        } else {
            Err("snowflake must contain only ASCII digits".to_string())
        }
    }

    /// Runs the `as_str` operation.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Runs the `as_u64` operation.
    pub fn as_u64(&self) -> Option<u64> {
        self.numeric
    }

    /// Runs the `to_u64` operation.
    pub fn to_u64(&self) -> Option<u64> {
        self.as_u64()
    }

    /// Runs the `is_valid` operation.
    pub fn is_valid(&self) -> bool {
        !self.raw.is_empty() && self.raw.chars().all(|ch| ch.is_ascii_digit())
    }

    /// Extracts the creation timestamp from this Snowflake as Unix milliseconds.
    ///
    /// Discord encodes the creation timestamp in the top 42 bits of every Snowflake ID.
    /// Returns `None` if the inner value is not a valid u64.
    pub fn timestamp(&self) -> Option<u64> {
        let raw = self.numeric?;
        // (raw >> 22) gives milliseconds since Discord epoch
        Some((raw >> 22) + Self::DISCORD_EPOCH)
    }
}

impl Display for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

impl From<u64> for Snowflake {
    fn from(value: u64) -> Self {
        Self {
            raw: value.to_string(),
            numeric: Some(value),
        }
    }
}

impl From<&str> for Snowflake {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Snowflake {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl FromStr for Snowflake {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl Serialize for Snowflake {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SnowflakeVisitor;

        impl<'de> Visitor<'de> for SnowflakeVisitor {
            type Value = Snowflake;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Discord snowflake encoded as a string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value < 0 {
                    return Err(E::custom("snowflake cannot be negative"));
                }
                Ok(Snowflake::from(value as u64))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }
        }

        deserializer.deserialize_any(SnowflakeVisitor)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
/// Typed Discord API object for `PermissionsBitField`.
pub struct PermissionsBitField(pub u64);

impl PermissionsBitField {
    /// Runs the `bits` operation.
    pub fn bits(self) -> u64 {
        self.0
    }

    /// Runs the `contains` operation.
    pub fn contains(self, permission: u64) -> bool {
        self.0 & permission == permission
    }

    /// Runs the `insert` operation.
    pub fn insert(&mut self, permission: u64) {
        self.0 |= permission;
    }

    /// Runs the `remove` operation.
    pub fn remove(&mut self, permission: u64) {
        self.0 &= !permission;
    }
}

impl Serialize for PermissionsBitField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for PermissionsBitField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PermissionsVisitor;

        impl<'de> Visitor<'de> for PermissionsVisitor {
            type Value = PermissionsBitField;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Discord permission bitfield encoded as a string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PermissionsBitField(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value
                    .parse()
                    .map(PermissionsBitField)
                    .map_err(|error| E::custom(format!("invalid permission bitfield: {error}")))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_any(PermissionsVisitor)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Data for a user's avatar decoration.
pub struct AvatarDecorationData {
    /// Decoration asset hash.
    pub asset: String,
    /// SKU that owns the decoration.
    pub sku_id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Data for a user's profile nameplate collectible.
pub struct UserNameplate {
    /// SKU that owns the nameplate.
    pub sku_id: Snowflake,
    /// Nameplate asset path.
    pub asset: String,
    /// Nameplate label.
    pub label: String,
    /// Nameplate palette name.
    pub palette: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// User collectibles exposed by Discord's user object.
pub struct UserCollectibles {
    /// Optional profile nameplate collectible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nameplate: Option<UserNameplate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Primary guild identity displayed on a user's profile.
pub struct UserPrimaryGuild {
    /// Guild ID for the displayed server identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_guild_id: Option<Snowflake>,
    /// Whether the identity is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_enabled: Option<bool>,
    /// Server tag text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Server tag badge hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `User`.
pub struct User {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `username`.
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `global_name`.
    pub global_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `discriminator`.
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `bot`.
    pub bot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `system`.
    pub system: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mfa_enabled`.
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner`.
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `accent_color`.
    pub accent_color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `locale`.
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `verified`.
    pub verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `email`.
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_type`.
    pub premium_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `public_flags`.
    pub public_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar_decoration_data`.
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `collectibles`.
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `primary_guild`.
    pub primary_guild: Option<UserPrimaryGuild>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Request body for modifying the current bot user.
pub struct ModifyCurrentUser {
    /// New username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// New avatar image data, or `None` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// New banner image data, or `None` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// OAuth2 connection attached to the current user.
pub struct UserConnection {
    /// Provider account ID.
    pub id: String,
    /// Provider account display name.
    pub name: String,
    /// Discord connection service type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Whether the connection has been revoked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked: Option<bool>,
    /// Partial integration objects associated with the connection.
    #[serde(default)]
    pub integrations: Vec<serde_json::Value>,
    /// Whether the connection is verified.
    #[serde(default)]
    pub verified: bool,
    /// Whether friend sync is enabled.
    #[serde(default)]
    pub friend_sync: bool,
    /// Whether activities from this connection appear in presence.
    #[serde(default)]
    pub show_activity: bool,
    /// Whether the connection has a matching third-party OAuth2 token.
    #[serde(default)]
    pub two_way_link: bool,
    /// Discord visibility setting for the connection.
    #[serde(default)]
    pub visibility: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Application role connection attached to the current user.
pub struct UserApplicationRoleConnection {
    /// Vanity platform name shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_name: Option<String>,
    /// Platform username shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_username: Option<String>,
    /// Metadata values keyed by application role connection metadata key.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Request body for updating the current user's application role connection.
pub struct UpdateUserApplicationRoleConnection {
    /// Vanity platform name shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_name: Option<String>,
    /// Platform username shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_username: Option<String>,
    /// Metadata values keyed by application role connection metadata key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl UpdateUserApplicationRoleConnection {
    /// Creates an empty update body.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the platform name.
    pub fn platform_name(mut self, platform_name: impl Into<String>) -> Self {
        self.platform_name = Some(platform_name.into());
        self
    }

    /// Sets the platform username.
    pub fn platform_username(mut self, platform_username: impl Into<String>) -> Self {
        self.platform_username = Some(platform_username.into());
        self
    }

    /// Sets the metadata map.
    pub fn metadata<I, K, V>(mut self, metadata: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.metadata = Some(
            metadata
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        );
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleTags`.
pub struct RoleTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `bot_id`.
    pub bot_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `integration_id`.
    pub integration_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_subscriber`.
    pub premium_subscriber: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `RoleColors`.
pub struct RoleColors {
    /// Discord API payload field `primary_color`.
    pub primary_color: u64,
    /// Discord API payload field `secondary_color`.
    pub secondary_color: Option<u64>,
    /// Discord API payload field `tertiary_color`.
    pub tertiary_color: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Role`.
pub struct Role {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `color`.
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `colors`.
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `hoist`.
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `managed`.
    pub managed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mentionable`.
    pub mentionable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permissions`.
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unicode_emoji`.
    pub unicode_emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `tags`.
    pub tags: Option<RoleTags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Attachment`.
pub struct Attachment {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `filename`.
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `content_type`.
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `size`.
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `url`.
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `height`.
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `width`.
    pub width: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `ephemeral`.
    pub ephemeral: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `duration_secs`.
    pub duration_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `waveform`.
    pub waveform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `title`.
    pub title: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
/// Typed Discord API enum for `ChannelType`.
pub enum ChannelType {
    /// Discord API enum variant `Text`.
    Text = 0,
    /// Discord API enum variant `Dm`.
    Dm = 1,
    /// Discord API enum variant `Voice`.
    Voice = 2,
    /// Discord API enum variant `GroupDm`.
    GroupDm = 3,
    /// Discord API enum variant `Category`.
    Category = 4,
    /// Discord API enum variant `News`.
    News = 5,
    /// Discord API enum variant `StageVoice`.
    StageVoice = 13,
    /// Discord API enum variant `GuildDirectory`.
    GuildDirectory = 14,
    /// Discord API enum variant `GuildForum`.
    GuildForum = 15,
    /// Discord API enum variant `GuildMedia`.
    GuildMedia = 16,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PermissionOverwrite`.
pub struct PermissionOverwrite {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `allow`.
    pub allow: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `deny`.
    pub deny: Option<PermissionsBitField>,
}

/// Request body for creating a guild channel.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as `parent_id`, `rtc_region`, or
/// `default_reaction_emoji`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuildChannel {
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `topic`.
    pub topic: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `bitrate`.
    pub bitrate: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_limit`.
    pub user_limit: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rate_limit_per_user`.
    pub rate_limit_per_user: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permission_overwrites`.
    pub permission_overwrites: Option<Option<Vec<PermissionOverwrite>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `parent_id`.
    pub parent_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nsfw`.
    pub nsfw: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rtc_region`.
    pub rtc_region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `video_quality_mode`.
    pub video_quality_mode: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_auto_archive_duration`.
    pub default_auto_archive_duration: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_reaction_emoji`.
    pub default_reaction_emoji: Option<Option<DefaultReaction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `available_tags`.
    pub available_tags: Option<Option<Vec<ForumTag>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_sort_order`.
    pub default_sort_order: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_forum_layout`.
    pub default_forum_layout: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_thread_rate_limit_per_user`.
    pub default_thread_rate_limit_per_user: Option<Option<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `EditChannelPermission`.
pub struct EditChannelPermission {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `allow`.
    pub allow: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `deny`.
    pub deny: Option<PermissionsBitField>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Channel`.
pub struct Channel {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `topic`.
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `parent_id`.
    pub parent_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nsfw`.
    pub nsfw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `bitrate`.
    pub bitrate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_limit`.
    pub user_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rate_limit_per_user`.
    pub rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `last_message_id`.
    pub last_message_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `owner_id`.
    pub owner_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permission_overwrites`.
    pub permission_overwrites: Option<Vec<PermissionOverwrite>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rtc_region`.
    pub rtc_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `video_quality_mode`.
    pub video_quality_mode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_auto_archive_duration`.
    pub default_auto_archive_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `last_pin_timestamp`.
    pub last_pin_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `thread_metadata`.
    pub thread_metadata: Option<ThreadMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `message_count`.
    pub message_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member_count`.
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `total_message_sent`.
    pub total_message_sent: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `available_tags`.
    pub available_tags: Option<Vec<ForumTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `applied_tags`.
    pub applied_tags: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_reaction_emoji`.
    pub default_reaction_emoji: Option<DefaultReaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_thread_rate_limit_per_user`.
    pub default_thread_rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_sort_order`.
    pub default_sort_order: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_forum_layout`.
    pub default_forum_layout: Option<u64>,
}

/// Lobby member object used by Discord's Lobby resource.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyMember {
    /// User ID of the lobby member.
    pub id: Snowflake,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Lobby object used by Discord matchmaking APIs.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Lobby {
    /// Lobby ID.
    pub id: Snowflake,
    /// Application that owns the lobby.
    pub application_id: Snowflake,
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Current lobby members.
    #[serde(default)]
    pub members: Vec<LobbyMember>,
    /// Text channel linked to this lobby, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_channel: Option<Channel>,
}

/// Request body for creating a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateLobby {
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Initial lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for modifying a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyLobby {
    /// Replacement app-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Replacement lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for adding or updating one lobby member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AddLobbyMember {
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Request body entry for bulk lobby member updates.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyMemberUpdate {
    /// User ID to add, update, or remove.
    pub id: Snowflake,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Removes this member instead of upserting when true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_member: Option<bool>,
}

/// Request body for linking or unlinking a channel from a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LinkLobbyChannel {
    /// Channel to link. Omit this field to unlink any currently linked channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `DefaultReaction`.
pub struct DefaultReaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_id`.
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_name`.
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ForumTag`.
pub struct ForumTag {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default)]
    /// Discord API payload field `moderated`.
    pub moderated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_id`.
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_name`.
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Member`.
pub struct Member {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
    #[serde(default)]
    /// Discord API payload field `roles`.
    pub roles: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nick`.
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `joined_at`.
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permissions`.
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `deaf`.
    pub deaf: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mute`.
    pub mute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner`.
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_since`.
    pub premium_since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `pending`.
    pub pending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `communication_disabled_until`.
    pub communication_disabled_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar_decoration_data`.
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `collectibles`.
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GetGuildQuery`.
pub struct GetGuildQuery {
    /// Discord API payload field `with_counts`.
    pub with_counts: Option<bool>,
}

/// Request body for adding an OAuth2-authorized user to a guild.
///
/// `access_token` must be a user OAuth2 token granted with the `guilds.join`
/// scope for the same application as the bot token used by the REST client.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AddGuildMember {
    /// OAuth2 user access token with the `guilds.join` scope.
    pub access_token: String,
    /// Optional nickname to apply when the user joins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    /// Optional initial guild roles for the member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Snowflake>>,
    /// Whether the member should be muted in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    /// Whether the member should be deafened in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<bool>,
}

/// Request body for modifying a guild.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as image data, linked channel IDs, or
/// description.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `region`.
    pub region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `verification_level`.
    pub verification_level: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_message_notifications`.
    pub default_message_notifications: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `explicit_content_filter`.
    pub explicit_content_filter: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `afk_channel_id`.
    pub afk_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `afk_timeout`.
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `splash`.
    pub splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `discovery_splash`.
    pub discovery_splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner`.
    pub banner: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `system_channel_id`.
    pub system_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `system_channel_flags`.
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rules_channel_id`.
    pub rules_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `public_updates_channel_id`.
    pub public_updates_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `preferred_locale`.
    pub preferred_locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `features`.
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_progress_bar_enabled`.
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `safety_alerts_channel_id`.
    pub safety_alerts_channel_id: Option<Option<Snowflake>>,
}

/// Request body for modifying a guild member.
///
/// The outer `Option` controls whether a field is omitted. Use `Some(None)` to
/// send JSON `null` for nullable Discord fields such as `channel_id` or
/// `communication_disabled_until`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildMember {
    /// New guild nickname, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<Option<String>>,
    /// Replacement role IDs for the member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Option<Vec<Snowflake>>>,
    /// Whether the member should be muted in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<Option<bool>>,
    /// Whether the member should be deafened in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<Option<bool>>,
    /// Voice channel to move the member to, or `Some(None)` to disconnect them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Option<Snowflake>>,
    /// Timeout expiration timestamp, or `Some(None)` to remove the timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communication_disabled_until: Option<Option<String>>,
    /// Guild member flags bit set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Option<u64>>,
}

/// Request body for modifying the current member in a guild.
///
/// The outer `Option` controls omission; `Some(None)` serializes JSON `null`
/// for fields Discord allows callers to clear.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentMember {
    /// New guild nickname, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<Option<String>>,
    /// Data URI guild banner image, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Option<String>>,
    /// Data URI guild avatar image, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    /// Guild member bio, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<Option<String>>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildMembersQuery`.
pub struct GuildMembersQuery {
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SearchGuildMembersQuery`.
pub struct SearchGuildMembersQuery {
    /// Discord API payload field `query`.
    pub query: String,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

/// Request entry for moving or reparenting a guild channel.
///
/// The outer `Option` controls whether a field is omitted. Use `Some(None)` to
/// send JSON `null` for nullable Discord fields such as `parent_id`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildChannelPosition {
    /// Channel to modify.
    pub id: Snowflake,
    /// New sorting position, or `Some(None)` to send `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
    /// Whether to sync permission overwrites with the new parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_permissions: Option<Option<bool>>,
    /// New parent category ID, or `Some(None)` to remove the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceState`.
pub struct VoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `session_id`.
    pub session_id: Option<String>,
    #[serde(default)]
    /// Discord API payload field `deaf`.
    pub deaf: bool,
    #[serde(default)]
    /// Discord API payload field `mute`.
    pub mute: bool,
    #[serde(default)]
    /// Discord API payload field `self_deaf`.
    pub self_deaf: bool,
    #[serde(default)]
    /// Discord API payload field `self_mute`.
    pub self_mute: bool,
    #[serde(default)]
    /// Discord API payload field `suppress`.
    pub suppress: bool,
    #[serde(default)]
    /// Discord API payload field `self_stream`.
    pub self_stream: bool,
    #[serde(default)]
    /// Discord API payload field `self_video`.
    pub self_video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `request_to_speak_timestamp`.
    pub request_to_speak_timestamp: Option<String>,
}

/// Request body for modifying the current user's guild voice state.
///
/// The outer `Option` controls omission. Use `Some(None)` to clear
/// `request_to_speak_timestamp`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `suppress`.
    pub suppress: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `request_to_speak_timestamp`.
    pub request_to_speak_timestamp: Option<Option<String>>,
}

/// Request body for modifying another user's guild voice state.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `suppress`.
    pub suppress: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceServerUpdate`.
pub struct VoiceServerUpdate {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `token`.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `endpoint`.
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Guild`.
pub struct Guild {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `owner_id`.
    pub owner_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unavailable`.
    pub unavailable: Option<bool>,
    #[serde(default)]
    /// Discord API payload field `roles`.
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `splash`.
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner`.
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(default)]
    /// Discord API payload field `features`.
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_tier`.
    pub premium_tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member_count`.
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_presences`.
    pub max_presences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_members`.
    pub max_members: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `vanity_url_code`.
    pub vanity_url_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `system_channel_id`.
    pub system_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `rules_channel_id`.
    pub rules_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `preferred_locale`.
    pub preferred_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `afk_channel_id`.
    pub afk_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `afk_timeout`.
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `verification_level`.
    pub verification_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_message_notifications`.
    pub default_message_notifications: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `explicit_content_filter`.
    pub explicit_content_filter: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mfa_level`.
    pub mfa_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `system_channel_flags`.
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `public_updates_channel_id`.
    pub public_updates_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nsfw_level`.
    pub nsfw_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_subscription_count`.
    pub premium_subscription_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium_progress_bar_enabled`.
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `safety_alerts_channel_id`.
    pub safety_alerts_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_member_count`.
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_presence_count`.
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `incidents_data`.
    pub incidents_data: Option<GuildIncidentsData>,
}

/// Active safety incident actions configured for a guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildIncidentsData {
    /// Timestamp when invite creation will be re-enabled, or `null` when invite blocking is disabled.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages will be re-enabled, or `null` when DM blocking is disabled.
    pub dms_disabled_until: Option<String>,
    /// Timestamp when Discord detected DM spam, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_spam_detected_at: Option<String>,
    /// Timestamp when Discord detected a raid, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_detected_at: Option<String>,
}

/// Request body for modifying guild incident actions.
///
/// Supplying `None` serializes `null`, which disables that incident action.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildIncidentActions {
    /// Timestamp when invites should be re-enabled, or `null` to disable invite blocking.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages should be re-enabled, or `null` to disable DM blocking.
    pub dms_disabled_until: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CurrentUserGuild`.
pub struct CurrentUserGuild {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner`.
    pub banner: Option<String>,
    #[serde(default)]
    /// Discord API payload field `owner`.
    pub owner: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permissions`.
    pub permissions: Option<String>,
    #[serde(default)]
    /// Discord API payload field `features`.
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_member_count`.
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_presence_count`.
    pub approximate_presence_count: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CurrentUserGuildsQuery`.
pub struct CurrentUserGuildsQuery {
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `with_counts`.
    pub with_counts: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPreview`.
pub struct GuildPreview {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `splash`.
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `discovery_splash`.
    pub discovery_splash: Option<String>,
    #[serde(default)]
    /// Discord API payload field `emojis`.
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    /// Discord API payload field `features`.
    pub features: Vec<String>,
    /// Discord API payload field `approximate_member_count`.
    pub approximate_member_count: u64,
    /// Discord API payload field `approximate_presence_count`.
    pub approximate_presence_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(default)]
    /// Discord API payload field `stickers`.
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VanityUrl`.
pub struct VanityUrl {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `code`.
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `uses`.
    pub uses: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPruneCount`.
pub struct GuildPruneCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `pruned`.
    pub pruned: Option<u64>,
}

/// Type alias for `GuildPruneResult`.
pub type GuildPruneResult = GuildPruneCount;

/// Request body for beginning a guild prune.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BeginGuildPruneRequest {
    /// Number of inactive days to prune, between 1 and 30.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u64>,
    /// Whether Discord should compute and return the prune count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_prune_count: Option<bool>,
    /// Role IDs to include in the prune.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_roles: Option<Vec<Snowflake>>,
    /// Deprecated Discord field for audit-log prune reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanRequest`.
pub struct BulkGuildBanRequest {
    /// Discord API payload field `user_ids`.
    pub user_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `delete_message_seconds`.
    pub delete_message_seconds: Option<u64>,
}

/// Request body for banning a single guild member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildBan {
    /// Deprecated Discord field for message deletion duration in days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_days: Option<u64>,
    /// Message deletion duration in seconds, between 0 and 604800.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanResponse`.
pub struct BulkGuildBanResponse {
    #[serde(default)]
    /// Discord API payload field `banned_users`.
    pub banned_users: Vec<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `failed_users`.
    pub failed_users: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ModifyGuildRolePosition`.
pub struct ModifyGuildRolePosition {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<Option<i64>>,
}

/// Request body for creating a guild role.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permissions`.
    pub permissions: Option<PermissionsBitField>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `colors`.
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `hoist`.
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unicode_emoji`.
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mentionable`.
    pub mentionable: Option<bool>,
}

/// Request body for modifying a guild role.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as `icon` or `unicode_emoji`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `permissions`.
    pub permissions: Option<Option<PermissionsBitField>>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `colors`.
    pub colors: Option<Option<RoleColors>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `hoist`.
    pub hoist: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unicode_emoji`.
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mentionable`.
    pub mentionable: Option<Option<bool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceRegion`.
pub struct VoiceRegion {
    /// Discord API payload field `id`.
    pub id: String,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default)]
    /// Discord API payload field `optimal`.
    pub optimal: bool,
    #[serde(default)]
    /// Discord API payload field `deprecated`.
    pub deprecated: bool,
    #[serde(default)]
    /// Discord API payload field `custom`.
    pub custom: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationRule`.
pub struct AutoModerationRule {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    /// Discord API payload field `creator_id`.
    pub creator_id: Snowflake,
    /// Discord API payload field `event_type`.
    pub event_type: u8,
    /// Discord API payload field `trigger_type`.
    pub trigger_type: u8,
    #[serde(default)]
    /// Discord API payload field `trigger_metadata`.
    pub trigger_metadata: AutoModerationTriggerMetadata,
    #[serde(default)]
    /// Discord API payload field `actions`.
    pub actions: Vec<AutoModerationAction>,
    #[serde(default)]
    /// Discord API payload field `enabled`.
    pub enabled: bool,
    #[serde(default)]
    /// Discord API payload field `exempt_roles`.
    pub exempt_roles: Vec<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `exempt_channels`.
    pub exempt_channels: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationTriggerMetadata`.
pub struct AutoModerationTriggerMetadata {
    #[serde(default)]
    /// Discord API payload field `keyword_filter`.
    pub keyword_filter: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `regex_patterns`.
    pub regex_patterns: Option<Vec<String>>,
    #[serde(default)]
    /// Discord API payload field `presets`.
    pub presets: Vec<u8>,
    #[serde(default)]
    /// Discord API payload field `allow_list`.
    pub allow_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mention_total_limit`.
    pub mention_total_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mention_raid_protection_enabled`.
    pub mention_raid_protection_enabled: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationAction`.
pub struct AutoModerationAction {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `metadata`.
    pub metadata: Option<AutoModerationActionMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationActionMetadata`.
pub struct AutoModerationActionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `duration_seconds`.
    pub duration_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `custom_message`.
    pub custom_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Message`.
pub struct Message {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `author`.
    pub author: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
    #[serde(default)]
    /// Discord API payload field `content`.
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `timestamp`.
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `edited_timestamp`.
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    /// Discord API payload field `mentions`.
    pub mentions: Vec<User>,
    #[serde(default)]
    /// Discord API payload field `mention_roles`.
    pub mention_roles: Vec<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `attachments`.
    pub attachments: Vec<Attachment>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `pinned`.
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `tts`.
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `webhook_id`.
    pub webhook_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application`.
    pub application: Option<serde_json::Value>,
    #[serde(default)]
    /// Discord API payload field `embeds`.
    pub embeds: Vec<Embed>,
    #[serde(default)]
    /// Discord API payload field `reactions`.
    pub reactions: Vec<Reaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `activity`.
    pub activity: Option<MessageActivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mention_everyone`.
    pub mention_everyone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mention_channels`.
    pub mention_channels: Option<Vec<ChannelMention>>,
    #[serde(default)]
    /// Discord API payload field `sticker_items`.
    pub sticker_items: Vec<StickerItem>,
    #[serde(default)]
    /// Discord API payload field `stickers`.
    pub stickers: Vec<Sticker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `role_subscription_data`.
    pub role_subscription_data: Option<RoleSubscriptionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `message_reference`.
    pub message_reference: Option<MessageReference>,
    #[serde(default)]
    /// Discord API payload field `message_snapshots`.
    pub message_snapshots: Vec<MessageSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `referenced_message`.
    pub referenced_message: Option<Box<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `interaction_metadata`.
    pub interaction_metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `interaction`.
    pub interaction: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `resolved`.
    pub resolved: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `components`.
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `poll`.
    pub poll: Option<Poll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `call`.
    pub call: Option<MessageCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `shared_client_theme`.
    pub shared_client_theme: Option<SharedClientTheme>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageActivity`.
pub struct MessageActivity {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `party_id`.
    pub party_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageCall`.
pub struct MessageCall {
    #[serde(default)]
    /// Discord API payload field `participants`.
    pub participants: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `ended_timestamp`.
    pub ended_timestamp: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshot`.
pub struct MessageSnapshot {
    /// Discord API payload field `message`.
    pub message: MessageSnapshotMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshotMessage`.
pub struct MessageSnapshotMessage {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(default)]
    /// Discord API payload field `content`.
    pub content: String,
    #[serde(default)]
    /// Discord API payload field `embeds`.
    pub embeds: Vec<Embed>,
    #[serde(default)]
    /// Discord API payload field `attachments`.
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `timestamp`.
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `edited_timestamp`.
    pub edited_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(default)]
    /// Discord API payload field `mentions`.
    pub mentions: Vec<User>,
    #[serde(default)]
    /// Discord API payload field `mention_roles`.
    pub mention_roles: Vec<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `stickers`.
    pub stickers: Vec<Sticker>,
    #[serde(default)]
    /// Discord API payload field `sticker_items`.
    pub sticker_items: Vec<StickerItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `components`.
    pub components: Option<Vec<serde_json::Value>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleSubscriptionData`.
pub struct RoleSubscriptionData {
    /// Discord API payload field `role_subscription_listing_id`.
    pub role_subscription_listing_id: Snowflake,
    /// Discord API payload field `tier_name`.
    pub tier_name: String,
    /// Discord API payload field `total_months_subscribed`.
    pub total_months_subscribed: u64,
    /// Discord API payload field `is_renewal`.
    pub is_renewal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SharedClientTheme`.
pub struct SharedClientTheme {
    #[serde(default)]
    /// Discord API payload field `colors`.
    pub colors: Vec<String>,
    /// Discord API payload field `gradient_angle`.
    pub gradient_angle: u64,
    /// Discord API payload field `base_mix`.
    pub base_mix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `base_theme`.
    pub base_theme: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessagePin`.
pub struct MessagePin {
    /// Discord API payload field `pinned_at`.
    pub pinned_at: String,
    /// Discord API payload field `message`.
    pub message: Message,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChannelPins`.
pub struct ChannelPins {
    #[serde(default)]
    /// Discord API payload field `items`.
    pub items: Vec<MessagePin>,
    #[serde(default)]
    /// Discord API payload field `has_more`.
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `ChannelPinsQuery`.
pub struct ChannelPinsQuery {
    /// Discord API payload field `before`.
    pub before: Option<String>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AllowedMentions`.
pub struct AllowedMentions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `parse`.
    pub parse: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `roles`.
    pub roles: Vec<Snowflake>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `users`.
    pub users: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `replied_user`.
    pub replied_user: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SearchGuildMessagesResponse`.
pub struct SearchGuildMessagesResponse {
    #[serde(default)]
    /// Discord API payload field `doing_deep_historical_index`.
    pub doing_deep_historical_index: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `documents_indexed`.
    pub documents_indexed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `retry_after`.
    pub retry_after: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `code`.
    pub code: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `message`.
    pub message: Option<String>,
    #[serde(default)]
    /// Discord API payload field `total_results`.
    pub total_results: u64,
    #[serde(default)]
    /// Discord API payload field `messages`.
    pub messages: Vec<Vec<Message>>,
    #[serde(default)]
    /// Discord API payload field `threads`.
    pub threads: Vec<Channel>,
    #[serde(default)]
    /// Discord API payload field `members`.
    pub members: Vec<ThreadMember>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `SearchGuildMessagesQuery`.
pub struct SearchGuildMessagesQuery {
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `offset`.
    pub offset: Option<u64>,
    /// Discord API payload field `max_id`.
    pub max_id: Option<Snowflake>,
    /// Discord API payload field `min_id`.
    pub min_id: Option<Snowflake>,
    /// Discord API payload field `slop`.
    pub slop: Option<u64>,
    /// Discord API payload field `content`.
    pub content: Option<String>,
    /// Discord API payload field `channel_ids`.
    pub channel_ids: Vec<Snowflake>,
    /// Discord API payload field `author_types`.
    pub author_types: Vec<String>,
    /// Discord API payload field `author_ids`.
    pub author_ids: Vec<Snowflake>,
    /// Discord API payload field `mentions`.
    pub mentions: Vec<Snowflake>,
    /// Discord API payload field `mentions_role_ids`.
    pub mentions_role_ids: Vec<Snowflake>,
    /// Discord API payload field `mention_everyone`.
    pub mention_everyone: Option<bool>,
    /// Discord API payload field `replied_to_user_ids`.
    pub replied_to_user_ids: Vec<Snowflake>,
    /// Discord API payload field `replied_to_message_ids`.
    pub replied_to_message_ids: Vec<Snowflake>,
    /// Discord API payload field `pinned`.
    pub pinned: Option<bool>,
    /// Discord API payload field `has`.
    pub has: Vec<String>,
    /// Discord API payload field `embed_types`.
    pub embed_types: Vec<String>,
    /// Discord API payload field `embed_providers`.
    pub embed_providers: Vec<String>,
    /// Discord API payload field `link_hostnames`.
    pub link_hostnames: Vec<String>,
    /// Discord API payload field `attachment_filenames`.
    pub attachment_filenames: Vec<String>,
    /// Discord API payload field `attachment_extensions`.
    pub attachment_extensions: Vec<String>,
    /// Discord API payload field `sort_by`.
    pub sort_by: Option<String>,
    /// Discord API payload field `sort_order`.
    pub sort_order: Option<String>,
    /// Discord API payload field `include_nsfw`.
    pub include_nsfw: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOptionChoice`.
pub struct ApplicationCommandOptionChoice {
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default)]
    /// Discord API payload field `value`.
    pub value: serde_json::Value,
}

impl ApplicationCommandOptionChoice {
    /// Creates or returns `new` data.
    pub fn new(name: impl Into<String>, value: impl Serialize) -> Self {
        Self {
            name: name.into(),
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Creates or returns `try_new` data.
    pub fn try_new(
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            value: serde_json::to_value(value)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollMedia`.
pub struct PollMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `text`.
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswer`.
pub struct PollAnswer {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `answer_id`.
    pub answer_id: Option<u64>,
    /// Discord API payload field `poll_media`.
    pub poll_media: PollMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerCount`.
pub struct PollAnswerCount {
    /// Discord API payload field `id`.
    pub id: u64,
    /// Discord API payload field `count`.
    pub count: u64,
    /// Discord API payload field `me_voted`.
    pub me_voted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollResults`.
pub struct PollResults {
    /// Discord API payload field `is_finalized`.
    pub is_finalized: bool,
    #[serde(default)]
    /// Discord API payload field `answer_counts`.
    pub answer_counts: Vec<PollAnswerCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Poll`.
pub struct Poll {
    /// Discord API payload field `question`.
    pub question: PollMedia,
    #[serde(default)]
    /// Discord API payload field `answers`.
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `expiry`.
    pub expiry: Option<String>,
    #[serde(default)]
    /// Discord API payload field `allow_multiselect`.
    pub allow_multiselect: bool,
    #[serde(default)]
    /// Discord API payload field `layout_type`.
    pub layout_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `results`.
    pub results: Option<PollResults>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerVoters`.
pub struct PollAnswerVoters {
    #[serde(default)]
    /// Discord API payload field `users`.
    pub users: Vec<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreatePoll`.
pub struct CreatePoll {
    /// Discord API payload field `question`.
    pub question: PollMedia,
    #[serde(default)]
    /// Discord API payload field `answers`.
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `duration`.
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `allow_multiselect`.
    pub allow_multiselect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `layout_type`.
    pub layout_type: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChannelMention`.
pub struct ChannelMention {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageReference`.
pub struct MessageReference {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `fail_if_not_exists`.
    pub fail_if_not_exists: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Ban`.
pub struct Ban {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `reason`.
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildBansQuery`.
pub struct GuildBansQuery {
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Invite`.
pub struct Invite {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `code`.
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild`.
    pub guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel`.
    pub channel: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `inviter`.
    pub inviter: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_type`.
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_user`.
    pub target_user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_application`.
    pub target_application: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_presence_count`.
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_member_count`.
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `expires_at`.
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_scheduled_event`.
    pub guild_scheduled_event: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `roles`.
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `uses`.
    pub uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_uses`.
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_age`.
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `temporary`.
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `created_at`.
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateChannelInvite`.
pub struct CreateChannelInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_age`.
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_uses`.
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `temporary`.
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unique`.
    pub unique: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_type`.
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_user_id`.
    pub target_user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_application_id`.
    pub target_application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `role_ids`.
    pub role_ids: Option<Vec<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `InviteTargetUsersJobStatus`.
pub struct InviteTargetUsersJobStatus {
    /// Discord API payload field `status`.
    pub status: u8,
    /// Discord API payload field `total_users`.
    pub total_users: u64,
    /// Discord API payload field `processed_users`.
    pub processed_users: u64,
    /// Discord API payload field `created_at`.
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `completed_at`.
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `error_message`.
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `SetVoiceChannelStatus`.
pub struct SetVoiceChannelStatus {
    /// Discord API payload field `status`.
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Webhook`.
pub struct Webhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `token`.
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `url`.
    pub url: Option<String>,
}

/// Request body for creating an incoming channel webhook.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateWebhook {
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<Option<String>>,
}

/// Request body for modifying a bot-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
}

/// Request body for modifying a token-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhookWithToken {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<Option<String>>,
}

/// Query options for executing a webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookExecuteQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `wait`.
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `thread_id`.
    pub thread_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `with_components`.
    pub with_components: Option<bool>,
}

/// Query options for webhook message get/edit/delete routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookMessageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `thread_id`.
    pub thread_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `with_components`.
    pub with_components: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLogEntry`.
pub struct AuditLogEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_id`.
    pub target_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `action_type`.
    pub action_type: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `changes`.
    pub changes: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `options`.
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `reason`.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLog`.
pub struct AuditLog {
    #[serde(default)]
    /// Discord API payload field `application_commands`.
    pub application_commands: Vec<ApplicationCommand>,
    #[serde(default)]
    /// Discord API payload field `audit_log_entries`.
    pub audit_log_entries: Vec<AuditLogEntry>,
    #[serde(default)]
    /// Discord API payload field `auto_moderation_rules`.
    pub auto_moderation_rules: Vec<AutoModerationRule>,
    #[serde(default)]
    /// Discord API payload field `guild_scheduled_events`.
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
    #[serde(default)]
    /// Discord API payload field `integrations`.
    pub integrations: Vec<Integration>,
    #[serde(default)]
    /// Discord API payload field `threads`.
    pub threads: Vec<Channel>,
    #[serde(default)]
    /// Discord API payload field `users`.
    pub users: Vec<User>,
    #[serde(default)]
    /// Discord API payload field `webhooks`.
    pub webhooks: Vec<Webhook>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `AuditLogQuery`.
pub struct AuditLogQuery {
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `action_type`.
    pub action_type: Option<u64>,
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadMember`.
pub struct ThreadMember {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `join_timestamp`.
    pub join_timestamp: Option<String>,
    #[serde(default)]
    /// Discord API payload field `flags`.
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadListResponse`.
pub struct ThreadListResponse {
    #[serde(default)]
    /// Discord API payload field `threads`.
    pub threads: Vec<Channel>,
    #[serde(default)]
    /// Discord API payload field `members`.
    pub members: Vec<ThreadMember>,
    #[serde(default)]
    /// Discord API payload field `has_more`.
    pub has_more: bool,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ThreadMemberQuery`.
pub struct ThreadMemberQuery {
    /// Discord API payload field `with_member`.
    pub with_member: Option<bool>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ArchivedThreadsQuery`.
pub struct ArchivedThreadsQuery {
    /// Discord API payload field `before`.
    pub before: Option<String>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `JoinedArchivedThreadsQuery`.
pub struct JoinedArchivedThreadsQuery {
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOption`.
pub struct ApplicationCommandOption {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default)]
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `required`.
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `autocomplete`.
    pub autocomplete: Option<bool>,
    #[serde(default)]
    /// Discord API payload field `options`.
    pub options: Vec<ApplicationCommandOption>,
    #[serde(default)]
    /// Discord API payload field `choices`.
    pub choices: Vec<ApplicationCommandOptionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `min_value`.
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_value`.
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `min_length`.
    pub min_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `max_length`.
    pub max_length: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationIntegrationType`.
pub struct ApplicationIntegrationType(pub u8);

impl ApplicationIntegrationType {
    /// Public API item `GUILD_INSTALL`.
    pub const GUILD_INSTALL: Self = Self(0);
    /// Public API item `USER_INSTALL`.
    pub const USER_INSTALL: Self = Self(1);
}

impl Default for ApplicationIntegrationType {
    fn default() -> Self {
        Self::GUILD_INSTALL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `InteractionContextType`.
pub struct InteractionContextType(pub u8);

impl InteractionContextType {
    /// Public API item `GUILD`.
    pub const GUILD: Self = Self(0);
    /// Public API item `BOT_DM`.
    pub const BOT_DM: Self = Self(1);
    /// Public API item `PRIVATE_CHANNEL`.
    pub const PRIVATE_CHANNEL: Self = Self(2);
}

impl Default for InteractionContextType {
    fn default() -> Self {
        Self::GUILD
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationCommandHandlerType`.
pub struct ApplicationCommandHandlerType(pub u8);

impl ApplicationCommandHandlerType {
    /// Public API item `APP_HANDLER`.
    pub const APP_HANDLER: Self = Self(1);
    /// Public API item `DISCORD_LAUNCH_ACTIVITY`.
    pub const DISCORD_LAUNCH_ACTIVITY: Self = Self(2);
}

impl Default for ApplicationCommandHandlerType {
    fn default() -> Self {
        Self::APP_HANDLER
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommand`.
pub struct ApplicationCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name_localizations`.
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description_localizations`.
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    /// Discord API payload field `options`.
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_member_permissions`.
    pub default_member_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `dm_permission`.
    pub dm_permission: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `integration_types`.
    pub integration_types: Option<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `contexts`.
    pub contexts: Option<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `handler`.
    pub handler: Option<ApplicationCommandHandlerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `version`.
    pub version: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nsfw`.
    pub nsfw: Option<bool>,
}

/// A running application Activity instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ActivityInstance {
    /// Application ID that owns the Activity.
    pub application_id: Snowflake,
    /// Activity instance ID.
    pub instance_id: String,
    /// Unique launch ID for this Activity session.
    pub launch_id: Snowflake,
    /// Location where the Activity instance is running.
    pub location: ActivityLocation,
    /// Users currently connected to the Activity instance.
    #[serde(default)]
    pub users: Vec<Snowflake>,
}

/// Location where an application Activity instance is running.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ActivityLocation {
    /// Unique location ID.
    pub id: String,
    /// Location kind, such as `gc` for guild channel or `pc` for private channel.
    pub kind: String,
    /// Channel ID for this location.
    pub channel_id: Snowflake,
    /// Guild ID for guild-channel Activity locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
}

/// A single application command permission overwrite.
///
/// Discord accepts role, user, and channel overwrites for a guild command.
/// The `id` can also be one of Discord's documented permission constants,
/// such as the guild ID for `@everyone`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationCommandPermission {
    /// Role, user, or channel ID targeted by the overwrite.
    pub id: Snowflake,
    /// Discord permission target type.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Whether this target is allowed to use the command.
    pub permission: bool,
}

impl ApplicationCommandPermission {
    /// Permission overwrite target type for roles.
    pub const ROLE: u8 = 1;
    /// Permission overwrite target type for users.
    pub const USER: u8 = 2;
    /// Permission overwrite target type for channels.
    pub const CHANNEL: u8 = 3;

    /// Creates a role permission overwrite.
    pub fn role(role_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: role_id.into(),
            kind: Self::ROLE,
            permission,
        }
    }

    /// Creates a user permission overwrite.
    pub fn user(user_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: user_id.into(),
            kind: Self::USER,
            permission,
        }
    }

    /// Creates a channel permission overwrite.
    pub fn channel(channel_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: channel_id.into(),
            kind: Self::CHANNEL,
            permission,
        }
    }
}

/// Permissions returned for one application command in one guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildApplicationCommandPermissions {
    /// Command ID.
    pub id: Snowflake,
    /// Application ID that owns the command.
    pub application_id: Snowflake,
    /// Guild ID where the permissions apply.
    pub guild_id: Snowflake,
    /// Permission overwrites attached to the command.
    #[serde(default)]
    pub permissions: Vec<ApplicationCommandPermission>,
}

/// Request body for editing a guild application command's permissions.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EditApplicationCommandPermissions {
    /// Permission overwrites to replace on the command.
    pub permissions: Vec<ApplicationCommandPermission>,
}

impl EditApplicationCommandPermissions {
    /// Creates a permission edit body from an iterator of overwrites.
    pub fn new(permissions: impl IntoIterator<Item = ApplicationCommandPermission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Application`.
pub struct Application {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(default)]
    /// Discord API payload field `rpc_origins`.
    pub rpc_origins: Vec<String>,
    #[serde(default)]
    /// Discord API payload field `bot_public`.
    pub bot_public: bool,
    #[serde(default)]
    /// Discord API payload field `bot_require_code_grant`.
    pub bot_require_code_grant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `terms_of_service_url`.
    pub terms_of_service_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `privacy_policy_url`.
    pub privacy_policy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `owner`.
    pub owner: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `verify_key`.
    pub verify_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `team`.
    pub team: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `primary_sku_id`.
    pub primary_sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `slug`.
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `cover_image`.
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_guild_count`.
    pub approximate_guild_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_user_install_count`.
    pub approximate_user_install_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `approximate_user_authorization_count`.
    pub approximate_user_authorization_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `redirect_uris`.
    pub redirect_uris: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `interactions_endpoint_url`.
    pub interactions_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `role_connections_verification_url`.
    pub role_connections_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_url`.
    pub event_webhooks_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_status`.
    pub event_webhooks_status: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_types`.
    pub event_webhooks_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `tags`.
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `custom_install_url`.
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `install_params`.
    pub install_params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `integration_types_config`.
    pub integration_types_config: Option<HashMap<String, serde_json::Value>>,
}

/// Default OAuth2 install settings for an application install context.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationInstallParams {
    /// Discord API payload field `scopes`.
    pub scopes: Vec<String>,
    /// Discord API payload field `permissions`.
    pub permissions: PermissionsBitField,
}

/// Default install settings for one application integration type.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIntegrationTypeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `oauth2_install_params`.
    pub oauth2_install_params: Option<ApplicationInstallParams>,
}

/// Request body for editing the current application.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `custom_install_url`.
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `role_connections_verification_url`.
    pub role_connections_verification_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `install_params`.
    pub install_params: Option<ApplicationInstallParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `integration_types_config`.
    pub integration_types_config: Option<HashMap<String, ApplicationIntegrationTypeConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `cover_image`.
    pub cover_image: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `interactions_endpoint_url`.
    pub interactions_endpoint_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `tags`.
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_url`.
    pub event_webhooks_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_status`.
    pub event_webhooks_status: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `event_webhooks_types`.
    pub event_webhooks_types: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationRoleConnectionMetadata`.
pub struct ApplicationRoleConnectionMetadata {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `key`.
    pub key: String,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name_localizations`.
    pub name_localizations: Option<HashMap<String, String>>,
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description_localizations`.
    pub description_localizations: Option<HashMap<String, String>>,
}

impl ApplicationCommand {
    /// Returns the command ID when Discord has assigned one.
    pub fn id_opt(&self) -> Option<&Snowflake> {
        self.id.as_ref()
    }

    /// Returns the creation timestamp once Discord has assigned an ID.
    pub fn created_at(&self) -> Option<u64> {
        self.id_opt().and_then(Snowflake::timestamp)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `InteractionContextData`.
pub struct InteractionContextData {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `application_id`.
    pub application_id: Snowflake,
    /// Discord API payload field `token`.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `app_permissions`.
    pub app_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `locale`.
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_locale`.
    pub guild_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `entitlements`.
    pub entitlements: Option<Vec<Entitlement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `context`.
    pub context: Option<InteractionContextType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `authorizing_integration_owners`.
    pub authorizing_integration_owners: Option<HashMap<String, Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionOption`.
pub struct CommandInteractionOption {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default)]
    /// Discord API payload field `options`.
    pub options: Vec<CommandInteractionOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `value`.
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `focused`.
    pub focused: Option<bool>,
}

impl CommandInteractionOption {
    /// Runs the `is_focused` operation.
    pub fn is_focused(&self) -> bool {
        self.focused.unwrap_or(false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionData`.
pub struct CommandInteractionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
    #[serde(default)]
    /// Discord API payload field `options`.
    pub options: Vec<CommandInteractionOption>,
    #[serde(default)]
    /// Discord API payload field `resolved`.
    pub resolved: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `target_id`.
    pub target_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ComponentInteractionData`.
pub struct ComponentInteractionData {
    /// Discord API payload field `custom_id`.
    pub custom_id: String,
    /// Discord API payload field `component_type`.
    pub component_type: u8,
    #[serde(default)]
    /// Discord API payload field `values`.
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChatInputCommandInteraction`.
pub struct ChatInputCommandInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `data`.
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `UserContextMenuInteraction`.
pub struct UserContextMenuInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `data`.
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageContextMenuInteraction`.
pub struct MessageContextMenuInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `data`.
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutocompleteInteraction`.
pub struct AutocompleteInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `data`.
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ComponentInteraction`.
pub struct ComponentInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `data`.
    pub data: ComponentInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ModalSubmitInteraction`.
pub struct ModalSubmitInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
    /// Discord API payload field `submission`.
    pub submission: V2ModalSubmission,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PingInteraction`.
pub struct PingInteraction {
    /// Discord API payload field `context`.
    pub context: InteractionContextData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// Typed Discord API enum for `Interaction`.
pub enum Interaction {
    /// Discord API enum variant `Ping`.
    Ping(PingInteraction),
    /// Discord API enum variant `ChatInputCommand`.
    ChatInputCommand(ChatInputCommandInteraction),
    /// Discord API enum variant `UserContextMenu`.
    UserContextMenu(UserContextMenuInteraction),
    /// Discord API enum variant `MessageContextMenu`.
    MessageContextMenu(MessageContextMenuInteraction),
    /// Discord API enum variant `Autocomplete`.
    Autocomplete(AutocompleteInteraction),
    /// Discord API enum variant `Component`.
    Component(ComponentInteraction),
    /// Discord API enum variant `ModalSubmit`.
    ModalSubmit(ModalSubmitInteraction),
    /// Discord API enum variant `Unknown`.
    Unknown {
        /// Parsed interaction context shared with known variants.
        context: InteractionContextData,
        /// Raw Discord interaction type value.
        kind: u8,
        /// Raw interaction data object for unsupported variants.
        raw_data: serde_json::Value,
    },
}

impl Interaction {
    /// Runs the `context` operation.
    pub fn context(&self) -> &InteractionContextData {
        match self {
            Interaction::Ping(interaction) => &interaction.context,
            Interaction::ChatInputCommand(interaction) => &interaction.context,
            Interaction::UserContextMenu(interaction) => &interaction.context,
            Interaction::MessageContextMenu(interaction) => &interaction.context,
            Interaction::Autocomplete(interaction) => &interaction.context,
            Interaction::Component(interaction) => &interaction.context,
            Interaction::ModalSubmit(interaction) => &interaction.context,
            Interaction::Unknown { context, .. } => context,
        }
    }

    /// Runs the `id` operation.
    pub fn id(&self) -> &Snowflake {
        &self.context().id
    }

    /// Runs the `token` operation.
    pub fn token(&self) -> &str {
        &self.context().token
    }

    /// Runs the `application_id` operation.
    pub fn application_id(&self) -> &Snowflake {
        &self.context().application_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `InteractionCallbackResponse`.
pub struct InteractionCallbackResponse {
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `data`.
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ReactionCountDetails`.
pub struct ReactionCountDetails {
    #[serde(default)]
    /// Discord API payload field `burst`.
    pub burst: u64,
    #[serde(default)]
    /// Discord API payload field `normal`.
    pub normal: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateMessage`.
pub struct CreateMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `content`.
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `components`.
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `flags`.
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `embeds`.
    pub embeds: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `allowed_mentions`.
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `message_reference`.
    pub message_reference: Option<MessageReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `sticker_ids`.
    pub sticker_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nonce`.
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `poll`.
    pub poll: Option<CreatePoll>,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Discord API payload field `enforce_nonce`.
    pub enforce_nonce: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateDmChannel`.
pub struct CreateDmChannel {
    /// Discord API payload field `recipient_id`.
    pub recipient_id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateGroupDmChannel`.
pub struct CreateGroupDmChannel {
    /// Discord API payload field `access_tokens`.
    pub access_tokens: Vec<String>,
    /// Discord API payload field `nicks`.
    pub nicks: HashMap<Snowflake, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AddGroupDmRecipient`.
pub struct AddGroupDmRecipient {
    /// Discord API payload field `access_token`.
    pub access_token: String,
    /// Discord API payload field `nick`.
    pub nick: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SessionStartLimit`.
pub struct SessionStartLimit {
    /// Discord API payload field `total`.
    pub total: u32,
    /// Discord API payload field `remaining`.
    pub remaining: u32,
    /// Discord API payload field `reset_after`.
    pub reset_after: u64,
    /// Discord API payload field `max_concurrency`.
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Gateway`.
pub struct Gateway {
    /// Discord API payload field `url`.
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GatewayBot`.
pub struct GatewayBot {
    /// Discord API payload field `url`.
    pub url: String,
    /// Discord API payload field `shards`.
    pub shards: u32,
    /// Discord API payload field `session_start_limit`.
    pub session_start_limit: SessionStartLimit,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for current OAuth2 authorization information.
pub struct AuthorizationInformation {
    /// Partial Discord application object for the current authorization.
    pub application: serde_json::Value,
    /// OAuth2 scopes granted to the authorization.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Discord API payload field `expires`.
    pub expires: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// User that authorized the application when the token includes `identify`.
    pub user: Option<User>,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedField`.
pub struct EmbedField {
    /// Discord API payload field `name`.
    pub name: String,
    /// Discord API payload field `value`.
    pub value: String,
    #[serde(default, skip_serializing_if = "is_false")]
    /// Discord API payload field `inline`.
    pub inline: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedAuthor`.
pub struct EmbedAuthor {
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `url`.
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon_url`.
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedFooter`.
pub struct EmbedFooter {
    /// Discord API payload field `text`.
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon_url`.
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedMedia`.
pub struct EmbedMedia {
    /// Discord API payload field `url`.
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `proxy_url`.
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `height`.
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `width`.
    pub width: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Embed`.
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `title`.
    pub title: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `url`.
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `color`.
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `timestamp`.
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `footer`.
    pub footer: Option<EmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `image`.
    pub image: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `thumbnail`.
    pub thumbnail: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `video`.
    pub video: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `provider`.
    pub provider: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `author`.
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    /// Discord API payload field `fields`.
    pub fields: Vec<EmbedField>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Reaction`.
pub struct Reaction {
    #[serde(default)]
    /// Discord API payload field `count`.
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `count_details`.
    pub count_details: Option<ReactionCountDetails>,
    #[serde(default)]
    /// Discord API payload field `me`.
    pub me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerItem`.
pub struct StickerItem {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(rename = "format_type", skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `kind`.
    pub kind: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Sticker`.
pub struct Sticker {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `pack_id`.
    pub pack_id: Option<Snowflake>,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(default)]
    /// Discord API payload field `tags`.
    pub tags: String,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `format_type`.
    pub format_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `available`.
    pub available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `sort_value`.
    pub sort_value: Option<u64>,
}

/// Multipart form fields for creating a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildSticker {
    /// Discord API payload field `name`.
    pub name: String,
    /// Discord API payload field `description`.
    pub description: String,
    /// Discord API payload field `tags`.
    pub tags: String,
}

/// JSON request body for modifying a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildSticker {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `name`.
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `tags`.
    pub tags: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerPack`.
pub struct StickerPack {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(default)]
    /// Discord API payload field `stickers`.
    pub stickers: Vec<Sticker>,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `sku_id`.
    pub sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `cover_sticker_id`.
    pub cover_sticker_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `banner_asset_id`.
    pub banner_asset_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerPackList`.
pub struct StickerPackList {
    #[serde(default)]
    /// Discord API payload field `sticker_packs`.
    pub sticker_packs: Vec<StickerPack>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StageInstance`.
pub struct StageInstance {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `topic`.
    pub topic: String,
    /// Discord API payload field `privacy_level`.
    pub privacy_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `discoverable_disabled`.
    pub discoverable_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_scheduled_event_id`.
    pub guild_scheduled_event_id: Option<Snowflake>,
}

/// Request body for creating a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateStageInstance {
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `topic`.
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `privacy_level`.
    pub privacy_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `send_start_notification`.
    pub send_start_notification: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_scheduled_event_id`.
    pub guild_scheduled_event_id: Option<Snowflake>,
}

/// Request body for modifying a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyStageInstance {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `privacy_level`.
    pub privacy_level: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEvent`.
pub struct GuildScheduledEvent {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `creator_id`.
    pub creator_id: Option<Snowflake>,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    /// Discord API payload field `scheduled_start_time`.
    pub scheduled_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `scheduled_end_time`.
    pub scheduled_end_time: Option<String>,
    /// Discord API payload field `privacy_level`.
    pub privacy_level: u8,
    /// Discord API payload field `status`.
    pub status: u8,
    /// Discord API payload field `entity_type`.
    pub entity_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `entity_id`.
    pub entity_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `entity_metadata`.
    pub entity_metadata: Option<GuildScheduledEventEntityMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `creator`.
    pub creator: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_count`.
    pub user_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `image`.
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `recurrence_rule`.
    pub recurrence_rule: Option<GuildScheduledEventRecurrenceRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventEntityMetadata`.
pub struct GuildScheduledEventEntityMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `location`.
    pub location: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRuleNWeekday`.
pub struct GuildScheduledEventRecurrenceRuleNWeekday {
    /// Discord API payload field `n`.
    pub n: i8,
    /// Discord API payload field `day`.
    pub day: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRule`.
pub struct GuildScheduledEventRecurrenceRule {
    /// Discord API payload field `start`.
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `end`.
    pub end: Option<String>,
    /// Discord API payload field `frequency`.
    pub frequency: u8,
    /// Discord API payload field `interval`.
    pub interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `by_weekday`.
    pub by_weekday: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `by_n_weekday`.
    pub by_n_weekday: Option<Vec<GuildScheduledEventRecurrenceRuleNWeekday>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `by_month`.
    pub by_month: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `by_month_day`.
    pub by_month_day: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `by_year_day`.
    pub by_year_day: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `count`.
    pub count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventUser`.
pub struct GuildScheduledEventUser {
    /// Discord API payload field `guild_scheduled_event_id`.
    pub guild_scheduled_event_id: Snowflake,
    /// Discord API payload field `user`.
    pub user: User,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Sku`.
pub struct Sku {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `application_id`.
    pub application_id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    /// Discord API payload field `slug`.
    pub slug: String,
    /// Discord API payload field `flags`.
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `dependent_sku_id`.
    pub dependent_sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `manifest_labels`.
    pub manifest_labels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `access_type`.
    pub access_type: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `features`.
    pub features: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `release_date`.
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `premium`.
    pub premium: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `show_age_gate`.
    pub show_age_gate: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Entitlement`.
pub struct Entitlement {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `sku_id`.
    pub sku_id: Snowflake,
    /// Discord API payload field `application_id`.
    pub application_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `promotion_id`.
    pub promotion_id: Option<Snowflake>,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: u8,
    /// Discord API payload field `deleted`.
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `gift_code_flags`.
    pub gift_code_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `starts_at`.
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `ends_at`.
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `consumed`.
    pub consumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `subscription_id`.
    pub subscription_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateTestEntitlement`.
pub struct CreateTestEntitlement {
    /// Discord API payload field `sku_id`.
    pub sku_id: Snowflake,
    /// Discord API payload field `owner_id`.
    pub owner_id: Snowflake,
    /// Discord API payload field `owner_type`.
    pub owner_type: u8,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `EntitlementQuery`.
pub struct EntitlementQuery {
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `sku_ids`.
    pub sku_ids: Vec<Snowflake>,
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `exclude_ended`.
    pub exclude_ended: Option<bool>,
    /// Discord API payload field `exclude_deleted`.
    pub exclude_deleted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Subscription`.
pub struct Subscription {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `user_id`.
    pub user_id: Snowflake,
    #[serde(default)]
    /// Discord API payload field `sku_ids`.
    pub sku_ids: Vec<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `entitlement_ids`.
    pub entitlement_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `renewal_sku_ids`.
    pub renewal_sku_ids: Option<Vec<Snowflake>>,
    /// Discord API payload field `current_period_start`.
    pub current_period_start: String,
    /// Discord API payload field `current_period_end`.
    pub current_period_end: String,
    /// Discord API payload field `status`.
    pub status: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `canceled_at`.
    pub canceled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `country`.
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SubscriptionQuery`.
pub struct SubscriptionQuery {
    /// Discord API payload field `before`.
    pub before: Option<Snowflake>,
    /// Discord API payload field `after`.
    pub after: Option<Snowflake>,
    /// Discord API payload field `limit`.
    pub limit: Option<u64>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationAccount`.
pub struct IntegrationAccount {
    /// Discord API payload field `id`.
    pub id: String,
    /// Discord API payload field `name`.
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationApplication`.
pub struct IntegrationApplication {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `icon`.
    pub icon: Option<String>,
    #[serde(default)]
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `bot`.
    pub bot: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Integration`.
pub struct Integration {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: String,
    #[serde(default)]
    /// Discord API payload field `enabled`.
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `syncing`.
    pub syncing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `role_id`.
    pub role_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `enable_emoticons`.
    pub enable_emoticons: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `expire_behavior`.
    pub expire_behavior: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `expire_grace_period`.
    pub expire_grace_period: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
    /// Discord API payload field `account`.
    pub account: IntegrationAccount,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `synced_at`.
    pub synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `subscriber_count`.
    pub subscriber_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `revoked`.
    pub revoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application`.
    pub application: Option<IntegrationApplication>,
    #[serde(default)]
    /// Discord API payload field `scopes`.
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SoundboardSound`.
pub struct SoundboardSound {
    /// Discord API payload field `name`.
    pub name: String,
    /// Discord API payload field `sound_id`.
    pub sound_id: Snowflake,
    /// Discord API payload field `volume`.
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_id`.
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_name`.
    pub emoji_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    /// Discord API payload field `available`.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SoundboardSoundList`.
pub struct SoundboardSoundList {
    #[serde(default)]
    /// Discord API payload field `items`.
    pub items: Vec<SoundboardSound>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetSettings`.
pub struct GuildWidgetSettings {
    /// Discord API payload field `enabled`.
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
}

/// Request body for modifying guild widget settings.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWidgetSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `enabled`.
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Option<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetChannel`.
pub struct GuildWidgetChannel {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `position`.
    pub position: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetMember`.
pub struct GuildWidgetMember {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `username`.
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `discriminator`.
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar`.
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `status`.
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `avatar_url`.
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidget`.
pub struct GuildWidget {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `instant_invite`.
    pub instant_invite: Option<String>,
    #[serde(default)]
    /// Discord API payload field `channels`.
    pub channels: Vec<GuildWidgetChannel>,
    #[serde(default)]
    /// Discord API payload field `members`.
    pub members: Vec<GuildWidgetMember>,
    #[serde(default)]
    /// Discord API payload field `presence_count`.
    pub presence_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GuildWidgetImageStyle`.
pub enum GuildWidgetImageStyle {
    /// Discord API enum variant `Shield`.
    Shield,
    /// Discord API enum variant `Banner1`.
    Banner1,
    /// Discord API enum variant `Banner2`.
    Banner2,
    /// Discord API enum variant `Banner3`.
    Banner3,
    /// Discord API enum variant `Banner4`.
    Banner4,
}

impl GuildWidgetImageStyle {
    /// Runs the `as_str` operation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Banner1 => "banner1",
            Self::Banner2 => "banner2",
            Self::Banner3 => "banner3",
            Self::Banner4 => "banner4",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `FollowedChannel`.
pub struct FollowedChannel {
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `webhook_id`.
    pub webhook_id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `WelcomeScreenChannel`.
pub struct WelcomeScreenChannel {
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `description`.
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_id`.
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji_name`.
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `WelcomeScreen`.
pub struct WelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    #[serde(default)]
    /// Discord API payload field `welcome_channels`.
    pub welcome_channels: Vec<WelcomeScreenChannel>,
}

/// Request body for modifying a guild welcome screen.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `enabled`.
    pub enabled: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `welcome_channels`.
    pub welcome_channels: Option<Option<Vec<WelcomeScreenChannel>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildTemplate`.
pub struct GuildTemplate {
    /// Discord API payload field `code`.
    pub code: String,
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `description`.
    pub description: Option<String>,
    /// Discord API payload field `usage_count`.
    pub usage_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `creator_id`.
    pub creator_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `creator`.
    pub creator: Option<User>,
    /// Discord API payload field `created_at`.
    pub created_at: String,
    /// Discord API payload field `updated_at`.
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `source_guild_id`.
    pub source_guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `serialized_source_guild`.
    pub serialized_source_guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `is_dirty`.
    pub is_dirty: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildOnboarding`.
pub struct GuildOnboarding {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(default)]
    /// Discord API payload field `prompts`.
    pub prompts: Vec<serde_json::Value>,
    #[serde(default)]
    /// Discord API payload field `default_channel_ids`.
    pub default_channel_ids: Vec<Snowflake>,
    /// Discord API payload field `enabled`.
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mode`.
    pub mode: Option<u8>,
}

/// Request body for modifying guild onboarding.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ModifyGuildOnboarding {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `prompts`.
    pub prompts: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `default_channel_ids`.
    pub default_channel_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `enabled`.
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mode`.
    pub mode: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Presence`.
pub struct Presence {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `status`.
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `activities`.
    pub activities: Option<Vec<Activity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `client_status`.
    pub client_status: Option<ClientStatus>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ClientStatus`.
pub struct ClientStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `desktop`.
    pub desktop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `mobile`.
    pub mobile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `web`.
    pub web: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ActivityType`.
pub struct ActivityType(pub u8);

impl ActivityType {
    /// Public API item `PLAYING`.
    pub const PLAYING: Self = Self(0);
    /// Public API item `STREAMING`.
    pub const STREAMING: Self = Self(1);
    /// Public API item `LISTENING`.
    pub const LISTENING: Self = Self(2);
    /// Public API item `WATCHING`.
    pub const WATCHING: Self = Self(3);
    /// Public API item `CUSTOM`.
    pub const CUSTOM: Self = Self(4);
    /// Public API item `COMPETING`.
    pub const COMPETING: Self = Self(5);
}

impl Default for ActivityType {
    fn default() -> Self {
        Self::PLAYING
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityTimestamps`.
pub struct ActivityTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `start`.
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `end`.
    pub end: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityParty`.
pub struct ActivityParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `id`.
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `size`.
    pub size: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityAssets`.
pub struct ActivityAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `large_image`.
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `large_text`.
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `small_image`.
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `small_text`.
    pub small_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivitySecrets`.
pub struct ActivitySecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `join`.
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `spectate`.
    pub spectate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `match_secret`.
    pub match_secret: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityButton`.
pub struct ActivityButton {
    /// Discord API payload field `label`.
    pub label: String,
    /// Discord API payload field `url`.
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Activity`.
pub struct Activity {
    /// Discord API payload field `name`.
    pub name: String,
    #[serde(default, rename = "type")]
    /// Discord API payload field `kind`.
    pub kind: ActivityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `url`.
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `created_at`.
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `timestamps`.
    pub timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `details`.
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `state`.
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `party`.
    pub party: Option<ActivityParty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `assets`.
    pub assets: Option<ActivityAssets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `secrets`.
    pub secrets: Option<ActivitySecrets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `buttons`.
    pub buttons: Option<Vec<ActivityButton>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UpdatePresence`.
pub struct UpdatePresence {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `since`.
    pub since: Option<u64>,
    #[serde(default)]
    /// Discord API payload field `activities`.
    pub activities: Vec<Activity>,
    /// Discord API payload field `status`.
    pub status: String,
    #[serde(default)]
    /// Discord API payload field `afk`.
    pub afk: bool,
}

impl UpdatePresence {
    /// Creates or returns `online_with_activity` data.
    pub fn online_with_activity(name: impl Into<String>) -> Self {
        Self {
            since: None,
            activities: vec![Activity {
                name: name.into(),
                kind: ActivityType::PLAYING,
                ..Activity::default()
            }],
            status: "online".to_string(),
            afk: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RequestGuildMembers`.
pub struct RequestGuildMembers {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `query`.
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `limit`.
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `presences`.
    pub presences: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user_ids`.
    pub user_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nonce`.
    pub nonce: Option<String>,
}

/// Gateway request for ephemeral channel metadata.
///
/// Discord currently documents `status` and `voice_start_time` as requestable
/// fields for voice-channel metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestChannelInfo {
    /// Guild whose channel metadata should be requested.
    pub guild_id: Snowflake,
    /// Metadata fields to request.
    pub fields: Vec<String>,
}

impl RequestChannelInfo {
    /// Channel status field name.
    pub const STATUS: &'static str = "status";
    /// Voice channel start-time field name.
    pub const VOICE_START_TIME: &'static str = "voice_start_time";

    /// Creates a channel-info request for explicit field names.
    pub fn new<I, S>(guild_id: impl Into<Snowflake>, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            guild_id: guild_id.into(),
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }

    /// Creates a request for Discord's documented voice metadata fields.
    pub fn voice_metadata(guild_id: impl Into<Snowflake>) -> Self {
        Self {
            guild_id: guild_id.into(),
            fields: vec![Self::STATUS.to_string(), Self::VOICE_START_TIME.to_string()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadMetadata`.
pub struct ThreadMetadata {
    #[serde(default)]
    /// Discord API payload field `archived`.
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `auto_archive_duration`.
    pub auto_archive_duration: Option<u64>,
    #[serde(default)]
    /// Discord API payload field `locked`.
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `invitable`.
    pub invitable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `create_timestamp`.
    pub create_timestamp: Option<String>,
}

// --- DiscordModel trait ---

/// Trait for all Discord data models that have a Snowflake ID.
///
/// Parallels discord.js's `Base` class, providing a common interface
/// for ID access and creation timestamp extraction.
pub trait DiscordModel: Send + Sync + 'static {
    /// Returns the Snowflake ID of this model.
    fn id(&self) -> &Snowflake;

    /// Returns the Snowflake ID when the model has one.
    ///
    /// Most Discord models always carry an ID, so the default implementation
    /// simply delegates to [`DiscordModel::id`]. Models that can exist before
    /// Discord assigns an ID, such as `ApplicationCommand`, override this.
    fn id_opt(&self) -> Option<&Snowflake> {
        Some(self.id())
    }

    /// Returns the creation timestamp as Unix milliseconds, extracted from the Snowflake ID.
    fn created_at(&self) -> Option<u64> {
        self.id_opt().and_then(Snowflake::timestamp)
    }
}

impl DiscordModel for User {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Guild {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Channel {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Message {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Role {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Attachment {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        AllowedMentions, ApplicationCommand, ApplicationCommandOptionChoice,
        ApplicationInstallParams, ApplicationIntegrationTypeConfig, Attachment,
        AutocompleteInteraction, BeginGuildPruneRequest, Channel, ChannelType,
        ChatInputCommandInteraction, CommandInteractionData, CommandInteractionOption,
        ComponentInteraction, ComponentInteractionData, CreateChannelInvite, CreateDmChannel,
        CreateGuildBan, CreateGuildChannel, CreateGuildRole, CreateGuildSticker, CreateMessage,
        CreatePoll, CreateStageInstance, CreateWebhook, DefaultReaction, DiscordModel, Embed,
        EmbedField, Entitlement, ForumTag, GatewayBot, Guild, GuildScheduledEvent, Integration,
        Interaction, InteractionCallbackResponse, InteractionContextData, Invite,
        InviteTargetUsersJobStatus, Member, Message, MessageContextMenuInteraction,
        ModalSubmitInteraction, ModifyCurrentApplication, ModifyCurrentMember,
        ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildMember, ModifyGuildOnboarding,
        ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker, ModifyGuildWelcomeScreen,
        ModifyGuildWidgetSettings, ModifyStageInstance, ModifyUserVoiceState, ModifyWebhook,
        ModifyWebhookWithToken, PermissionOverwrite, PermissionsBitField, PingInteraction,
        PollAnswer, PollAnswerCount, PollAnswerVoters, PollMedia, PollResults, Presence, Reaction,
        Role, RoleColors, SessionStartLimit, Sku, Snowflake, StickerItem, Subscription,
        ThreadListResponse, ThreadMember, ThreadMetadata, User, UserContextMenuInteraction,
        WebhookExecuteQuery, WebhookMessageQuery, WelcomeScreenChannel,
    };
    use crate::parsers::V2ModalSubmission;

    #[test]
    fn snowflake_deserializes_from_string_and_number() {
        let string_value: Snowflake = serde_json::from_value(json!("123")).unwrap();
        let number_value: Snowflake = serde_json::from_value(json!(123)).unwrap();

        assert_eq!(string_value.as_str(), "123");
        assert_eq!(number_value.as_str(), "123");
    }

    #[test]
    fn permissions_round_trip_through_string_wire_format() {
        let permissions = PermissionsBitField(8);
        let json = serde_json::to_value(permissions).unwrap();
        assert_eq!(json, json!("8"));

        let parsed: PermissionsBitField = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.bits(), 8);
    }

    #[test]
    fn typed_models_keep_wire_shape() {
        let user: User = serde_json::from_value(json!({
            "id": "42",
            "username": "discordrs",
            "global_name": "discordrs"
        }))
        .unwrap();

        let serialized = serde_json::to_value(&user).unwrap();
        assert_eq!(serialized["id"], json!("42"));
        assert_eq!(serialized["username"], json!("discordrs"));
    }

    #[test]
    fn application_command_option_choice_new_serializes_value() {
        let choice = ApplicationCommandOptionChoice::new("Support", "support");
        let serialized = serde_json::to_value(choice).unwrap();

        assert_eq!(serialized["name"], json!("Support"));
        assert_eq!(serialized["value"], json!("support"));
    }

    #[test]
    fn snowflake_timestamp_extracts_creation_time() {
        // Discord Snowflake: timestamp is in the top 42 bits
        let sf = Snowflake::from(1759288472266248192u64);
        let ts = sf.timestamp().expect("should extract timestamp");
        // Should be a reasonable Unix timestamp (after 2020)
        assert!(ts > 1_577_836_800_000u64); // after 2020-01-01
    }

    #[test]
    fn application_command_id_opt_is_none_until_discord_assigns_an_id() {
        let command = ApplicationCommand {
            name: "ping".to_string(),
            description: "Ping".to_string(),
            ..ApplicationCommand::default()
        };

        assert!(command.id_opt().is_none());
        assert_eq!(command.created_at(), None);
    }

    #[test]
    fn application_command_created_at_uses_assigned_id() {
        let command = ApplicationCommand {
            id: Some(Snowflake::from(1759288472266248192u64)),
            name: "ping".to_string(),
            description: "Ping".to_string(),
            ..ApplicationCommand::default()
        };

        assert_eq!(
            command.id_opt().map(Snowflake::as_str),
            Some("1759288472266248192")
        );
        assert!(command.created_at().is_some());
    }

    #[test]
    fn snowflake_helpers_cover_string_numeric_and_error_paths() {
        let snowflake = Snowflake::new("1759288472266248192");
        let parsed = "42".parse::<Snowflake>().unwrap();
        let invalid = Snowflake::new("not-a-number");

        assert_eq!(snowflake.as_str(), "1759288472266248192");
        assert_eq!(snowflake.as_u64(), Some(1_759_288_472_266_248_192));
        assert_eq!(snowflake.to_string(), "1759288472266248192");
        assert_eq!(parsed.as_str(), "42");
        assert_eq!(invalid.as_u64(), None);
        assert!(snowflake.is_valid());
        assert!(!invalid.is_valid());
        assert!(Snowflake::try_new("42").is_ok());
        assert!(Snowflake::try_new("not-a-number").is_err());

        let error = serde_json::from_value::<Snowflake>(json!(-1)).unwrap_err();
        assert!(error.to_string().contains("snowflake cannot be negative"));
    }

    #[test]
    fn permissions_bitfield_helpers_cover_mutation_and_invalid_wire_values() {
        let mut permissions = PermissionsBitField(0b0011);
        assert!(permissions.contains(0b0001));
        assert!(!permissions.contains(0b0100));

        permissions.insert(0b0100);
        assert_eq!(permissions.bits(), 0b0111);
        assert!(permissions.contains(0b0110));

        permissions.remove(0b0010);
        assert_eq!(permissions.bits(), 0b0101);
        assert!(!permissions.contains(0b0010));

        let error = serde_json::from_value::<PermissionsBitField>(json!("oops")).unwrap_err();
        assert!(error.to_string().contains("invalid permission bitfield"));
    }

    #[test]
    fn channel_and_create_message_keep_wire_aliases_and_omit_absent_optionals() {
        let channel = Channel {
            id: Snowflake::from("10"),
            kind: 5,
            name: Some("announcements".to_string()),
            ..Channel::default()
        };
        let message = CreateMessage {
            content: Some("hello".to_string()),
            ..CreateMessage::default()
        };

        let channel_json = serde_json::to_value(&channel).unwrap();
        let message_json = serde_json::to_value(&message).unwrap();

        assert_eq!(channel_json["id"], json!("10"));
        assert_eq!(channel_json["type"], json!(5));
        assert_eq!(channel_json["name"], json!("announcements"));
        assert!(channel_json.get("guild_id").is_none());
        assert!(channel_json.get("topic").is_none());

        assert_eq!(message_json, json!({ "content": "hello" }));
    }

    #[test]
    fn forum_channel_fields_decode_tags_and_default_reaction() {
        let channel: Channel = serde_json::from_value(json!({
            "id": "10",
            "type": 15,
            "available_tags": [{
                "id": "11",
                "name": "Support",
                "moderated": true,
                "emoji_name": "ticket"
            }],
            "applied_tags": ["11"],
            "default_reaction_emoji": { "emoji_name": "ok" },
            "default_thread_rate_limit_per_user": 30,
            "default_sort_order": 1,
            "default_forum_layout": 2
        }))
        .unwrap();

        let tag = &channel.available_tags.as_ref().unwrap()[0];
        assert_eq!(tag.id.as_str(), "11");
        assert_eq!(tag.name, "Support");
        assert!(tag.moderated);
        assert_eq!(tag.emoji_name.as_deref(), Some("ticket"));
        assert_eq!(channel.applied_tags.as_ref().unwrap()[0].as_str(), "11");
        assert_eq!(
            channel
                .default_reaction_emoji
                .as_ref()
                .and_then(|reaction| reaction.emoji_name.as_deref()),
            Some("ok")
        );
        assert_eq!(channel.default_thread_rate_limit_per_user, Some(30));

        let serialized = serde_json::to_value(Channel {
            id: Snowflake::from("20"),
            kind: 15,
            available_tags: Some(vec![ForumTag {
                id: Snowflake::from("21"),
                name: "News".to_string(),
                ..ForumTag::default()
            }]),
            default_reaction_emoji: Some(DefaultReaction {
                emoji_id: Some(Snowflake::from("22")),
                ..DefaultReaction::default()
            }),
            ..Channel::default()
        })
        .unwrap();
        assert_eq!(serialized["available_tags"][0]["name"], json!("News"));
        assert_eq!(
            serialized["default_reaction_emoji"]["emoji_id"],
            json!("22")
        );
    }

    #[test]
    fn message_poll_decodes_and_create_poll_keeps_wire_shape() {
        let message: Message = serde_json::from_value(json!({
            "id": "500",
            "channel_id": "600",
            "poll": {
                "question": {
                    "text": "Ship it?"
                },
                "answers": [
                    {
                        "answer_id": 1,
                        "poll_media": {
                            "text": "Yes",
                            "emoji": { "name": "yes" }
                        }
                    },
                    {
                        "answer_id": 2,
                        "poll_media": {
                            "text": "No"
                        }
                    }
                ],
                "expiry": "2026-04-30T00:00:00Z",
                "allow_multiselect": true,
                "layout_type": 1,
                "results": {
                    "is_finalized": false,
                    "answer_counts": [
                        { "id": 1, "count": 3, "me_voted": true },
                        { "id": 2, "count": 1, "me_voted": false }
                    ]
                }
            },
            "reactions": [{
                "count": 5,
                "count_details": { "burst": 2, "normal": 3 },
                "me": true,
                "emoji": { "name": "spark" }
            }]
        }))
        .unwrap();

        let poll = message.poll.expect("poll should decode");
        assert_eq!(poll.question.text.as_deref(), Some("Ship it?"));
        assert_eq!(poll.answers[0].answer_id, Some(1));
        assert_eq!(poll.answers[0].poll_media.text.as_deref(), Some("Yes"));
        assert_eq!(
            poll.answers[0]
                .poll_media
                .emoji
                .as_ref()
                .and_then(|emoji| emoji.name.as_deref()),
            Some("yes")
        );
        assert!(poll.allow_multiselect);
        assert_eq!(poll.layout_type, 1);
        let results = poll.results.expect("poll results should decode");
        assert!(!results.is_finalized);
        assert_eq!(results.answer_counts[0].count, 3);
        assert!(results.answer_counts[0].me_voted);
        let reaction = &message.reactions[0];
        assert_eq!(reaction.count_details.as_ref().unwrap().burst, 2);
        assert_eq!(reaction.count_details.as_ref().unwrap().normal, 3);

        let create_message = CreateMessage {
            allowed_mentions: Some(AllowedMentions {
                users: vec![Snowflake::from("42")],
                replied_user: Some(false),
                ..AllowedMentions::default()
            }),
            poll: Some(CreatePoll {
                question: PollMedia {
                    text: Some("Pick one".to_string()),
                    ..PollMedia::default()
                },
                answers: vec![PollAnswer {
                    poll_media: PollMedia {
                        text: Some("A".to_string()),
                        ..PollMedia::default()
                    },
                    ..PollAnswer::default()
                }],
                duration: Some(24),
                allow_multiselect: Some(false),
                layout_type: Some(1),
            }),
            ..CreateMessage::default()
        };

        assert_eq!(
            serde_json::to_value(&create_message).unwrap(),
            json!({
                "allowed_mentions": {
                    "users": ["42"],
                    "replied_user": false
                },
                "poll": {
                    "question": { "text": "Pick one" },
                    "answers": [
                        { "poll_media": { "text": "A" } }
                    ],
                    "duration": 24,
                    "allow_multiselect": false,
                    "layout_type": 1
                }
            })
        );

        let _default_results = PollResults {
            answer_counts: vec![PollAnswerCount {
                id: 1,
                count: 0,
                me_voted: false,
            }],
            ..PollResults::default()
        };
    }

    #[test]
    fn guild_member_decodes_current_profile_fields() {
        let member: Member = serde_json::from_value(json!({
            "user": { "id": "42", "username": "profiled" },
            "roles": ["100"],
            "avatar": "guild_avatar",
            "banner": "guild_banner",
            "avatar_decoration_data": {
                "asset": "decoration_asset",
                "sku_id": "555"
            },
            "collectibles": {
                "nameplate": {
                    "sku_id": "777",
                    "asset": "nameplate_asset",
                    "label": "Champion",
                    "palette": "violet"
                }
            },
            "flags": 1
        }))
        .unwrap();

        assert_eq!(member.banner.as_deref(), Some("guild_banner"));
        assert_eq!(
            member
                .avatar_decoration_data
                .as_ref()
                .map(|decoration| decoration.sku_id.as_str()),
            Some("555")
        );
        assert_eq!(
            member
                .collectibles
                .as_ref()
                .and_then(|collectibles| collectibles.nameplate.as_ref())
                .map(|nameplate| nameplate.label.as_str()),
            Some("Champion")
        );

        let serialized = serde_json::to_value(&member).unwrap();
        assert_eq!(serialized["banner"], json!("guild_banner"));
        assert_eq!(
            serialized["avatar_decoration_data"]["asset"],
            json!("decoration_asset")
        );
        assert_eq!(
            serialized["collectibles"]["nameplate"]["palette"],
            json!("violet")
        );
    }

    #[test]
    fn guild_role_position_body_keeps_nullable_wire_shape() {
        let positions = vec![
            ModifyGuildRolePosition {
                id: Snowflake::from("300"),
                position: Some(Some(1)),
            },
            ModifyGuildRolePosition {
                id: Snowflake::from("301"),
                position: Some(None),
            },
        ];

        assert_eq!(
            serde_json::to_value(&positions).unwrap(),
            json!([
                { "id": "300", "position": 1 },
                { "id": "301", "position": null }
            ])
        );
    }

    #[test]
    fn guild_request_bodies_keep_nullable_wire_shape() {
        let member = ModifyGuildMember {
            nick: Some(None),
            roles: Some(Some(vec![Snowflake::from("300")])),
            mute: Some(Some(false)),
            deaf: None,
            channel_id: Some(None),
            communication_disabled_until: Some(Some("2026-05-01T00:00:00Z".to_string())),
            flags: Some(Some(1)),
        };
        assert_eq!(
            serde_json::to_value(&member).unwrap(),
            json!({
                "nick": null,
                "roles": ["300"],
                "mute": false,
                "channel_id": null,
                "communication_disabled_until": "2026-05-01T00:00:00Z",
                "flags": 1
            })
        );

        let current_member = ModifyCurrentMember {
            nick: Some(Some("bot".to_string())),
            banner: Some(None),
            avatar: None,
            bio: Some(Some("shipping".to_string())),
        };
        assert_eq!(
            serde_json::to_value(&current_member).unwrap(),
            json!({
                "nick": "bot",
                "banner": null,
                "bio": "shipping"
            })
        );

        let ban = CreateGuildBan {
            delete_message_days: None,
            delete_message_seconds: Some(60),
        };
        assert_eq!(
            serde_json::to_value(&ban).unwrap(),
            json!({ "delete_message_seconds": 60 })
        );

        let create_role = CreateGuildRole {
            name: Some("gradient".to_string()),
            colors: Some(RoleColors {
                primary_color: 11127295,
                secondary_color: Some(16759788),
                tertiary_color: Some(16761760),
            }),
            icon: Some(None),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&create_role).unwrap(),
            json!({
                "name": "gradient",
                "colors": {
                    "primary_color": 11127295,
                    "secondary_color": 16759788,
                    "tertiary_color": 16761760
                },
                "icon": null
            })
        );

        let modify_role = ModifyGuildRole {
            name: Some(None),
            permissions: Some(Some(PermissionsBitField(8))),
            colors: Some(None),
            unicode_emoji: Some(None),
            mentionable: Some(Some(true)),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&modify_role).unwrap(),
            json!({
                "name": null,
                "permissions": "8",
                "colors": null,
                "unicode_emoji": null,
                "mentionable": true
            })
        );
    }

    #[test]
    fn guild_admin_request_bodies_keep_nullable_wire_shape() {
        let guild = ModifyGuild {
            name: Some("renamed".to_string()),
            afk_channel_id: Some(None),
            features: Some(vec!["COMMUNITY".to_string()]),
            description: Some(None),
            premium_progress_bar_enabled: Some(true),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&guild).unwrap(),
            json!({
                "name": "renamed",
                "afk_channel_id": null,
                "features": ["COMMUNITY"],
                "description": null,
                "premium_progress_bar_enabled": true
            })
        );

        let channel = CreateGuildChannel {
            name: "rules".to_string(),
            kind: Some(ChannelType::Text as u8),
            topic: Some(Some("read first".to_string())),
            permission_overwrites: Some(Some(vec![PermissionOverwrite {
                id: Snowflake::from("300"),
                kind: 0,
                allow: Some(PermissionsBitField(1024)),
                deny: None,
            }])),
            parent_id: Some(None),
            default_reaction_emoji: Some(None),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&channel).unwrap(),
            json!({
                "name": "rules",
                "type": 0,
                "topic": "read first",
                "permission_overwrites": [{
                    "id": "300",
                    "type": 0,
                    "allow": "1024"
                }],
                "parent_id": null,
                "default_reaction_emoji": null
            })
        );

        let widget = ModifyGuildWidgetSettings {
            enabled: Some(true),
            channel_id: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&widget).unwrap(),
            json!({ "enabled": true, "channel_id": null })
        );

        let welcome_screen = ModifyGuildWelcomeScreen {
            enabled: Some(Some(true)),
            welcome_channels: Some(Some(vec![WelcomeScreenChannel {
                channel_id: Snowflake::from("202"),
                description: "Start here".to_string(),
                emoji_id: None,
                emoji_name: Some("wave".to_string()),
            }])),
            description: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&welcome_screen).unwrap(),
            json!({
                "enabled": true,
                "welcome_channels": [{
                    "channel_id": "202",
                    "description": "Start here",
                    "emoji_name": "wave"
                }],
                "description": null
            })
        );

        let onboarding = ModifyGuildOnboarding {
            prompts: Some(vec![json!({ "id": "1", "title": "Pick a topic" })]),
            default_channel_ids: Some(vec![Snowflake::from("202")]),
            enabled: Some(true),
            mode: Some(0),
        };
        assert_eq!(
            serde_json::to_value(&onboarding).unwrap(),
            json!({
                "prompts": [{ "id": "1", "title": "Pick a topic" }],
                "default_channel_ids": ["202"],
                "enabled": true,
                "mode": 0
            })
        );

        let prune = BeginGuildPruneRequest {
            days: Some(7),
            compute_prune_count: Some(false),
            include_roles: Some(vec![Snowflake::from("300")]),
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(&prune).unwrap(),
            json!({
                "days": 7,
                "compute_prune_count": false,
                "include_roles": ["300"]
            })
        );
    }

    #[test]
    fn monetization_poll_and_thread_response_models_decode() {
        let sku: Sku = serde_json::from_value(json!({
            "id": "1088510058284990888",
            "type": 5,
            "dependent_sku_id": null,
            "application_id": "788708323867885999",
            "manifest_labels": null,
            "access_type": 1,
            "name": "Test Premium",
            "features": [],
            "release_date": null,
            "premium": false,
            "slug": "test-premium",
            "flags": 128,
            "show_age_gate": false
        }))
        .unwrap();
        assert_eq!(sku.kind, 5);
        assert_eq!(sku.access_type, Some(1));
        assert_eq!(sku.premium, Some(false));
        assert_eq!(sku.show_age_gate, Some(false));
        assert!(sku.features.is_empty());

        let entitlement: Entitlement = serde_json::from_value(json!({
            "id": "1019653849998299136",
            "sku_id": "1019475255913222144",
            "application_id": "1019370614521200640",
            "user_id": "771129655544643584",
            "promotion_id": null,
            "type": 8,
            "deleted": false,
            "gift_code_flags": 0,
            "consumed": false,
            "starts_at": "2022-09-14T17:00:18.704163+00:00",
            "ends_at": "2022-10-14T17:00:18.704163+00:00",
            "guild_id": "1015034326372454400",
            "subscription_id": "1019653835926409216"
        }))
        .unwrap();
        assert_eq!(entitlement.kind, 8);
        assert_eq!(entitlement.gift_code_flags, Some(0));
        assert_eq!(
            entitlement.subscription_id.as_ref().map(Snowflake::as_str),
            Some("1019653835926409216")
        );

        let subscription: Subscription = serde_json::from_value(json!({
            "id": "1278078770116427839",
            "user_id": "1088605110638227537",
            "sku_ids": ["1158857122189168803"],
            "entitlement_ids": ["1"],
            "renewal_sku_ids": null,
            "current_period_start": "2024-08-27T19:48:44.406602+00:00",
            "current_period_end": "2024-09-27T19:48:44.406602+00:00",
            "status": 0,
            "canceled_at": null
        }))
        .unwrap();
        assert_eq!(subscription.sku_ids[0].as_str(), "1158857122189168803");
        assert_eq!(subscription.status, 0);

        let voters: PollAnswerVoters = serde_json::from_value(json!({
            "users": [{ "id": "42", "username": "voter" }]
        }))
        .unwrap();
        assert_eq!(voters.users[0].username, "voter");

        let threads: ThreadListResponse = serde_json::from_value(json!({
            "threads": [{ "id": "50", "type": 11, "name": "thread" }],
            "members": [{ "id": "50", "user_id": "42", "join_timestamp": "2026-04-29T00:00:00Z", "flags": 0 }],
            "has_more": false
        }))
        .unwrap();
        assert_eq!(threads.threads[0].id.as_str(), "50");
        assert_eq!(threads.members[0].user_id.as_ref().unwrap().as_str(), "42");

        let thread_member = ThreadMember {
            user_id: Some(Snowflake::from("42")),
            member: Some(Member {
                user: Some(User {
                    id: Snowflake::from("42"),
                    username: "member".to_string(),
                    ..User::default()
                }),
                ..Member::default()
            }),
            ..ThreadMember::default()
        };
        assert_eq!(
            thread_member
                .member
                .as_ref()
                .and_then(|member| member.user.as_ref())
                .map(|user| user.username.as_str()),
            Some("member")
        );
    }

    #[test]
    fn message_resource_extended_fields_decode() {
        let message: Message = serde_json::from_value(json!({
            "id": "1000",
            "channel_id": "2000",
            "content": "forwarded",
            "mention_roles": ["3000"],
            "application_id": "4000",
            "application": { "id": "4000", "name": "Activity App" },
            "activity": { "type": 1, "party_id": "party" },
            "role_subscription_data": {
                "role_subscription_listing_id": "5000",
                "tier_name": "Gold",
                "total_months_subscribed": 7,
                "is_renewal": true
            },
            "message_reference": {
                "type": 1,
                "message_id": "6000",
                "channel_id": "7000"
            },
            "message_snapshots": [{
                "message": {
                    "type": 0,
                    "content": "snapshot",
                    "timestamp": "2026-05-01T00:00:00.000000+00:00",
                    "flags": 16384,
                    "mentions": [{ "id": "42", "username": "mentioned" }],
                    "mention_roles": ["8000"],
                    "sticker_items": [{
                        "id": "9000",
                        "name": "ship",
                        "format_type": 1
                    }],
                    "components": [{ "type": 10, "content": "snapshot text" }]
                }
            }],
            "referenced_message": {
                "id": "6000",
                "channel_id": "7000",
                "content": "original"
            },
            "interaction_metadata": { "id": "9100", "type": 2 },
            "interaction": { "id": "9100", "name": "old" },
            "resolved": { "users": {} },
            "call": {
                "participants": ["42"],
                "ended_timestamp": null
            },
            "shared_client_theme": {
                "colors": ["5865F2", "7258F2"],
                "gradient_angle": 45,
                "base_mix": 58,
                "base_theme": 1
            }
        }))
        .unwrap();

        assert_eq!(message.mention_roles[0].as_str(), "3000");
        assert_eq!(message.application_id.unwrap().as_str(), "4000");
        assert_eq!(message.activity.unwrap().party_id.as_deref(), Some("party"));
        assert!(message
            .role_subscription_data
            .as_ref()
            .is_some_and(|data| data.is_renewal));
        assert_eq!(message.message_reference.unwrap().kind, Some(1));
        assert_eq!(message.message_snapshots[0].message.content, "snapshot");
        assert_eq!(
            message.message_snapshots[0].message.sticker_items[0]
                .id
                .as_str(),
            "9000"
        );
        assert_eq!(
            message
                .referenced_message
                .as_ref()
                .map(|message| message.content.as_str()),
            Some("original")
        );
        assert_eq!(message.call.unwrap().participants[0].as_str(), "42");
        assert_eq!(
            message.shared_client_theme.unwrap().colors,
            vec!["5865F2".to_string(), "7258F2".to_string()]
        );
    }

    #[test]
    fn integration_model_decodes_core_guild_integration_shape() {
        let integration: Integration = serde_json::from_value(json!({
            "id": "100",
            "name": "Twitch",
            "type": "twitch",
            "enabled": true,
            "account": { "id": "abc", "name": "stream" },
            "application": {
                "id": "200",
                "name": "App",
                "description": "integration app"
            },
            "scopes": ["bot"]
        }))
        .unwrap();

        assert_eq!(integration.id.as_str(), "100");
        assert_eq!(integration.kind, "twitch");
        assert_eq!(integration.account.name, "stream");
        assert_eq!(integration.application.as_ref().unwrap().id.as_str(), "200");
    }

    #[test]
    fn embed_field_and_focus_helpers_follow_default_and_true_branches() {
        let default_field = EmbedField {
            name: "Name".to_string(),
            value: "Value".to_string(),
            ..EmbedField::default()
        };
        let inline_field = EmbedField {
            inline: true,
            ..default_field.clone()
        };
        let unfocused = CommandInteractionOption::default();
        let focused = CommandInteractionOption {
            focused: Some(true),
            ..CommandInteractionOption::default()
        };

        let default_json = serde_json::to_value(&default_field).unwrap();
        let inline_json = serde_json::to_value(&inline_field).unwrap();

        assert!(default_json.get("inline").is_none());
        assert_eq!(inline_json["inline"], json!(true));
        assert!(!unfocused.is_focused());
        assert!(focused.is_focused());
    }

    #[test]
    fn interaction_accessors_and_discord_model_trait_delegate_to_context_and_ids() {
        let context = InteractionContextData {
            id: Snowflake::from("99"),
            application_id: Snowflake::from("77"),
            token: "token-123".to_string(),
            ..InteractionContextData::default()
        };
        let interaction = Interaction::Component(ComponentInteraction {
            context: context.clone(),
            data: ComponentInteractionData {
                custom_id: "button".to_string(),
                component_type: 2,
                values: vec!["x".to_string()],
            },
        });
        let user = User {
            id: Snowflake::from(1759288472266248192u64),
            username: "discordrs".to_string(),
            ..User::default()
        };

        assert_eq!(interaction.context().id.as_str(), "99");
        assert_eq!(interaction.id().as_str(), "99");
        assert_eq!(interaction.application_id().as_str(), "77");
        assert_eq!(interaction.token(), "token-123");

        assert_eq!(DiscordModel::id(&user).as_str(), "1759288472266248192");
        assert_eq!(
            DiscordModel::id_opt(&user).map(Snowflake::as_str),
            Some("1759288472266248192")
        );
        assert!(DiscordModel::created_at(&user).is_some());
    }

    #[test]
    fn interaction_accessors_cover_all_variants() {
        fn context(id: &str, application_id: &str, token: &str) -> InteractionContextData {
            InteractionContextData {
                id: Snowflake::from(id),
                application_id: Snowflake::from(application_id),
                token: token.to_string(),
                ..InteractionContextData::default()
            }
        }

        let interactions = [
            Interaction::Ping(PingInteraction {
                context: context("1", "10", "ping-token"),
            }),
            Interaction::ChatInputCommand(ChatInputCommandInteraction {
                context: context("2", "20", "chat-token"),
                data: CommandInteractionData::default(),
            }),
            Interaction::UserContextMenu(UserContextMenuInteraction {
                context: context("3", "30", "user-token"),
                data: CommandInteractionData::default(),
            }),
            Interaction::MessageContextMenu(MessageContextMenuInteraction {
                context: context("4", "40", "message-token"),
                data: CommandInteractionData::default(),
            }),
            Interaction::Autocomplete(AutocompleteInteraction {
                context: context("5", "50", "autocomplete-token"),
                data: CommandInteractionData::default(),
            }),
            Interaction::Component(ComponentInteraction {
                context: context("6", "60", "component-token"),
                data: ComponentInteractionData::default(),
            }),
            Interaction::ModalSubmit(ModalSubmitInteraction {
                context: context("7", "70", "modal-token"),
                submission: V2ModalSubmission {
                    custom_id: "modal".to_string(),
                    components: vec![],
                },
            }),
            Interaction::Unknown {
                context: context("8", "80", "unknown-token"),
                kind: 99,
                raw_data: json!({ "kind": "unknown" }),
            },
        ];

        let expected = [
            ("1", "10", "ping-token"),
            ("2", "20", "chat-token"),
            ("3", "30", "user-token"),
            ("4", "40", "message-token"),
            ("5", "50", "autocomplete-token"),
            ("6", "60", "component-token"),
            ("7", "70", "modal-token"),
            ("8", "80", "unknown-token"),
        ];

        for (interaction, (id, application_id, token)) in interactions.iter().zip(expected) {
            assert_eq!(interaction.context().id.as_str(), id);
            assert_eq!(interaction.id().as_str(), id);
            assert_eq!(interaction.application_id().as_str(), application_id);
            assert_eq!(interaction.token(), token);
        }
    }

    #[test]
    fn discord_model_trait_returns_ids_for_core_models() {
        let guild = Guild {
            id: Snowflake::from("11"),
            name: "Guild".to_string(),
            ..Guild::default()
        };
        let channel = Channel {
            id: Snowflake::from("12"),
            kind: 0,
            ..Channel::default()
        };
        let message = Message {
            id: Snowflake::from("13"),
            channel_id: Snowflake::from("99"),
            ..Message::default()
        };
        let role = Role {
            id: Snowflake::from("14"),
            name: "Admin".to_string(),
            ..Role::default()
        };
        let attachment = Attachment {
            id: Snowflake::from("15"),
            filename: "file.txt".to_string(),
            ..Attachment::default()
        };

        assert_eq!(DiscordModel::id(&guild).as_str(), "11");
        assert_eq!(DiscordModel::id(&channel).as_str(), "12");
        assert_eq!(DiscordModel::id(&message).as_str(), "13");
        assert_eq!(DiscordModel::id(&role).as_str(), "14");
        assert_eq!(DiscordModel::id(&attachment).as_str(), "15");
    }

    #[test]
    fn serde_defaults_fill_missing_fields_for_core_models() {
        let member: Member = serde_json::from_value(json!({})).unwrap();
        let message: Message = serde_json::from_value(json!({
            "id": "55",
            "channel_id": "66"
        }))
        .unwrap();
        let reaction: Reaction = serde_json::from_value(json!({})).unwrap();
        let component: ComponentInteractionData = serde_json::from_value(json!({
            "custom_id": "menu",
            "component_type": 3
        }))
        .unwrap();
        let thread_metadata: ThreadMetadata = serde_json::from_value(json!({})).unwrap();

        assert!(member.roles.is_empty());
        assert_eq!(message.content, "");
        assert!(message.mentions.is_empty());
        assert!(message.attachments.is_empty());
        assert!(message.embeds.is_empty());
        assert!(message.reactions.is_empty());
        assert_eq!(reaction.count, 0);
        assert!(!reaction.me);
        assert!(reaction.emoji.is_none());
        assert!(component.values.is_empty());
        assert!(!thread_metadata.archived);
        assert!(!thread_metadata.locked);
        assert!(thread_metadata.auto_archive_duration.is_none());
    }

    #[test]
    fn simple_payload_models_keep_wire_aliases_and_omit_absent_optionals() {
        let callback = InteractionCallbackResponse {
            kind: 4,
            ..InteractionCallbackResponse::default()
        };
        let dm_channel = CreateDmChannel {
            recipient_id: Snowflake::from("321"),
        };
        let sticker = StickerItem {
            id: Snowflake::from("654"),
            name: "party".to_string(),
            kind: Some(1),
        };
        let invite = Invite {
            kind: Some(0),
            code: Some("abc".to_string()),
            approximate_member_count: Some(42),
            roles: vec![Role {
                id: Snowflake::from("700"),
                name: "guest".to_string(),
                ..Role::default()
            }],
            ..Invite::default()
        };
        let invite_body = CreateChannelInvite {
            role_ids: Some(vec![Snowflake::from("700")]),
            ..CreateChannelInvite::default()
        };
        let invite_job = InviteTargetUsersJobStatus {
            status: 1,
            total_users: 100,
            processed_users: 41,
            created_at: "2025-01-08T12:00:00.000000+00:00".to_string(),
            completed_at: None,
            error_message: None,
        };
        let gateway = GatewayBot {
            url: "wss://gateway.discord.gg".to_string(),
            shards: 2,
            session_start_limit: SessionStartLimit {
                total: 1000,
                remaining: 999,
                reset_after: 60_000,
                max_concurrency: 1,
            },
        };

        assert_eq!(
            serde_json::to_value(&callback).unwrap(),
            json!({ "type": 4 })
        );
        assert_eq!(
            serde_json::to_value(&dm_channel).unwrap(),
            json!({ "recipient_id": "321" })
        );
        assert_eq!(
            serde_json::to_value(&sticker).unwrap(),
            json!({ "id": "654", "name": "party", "format_type": 1 })
        );
        assert_eq!(
            serde_json::to_value(&invite).unwrap(),
            json!({
                "type": 0,
                "code": "abc",
                "approximate_member_count": 42,
                "roles": [{
                    "id": "700",
                    "name": "guest"
                }]
            })
        );
        assert_eq!(
            serde_json::to_value(&invite_body).unwrap(),
            json!({ "role_ids": ["700"] })
        );
        assert_eq!(
            serde_json::to_value(&invite_job).unwrap(),
            json!({
                "status": 1,
                "total_users": 100,
                "processed_users": 41,
                "created_at": "2025-01-08T12:00:00.000000+00:00"
            })
        );
        assert_eq!(
            serde_json::to_value(&gateway).unwrap()["session_start_limit"]["remaining"],
            999
        );
    }

    #[test]
    fn stage_instance_request_bodies_keep_wire_shape() {
        let create = CreateStageInstance {
            channel_id: Snowflake::from("400"),
            topic: "town hall".to_string(),
            privacy_level: Some(2),
            send_start_notification: Some(true),
            guild_scheduled_event_id: Some(Snowflake::from("500")),
        };
        assert_eq!(
            serde_json::to_value(&create).unwrap(),
            json!({
                "channel_id": "400",
                "topic": "town hall",
                "privacy_level": 2,
                "send_start_notification": true,
                "guild_scheduled_event_id": "500"
            })
        );

        let modify = ModifyStageInstance {
            privacy_level: Some(2),
        };
        assert_eq!(
            serde_json::to_value(&modify).unwrap(),
            json!({ "privacy_level": 2 })
        );
    }

    #[test]
    fn sticker_request_bodies_keep_wire_shape() {
        let create = CreateGuildSticker {
            name: "wave".to_string(),
            description: "Waves hello".to_string(),
            tags: "wave,hello".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&create).unwrap(),
            json!({
                "name": "wave",
                "description": "Waves hello",
                "tags": "wave,hello"
            })
        );

        let modify = ModifyGuildSticker {
            name: Some("wave2".to_string()),
            description: Some(None),
            tags: Some("wave".to_string()),
        };
        assert_eq!(
            serde_json::to_value(&modify).unwrap(),
            json!({
                "name": "wave2",
                "description": null,
                "tags": "wave"
            })
        );
    }

    #[test]
    fn voice_state_request_bodies_keep_wire_shape() {
        let current = ModifyCurrentUserVoiceState {
            channel_id: Some(Snowflake::from("200")),
            suppress: Some(false),
            request_to_speak_timestamp: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            json!({
                "channel_id": "200",
                "suppress": false,
                "request_to_speak_timestamp": null
            })
        );

        let user = ModifyUserVoiceState {
            channel_id: Some(Snowflake::from("200")),
            suppress: Some(true),
        };
        assert_eq!(
            serde_json::to_value(&user).unwrap(),
            json!({
                "channel_id": "200",
                "suppress": true
            })
        );
    }

    #[test]
    fn current_application_request_body_keeps_wire_shape() {
        let body = ModifyCurrentApplication {
            description: Some("updated".to_string()),
            role_connections_verification_url: Some(None),
            install_params: Some(ApplicationInstallParams {
                scopes: vec!["bot".to_string(), "applications.commands".to_string()],
                permissions: PermissionsBitField(2048),
            }),
            integration_types_config: Some(HashMap::from([(
                "0".to_string(),
                ApplicationIntegrationTypeConfig {
                    oauth2_install_params: Some(ApplicationInstallParams {
                        scopes: vec!["applications.commands".to_string()],
                        permissions: PermissionsBitField(0),
                    }),
                },
            )])),
            flags: Some(1 << 14),
            icon: Some(None),
            cover_image: Some(Some("data:image/png;base64,abc".to_string())),
            interactions_endpoint_url: Some(Some("https://example.com/interactions".to_string())),
            tags: Some(vec!["utility".to_string()]),
            event_webhooks_url: Some(None),
            event_webhooks_status: Some(2),
            event_webhooks_types: Some(vec!["APPLICATION_AUTHORIZED".to_string()]),
            ..ModifyCurrentApplication::default()
        };

        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({
                "description": "updated",
                "role_connections_verification_url": null,
                "install_params": {
                    "scopes": ["bot", "applications.commands"],
                    "permissions": "2048"
                },
                "integration_types_config": {
                    "0": {
                        "oauth2_install_params": {
                            "scopes": ["applications.commands"],
                            "permissions": "0"
                        }
                    }
                },
                "flags": 16384,
                "icon": null,
                "cover_image": "data:image/png;base64,abc",
                "interactions_endpoint_url": "https://example.com/interactions",
                "tags": ["utility"],
                "event_webhooks_url": null,
                "event_webhooks_status": 2,
                "event_webhooks_types": ["APPLICATION_AUTHORIZED"]
            })
        );
    }

    #[test]
    fn webhook_request_bodies_and_queries_keep_wire_shape() {
        let create = CreateWebhook {
            name: "deployments".to_string(),
            avatar: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&create).unwrap(),
            json!({
                "name": "deployments",
                "avatar": null
            })
        );

        let modify = ModifyWebhook {
            name: Some("ops".to_string()),
            avatar: Some(Some("data:image/png;base64,abc".to_string())),
            channel_id: Some(Snowflake::from("500")),
        };
        assert_eq!(
            serde_json::to_value(&modify).unwrap(),
            json!({
                "name": "ops",
                "avatar": "data:image/png;base64,abc",
                "channel_id": "500"
            })
        );

        let token_modify = ModifyWebhookWithToken {
            name: Some("public".to_string()),
            avatar: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&token_modify).unwrap(),
            json!({
                "name": "public",
                "avatar": null
            })
        );

        let query = WebhookExecuteQuery {
            wait: Some(false),
            thread_id: Some(Snowflake::from("600")),
            with_components: Some(true),
        };
        assert_eq!(
            serde_json::to_value(&query).unwrap(),
            json!({
                "wait": false,
                "thread_id": "600",
                "with_components": true
            })
        );

        let message_query = WebhookMessageQuery {
            thread_id: Some(Snowflake::from("601")),
            with_components: Some(false),
        };
        assert_eq!(
            serde_json::to_value(&message_query).unwrap(),
            json!({
                "thread_id": "601",
                "with_components": false
            })
        );
    }

    #[test]
    fn scheduled_event_recurrence_and_reaction_emoji_are_typed() {
        let event: GuildScheduledEvent = serde_json::from_value(json!({
            "id": "1",
            "guild_id": "2",
            "name": "standup",
            "scheduled_start_time": "2026-04-29T00:00:00.000000+00:00",
            "privacy_level": 2,
            "status": 1,
            "entity_type": 3,
            "entity_metadata": { "location": "voice" },
            "recurrence_rule": {
                "start": "2026-04-29T00:00:00.000000+00:00",
                "frequency": 2,
                "interval": 1,
                "by_weekday": [1],
                "by_n_weekday": [{ "n": 1, "day": 1 }]
            }
        }))
        .unwrap();
        let reaction: Reaction = serde_json::from_value(json!({
            "count": 2,
            "me": true,
            "emoji": { "id": "10", "name": "party", "animated": true }
        }))
        .unwrap();

        assert_eq!(
            event
                .entity_metadata
                .as_ref()
                .and_then(|metadata| metadata.location.as_deref()),
            Some("voice")
        );
        assert_eq!(
            event
                .recurrence_rule
                .as_ref()
                .and_then(|rule| rule.by_n_weekday.as_ref())
                .and_then(|weekdays| weekdays.first())
                .map(|weekday| weekday.day),
            Some(1)
        );
        assert_eq!(
            reaction
                .emoji
                .as_ref()
                .and_then(|emoji| emoji.name.as_deref()),
            Some("party")
        );
    }

    #[test]
    fn embed_presence_and_permissions_cover_optional_and_numeric_serde_paths() {
        let embed = Embed {
            title: Some("Docs".to_string()),
            ..Embed::default()
        };
        let presence = Presence {
            user_id: Some(Snowflake::from("777")),
            ..Presence::default()
        };
        let numeric_permissions: PermissionsBitField = serde_json::from_value(json!(16)).unwrap();
        let invalid_timestamp = Snowflake::new("not-a-number");

        let embed_json = serde_json::to_value(&embed).unwrap();
        let presence_json = serde_json::to_value(&presence).unwrap();

        assert_eq!(embed_json["title"], json!("Docs"));
        assert_eq!(embed_json["fields"], json!([]));
        assert!(embed_json.get("description").is_none());
        assert_eq!(presence_json, json!({ "user_id": "777" }));
        assert_eq!(numeric_permissions.bits(), 16);
        assert_eq!(invalid_timestamp.timestamp(), None);
    }
}
