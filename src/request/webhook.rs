//! Webhook create and modify bodies (execute and message bodies are in `message`).
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::model::ChannelId;
use serde::Serialize;

/// Request body for creating an incoming channel webhook.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CreateWebhook {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}

/// Request body for modifying a bot-authenticated webhook.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyWebhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
}

/// Request body for modifying a token-authenticated webhook.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyWebhookWithToken {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}
