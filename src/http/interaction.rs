//! Interaction callback, original response, and follow-up message endpoints.

use std::borrow::Cow;

use crate::error::DiscordError;
use crate::model::{
    ApplicationId, InteractionCallbackResponse, InteractionCallbackResult, InteractionId,
    InteractionResponseType, Message,
};
use crate::request::{
    encode_interaction_response, CreateFollowup, EditInteractionResponse, InteractionResponse,
};

use super::paths::{configured_application_id, followup_webhook_path, interaction_callback_path};
use super::{json_body, multipart_body, FileAttachment, RequestBody, RestClient, RouteId, NO_BODY};

impl RestClient {
    /// Sends an interaction callback (`POST
    /// /interactions/{id}/{token}/callback`).
    ///
    /// Message callbacks (types 4 and 7) whose `allowed_mentions` is unset
    /// get the client's default
    /// ([`RestClientBuilder::default_allowed_mentions`](super::RestClientBuilder::default_allowed_mentions)).
    /// The body is encoded once and sent as is. This is a stateless call: a
    /// parsed interaction does not learn about it; prefer
    /// [`InteractionResponder`](crate::response::InteractionResponder),
    /// which records the initial response.
    pub async fn create_interaction_response(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: &InteractionResponse,
    ) -> Result<(), DiscordError> {
        let body = self.encode_interaction_callback(response)?;
        self.post_interaction_callback(interaction_id, interaction_token, body, &[])
            .await
    }

    /// Sends an interaction callback with `with_response=true` and returns
    /// the callback resource (the created message, or the launched
    /// Activity instance), mirroring discord.js's `withResponse: true`.
    pub async fn create_interaction_response_with_result(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: &InteractionResponse,
    ) -> Result<InteractionCallbackResult, DiscordError> {
        let body = self.encode_interaction_callback(response)?;
        self.post_interaction_callback_with_result(interaction_id, interaction_token, body)
            .await
    }

    /// Sends an interaction callback with file uploads (`multipart/form-data`,
    /// the encoded callback as `payload_json`). Reference each upload from
    /// the message's `attachments`.
    pub async fn create_interaction_response_with_files(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: &InteractionResponse,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let body = self.encode_interaction_callback(response)?;
        self.post_interaction_callback(interaction_id, interaction_token, body, files)
            .await
    }

    /// The 2.x callback body (`kind` + JSON `data`).
    ///
    /// Kept until the phase 5 integration removes the `_typed` names;
    /// prefer [`Self::create_interaction_response`].
    pub async fn create_interaction_response_typed(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: &InteractionCallbackResponse,
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        let body = self.interaction_response_with_default_allowed_mentions(body);
        self.send_unit(
            RouteId::CreateInteractionResponse,
            &path,
            json_body(body.as_ref()),
        )
        .await
    }

    /// Encodes a callback once, filling the client's default
    /// `allowed_mentions` into message callbacks that leave it unset.
    pub(crate) fn encode_interaction_callback(
        &self,
        response: &InteractionResponse,
    ) -> Result<Vec<u8>, DiscordError> {
        encode_interaction_response(response, self.default_allowed_mentions())
    }

    /// Posts an already encoded callback body: as JSON, or as the
    /// `payload_json` part of a multipart request when there are files.
    pub(crate) async fn post_interaction_callback(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: Vec<u8>,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        let body = if files.is_empty() {
            Ok(Some(RequestBody::json(body)))
        } else {
            RequestBody::multipart(body, files)
        };
        self.send_unit(RouteId::CreateInteractionResponse, &path, body)
            .await
    }

    /// Posts an already encoded callback body with `with_response=true`.
    pub(crate) async fn post_interaction_callback_with_result(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        body: Vec<u8>,
    ) -> Result<InteractionCallbackResult, DiscordError> {
        let path = interaction_callback_path(interaction_id, interaction_token)?;
        self.send(
            RouteId::CreateInteractionResponse,
            &format!("{path}?with_response=true"),
            Ok(Some(RequestBody::json(body))),
        )
        .await
    }

    /// Sends a follow-up message (`POST /webhooks/{application.id}/{token}`)
    /// with the application ID configured on the client. An unset
    /// `allowed_mentions` gets the client's default.
    pub async fn create_followup_message(
        &self,
        interaction_token: &str,
        body: &CreateFollowup,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id(application_id, interaction_token, body)
            .await
    }

    /// [`Self::create_followup_message`] with file uploads.
    pub async fn create_followup_message_with_files(
        &self,
        interaction_token: &str,
        body: &CreateFollowup,
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

    /// [`Self::create_followup_message`] for an explicit application ID.
    pub async fn create_followup_message_with_application_id(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateFollowup,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(RouteId::CreateFollowupMessage, &path, json_body(&body))
            .await
    }

    /// [`Self::create_followup_message_with_application_id`] with file
    /// uploads.
    pub async fn create_followup_message_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &CreateFollowup,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(
            RouteId::CreateFollowupMessage,
            &path,
            multipart_body(&body, files),
        )
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
        self.send(RouteId::GetOriginalInteractionResponse, &path, NO_BODY)
            .await
    }

    pub async fn edit_original_interaction_response(
        &self,
        interaction_token: &str,
        body: &EditInteractionResponse,
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
        body: &EditInteractionResponse,
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
        body: &EditInteractionResponse,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(
            RouteId::EditOriginalInteractionResponse,
            &path,
            json_body(&body),
        )
        .await
    }

    pub async fn edit_original_interaction_response_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        body: &EditInteractionResponse,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(
            RouteId::EditOriginalInteractionResponse,
            &path,
            multipart_body(&body, files),
        )
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
        self.send_unit(RouteId::DeleteOriginalInteractionResponse, &path, NO_BODY)
            .await
    }

    pub async fn edit_followup_message(
        &self,
        interaction_token: &str,
        message_id: &str,
        body: &EditInteractionResponse,
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
        body: &EditInteractionResponse,
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
        body: &EditInteractionResponse,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(RouteId::EditFollowupMessage, &path, json_body(&body))
            .await
    }

    pub async fn edit_followup_message_with_application_id_and_files(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: &str,
        body: &EditInteractionResponse,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        let body = body.allowed_mentions_or(self.default_allowed_mentions());
        self.send(
            RouteId::EditFollowupMessage,
            &path,
            multipart_body(&body, files),
        )
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
        self.send_unit(RouteId::DeleteFollowupMessage, &path, NO_BODY)
            .await
    }

    /// Applies the default `allowed_mentions` to message-bearing interaction
    /// callbacks (`CHANNEL_MESSAGE_WITH_SOURCE`, `UPDATE_MESSAGE`) of the
    /// 2.x callback body whose data object leaves it unset.
    fn interaction_response_with_default_allowed_mentions<'a>(
        &self,
        body: &'a InteractionCallbackResponse,
    ) -> Cow<'a, InteractionCallbackResponse> {
        if self.default_allowed_mentions().is_none()
            || !matches!(
                body.kind,
                InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE
                    | InteractionResponseType::UPDATE_MESSAGE
            )
        {
            return Cow::Borrowed(body);
        }
        let Some(data) = body.data.as_ref() else {
            return Cow::Borrowed(body);
        };
        match self.json_with_default_allowed_mentions(data) {
            Cow::Borrowed(_) => Cow::Borrowed(body),
            Cow::Owned(data) => {
                let mut body = body.clone();
                body.data = Some(data);
                Cow::Owned(body)
            }
        }
    }
}
