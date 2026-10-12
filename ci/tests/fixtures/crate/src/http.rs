// Fixture: endpoint shapes the manifest binding check must (and must not)
// accept. Every endpoint names its route with `RouteId`; only this transport
// file may name an HTTP method (`direct_method_files`).
// Evidence quoted only in a comment must not count: "/only/in/a/comment"
use reqwest::Method;

impl RestClient {
    pub async fn get_widget(&self, guild_id: u64) -> Result<Value, DiscordError> {
        self.send(RouteId::GetGuildWidget, &format!("/guilds/{guild_id}/widget"), NO_BODY)
            .await
    }

    pub async fn modify_widget(&self, guild_id: u64, body: &Value) -> Result<Value, DiscordError> {
        let path = format!("/guilds/{}/widget?reason=fixture", guild_id);
        self.send(RouteId::ModifyGuildWidget, &path, json_body(body)).await
    }

    pub async fn get_message(&self, channel_id: u64, message_id: u64) -> Result<Value, DiscordError> {
        self.send(
            RouteId::GetChannelMessage,
            &format!("/channels/{}/messages/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    /// Delegates: bound to its route through `self.get_pins_limited(`.
    pub async fn get_pins(&self, channel_id: u64) -> Result<Value, DiscordError> {
        self.get_pins_limited(channel_id, None).await
    }

    pub async fn get_pins_limited(&self, channel_id: u64, limit: Option<u8>) -> Result<Value, DiscordError> {
        let path = match limit {
            Some(limit) => format!("/channels/{channel_id}/pins?limit={limit}"),
            None => format!("/channels/{channel_id}/pins"),
        };
        self.send(RouteId::GetPinnedMessages, &path, NO_BODY).await
    }

    /// Removed in 3.0 (`was [pins => get_pins]`); allowed only as a
    /// deprecated forwarding method.
    #[deprecated(note = "use get_pins")]
    pub async fn pins(&self, channel_id: u64) -> Result<Value, DiscordError> {
        self.get_pins(channel_id).await
    }

    pub async fn get_sparkle(&self, id: &str) -> Result<Value, DiscordError> {
        // A comment naming RouteId::GetChannelMessage must not bind it.
        let _brace = '{';
        let _raw = r#"RouteId::LegacyThing }"#;
        self.send(RouteId::GetSparkle, &format!("/sparkles/{id}"), NO_BODY)
            .await
    }

    pub async fn execute_widget_webhook(&self, id: u64, token: &str) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(id, token)?;
        self.send(RouteId::ExecuteWebhook, &path, NO_BODY).await
    }

    pub async fn removed_upstream(&self) -> Result<Value, DiscordError> {
        self.send(RouteId::LegacyThing, "/legacy/thing", NO_BODY).await
    }

    /// The raw escape hatch (`non_endpoint_functions`).
    pub async fn request(&self, method: Method, path: &str) -> Result<Value, DiscordError> {
        self.execute(Target::Raw(&method), path, NO_BODY).await
    }

    fn is_replay_safe(method: &Method) -> bool {
        matches!(method, &Method::GET | &Method::PUT)
    }
}

pub(crate) fn execute_webhook_path(id: u64, token: &str) -> Result<String, DiscordError> {
    Ok(format!("/webhooks/{id}/{token}"))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inline_tests {
    impl RestClient {
        pub async fn test_only_endpoint(&self) -> Result<Value, DiscordError> {
            self.request(Method::DELETE, "/test/only/route").await
        }
    }
}
