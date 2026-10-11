//! Convenience helpers for channel messages, interaction responses, and
//! follow-ups.
//!
//! Interaction helpers come in two kinds:
//!
//! - **State-aware** helpers take an [`InteractionContextData`] and delegate
//!   to its response methods (the same ones [`InteractionResponder`] uses),
//!   so they claim and settle the interaction's shared response state. Most
//!   are deprecated in favour of calling those methods directly.
//! - **Stateless** helpers take only an interaction id and token. They cannot
//!   see the shared response state of a parsed interaction, so nothing stops
//!   a double response and a parsed interaction does not learn about what
//!   they sent. Use them with raw handlers ([`InteractionContext`]) or when
//!   only the id and token are available; with a parsed interaction prefer
//!   [`InteractionResponder`]. Mixing both kinds on one interaction is not
//!   coordinated.
//!
//! There is deliberately no global interaction registry that stateless
//! helpers could consult: it would need eviction rules and still could not
//! see responses sent by other processes.
//!
//! [`InteractionResponder`]: crate::response::InteractionResponder
//! [`InteractionContext`]: crate::parsers::InteractionContext

use serde_json::Value;

use crate::builders::{create_container, ComponentsV2Message, ContainerBuilder, ModalBuilder};
use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
use crate::error::DiscordError;
use crate::http::DiscordHttpClient;
use crate::model::{
    ApplicationCommandOptionChoice, ChannelId, CreateMessage, InteractionCallbackResponse,
    InteractionContextData, InteractionId, InteractionResponseType, Message, MessageId,
};
use crate::response::InteractionReplyData;
use crate::types::ButtonConfig;

fn components_v2_flags(ephemeral: bool) -> u64 {
    let mut flags = MESSAGE_FLAG_IS_COMPONENTS_V2;
    if ephemeral {
        flags |= crate::response::EPHEMERAL_FLAG;
    }
    flags
}

struct ComponentsV2Payload {
    components: Vec<Value>,
    ephemeral: bool,
}

impl ComponentsV2Payload {
    fn new(components: Vec<Value>) -> Self {
        Self {
            components,
            ephemeral: false,
        }
    }

    fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    fn into_value(self) -> Value {
        serde_json::json!({
            "components": self.components,
            "flags": components_v2_flags(self.ephemeral),
        })
    }
}

/// Provides the `send_container_message` helper.
pub async fn send_container_message(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    container: ContainerBuilder,
) -> Result<Value, DiscordError> {
    let body = ComponentsV2Payload::new(vec![container.build()]).into_value();
    http.send_message_json(channel_id, &body).await
}

/// Provides the `send_message` helper.
pub async fn send_message(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    body: &CreateMessage,
) -> Result<Message, DiscordError> {
    http.create_message(channel_id, body).await
}

/// Provides the `send_to_channel` helper.
pub async fn send_to_channel(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    title: &str,
    description: &str,
    buttons: Vec<ButtonConfig>,
    image_url: Option<&str>,
) -> Result<Value, DiscordError> {
    let container = create_container(title, description, buttons, image_url);
    send_container_message(http, channel_id, container).await
}

/// Replies (type 4) with a Components V2 container.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_with_container(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Sends an arbitrary initial response callback.
///
/// State-aware: delegates to [`InteractionContextData::create_response`], so
/// the callback claims and settles the interaction's shared response state.
pub async fn respond_to_interaction(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    response: InteractionCallbackResponse,
) -> Result<(), DiscordError> {
    context.create_response(http, &response).await
}

/// Replies (type 4) with a message, optionally ephemeral.
///
/// State-aware: delegates to [`InteractionContextData::reply`]. Fails before
/// any request when `message` sets channel-message-only fields (see
/// [`InteractionReplyData`]'s `TryFrom<CreateMessage>`). Prefer
/// `reply`/`reply_ephemeral` on the interaction; this helper will be
/// deprecated once the bundled examples have moved off it.
pub async fn respond_with_message(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message: CreateMessage,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    context.reply(http, reply_data(message, ephemeral)?).await
}

/// Converts a `CreateMessage` into reply data, adding the ephemeral flag.
fn reply_data(
    message: CreateMessage,
    ephemeral: bool,
) -> Result<InteractionReplyData, DiscordError> {
    let data = InteractionReplyData::try_from(message)?;
    Ok(if ephemeral { data.ephemeral() } else { data })
}

