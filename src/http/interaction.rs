//! Interaction callback, original response, and follow-up message endpoints.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ApplicationId, CreateMessage, InteractionCallbackResponse, InteractionId, Message,
};

use super::paths::{configured_application_id, followup_webhook_path, interaction_callback_path};
use super::{FileAttachment, RestClient};

impl RestClient {
    pub async fn create_interaction_response_typed(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: &InteractionCallbackResponse,
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        let body = self.interaction_response_with_default_allowed_mentions(body);
        self.request_no_content(Method::POST, &path, Some(body.as_ref()))
            .await
    }

    pub async fn create_interaction_response_with_files(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: &InteractionCallbackResponse,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        self.request_multipart_no_content(Method::POST, &path, body, files)
            .await
    }

    pub(crate) async fn create_interaction_response_json(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: &Value,
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        self.request_no_content(Method::POST, &path, Some(body))
            .await
    }

    pub(crate) async fn create_followup_message_json(
        &self,
        interaction_token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_json_with_application_id(
            application_id,
            interaction_token,
            body,
        )
        .await
    }

    pub(crate) async fn create_followup_message_json_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn create_followup_message(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id(application_id, interaction_token, body)
            .await
    }

    pub async fn create_followup_message_with_files(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id_and_files(
            application_id,
            interaction_token,
            body,
            files,
        )
        .await
    }

    pub async fn create_followup_message_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request_typed(Method::POST, &path, Some(body)).await
    }

    pub async fn create_followup_message_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request_typed_multipart(Method::POST, &path, body, files)
            .await
    }

    pub async fn get_original_interaction_response(
        &self,
        interaction_token: &str,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.get_original_interaction_response_with_application_id(
            application_id,
            interaction_token,
        )
        .await
    }

    pub async fn get_original_interaction_response_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn edit_original_interaction_response(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_original_interaction_response_with_application_id(
            application_id,
            interaction_token,
            body,
        )
        .await
    }

    pub async fn edit_original_interaction_response_with_files(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_original_interaction_response_with_application_id_and_files(
            application_id,
            interaction_token,
            body,
            files,
        )
        .await
    }

    pub async fn edit_original_interaction_response_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_original_interaction_response_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn delete_original_interaction_response(
        &self,
        interaction_token: &str,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.delete_original_interaction_response_with_application_id(
            application_id,
            interaction_token,
        )
        .await
    }

    pub async fn delete_original_interaction_response_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Result<(), DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub async fn edit_followup_message(
        &self,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_followup_message_with_application_id(
            application_id,
            interaction_token,
            message_id,
            body,
        )
        .await
    }

    pub async fn edit_followup_message_with_files(
        &self,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_followup_message_with_application_id_and_files(
            application_id,
            interaction_token,
            message_id,
            body,
            files,
        )
        .await
    }

    pub async fn edit_followup_message_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_followup_message_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn delete_followup_message(
        &self,
        interaction_token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.delete_followup_message_with_application_id(
            application_id,
            interaction_token,
            message_id,
        )
        .await
    }

    pub async fn delete_followup_message_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    /// Sends an interaction response with `with_response=true` and returns
    /// the typed interaction callback resource, mirroring discord.js's
    /// `withResponse: true`.
    pub async fn create_interaction_response_with_result(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: &InteractionCallbackResponse,
    ) -> Result<crate::model::InteractionCallbackResult, DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        self.request_typed(
            Method::POST,
            &format!("{path}?with_response=true"),
            Some(response),
        )
        .await
    }
}
