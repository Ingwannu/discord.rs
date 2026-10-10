// Fixture: REST call shapes the drift scanner must (and must not) recognize.
// Evidence quoted only in a comment must not count: "/only/in/a/comment"
use reqwest::Method;

impl RestClient {
    pub async fn get_widget(&self, guild_id: u64) -> Result<Value, DiscordError> {
        self.request(Method::GET, &format!("/guilds/{guild_id}/widget"), None)
            .await
    }

    pub async fn modify_widget(&self, guild_id: u64, body: &Value) -> Result<Value, DiscordError> {
        let query = "?reason=fixture";
        let path = format!("/guilds/{}/widget{query}", guild_id);
        self.request(Method::PATCH, &path, Some(body)).await
    }

    pub async fn get_message(
        &self,
        channel_id: u64,
        message_id: u64,
    ) -> Result<Value, DiscordError> {
        self.request(
            Method::GET,
            &format!(
                "/channels/{}/messages/{}",
                channel_id, message_id
            ),
            None,
        )
        .await
    }

    pub async fn get_pins(&self, channel_id: u64, limit: Option<u8>) -> Result<Value, DiscordError> {
        let path = match limit {
            Some(limit) => format!("/channels/{channel_id}/pins?limit={limit}"),
            None => format!("/channels/{channel_id}/pins"),
        };
        self.request(Method::GET, &path, None).await
    }

    pub async fn get_sparkle(&self, id: &str) -> Result<Value, DiscordError> {
        // A comment with Method::DELETE, "/commented/out" must not count.
        /* block { comment } with Method::PUT, "/also/commented" */
        let _brace = '{';
        let _raw = r#"Method::PATCH, "/raw/string" }"#;
        self.request(Method::GET, &format!("/sparkles/{id}"), None)
            .await
    }

    pub async fn execute_widget_webhook(&self, id: u64, token: &str) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(id, token, Some("/slack"))?;
        self.request(Method::POST, &path, None).await
    }

    pub async fn removed_upstream(&self) -> Result<Value, DiscordError> {
        self.request(Method::POST, "/legacy/thing", None).await
    }

    fn is_replay_safe(method: &Method) -> bool {
        matches!(method, &Method::GET | &Method::PUT)
    }
}

fn route_kind(method: &Method, segments: &[&str]) -> u8 {
    match (method, segments) {
        (&Method::POST, ["channels", _, "messages"]) => 1,
        _ => 0,
    }
}

pub(crate) fn execute_webhook_path(
    id: u64,
    token: &str,
    suffix: Option<&str>,
) -> Result<String, DiscordError> {
    Ok(format!("/webhooks/{id}/{token}{}", suffix.unwrap_or_default()))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod inline_tests {
    #[test]
    fn harness() {
        let _ = client.request(Method::DELETE, "/test/only/route", None);
        let _brace = "}";
    }
}
