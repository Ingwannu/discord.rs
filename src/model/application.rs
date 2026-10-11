//! Applications, activity instances, and OAuth2 authorization info.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    ActivityLaunchId, ApplicationEventWebhookStatus, ApplicationId, ChannelId, GuildId,
    PermissionsBitField, RoleConnectionMetadataType, SkuId, User, UserId,
};

/// Partial application object attached to messages sent by an app
/// (e.g. Rich Presence or activity invites).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialApplication {
    pub id: ApplicationId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_sku_id: Option<SkuId>,
}

/// A running application Activity instance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityInstance {
    /// Application ID that owns the Activity.
    pub application_id: ApplicationId,
    /// Activity instance ID.
    pub instance_id: String,
    /// Unique launch ID for this Activity session.
    pub launch_id: ActivityLaunchId,
    /// Location where the Activity instance is running.
    pub location: ActivityLocation,
    /// Users currently connected to the Activity instance.
    #[serde(default)]
    pub users: Vec<UserId>,
}

/// Location where an application Activity instance is running.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityLocation {
    /// Unique location ID.
    pub id: String,
    /// Location kind, such as `gc` for guild channel or `pc` for private channel.
    pub kind: String,
    /// Channel ID for this location.
    pub channel_id: ChannelId,
    /// Guild ID for guild-channel Activity locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Application`.
pub struct Application {
    pub id: ApplicationId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub description: String,
    #[serde(default)]
    pub rpc_origins: Vec<String>,
    #[serde(default)]
    pub bot_public: bool,
    #[serde(default)]
    pub bot_require_code_grant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_policy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_sku_id: Option<SkuId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_guild_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_install_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_authorization_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uris: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<ApplicationEventWebhookStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, serde_json::Value>>,
}

/// Default OAuth2 install settings for an application install context.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationInstallParams {
    pub scopes: Vec<String>,
    pub permissions: PermissionsBitField,
}

/// Default install settings for one application integration type.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIntegrationTypeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2_install_params: Option<ApplicationInstallParams>,
}

/// Request body for editing the current application.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<ApplicationInstallParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, ApplicationIntegrationTypeConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<ApplicationEventWebhookStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
}

/// One application role connection metadata record. No `Default`: `type`,
/// `key`, `name`, and `description` are required; use
/// [`ApplicationRoleConnectionMetadata::new`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplicationRoleConnectionMetadata {
    #[serde(rename = "type")]
    pub kind: RoleConnectionMetadataType,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
}

impl ApplicationRoleConnectionMetadata {
    /// A record comparing the user's `key` value with `kind`; localizations
    /// unset.
    pub fn new(
        kind: RoleConnectionMetadataType,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            key: key.into(),
            name: name.into(),
            name_localizations: None,
            description: description.into(),
            description_localizations: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for current OAuth2 authorization information.
pub struct AuthorizationInformation {
    /// Partial Discord application object for the current authorization.
    pub application: serde_json::Value,
    /// OAuth2 scopes granted to the authorization.
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// User that authorized the application when the token includes `identify`.
    pub user: Option<User>,
}