/// Replies (type 4) to a component interaction with a Components V2
/// container.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_component_with_container(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Replies (type 4) to a modal submission with a Components V2 container.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_modal_with_container(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Sends a Components V2 container as a follow-up, using the application id
/// configured on `http`.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder). Unlike
/// [`InteractionContextData::follow_up`] it does not check that the
/// interaction was acknowledged.
pub async fn followup_with_container(
    http: &DiscordHttpClient,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<Value, DiscordError> {
    let body = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    http.create_followup_message_json(interaction_token, &body)
        .await
}

/// Sends a follow-up message, optionally ephemeral.
///
/// State-aware: delegates to [`InteractionContextData::follow_up`], so it
/// waits for an in-flight initial response and fails locally when the
/// interaction is not acknowledged.
#[deprecated(
    since = "3.0.0",
    note = "use `follow_up`/`follow_up_ephemeral` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn followup_message(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message: CreateMessage,
    ephemeral: bool,
) -> Result<Message, DiscordError> {
    context
        .follow_up(http, reply_data(message, ephemeral)?)
        .await
}

/// Fetches the original response.
///
/// State-aware: delegates to [`InteractionContextData::fetch_reply`].
#[deprecated(
    since = "3.0.0",
    note = "use `fetch_reply` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn get_original_response(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
) -> Result<Message, DiscordError> {
    context.fetch_reply(http).await
}

/// Edits the original response.
///
/// State-aware: delegates to [`InteractionContextData::edit_reply`], so it
/// fails locally when the interaction is not acknowledged and promotes a
/// deferred response to replied.
#[deprecated(
    since = "3.0.0",
    note = "use `edit_reply` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn edit_original_response(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message: CreateMessage,
) -> Result<Message, DiscordError> {
    context
        .edit_reply(http, InteractionReplyData::try_from(message)?)
        .await
}

/// Deletes the original response.
///
/// State-aware: delegates to [`InteractionContextData::delete_reply`].
#[deprecated(
    since = "3.0.0",
    note = "use `delete_reply` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn delete_original_response(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
) -> Result<(), DiscordError> {
    context.delete_reply(http).await
}

/// Deletes a follow-up message.
///
/// State-aware: delegates to [`InteractionContextData::delete_follow_up`].
#[deprecated(
    since = "3.0.0",
    note = "use `delete_follow_up` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn delete_followup_response(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message_id: MessageId,
) -> Result<(), DiscordError> {
    context.delete_follow_up(http, message_id).await
}

/// Edits a follow-up message.
///
/// State-aware: delegates to [`InteractionContextData::edit_follow_up`].
#[deprecated(
    since = "3.0.0",
    note = "use `edit_follow_up` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn edit_followup_response(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message_id: MessageId,
    message: CreateMessage,
) -> Result<Message, DiscordError> {
    context
        .edit_follow_up(http, message_id, InteractionReplyData::try_from(message)?)
        .await
}

/// Provides the `edit_message_with_container` helper.
pub async fn edit_message_with_container(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    message_id: MessageId,
    container: ContainerBuilder,
) -> Result<Value, DiscordError> {
    let body = ComponentsV2Payload::new(vec![container.build()]).into_value();
    http.edit_message_json(channel_id, message_id, &body).await
}

/// Updates the component's message (type 7) with a Components V2 container.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn update_component_with_container(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    container: ContainerBuilder,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(vec![container.build()]).into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::UPDATE_MESSAGE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Opens a modal (type 9).
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_with_modal(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    modal: ModalBuilder,
) -> Result<(), DiscordError> {
    let response = serde_json::json!({
        "type": InteractionResponseType::MODAL,
        "data": modal.build(),
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Opens a modal (type 9).
///
/// State-aware: delegates to [`InteractionContextData::show_modal`].
#[deprecated(
    since = "3.0.0",
    note = "use `show_modal` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn respond_with_modal_typed(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    modal: ModalBuilder,
) -> Result<(), DiscordError> {
    context.show_modal(http, modal).await
}

/// Defers the reply (type 5), optionally with an ephemeral loading state.
///
/// State-aware: delegates to [`InteractionContextData::defer`] or
/// [`InteractionContextData::defer_ephemeral`].
#[deprecated(
    since = "3.0.0",
    note = "use `defer`/`defer_ephemeral` on the interaction (InteractionResponder) or InteractionContextData"
)]
pub async fn defer_interaction(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    if ephemeral {
        context.defer_ephemeral(http).await
    } else {
        context.defer(http).await
    }
}

