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

use crate::bitfield::MessageFlags;
use crate::builders::{create_container, ComponentsV2Message, ContainerBuilder, ModalBuilder};
use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
use crate::error::DiscordError;
use crate::http::DiscordHttpClient;
use crate::model::{
    ApplicationCommandOptionChoice, ChannelId, InteractionContextData, InteractionId, Message,
    MessageId,
};
use crate::request::{
    self, Component, CreateFollowup, EditInteractionResponse, InteractionMessage,
    InteractionResponse, UpdateMessage,
};
use crate::types::ButtonConfig;

fn components_v2_flags(ephemeral: bool) -> u64 {
    let mut flags = MESSAGE_FLAG_IS_COMPONENTS_V2;
    if ephemeral {
        flags |= crate::response::EPHEMERAL_FLAG;
    }
    flags
}

/// The flags of a Components V2 message.
fn components_v2_message_flags(ephemeral: bool) -> MessageFlags {
    MessageFlags::from_bits(components_v2_flags(ephemeral))
}

/// A Components V2 message callback: `UPDATE_MESSAGE` when `update`,
/// otherwise `CHANNEL_MESSAGE_WITH_SOURCE`.
fn components_v2_callback(
    update: bool,
    components: Vec<Component>,
    ephemeral: bool,
) -> InteractionResponse {
    let flags = components_v2_message_flags(ephemeral);
    if update {
        InteractionResponse::UpdateMessage(UpdateMessage::new().components(components).flags(flags))
    } else {
        InteractionResponse::ChannelMessage(
            InteractionMessage::new()
                .components(components)
                .flags(flags),
        )
    }
}

/// Sends a Components V2 message holding one container.
pub async fn send_container_message(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    container: ContainerBuilder,
) -> Result<Message, DiscordError> {
    let body = request::CreateMessage::new().components_v2(vec![container.build().into()]);
    http.create_message(channel_id, &body).await
}

