use reqwest::Method;
use serde_json::Value;

use super::paths::{
    execute_webhook_path, execute_webhook_path_with_query, validate_token_path_segment,
    webhook_message_path, webhook_message_path_with_query,
};
use super::{FileAttachment, RestClient};
use crate::error::DiscordError;
use crate::model::{
    CreateMessage, CreateWebhook, Message, ModifyWebhook, ModifyWebhookWithToken, Snowflake,
    Webhook, WebhookExecuteQuery, WebhookMessageQuery,
};

impl RestClient {
    pub async fn create_webhook(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.create_webhook_raw(channel_id, body).await
    }

    pub async fn create_webhook_typed<B>(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_webhook_from_request(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.create_webhook_typed(channel_id, body).await
    }

    pub async fn create_webhook_raw(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn get_channel_webhooks(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Value>, DiscordError> {
        self.get_channel_webhooks_raw(channel_id).await
    }

    pub async fn get_channel_webhooks_typed(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_channel_webhooks_raw(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Value>, DiscordError> {
        let response = self
            .request(
                Method::GET,
                &format!("/channels/{}/webhooks", channel_id.into()),
                Option::<&Value>::None,
            )
            .await?;
        match response {
            Value::Array(webhooks) => Ok(webhooks),
            _ => Ok(vec![]),
        }
    }

    pub async fn execute_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id.into(), token)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn execute_webhook_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path_with_query(webhook_id.into(), token, query, None)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn execute_slack_compatible_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path =
            execute_webhook_path_with_query(webhook_id.into(), token, query, Some("/slack"))?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn execute_github_compatible_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path =
            execute_webhook_path_with_query(webhook_id.into(), token, query, Some("/github"))?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn execute_webhook_with_files(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
        files: &[FileAttachment],
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id.into(), token)?;
        self.request_multipart(Method::POST, &path, body, files)
            .await
    }

    pub async fn get_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn get_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, false)?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn edit_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, true)?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_webhook_message_with_files(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn edit_webhook_message_with_files_and_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, true)?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn delete_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub async fn delete_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<(), DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, false)?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub async fn get_guild_webhooks(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/webhooks", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
    ) -> Result<Webhook, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/webhooks/{}", webhook_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        self.request_typed(
            Method::GET,
            &format!("/webhooks/{}/{}", webhook_id.into(), token),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Webhook, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/webhooks/{}", webhook_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_webhook_typed<B>(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/webhooks/{}", webhook_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_webhook_from_request(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &ModifyWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook_typed(webhook_id, body).await
    }

    pub async fn delete_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/webhooks/{}", webhook_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn modify_webhook_with_token_typed<B>(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn modify_webhook_with_token_from_request(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &ModifyWebhookWithToken,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook_with_token_typed(webhook_id, token, body)
            .await
    }

    pub async fn delete_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
    ) -> Result<(), DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }
}
