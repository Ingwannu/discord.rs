//! Webhooks and webhook request/query types.

use serde::{Deserialize, Serialize};

use super::{ApplicationId, ChannelId, GuildId, WebhookId, WebhookType};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Webhook`.
pub struct Webhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<WebhookId>,
    /// Webhook type. Read from and written as `type` (2.x used the field
    /// name `kind` on the wire, so Discord's value was never decoded).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<WebhookType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Request body for creating an incoming channel webhook.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateWebhook {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}

/// Request body for modifying a bot-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
}

/// Request body for modifying a token-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhookWithToken {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}

/// Query options for executing a webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookExecuteQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_components: Option<bool>,
}

/// Query options for webhook message get/edit/delete routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookMessageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_components: Option<bool>,
}