/// Acknowledges a component or modal interaction without a visible
/// response (type 6).
///
/// State-aware: delegates to [`InteractionContextData::defer_update`].
#[deprecated(
    since = "3.0.0",
    note = "use `defer_update` on ComponentInteraction/ModalSubmitInteraction or InteractionContextData"
)]
pub async fn defer_update_interaction(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
) -> Result<(), DiscordError> {
    context.defer_update(http).await
}

/// Updates the component's message (type 7).
///
/// State-aware: delegates to [`InteractionContextData::update_message`].
#[deprecated(
    since = "3.0.0",
    note = "use `update_message` on ComponentInteraction/ModalSubmitInteraction or InteractionContextData"
)]
pub async fn update_interaction_message(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message: CreateMessage,
) -> Result<(), DiscordError> {
    context
        .update_message(http, InteractionReplyData::try_from(message)?)
        .await
}

/// Sends autocomplete choices (type 8).
///
/// State-aware: sends through [`InteractionContextData::create_response`].
#[deprecated(
    since = "3.0.0",
    note = "use `respond_autocomplete` on AutocompleteInteraction or InteractionContextData"
)]
pub async fn respond_with_autocomplete_choices(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    choices: Vec<ApplicationCommandOptionChoice>,
) -> Result<(), DiscordError> {
    context
        .create_response(
            http,
            &InteractionCallbackResponse {
                kind: InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT,
                data: Some(serde_json::json!({ "choices": choices })),
            },
        )
        .await
}

/// Launches the application's activity (type 12).
///
/// State-aware: sends through [`InteractionContextData::create_response`].
pub async fn launch_activity(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
) -> Result<(), DiscordError> {
    context
        .create_response(
            http,
            &InteractionCallbackResponse {
                kind: InteractionResponseType::LAUNCH_ACTIVITY,
                data: None,
            },
        )
        .await
}

/// Defers (type 5), then sends a Components V2 container as a follow-up.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn defer_and_followup_container(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<Value, DiscordError> {
    let mut flags: u64 = 0;
    if ephemeral {
        flags |= crate::response::EPHEMERAL_FLAG;
    }

    let defer_data = serde_json::json!({
        "type": InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
        "data": { "flags": flags },
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &defer_data)
        .await?;

    followup_with_container(http, interaction_token, container, ephemeral).await
}

/// Provides the `send_components_v2` helper.
pub async fn send_components_v2(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    message: ComponentsV2Message,
) -> Result<Value, DiscordError> {
    let body = ComponentsV2Payload::new(message.build()).into_value();
    http.send_message_json(channel_id, &body).await
}

/// Replies (type 4) with a Components V2 message.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_with_components_v2(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    message: ComponentsV2Message,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(message.build())
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

/// Replies (type 4) to a component interaction with a Components V2
/// message.
///
/// **Stateless:** takes only the interaction id and token, so it cannot see
/// or update the shared response state of a parsed interaction. With a parsed
/// interaction prefer [`InteractionResponder`](crate::response::InteractionResponder).
pub async fn respond_component_with_components_v2(
    http: &DiscordHttpClient,
    interaction_id: InteractionId,
    interaction_token: &str,
    message: ComponentsV2Message,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let data = ComponentsV2Payload::new(message.build())
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        "data": data,
    });

    http.create_interaction_response_json(interaction_id, interaction_token, &response)
        .await
}

// The deprecated helpers stay covered until they are removed.
#[cfg(test)]
#[expect(deprecated, reason = "covers the deprecated helpers")]
mod tests {
    use crate::model::{
        test_id, ApplicationCommandOptionChoice, CreateMessage, InteractionCallbackResponse,
        InteractionContextData, InteractionResponseType,
    };
    use serde_json::json;

    use super::{
        components_v2_flags, defer_and_followup_container, defer_interaction,
        defer_update_interaction, delete_original_response, edit_original_response,
        followup_message, followup_with_container, launch_activity,
        respond_component_with_components_v2, respond_component_with_container,
        respond_modal_with_container, respond_to_interaction, respond_with_autocomplete_choices,
        respond_with_components_v2, respond_with_container, respond_with_message,
        respond_with_modal, respond_with_modal_typed, update_component_with_container,
        update_interaction_message, ComponentsV2Payload,
    };
    use crate::builders::{
        ComponentsV2Message, ContainerBuilder, ModalBuilder, TextDisplayBuilder, TextInputBuilder,
    };
    use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
    use crate::error::DiscordError;
    use crate::http::DiscordHttpClient;
    use crate::response::AckKind;

