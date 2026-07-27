use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::builders::{ComponentsV2Message, ModalBuilder};
use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
use crate::error::DiscordError;
use crate::helpers::{
    INTERACTION_RESPONSE_AUTOCOMPLETE_RESULT, INTERACTION_RESPONSE_CHANNEL_MESSAGE,
    INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE, INTERACTION_RESPONSE_DEFERRED_UPDATE_MESSAGE,
    INTERACTION_RESPONSE_MODAL, INTERACTION_RESPONSE_UPDATE_MESSAGE,
};
use crate::http::RestClient;
use crate::model::{
    AllowedMentions, AutocompleteInteraction, ChatInputCommandInteraction, ComponentInteraction,
    CreateMessage, InteractionCallbackResponse, InteractionCallbackResult, InteractionContextData,
    InteractionResponseState, Message, MessageContextMenuInteraction, ModalSubmitInteraction,
    UserContextMenuInteraction,
};

/// Discord message flag marking a response as ephemeral.
const EPHEMERAL_FLAG: u64 = 1 << 6;

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `MessageBuilder`.
pub struct MessageBuilder {
    inner: CreateMessage,
}

impl MessageBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.inner.content = Some(content.into());
        self
    }

    pub fn components(mut self, components: Vec<Value>) -> Self {
        self.inner.components = Some(components);
        self
    }

    pub fn components_v2(mut self, message: ComponentsV2Message) -> Self {
        self.inner.components = Some(message.build());
        self.inner.flags = Some(self.inner.flags.unwrap_or(0) | MESSAGE_FLAG_IS_COMPONENTS_V2);
        self
    }

    pub fn flags(mut self, flags: u64) -> Self {
        self.inner.flags = Some(flags);
        self
    }

    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        if ephemeral {
            self.inner.flags = Some(self.inner.flags.unwrap_or(0) | (1 << 6));
        }
        self
    }

    pub fn build(self) -> CreateMessage {
        self.inner
    }
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InteractionResponseBuilder`.
pub struct InteractionResponseBuilder {
    inner: InteractionCallbackResponse,
}

impl InteractionResponseBuilder {
    /// Creates a `channel_message` value.
    pub fn channel_message(message: MessageBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionCallbackResponse {
                kind: INTERACTION_RESPONSE_CHANNEL_MESSAGE,
                data: Some(serde_json::to_value(message.build())?),
            },
        })
    }

    /// Creates a `deferred_channel_message` value.
    pub fn deferred_channel_message(ephemeral: bool) -> Self {
        let mut flags = 0_u64;
        if ephemeral {
            flags |= 1 << 6;
        }

        Self {
            inner: InteractionCallbackResponse {
                kind: INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE,
                data: Some(serde_json::json!({ "flags": flags })),
            },
        }
    }

    /// Creates a `update_message` value.
    pub fn update_message(message: MessageBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionCallbackResponse {
                kind: INTERACTION_RESPONSE_UPDATE_MESSAGE,
                data: Some(serde_json::to_value(message.build())?),
            },
        })
    }

    /// Creates a `modal` value.
    pub fn modal(modal: ModalBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionCallbackResponse {
                kind: INTERACTION_RESPONSE_MODAL,
                data: Some(serde_json::to_value(modal.build())?),
            },
        })
    }

    pub fn build(self) -> InteractionCallbackResponse {
        self.inner
    }
}

#[derive(Clone, Debug, Default)]
/// Message payload accepted by the discord.js-style interaction response
/// methods (`reply`, `edit_reply`, `follow_up`, `update_message`).
///
/// Converts from `&str`/`String` (content-only), [`MessageBuilder`], and
/// [`CreateMessage`], so both `interaction.reply(&http, "hi")` and fully
/// built payloads work.
pub struct InteractionReplyData {
    /// Message text content.
    pub content: Option<String>,
    /// Raw embed objects.
    pub embeds: Option<Vec<Value>>,
    /// Raw component objects (action rows or components-v2 payloads).
    pub components: Option<Vec<Value>>,
    /// Message flags (e.g. `1 << 6` for ephemeral).
    pub flags: Option<u64>,
    /// Allowed-mentions overrides.
    pub allowed_mentions: Option<AllowedMentions>,
}

impl InteractionReplyData {
    /// Creates an empty payload.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the message content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Sets raw embed objects.
    pub fn embeds(mut self, embeds: Vec<Value>) -> Self {
        self.embeds = Some(embeds);
        self
    }

    /// Sets raw component objects.
    pub fn components(mut self, components: Vec<Value>) -> Self {
        self.components = Some(components);
        self
    }

    /// Sets the message flags, replacing any previous value.
    pub fn flags(mut self, flags: u64) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sets the allowed-mentions overrides.
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Marks the response ephemeral (message flag `1 << 6`).
    pub fn ephemeral(mut self) -> Self {
        self.flags = Some(self.flags.unwrap_or(0) | EPHEMERAL_FLAG);
        self
    }

    /// Converts into the REST `CreateMessage` payload.
    fn into_create_message(self) -> CreateMessage {
        CreateMessage {
            content: self.content,
            components: self.components,
            flags: self.flags,
            embeds: self.embeds,
            allowed_mentions: self.allowed_mentions,
            ..CreateMessage::default()
        }
    }
}

impl From<&str> for InteractionReplyData {
    fn from(content: &str) -> Self {
        Self::new().content(content)
    }
}

impl From<String> for InteractionReplyData {
    fn from(content: String) -> Self {
        Self::new().content(content)
    }
}

impl From<MessageBuilder> for InteractionReplyData {
    fn from(builder: MessageBuilder) -> Self {
        Self::from(builder.build())
    }
}

impl From<CreateMessage> for InteractionReplyData {
    fn from(message: CreateMessage) -> Self {
        Self {
            content: message.content,
            embeds: message.embeds,
            components: message.components,
            flags: message.flags,
            allowed_mentions: message.allowed_mentions,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// One autocomplete choice sent with
/// [`InteractionContextData::respond_autocomplete`].
pub struct AutocompleteChoice {
    /// Choice name shown to the user (1-100 characters).
    pub name: String,
    /// Choice value returned when selected (string, integer, or double).
    pub value: Value,
    /// Localized choice names keyed by locale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
}

impl AutocompleteChoice {
    /// Creates a choice from a name and value.
    pub fn new(name: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            name_localizations: None,
        }
    }

    /// Sets localized names keyed by locale (e.g. `"de"`, `"fr"`).
    pub fn name_localizations(mut self, localizations: HashMap<String, String>) -> Self {
        self.name_localizations = Some(localizations);
        self
    }
}

/// Builds a type 4 (reply) or type 7 (update message) callback payload.
fn message_callback(
    kind: u8,
    data: InteractionReplyData,
) -> Result<InteractionCallbackResponse, DiscordError> {
    Ok(InteractionCallbackResponse {
        kind,
        data: Some(serde_json::to_value(data.into_create_message())?),
    })
}

/// Builds a type 5 (deferred channel message) callback payload.
fn deferred_message_callback(ephemeral: bool) -> InteractionCallbackResponse {
    let flags = if ephemeral { EPHEMERAL_FLAG } else { 0 };
    InteractionCallbackResponse {
        kind: INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE,
        data: Some(serde_json::json!({ "flags": flags })),
    }
}

/// Builds a type 6 (deferred message update) callback payload.
fn deferred_update_callback() -> InteractionCallbackResponse {
    InteractionCallbackResponse {
        kind: INTERACTION_RESPONSE_DEFERRED_UPDATE_MESSAGE,
        data: None,
    }
}

/// Builds a type 9 (modal) callback payload.
fn modal_callback(modal: ModalBuilder) -> InteractionCallbackResponse {
    InteractionCallbackResponse {
        kind: INTERACTION_RESPONSE_MODAL,
        data: Some(modal.build()),
    }
}

/// Builds a type 8 (autocomplete result) callback payload.
fn autocomplete_callback(
    choices: Vec<AutocompleteChoice>,
) -> Result<InteractionCallbackResponse, DiscordError> {
    Ok(InteractionCallbackResponse {
        kind: INTERACTION_RESPONSE_AUTOCOMPLETE_RESULT,
        data: Some(serde_json::json!({ "choices": serde_json::to_value(choices)? })),
    })
}

/// discord.js-style response methods, implemented once on the shared
/// interaction context and re-exposed on each variant through
/// [`InteractionResponder`].
impl InteractionContextData {
    /// Sends the initial reply (type 4, `CHANNEL_MESSAGE_WITH_SOURCE`).
    ///
    /// Errors with `DiscordError::Model` when the interaction was already
    /// acknowledged (replied, deferred, or shown a modal).
    pub async fn reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        let response = message_callback(INTERACTION_RESPONSE_CHANNEL_MESSAGE, data.into())?;
        self.send_initial_response(http, InteractionResponseState::REPLIED, &response)
            .await
    }

    /// Sends the initial reply as an ephemeral message (flag `1 << 6`).
    pub async fn reply_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        let response = message_callback(
            INTERACTION_RESPONSE_CHANNEL_MESSAGE,
            data.into().ephemeral(),
        )?;
        self.send_initial_response(http, InteractionResponseState::REPLIED, &response)
            .await
    }

    /// Sends the initial reply with `with_response=true` and returns the
    /// typed callback result, mirroring discord.js's `withResponse: true`.
    pub async fn reply_with_result(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<InteractionCallbackResult, DiscordError> {
        let response = message_callback(INTERACTION_RESPONSE_CHANNEL_MESSAGE, data.into())?;
        self.response_state
            .acknowledge(InteractionResponseState::REPLIED)?;
        let result = http
            .create_interaction_response_with_result(self.id.clone(), &self.token, &response)
            .await;
        if result.is_err() {
            self.response_state
                .revert(InteractionResponseState::REPLIED);
        }
        result
    }

    /// Defers the reply (type 5), showing a public "thinking" state.
    pub async fn defer(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(
            http,
            InteractionResponseState::DEFERRED,
            &deferred_message_callback(false),
        )
        .await
    }

    /// Defers the reply (type 5) with an ephemeral "thinking" state.
    pub async fn defer_ephemeral(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(
            http,
            InteractionResponseState::DEFERRED,
            &deferred_message_callback(true),
        )
        .await
    }

    /// Acknowledges without any visible response (type 6). Valid only for
    /// component and modal-submit interactions.
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(
            http,
            InteractionResponseState::DEFERRED,
            &deferred_update_callback(),
        )
        .await
    }

    /// Edits the message the component was attached to (type 7). Valid only
    /// for component interactions (and modals launched from one).
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        let response = message_callback(INTERACTION_RESPONSE_UPDATE_MESSAGE, data.into())?;
        self.send_initial_response(http, InteractionResponseState::REPLIED, &response)
            .await
    }

    /// Opens a modal (type 9). Discord accepts this only as the initial
    /// response of command and component interactions.
    pub async fn show_modal(
        &self,
        http: &RestClient,
        modal: ModalBuilder,
    ) -> Result<(), DiscordError> {
        self.send_initial_response(
            http,
            InteractionResponseState::REPLIED,
            &modal_callback(modal),
        )
        .await
    }

    /// Sends autocomplete choices (type 8). Valid only for autocomplete
    /// interactions; errors if the interaction was already responded to.
    pub async fn respond_autocomplete(
        &self,
        http: &RestClient,
        choices: Vec<AutocompleteChoice>,
    ) -> Result<(), DiscordError> {
        let response = autocomplete_callback(choices)?;
        self.send_initial_response(http, InteractionResponseState::REPLIED, &response)
            .await
    }

    /// Edits the original response (PATCH `@original`) and returns the
    /// resulting message.
    ///
    /// Errors with `DiscordError::Model` when the interaction has not been
    /// acknowledged yet. Editing a deferred reply promotes the state to
    /// replied, as in discord.js.
    pub async fn edit_reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.response_state.require_acknowledged()?;
        let message = http
            .edit_original_interaction_response_with_application_id(
                self.application_id.as_str(),
                &self.token,
                &data.into().into_create_message(),
            )
            .await?;
        self.response_state.mark_replied();
        Ok(message)
    }

    /// Fetches the original response message (GET `@original`).
    pub async fn fetch_reply(&self, http: &RestClient) -> Result<Message, DiscordError> {
        http.get_original_interaction_response_with_application_id(
            self.application_id.as_str(),
            &self.token,
        )
        .await
    }

    /// Deletes the original response message.
    ///
    /// Errors with `DiscordError::Model` when the interaction has not been
    /// acknowledged yet.
    pub async fn delete_reply(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.response_state.require_acknowledged()?;
        http.delete_original_interaction_response_with_application_id(
            self.application_id.as_str(),
            &self.token,
        )
        .await
    }

    /// Sends a follow-up message and returns it.
    ///
    /// Errors with `DiscordError::Model` when the interaction has not been
    /// acknowledged yet.
    pub async fn follow_up(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.response_state.require_acknowledged()?;
        http.create_followup_message_with_application_id(
            self.application_id.as_str(),
            &self.token,
            &data.into().into_create_message(),
        )
        .await
    }

    /// Sends an ephemeral follow-up message and returns it.
    pub async fn follow_up_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.follow_up(http, data.into().ephemeral()).await
    }

    /// Claims the initial-response slot, posts the callback, and rolls the
    /// state back when the HTTP call fails so the caller may retry.
    async fn send_initial_response(
        &self,
        http: &RestClient,
        next_state: u8,
        response: &InteractionCallbackResponse,
    ) -> Result<(), DiscordError> {
        self.response_state.acknowledge(next_state)?;
        let result = http
            .create_interaction_response_typed(self.id.clone(), &self.token, response)
            .await;
        if result.is_err() {
            self.response_state.revert(next_state);
        }
        result
    }
}

/// discord.js-style responder methods for interaction variants that can send
/// an initial response.
///
/// Every method delegates to the shared [`InteractionContextData`], so the
/// acknowledgement state is shared between clones of one interaction.
#[allow(async_fn_in_trait)]
pub trait InteractionResponder {
    /// The shared context (id, token, response state) used for responding.
    fn responder_context(&self) -> &InteractionContextData;

    /// Returns true when an initial response has been sent.
    fn is_replied(&self) -> bool {
        self.responder_context().is_replied()
    }

    /// Returns true when the interaction was deferred and not yet replied to.
    fn is_deferred(&self) -> bool {
        self.responder_context().is_deferred()
    }

    /// Returns true when the interaction was acknowledged in any way.
    fn is_acknowledged(&self) -> bool {
        self.responder_context().is_acknowledged()
    }

    /// Sends the initial reply (type 4). See [`InteractionContextData::reply`].
    async fn reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        self.responder_context().reply(http, data).await
    }

    /// Sends the initial reply as an ephemeral message.
    async fn reply_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        self.responder_context().reply_ephemeral(http, data).await
    }

    /// Sends the initial reply and returns the typed callback result.
    async fn reply_with_result(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<InteractionCallbackResult, DiscordError> {
        self.responder_context().reply_with_result(http, data).await
    }

    /// Defers the reply (type 5).
    async fn defer(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.responder_context().defer(http).await
    }

    /// Defers the reply (type 5) with an ephemeral "thinking" state.
    async fn defer_ephemeral(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.responder_context().defer_ephemeral(http).await
    }

    /// Edits the original response and returns the resulting message.
    async fn edit_reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.responder_context().edit_reply(http, data).await
    }

    /// Fetches the original response message.
    async fn fetch_reply(&self, http: &RestClient) -> Result<Message, DiscordError> {
        self.responder_context().fetch_reply(http).await
    }

    /// Deletes the original response message.
    async fn delete_reply(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.responder_context().delete_reply(http).await
    }

    /// Sends a follow-up message and returns it.
    async fn follow_up(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.responder_context().follow_up(http, data).await
    }

    /// Sends an ephemeral follow-up message and returns it.
    async fn follow_up_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<Message, DiscordError> {
        self.responder_context()
            .follow_up_ephemeral(http, data)
            .await
    }

    /// Opens a modal (type 9). Discord accepts this only for command and
    /// component interactions.
    async fn show_modal(&self, http: &RestClient, modal: ModalBuilder) -> Result<(), DiscordError> {
        self.responder_context().show_modal(http, modal).await
    }
}

impl InteractionResponder for ChatInputCommandInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for UserContextMenuInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for MessageContextMenuInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for ComponentInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for ModalSubmitInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl ComponentInteraction {
    /// Acknowledges the component interaction without a visible response
    /// (type 6).
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.context.defer_update(http).await
    }

    /// Edits the message the component is attached to (type 7).
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        self.context.update_message(http, data).await
    }
}

impl ModalSubmitInteraction {
    /// Acknowledges the modal submission without a visible response (type 6).
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.context.defer_update(http).await
    }

    /// Edits the message the modal was launched from (type 7). Valid only
    /// when the modal originated from a component interaction.
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<InteractionReplyData>,
    ) -> Result<(), DiscordError> {
        self.context.update_message(http, data).await
    }
}

impl AutocompleteInteraction {
    /// Sends autocomplete choices (type 8); errors if this interaction was
    /// already responded to.
    pub async fn respond_autocomplete(
        &self,
        http: &RestClient,
        choices: Vec<AutocompleteChoice>,
    ) -> Result<(), DiscordError> {
        self.context.respond_autocomplete(http, choices).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::builders::{ComponentsV2Message, ModalBuilder};
    use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
    use crate::helpers::{
        INTERACTION_RESPONSE_AUTOCOMPLETE_RESULT, INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE,
        INTERACTION_RESPONSE_DEFERRED_UPDATE_MESSAGE, INTERACTION_RESPONSE_MODAL,
        INTERACTION_RESPONSE_UPDATE_MESSAGE,
    };
    use crate::http::RestClient;
    use crate::model::{
        ChatInputCommandInteraction, ComponentInteraction, CreateMessage, InteractionContextData,
        InteractionResponseState, Snowflake,
    };

    use super::{
        autocomplete_callback, deferred_message_callback, deferred_update_callback,
        message_callback, modal_callback, AutocompleteChoice, InteractionReplyData,
        InteractionResponder, InteractionResponseBuilder, MessageBuilder,
    };

    fn sample_context() -> InteractionContextData {
        InteractionContextData {
            id: Snowflake::from("1"),
            application_id: Snowflake::from("2"),
            token: "token".to_string(),
            ..InteractionContextData::default()
        }
    }

    /// Client that is never allowed to reach the network; every test using it
    /// asserts an error is returned before any HTTP request is attempted.
    fn offline_client() -> RestClient {
        RestClient::new("test-token", 2)
    }

    #[test]
    fn message_builder_sets_ephemeral_components_v2_flags() {
        let message = MessageBuilder::new()
            .components_v2(ComponentsV2Message::new())
            .ephemeral(true)
            .build();

        assert_eq!(message.flags, Some((1 << 15) | (1 << 6)));
    }

    #[test]
    fn message_builder_serializes_components_and_preserves_existing_flags() {
        let message = MessageBuilder::new()
            .content("hello")
            .components(vec![json!({"type": 1, "components": []})])
            .flags(1 << 3)
            .ephemeral(true)
            .build();

        let value = serde_json::to_value(message).unwrap();
        assert_eq!(value["content"], json!("hello"));
        assert_eq!(value["components"][0]["type"], json!(1));
        assert_eq!(value["flags"], json!((1 << 3) | (1 << 6)));
    }

    #[test]
    fn interaction_response_builder_wraps_message_payload() {
        let response =
            InteractionResponseBuilder::channel_message(MessageBuilder::new().content("hello"))
                .unwrap()
                .build();

        assert_eq!(response.kind, 4);
        assert_eq!(response.data.unwrap()["content"], "hello");
    }

    #[test]
    fn interaction_response_builder_serializes_deferred_channel_message_flags() {
        let public_response = InteractionResponseBuilder::deferred_channel_message(false).build();
        let ephemeral_response = InteractionResponseBuilder::deferred_channel_message(true).build();

        assert_eq!(
            public_response.kind,
            INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE
        );
        assert_eq!(public_response.data.unwrap()["flags"], json!(0));
        assert_eq!(
            ephemeral_response.kind,
            INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE
        );
        assert_eq!(ephemeral_response.data.unwrap()["flags"], json!(1 << 6));
    }

    #[test]
    fn interaction_response_builder_wraps_update_message_payload() {
        let response = InteractionResponseBuilder::update_message(
            MessageBuilder::new()
                .content("updated")
                .components_v2(ComponentsV2Message::new()),
        )
        .unwrap()
        .build();

        let data = response.data.unwrap();
        assert_eq!(response.kind, INTERACTION_RESPONSE_UPDATE_MESSAGE);
        assert_eq!(data["content"], json!("updated"));
        assert_eq!(data["components"], json!([]));
        assert_eq!(data["flags"], json!(MESSAGE_FLAG_IS_COMPONENTS_V2));
    }

    #[test]
    fn interaction_response_builder_wraps_modal_payload() {
        let response = InteractionResponseBuilder::modal(ModalBuilder::new("feedback", "Feedback"))
            .unwrap()
            .build();

        let data = response.data.unwrap();
        assert_eq!(response.kind, INTERACTION_RESPONSE_MODAL);
        assert_eq!(data["custom_id"], json!("feedback"));
        assert_eq!(data["title"], json!("Feedback"));
        assert_eq!(data["components"], json!([]));
    }

    #[test]
    fn reply_data_converts_from_str_string_builder_and_create_message() {
        let from_str = InteractionReplyData::from("hi");
        assert_eq!(from_str.content.as_deref(), Some("hi"));
        assert_eq!(from_str.flags, None);

        let from_string = InteractionReplyData::from(String::from("hello"));
        assert_eq!(from_string.content.as_deref(), Some("hello"));

        let from_builder =
            InteractionReplyData::from(MessageBuilder::new().content("built").ephemeral(true));
        assert_eq!(from_builder.content.as_deref(), Some("built"));
        assert_eq!(from_builder.flags, Some(1 << 6));

        let from_message = InteractionReplyData::from(CreateMessage {
            content: Some("full".to_string()),
            embeds: Some(vec![json!({"title": "t"})]),
            components: Some(vec![json!({"type": 1, "components": []})]),
            flags: Some(1 << 2),
            ..CreateMessage::default()
        });
        assert_eq!(from_message.content.as_deref(), Some("full"));
        assert_eq!(from_message.embeds.as_ref().unwrap().len(), 1);
        assert_eq!(from_message.components.as_ref().unwrap().len(), 1);
        assert_eq!(from_message.flags, Some(1 << 2));
    }

    #[test]
    fn reply_data_ephemeral_merges_with_existing_flags() {
        let data = InteractionReplyData::new().flags(1 << 2).ephemeral();
        assert_eq!(data.flags, Some((1 << 2) | (1 << 6)));

        let plain = InteractionReplyData::from("x").ephemeral();
        assert_eq!(plain.flags, Some(1 << 6));
    }

    #[test]
    fn message_callback_wraps_reply_and_update_payloads() {
        let reply = message_callback(
            INTERACTION_RESPONSE_CHANNEL_MESSAGE,
            InteractionReplyData::from("hi").ephemeral(),
        )
        .unwrap();
        assert_eq!(reply.kind, INTERACTION_RESPONSE_CHANNEL_MESSAGE);
        let data = reply.data.unwrap();
        assert_eq!(data["content"], json!("hi"));
        assert_eq!(data["flags"], json!(1 << 6));

        let update = message_callback(
            INTERACTION_RESPONSE_UPDATE_MESSAGE,
            InteractionReplyData::new().components(vec![json!({"type": 1, "components": []})]),
        )
        .unwrap();
        assert_eq!(update.kind, INTERACTION_RESPONSE_UPDATE_MESSAGE);
        assert_eq!(update.data.unwrap()["components"][0]["type"], json!(1));
    }

    #[test]
    fn deferred_and_modal_callbacks_use_expected_kinds() {
        let public = deferred_message_callback(false);
        assert_eq!(public.kind, INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE);
        assert_eq!(public.data.unwrap()["flags"], json!(0));

        let ephemeral = deferred_message_callback(true);
        assert_eq!(ephemeral.data.unwrap()["flags"], json!(1 << 6));

        let update = deferred_update_callback();
        assert_eq!(update.kind, INTERACTION_RESPONSE_DEFERRED_UPDATE_MESSAGE);
        assert!(update.data.is_none());

        let modal = modal_callback(ModalBuilder::new("feedback", "Feedback"));
        assert_eq!(modal.kind, INTERACTION_RESPONSE_MODAL);
        assert_eq!(modal.data.unwrap()["custom_id"], json!("feedback"));
    }

    #[test]
    fn autocomplete_callback_serializes_choices_and_skips_missing_localizations() {
        let mut localizations = HashMap::new();
        localizations.insert("de".to_string(), "Abrechnung".to_string());

        let response = autocomplete_callback(vec![
            AutocompleteChoice::new("billing", "billing-value"),
            AutocompleteChoice::new("priority", 2).name_localizations(localizations),
        ])
        .unwrap();

        assert_eq!(response.kind, INTERACTION_RESPONSE_AUTOCOMPLETE_RESULT);
        let data = response.data.unwrap();
        let choices = data["choices"].as_array().unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0]["name"], json!("billing"));
        assert_eq!(choices[0]["value"], json!("billing-value"));
        assert!(choices[0].get("name_localizations").is_none());
        assert_eq!(choices[1]["value"], json!(2));
        assert_eq!(choices[1]["name_localizations"]["de"], json!("Abrechnung"));
    }

    #[test]
    fn response_state_ladder_tracks_defer_edit_and_revert() {
        let context = sample_context();
        assert!(!context.is_acknowledged());
        assert!(!context.is_deferred());
        assert!(!context.is_replied());

        // Deferring claims the slot exactly once.
        context
            .response_state
            .acknowledge(InteractionResponseState::DEFERRED)
            .unwrap();
        assert!(context.is_acknowledged());
        assert!(context.is_deferred());
        assert!(!context.is_replied());

        let error = context
            .response_state
            .acknowledge(InteractionResponseState::REPLIED)
            .unwrap_err();
        assert!(error.to_string().contains("already acknowledged"));

        // A deferred interaction may be edited; editing promotes it to replied.
        context.response_state.require_acknowledged().unwrap();
        context.response_state.mark_replied();
        assert!(context.is_replied());
        assert!(!context.is_deferred());
    }

    #[test]
    fn response_state_revert_frees_the_slot_for_a_retry() {
        let state = InteractionResponseState::new();
        state
            .acknowledge(InteractionResponseState::REPLIED)
            .unwrap();
        state.revert(InteractionResponseState::REPLIED);
        assert!(!state.is_acknowledged());
        assert!(state.require_acknowledged().is_err());

        // Retrying after a rollback succeeds.
        state
            .acknowledge(InteractionResponseState::DEFERRED)
            .unwrap();
        assert!(state.is_deferred());

        // Revert only undoes the state it claimed, never a later one.
        state.revert(InteractionResponseState::REPLIED);
        assert!(state.is_deferred());
    }

    #[test]
    fn response_state_is_shared_between_clones() {
        let context = sample_context();
        let clone = context.clone();

        clone
            .response_state
            .acknowledge(InteractionResponseState::REPLIED)
            .unwrap();
        assert!(context.is_replied());
        assert!(context.is_acknowledged());
    }

    #[test]
    fn concurrent_acknowledgements_only_let_one_caller_win() {
        let state = InteractionResponseState::new();
        let winners: usize = std::thread::scope(|scope| {
            (0..8)
                .map(|_| {
                    let state = &state;
                    scope.spawn(move || {
                        usize::from(state.acknowledge(InteractionResponseState::REPLIED).is_ok())
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .sum()
        });

        assert_eq!(winners, 1);
        assert!(state.is_replied());
    }

    #[tokio::test]
    async fn initial_responses_error_after_acknowledgement_without_touching_http() {
        let http = offline_client();
        let context = sample_context();
        context
            .response_state
            .acknowledge(InteractionResponseState::DEFERRED)
            .unwrap();

        for error in [
            context.reply(&http, "hi").await.unwrap_err(),
            context.reply_ephemeral(&http, "hi").await.unwrap_err(),
            context.reply_with_result(&http, "hi").await.unwrap_err(),
            context.defer(&http).await.unwrap_err(),
            context.defer_ephemeral(&http).await.unwrap_err(),
            context.defer_update(&http).await.unwrap_err(),
            context.update_message(&http, "hi").await.unwrap_err(),
            context
                .show_modal(&http, ModalBuilder::new("m", "M"))
                .await
                .unwrap_err(),
            context
                .respond_autocomplete(&http, vec![AutocompleteChoice::new("a", "b")])
                .await
                .unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("already acknowledged"),
                "unexpected error: {error}"
            );
        }

        // The double acknowledgement must not clobber the original state.
        assert!(context.is_deferred());
    }

    #[tokio::test]
    async fn reply_dependent_methods_require_acknowledgement() {
        let http = offline_client();
        let context = sample_context();

        for error in [
            context.edit_reply(&http, "edited").await.unwrap_err(),
            context
                .follow_up(&http, "extra")
                .await
                .map(drop)
                .unwrap_err(),
            context
                .follow_up_ephemeral(&http, "extra")
                .await
                .map(drop)
                .unwrap_err(),
            context.delete_reply(&http).await.unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("has not been acknowledged"),
                "unexpected error: {error}"
            );
        }

        assert!(!context.is_acknowledged());
    }

    #[tokio::test]
    async fn responder_trait_delegates_to_the_shared_context() {
        let http = offline_client();
        let interaction = ChatInputCommandInteraction {
            context: sample_context(),
            ..ChatInputCommandInteraction::default()
        };

        assert!(!interaction.is_acknowledged());
        let error = interaction.follow_up(&http, "early").await.unwrap_err();
        assert!(error.to_string().contains("has not been acknowledged"));

        interaction
            .context
            .response_state
            .acknowledge(InteractionResponseState::REPLIED)
            .unwrap();
        assert!(interaction.is_replied());
        let error = interaction.reply(&http, "again").await.unwrap_err();
        assert!(error.to_string().contains("already acknowledged"));

        let component = ComponentInteraction {
            context: sample_context(),
            ..ComponentInteraction::default()
        };
        component
            .context
            .response_state
            .acknowledge(InteractionResponseState::REPLIED)
            .unwrap();
        let error = component.defer_update(&http).await.unwrap_err();
        assert!(error.to_string().contains("already acknowledged"));
        let error = component.update_message(&http, "new").await.unwrap_err();
        assert!(error.to_string().contains("already acknowledged"));
    }
}
