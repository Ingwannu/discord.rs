//! Gateway discovery endpoints.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{Gateway, GatewayBot};

use super::RestClient;

impl RestClient {
    pub async fn get_gateway_bot(&self) -> Result<GatewayBot, DiscordError> {
        self.request_typed(Method::GET, "/gateway/bot", Option::<&Value>::None)
            .await
    }

    pub async fn get_gateway(&self) -> Result<Gateway, DiscordError> {
        self.request_typed(Method::GET, "/gateway", Option::<&Value>::None)
            .await
    }
}
