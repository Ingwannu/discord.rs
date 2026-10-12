use serde_json::Value;

use super::paths::{
    execute_webhook_path, execute_webhook_path_with_query, validate_token_path_segment,
    webhook_message_path, webhook_message_path_with_query,
};
use super::{json_body, multipart_body, FileAttachment, RestClient, RouteId, NO_BODY};
use crate::error::DiscordError;
use crate::model::{
    ChannelId, CreateMessage, CreateWebhook, GuildId, Message, ModifyWebhook,
    ModifyWebhookWithToken, Webhook, WebhookExecuteQuery, WebhookId, WebhookMessageQuery,
};

impl RestClient {
    pub async fn create_webhook(
        &self,
        channel_id: ChannelId,
        body: &CreateWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.send(
            RouteId::CreateWebhook,
            &format!("/channels/{}/webhooks", channel_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `create_webhook`")]
    pub async fn create_webhook_from_request(
        &self,
        channel_id: ChannelId,
        body: &CreateWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.create_webhook(channel_id, body).await
    }

    pub async fn get_channel_webhooks(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.send(
            RouteId::GetChannelWebhooks,
            &format!("/channels/{}/webhooks", channel_id),
            NO_BODY,
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `get_channel_webhooks`")]
    pub async fn get_channel_webhooks_typed(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.get_channel_webhooks(channel_id).await
    }

    pub async fn execute_webhook(
        &self,
        webhook_id: WebhookId,
        token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id, token)?;
        let body = self.json_with_default_allowed_mentions(body);
        self.send(RouteId::ExecuteWebhook, &path, json_body(body.as_ref()))
            .await
    }

    pub async fn execute_webhook_with_query(
        &self,
        webhook_id: WebhookId,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path_with_query(webhook_id, token, query, None)?;
        self.send(RouteId::ExecuteWebhook, &path, json_body(body))
            .await
    }

    pub async fn execute_slack_compatible_webhook(
        &self,
        webhook_id: WebhookId,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path_with_query(webhook_id, token, query, Some("/slack"))?;
        self.send(
            RouteId::ExecuteSlackCompatibleWebhook,
            &path,
            json_body(body),
        )
        .await
    }

    pub async fn execute_github_compatible_webhook(
        &self,
        webhook_id: WebhookId,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path_with_query(webhook_id, token, query, Some("/github"))?;
        self.send(
            RouteId::ExecuteGitHubCompatibleWebhook,
            &path,
            json_body(body),
        )
        .await
    }

    pub async fn execute_webhook_with_files(
        &self,
        webhook_id: WebhookId,
        token: &str,
        body: &Value,
        files: &[FileAttachment],
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id, token)?;
        self.send(RouteId::ExecuteWebhook, &path, multipart_body(body, files))
            .await
    }

    pub async fn get_webhook_message(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id, token, message_id)?;
        self.send(RouteId::GetWebhookMessage, &path, NO_BODY).await
    }

    pub async fn get_webhook_message_with_query(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path_with_query(webhook_id, token, message_id, query, false)?;
        self.send(RouteId::GetWebhookMessage, &path, NO_BODY).await
    }

    pub async fn edit_webhook_message(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id, token, message_id)?;
        self.send(RouteId::EditWebhookMessage, &path, json_body(body))
            .await
    }

    pub async fn edit_webhook_message_with_query(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path_with_query(webhook_id, token, message_id, query, true)?;
        self.send(RouteId::EditWebhookMessage, &path, json_body(body))
            .await
    }

    pub async fn edit_webhook_message_with_files(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id, token, message_id)?;
        self.send(
            RouteId::EditWebhookMessage,
            &path,
            multipart_body(body, files),
        )
        .await
    }

    pub async fn edit_webhook_message_with_files_and_query(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path_with_query(webhook_id, token, message_id, query, true)?;
        self.send(
            RouteId::EditWebhookMessage,
            &path,
            multipart_body(body, files),
        )
        .await
    }

    pub async fn delete_webhook_message(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let path = webhook_message_path(webhook_id, token, message_id)?;
        self.send_unit(RouteId::DeleteWebhookMessage, &path, NO_BODY)
            .await
    }

    pub async fn delete_webhook_message_with_query(
        &self,
        webhook_id: WebhookId,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<(), DiscordError> {
        let path = webhook_message_path_with_query(webhook_id, token, message_id, query, false)?;
        self.send_unit(RouteId::DeleteWebhookMessage, &path, NO_BODY)
            .await
    }

    pub async fn get_guild_webhooks(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.send(
            RouteId::GetGuildWebhooks,
            &format!("/guilds/{}/webhooks", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_webhook(&self, webhook_id: WebhookId) -> Result<Webhook, DiscordError> {
        self.send(
            RouteId::GetWebhook,
            &format!("/webhooks/{}", webhook_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_webhook_with_token(
        &self,
        webhook_id: WebhookId,
        token: &str,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        self.send(
            RouteId::GetWebhookWithToken,
            &format!("/webhooks/{}/{}", webhook_id, token),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_webhook(
        &self,
        webhook_id: WebhookId,
        body: &ModifyWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.send(
            RouteId::ModifyWebhook,
            &format!("/webhooks/{}", webhook_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_webhook`")]
    pub async fn modify_webhook_from_request(
        &self,
        webhook_id: WebhookId,
        body: &ModifyWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook(webhook_id, body).await
    }

    pub async fn delete_webhook(&self, webhook_id: WebhookId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteWebhook,
            &format!("/webhooks/{}", webhook_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_webhook_with_token(
        &self,
        webhook_id: WebhookId,
        token: &str,
        body: &ModifyWebhookWithToken,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id, token);
        self.send(RouteId::ModifyWebhookWithToken, &path, json_body(body))
            .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_webhook_with_token`")]
    pub async fn modify_webhook_with_token_from_request(
        &self,
        webhook_id: WebhookId,
        token: &str,
        body: &ModifyWebhookWithToken,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook_with_token(webhook_id, token, body)
            .await
    }

    pub async fn delete_webhook_with_token(
        &self,
        webhook_id: WebhookId,
        token: &str,
    ) -> Result<(), DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id, token);
        self.send_unit(RouteId::DeleteWebhookWithToken, &path, NO_BODY)
            .await
    }
}
