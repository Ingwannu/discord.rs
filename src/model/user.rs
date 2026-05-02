use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Snowflake;

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
    pub id: Snowflake,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
