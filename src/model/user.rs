use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    ConnectionVisibility, DisplayNameEffect, DisplayNameFont, GuildId, PartialIntegration, SkuId,
    UserId, UserPremiumType,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Data for a user's avatar decoration.
pub struct AvatarDecorationData {
    /// Decoration asset hash.
    pub asset: String,
    /// SKU that owns the decoration.
    pub sku_id: SkuId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Data for a user's profile nameplate collectible.
pub struct UserNameplate {
    /// SKU that owns the nameplate.
    pub sku_id: SkuId,
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
    pub identity_guild_id: Option<GuildId>,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// How a user's or member's name is displayed: font, effect, and colors
/// (Display Name Styles Object).
///
/// **Preview:** based on the unreleased discord-api-docs PR #8178, which is
/// still open; the shape may change.
///
/// Every field is optional here although the docs mark them required: the
/// object sits inside every [`User`] and [`Member`](super::Member), so a
/// shape change in this not yet released part of the API must not fail the
/// whole message, member, or event that carries the user.
pub struct DisplayNameStyles {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_id: Option<DisplayNameFont>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<DisplayNameEffect>,
    /// Colors as integers representing hexadecimal color codes (at most 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `User`.
pub struct User {
    pub id: UserId,
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
    pub premium_type: Option<UserPremiumType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_guild: Option<UserPrimaryGuild>,
    /// Preview ([`DisplayNameStyles`]: unreleased discord-api-docs PR
    /// #8178, may change). Boxed: rarely set, and every cached user, member,
    /// and message author would otherwise carry its 32 bytes inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name_styles: Option<Box<DisplayNameStyles>>,
}

/// OAuth2 connection attached to the current user. No `Default`: `id`,
/// `name`, `type`, and `visibility` are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub integrations: Vec<PartialIntegration>,
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
    /// Who can see the connection (required; 2.x read a missing value as
    /// `0`).
    pub visibility: ConnectionVisibility,
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::UserConnection;
    use crate::model::test_id;

    /// A connection (Connection Structure) whose integrations are the
    /// documented partial integration (resources/audit-log.mdx — Example
    /// Partial Integration Object) and a bare one with only an ID.
    const CONNECTION: &str = r#"{
      "id": "1234567",
      "name": "twitchusername",
      "type": "twitch",
      "revoked": false,
      "integrations": [
        {
          "id": "33590653072239123",
          "name": "A Name",
          "type": "twitch",
          "account": {
            "name": "twitchusername",
            "id": "1234567"
          },
          "application_id": "94651234501213162"
        },
        { "id": "33590653072239124" }
      ],
      "verified": true,
      "friend_sync": false,
      "show_activity": true,
      "two_way_link": false,
      "visibility": 1
    }"#;

    fn check(connection: &UserConnection) {
        let [documented, bare] = &connection.integrations[..] else {
            panic!("two integrations");
        };
        assert_eq!(documented.id, test_id(33590653072239123));
        assert_eq!(documented.name.as_deref(), Some("A Name"));
        assert_eq!(documented.kind.as_deref(), Some("twitch"));
        let account = documented.account.as_ref().unwrap();
        assert_eq!(
            (account.id.as_str(), account.name.as_str()),
            ("1234567", "twitchusername")
        );
        assert_eq!(documented.application_id, Some(test_id(94651234501213162)));
        assert_eq!(bare.id, test_id(33590653072239124));
        assert!(bare.name.is_none() && bare.account.is_none());
    }

    #[test]
    fn connection_integrations_decode_as_partial_integrations() {
        let from_text: UserConnection = serde_json::from_str(CONNECTION).unwrap();
        check(&from_text);
        let wire: Value = serde_json::from_str(CONNECTION).unwrap();
        let from_value: UserConnection = serde_json::from_value(wire.clone()).unwrap();
        check(&from_value);
        assert_eq!(
            serde_json::to_value(&from_value).unwrap()["integrations"],
            wire["integrations"]
        );

        // `integrations` is optional on the wire.
        let mut without = wire;
        if let Some(connection) = without.as_object_mut() {
            connection.remove("integrations");
        }
        let connection: UserConnection = serde_json::from_value(without).unwrap();
        assert!(connection.integrations.is_empty());
        assert_eq!(
            serde_json::to_value(&connection).unwrap()["integrations"],
            json!([])
        );
    }
}
