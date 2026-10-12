//! Gateway discovery endpoints.

use crate::error::DiscordError;
use crate::model::{Gateway, GatewayBot};

use super::{RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_gateway_bot(&self) -> Result<GatewayBot, DiscordError> {
        self.send(RouteId::GetGatewayBot, "/gateway/bot", NO_BODY)
            .await
    }

    pub async fn get_gateway(&self) -> Result<Gateway, DiscordError> {
        self.send(RouteId::GetGateway, "/gateway", NO_BODY).await
    }
}