    fn sample_http(application_id: u64) -> DiscordHttpClient {
        use crate::model::ApplicationId;

        DiscordHttpClient::new("test-token", ApplicationId::try_new(application_id).ok())
    }

    fn sample_context(application_id: &str, token: &str) -> InteractionContextData {
        InteractionContextData {
            id: test_id(111),
            application_id: application_id.parse().unwrap(),
            token: token.to_string(),
            ..InteractionContextData::fixture()
        }
    }

    /// Marks the context acknowledged, as a delivered initial response would.
    fn acknowledged(context: InteractionContextData) -> InteractionContextData {
        assert!(context.response_state.commit_inline(AckKind::Replied));
        context
    }

    fn sample_container() -> ContainerBuilder {
        ContainerBuilder::new().add_text_display(TextDisplayBuilder::new("hello helpers"))
    }

    fn sample_modal() -> ModalBuilder {
        ModalBuilder::new("helper-modal", "Helper Modal")
            .add_text_input(TextInputBuilder::short("name", "Name"))
    }

    fn sample_components_message() -> ComponentsV2Message {
        ComponentsV2Message::new().add_text_display(TextDisplayBuilder::new("hello helpers"))
    }

    fn sample_message() -> CreateMessage {
        CreateMessage {
            content: Some("hello".to_string()),
            flags: Some(8),
            ..CreateMessage::default()
        }
    }

    fn assert_model_error_contains(error: DiscordError, expected: &str) {
        match error {
            DiscordError::Model { message } => {
                assert!(
                    message.contains(expected),
                    "expected `{expected}` in `{message}`"
                );
            }
            other => panic!("expected model error containing `{expected}`, got {other:?}"),
        }
    }

    #[test]
    fn helper_constants_cover_new_callback_types() {
        assert_eq!(InteractionResponseType::DEFERRED_UPDATE_MESSAGE.get(), 6);
        assert_eq!(
            InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT.get(),
            8
        );
        assert_eq!(InteractionResponseType::LAUNCH_ACTIVITY.get(), 12);
    }