/// Provides the `send_message` helper.
pub async fn send_message(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    body: &crate::request::CreateMessage,
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
) -> Result<Message, DiscordError> {
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
    let response = components_v2_callback(false, vec![container.build().into()], ephemeral);
    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

/// Sends an arbitrary initial response callback.
///
/// State-aware: delegates to [`InteractionContextData::create_response`], so
/// the callback claims and settles the interaction's shared response state.
pub async fn respond_to_interaction(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    response: InteractionResponse,
) -> Result<(), DiscordError> {
    context.create_response(http, &response).await
}

/// Replies (type 4) with a message, optionally ephemeral.
///
/// State-aware: delegates to [`InteractionContextData::reply`]. Fails before
/// any request when `message` sets channel-message-only fields (see
/// [`InteractionMessage`]'s `TryFrom<request::CreateMessage>`). Prefer
/// `reply`/`reply_ephemeral` on the interaction; this helper will be
/// deprecated once the bundled examples have moved off it.
pub async fn respond_with_message(
    http: &DiscordHttpClient,
    context: &InteractionContextData,
    message: request::CreateMessage,
    ephemeral: bool,
) -> Result<(), DiscordError> {
    let message = InteractionMessage::try_from(message)?;
    context
        .reply(
            http,
            if ephemeral {
                message.ephemeral(true)
            } else {
                message
            },
        )
        .await
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
    let response = components_v2_callback(false, vec![container.build().into()], ephemeral);
    http.create_interaction_response(interaction_id, interaction_token, &response)
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
    let response = components_v2_callback(false, vec![container.build().into()], ephemeral);
    http.create_interaction_response(interaction_id, interaction_token, &response)
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
) -> Result<Message, DiscordError> {
    let body = CreateFollowup::new()
        .components(vec![container.build().into()])
        .flags(components_v2_message_flags(ephemeral));
    http.create_followup_message(interaction_token, &body).await
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
    message: request::CreateMessage,
    ephemeral: bool,
) -> Result<Message, DiscordError> {
    let message = CreateFollowup::try_from(message)?;
    context
        .follow_up(
            http,
            if ephemeral {
                message.ephemeral(true)
            } else {
                message
            },
        )
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
    message: request::CreateMessage,
) -> Result<Message, DiscordError> {
    context
        .edit_reply(http, EditInteractionResponse::try_from(message)?)
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
    message: request::CreateMessage,
) -> Result<Message, DiscordError> {
    context
        .edit_follow_up(
            http,
            message_id,
            EditInteractionResponse::try_from(message)?,
        )
        .await
}

/// Replaces a message's components with one container and sets
/// `IS_COMPONENTS_V2`. Discord rejects the edit if the message still has
/// content or embeds; clear them with [`DiscordHttpClient::edit_message`]
/// and [`request::EditMessage::components_v2`] instead.
pub async fn edit_message_with_container(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    message_id: MessageId,
    container: ContainerBuilder,
) -> Result<Message, DiscordError> {
    let body = request::EditMessage::new().components_v2(vec![container.build().into()]);
    http.edit_message(channel_id, message_id, &body).await
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
    let response = components_v2_callback(true, vec![container.build().into()], false);
    http.create_interaction_response(interaction_id, interaction_token, &response)
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
    let response = InteractionResponse::Modal(modal.build());
    http.create_interaction_response(interaction_id, interaction_token, &response)
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
    message: request::CreateMessage,
) -> Result<(), DiscordError> {
    context
        .update_message(http, UpdateMessage::try_from(message)?)
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
        .create_response(http, &InteractionResponse::AutocompleteResult(choices))
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
        .create_response(http, &InteractionResponse::LaunchActivity)
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
) -> Result<Message, DiscordError> {
    http.create_interaction_response(
        interaction_id,
        interaction_token,
        &InteractionResponse::defer(ephemeral),
    )
    .await?;

    followup_with_container(http, interaction_token, container, ephemeral).await
}

/// Sends a Components V2 message.
pub async fn send_components_v2(
    http: &DiscordHttpClient,
    channel_id: ChannelId,
    message: ComponentsV2Message,
) -> Result<Message, DiscordError> {
    let body = request::CreateMessage::new().components_v2(message.build());
    http.create_message(channel_id, &body).await
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
    let response = components_v2_callback(false, message.build(), ephemeral);
    http.create_interaction_response(interaction_id, interaction_token, &response)
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
    let response = components_v2_callback(false, message.build(), ephemeral);
    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

// The deprecated helpers stay covered until they are removed.
#[cfg(test)]
#[expect(deprecated, reason = "covers the deprecated helpers")]
mod tests {
    use crate::model::{
        test_id, ApplicationCommandOptionChoice, InteractionContextData, InteractionResponseType,
    };
    use crate::request::{CreateMessage, InteractionResponse};
    use serde_json::json;

    use super::{
        components_v2_flags, defer_and_followup_container, defer_interaction,
        defer_update_interaction, delete_original_response, edit_original_response,
        followup_message, followup_with_container, launch_activity,
        respond_component_with_components_v2, respond_component_with_container,
        respond_modal_with_container, respond_to_interaction, respond_with_autocomplete_choices,
        respond_with_components_v2, respond_with_container, respond_with_message,
        respond_with_modal, respond_with_modal_typed, update_component_with_container,
        update_interaction_message,
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
        CreateMessage::new().content("hello").flags(4_u64)
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
    fn components_v2_bodies_serialize_components_and_flags() {
        let component = crate::request::Component::from(crate::request::TextDisplay::new("hello"));

        // The channel-message helpers' bodies (2.x sent this JSON as a Value).
        let standard = json!({
            "components": [{ "type": 10, "content": "hello" }],
            "flags": MESSAGE_FLAG_IS_COMPONENTS_V2,
        });
        let create = CreateMessage::new().components_v2(vec![component.clone()]);
        assert_eq!(serde_json::to_value(&create).unwrap(), standard);
        let edit = crate::request::EditMessage::new().components_v2(vec![component.clone()]);
        assert_eq!(serde_json::to_value(&edit).unwrap(), standard);

        let reply = serde_json::to_value(super::components_v2_callback(
            false,
            vec![component.clone()],
            true,
        ))
        .unwrap();
        assert_eq!(
            reply["data"]["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6))
        );
        let update =
            serde_json::to_value(super::components_v2_callback(true, vec![component], false))
                .unwrap();
        assert_eq!(update["type"], json!(7));
        assert_eq!(
            update["data"]["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2)
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
            respond_to_interaction(&http, &context, InteractionResponse::update("hello"))
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
            respond_to_interaction(&http, &context, InteractionResponse::DeferredUpdateMessage)
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
        let referencing = || sample_message().reply_to(test_id(1));

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

    /// The id-and-token helpers send typed callbacks and return the created
    /// message; the deprecated context helpers reach the same endpoints.
    #[tokio::test]
    async fn container_and_context_helpers_reach_discord() {
        use crate::response::tests::mock;

        const MESSAGE: &str = r#"{"id":"30","channel_id":"40","content":"ok"}"#;
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE),
            mock::Reply::Now(200, MESSAGE),
            mock::Reply::Now(200, MESSAGE),
            mock::Reply::Now(200, MESSAGE),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = DiscordHttpClient::builder("bot-token", Some(test_id(123)))
            .api_base(server.base_url.clone())
            .build()
            .unwrap();

        respond_with_components_v2(
            &http,
            test_id(111),
            "tok",
            sample_components_message(),
            true,
        )
        .await
        .unwrap();
        update_component_with_container(&http, test_id(111), "tok", sample_container())
            .await
            .unwrap();
        respond_with_modal(&http, test_id(111), "tok", sample_modal())
            .await
            .unwrap();
        let followup = followup_with_container(&http, "tok", sample_container(), true)
            .await
            .unwrap();
        assert_eq!(followup.id, test_id(30));
        let deferred =
            defer_and_followup_container(&http, test_id(111), "tok", sample_container(), false)
                .await
                .unwrap();
        assert_eq!(deferred.id, test_id(30));

        let context = acknowledged(sample_context("123", "tok"));
        super::get_original_response(&http, &context).await.unwrap();
        super::edit_followup_response(&http, &context, test_id(30), sample_message())
            .await
            .unwrap();
        edit_original_response(&http, &context, sample_message())
            .await
            .unwrap();
        super::delete_followup_response(&http, &context, test_id(30))
            .await
            .unwrap();

        let mut bodies = Vec::new();
        for expected in [
            "POST /interactions/111/tok/callback",
            "POST /interactions/111/tok/callback",
            "POST /interactions/111/tok/callback",
            "POST /webhooks/123/tok",
            "POST /interactions/111/tok/callback",
            "POST /webhooks/123/tok",
            "GET /webhooks/123/tok/messages/@original",
            "PATCH /webhooks/123/tok/messages/30",
            "PATCH /webhooks/123/tok/messages/@original",
            "DELETE /webhooks/123/tok/messages/30",
        ] {
            let (line, body) = server.next_request_with_body().await;
            assert_eq!(line, expected);
            bodies.push(body);
        }
        let json =
            |index: usize| -> serde_json::Value { serde_json::from_slice(&bodies[index]).unwrap() };
        let reply = json(0);
        assert_eq!(reply["type"], json!(4));
        assert_eq!(
            reply["data"]["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6))
        );
        let update = json(1);
        assert_eq!(update["type"], json!(7));
        assert_eq!(
            update["data"]["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2)
        );
        assert_eq!(json(2)["type"], json!(9));
        assert_eq!(
            json(3)["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6))
        );
        assert_eq!(json(4), json!({ "type": 5, "data": { "flags": 0 } }));
        assert_eq!(json(7)["content"], json!("hello"));
    }

    /// The channel-message helpers post the Components V2 body to the
    /// channel endpoints.
    #[tokio::test]
    async fn channel_message_helpers_reach_discord() {
        use crate::response::tests::mock;

        const MESSAGE: &str = r#"{"id":"30","channel_id":"40","content":"ok"}"#;
        let mut server =
            mock::spawn((0..5).map(|_| mock::Reply::Now(200, MESSAGE)).collect()).await;
        let http = DiscordHttpClient::builder("bot-token", Some(test_id(123)))
            .api_base(server.base_url.clone())
            .build()
            .unwrap();
        let channel = test_id(40);

        assert_eq!(
            super::send_container_message(&http, channel, sample_container())
                .await
                .unwrap()
                .id,
            test_id(30)
        );
        super::send_to_channel(&http, channel, "Title", "Body", Vec::new(), None)
            .await
            .unwrap();
        super::send_components_v2(&http, channel, sample_components_message())
            .await
            .unwrap();
        super::edit_message_with_container(&http, channel, test_id(30), sample_container())
            .await
            .unwrap();
        let sent = super::send_message(&http, channel, &CreateMessage::new().content("plain"))
            .await
            .unwrap();
        assert_eq!(sent.id, test_id(30));

        for expected in [
            "POST /channels/40/messages",
            "POST /channels/40/messages",
            "POST /channels/40/messages",
            "PATCH /channels/40/messages/30",
            "POST /channels/40/messages",
        ] {
            let (line, body) = server.next_request_with_body().await;
            assert_eq!(line, expected);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if expected.starts_with("PATCH") || body.get("components").is_some() {
                let keys = body.as_object().unwrap().keys().collect::<Vec<_>>();
                assert_eq!(keys, ["components", "flags"]);
                assert_eq!(body["flags"], json!(MESSAGE_FLAG_IS_COMPONENTS_V2));
            }
        }
    }
}