    #[test]
    fn components_v2_flags_only_include_ephemeral_when_requested() {
        assert_eq!(components_v2_flags(false), MESSAGE_FLAG_IS_COMPONENTS_V2);
        assert_eq!(
            components_v2_flags(true),
            MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6)
        );
    }

    #[test]
    fn components_v2_payload_serializes_components_and_flags() {
        let component = json!({ "type": 17, "content": "hello" });

        let standard = ComponentsV2Payload::new(vec![component.clone()]).into_value();
        assert_eq!(
            standard,
            json!({
                "components": [component.clone()],
                "flags": MESSAGE_FLAG_IS_COMPONENTS_V2,
            })
        );

        let ephemeral = ComponentsV2Payload::new(vec![component.clone()])
            .ephemeral(true)
            .into_value();
        assert_eq!(
            ephemeral,
            json!({
                "components": [component],
                "flags": MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6),
            })
        );
    }

    #[tokio::test]
    async fn json_interaction_helpers_validate_bad_tokens_before_network() {
        let http = sample_http(123);

        assert_model_error_contains(
            respond_with_container(&http, test_id(111), "bad/token", sample_container(), true)
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_component_with_container(
                &http,
                test_id(111),
                "bad/token",
                sample_container(),
                false,
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_modal_with_container(
                &http,
                test_id(111),
                "bad/token",
                sample_container(),
                true,
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            update_component_with_container(&http, test_id(111), "bad/token", sample_container())
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_with_modal(&http, test_id(111), "bad/token", sample_modal())
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_with_components_v2(
                &http,
                test_id(111),
                "bad/token",
                sample_components_message(),
                true,
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_component_with_components_v2(
                &http,
                test_id(111),
                "bad/token",
                sample_components_message(),
                false,
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            defer_and_followup_container(
                &http,
                test_id(111),
                "bad/token",
                sample_container(),
                true,
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
    }

    #[tokio::test]
    async fn typed_interaction_helpers_validate_bad_context_tokens_before_network() {
        let http = sample_http(123);
        let context = sample_context("456", "bad/token");

        assert_model_error_contains(
            respond_to_interaction(
                &http,
                &context,
                InteractionCallbackResponse {
                    kind: InteractionResponseType::UPDATE_MESSAGE,
                    data: Some(json!({ "content": "hello" })),
                },
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_with_message(&http, &context, sample_message(), true)
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_with_modal_typed(&http, &context, sample_modal())
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            defer_interaction(&http, &context, true).await.unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            defer_update_interaction(&http, &context).await.unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            update_interaction_message(&http, &context, sample_message())
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            respond_with_autocomplete_choices(
                &http,
                &context,
                vec![ApplicationCommandOptionChoice::new("One", "1")],
            )
            .await
            .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            launch_activity(&http, &context).await.unwrap_err(),
            "interaction_token",
        );
    }

    #[tokio::test]
    async fn followup_helpers_validate_application_id_and_path_segments_before_network() {
        let http_without_application_id = sample_http(0);
        let http = sample_http(123);
        let bad_token_context = acknowledged(sample_context("456", "bad/token"));

        assert_model_error_contains(
            followup_with_container(
                &http_without_application_id,
                "followup-token",
                sample_container(),
                true,
            )
            .await
            .unwrap_err(),
            "application_id must be set",
        );
        assert_model_error_contains(
            followup_message(&http, &bad_token_context, sample_message(), true)
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            edit_original_response(&http, &bad_token_context, sample_message())
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert_model_error_contains(
            delete_original_response(&http, &bad_token_context)
                .await
                .unwrap_err(),
            "interaction_token",
        );
    }

    #[tokio::test]
    async fn context_helpers_share_the_interaction_response_state() {
        let http = sample_http(123);

        // A locally rejected initial response rolls the shared state back.
        let context = sample_context("456", "bad/token");
        assert_model_error_contains(
            respond_with_message(&http, &context, sample_message(), false)
                .await
                .unwrap_err(),
            "interaction_token",
        );
        assert!(!context.is_acknowledged());

        // Follow-ups and original-response edits fail before acknowledgement,
        // exactly like the responder methods they delegate to.
        assert_model_error_contains(
            followup_message(&http, &context, sample_message(), true)
                .await
                .unwrap_err(),
            "has not been acknowledged",
        );
        assert_model_error_contains(
            edit_original_response(&http, &context, sample_message())
                .await
                .unwrap_err(),
            "has not been acknowledged",
        );

        // Once a clone acknowledged the interaction, every initial-response
        // helper refuses locally instead of sending a second response.
        let clone = context.clone();
        assert!(clone.response_state.commit_inline(AckKind::Deferred));
        for error in [
            respond_with_message(&http, &context, sample_message(), true)
                .await
                .unwrap_err(),
            respond_to_interaction(
                &http,
                &context,
                InteractionCallbackResponse {
                    kind: InteractionResponseType::UPDATE_MESSAGE,
                    data: None,
                },
            )
            .await
            .unwrap_err(),
            respond_with_modal_typed(&http, &context, sample_modal())
                .await
                .unwrap_err(),
            defer_interaction(&http, &context, false).await.unwrap_err(),
            defer_update_interaction(&http, &context).await.unwrap_err(),
            update_interaction_message(&http, &context, sample_message())
                .await
                .unwrap_err(),
            respond_with_autocomplete_choices(
                &http,
                &context,
                vec![ApplicationCommandOptionChoice::new("One", "1")],
            )
            .await
            .unwrap_err(),
            launch_activity(&http, &context).await.unwrap_err(),
        ] {
            assert_model_error_contains(error, "already acknowledged");
        }
        assert!(context.is_deferred());
    }

    #[tokio::test]
    async fn create_message_helpers_reject_channel_message_only_fields() {
        let http = sample_http(123);
        let context = acknowledged(sample_context("456", "followup-token"));
        let referencing = || CreateMessage {
            message_reference: Some(crate::model::MessageReference::default()),
            ..sample_message()
        };

        for error in [
            respond_with_message(&http, &sample_context("456", "t"), referencing(), false)
                .await
                .unwrap_err(),
            followup_message(&http, &context, referencing(), false)
                .await
                .map(drop)
                .unwrap_err(),
            edit_original_response(&http, &context, referencing())
                .await
                .map(drop)
                .unwrap_err(),
            super::edit_followup_response(&http, &context, test_id(30), referencing())
                .await
                .map(drop)
                .unwrap_err(),
            update_interaction_message(&http, &sample_context("456", "t"), referencing())
                .await
                .unwrap_err(),
        ] {
            assert_model_error_contains(error, "message_reference");
        }
    }
}
