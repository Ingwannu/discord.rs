use serde_json::Value;

use crate::builders::{create_container, ComponentsV2Message, ContainerBuilder, ModalBuilder};
use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
use crate::http::DiscordHttpClient;
use crate::types::{ButtonConfig, Error};

pub const INTERACTION_RESPONSE_CHANNEL_MESSAGE: u8 = 4;
pub const INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE: u8 = 5;
pub const INTERACTION_RESPONSE_DEFERRED_UPDATE_MESSAGE: u8 = 6;
pub const INTERACTION_RESPONSE_UPDATE_MESSAGE: u8 = 7;
pub const INTERACTION_RESPONSE_MODAL: u8 = 9;

fn components_v2_flags(ephemeral: bool) -> u64 {
    let mut flags = MESSAGE_FLAG_IS_COMPONENTS_V2;
    if ephemeral {
        flags |= 1 << 6;
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

pub async fn send_container_message(
    http: &DiscordHttpClient,
    channel_id: u64,
    container: ContainerBuilder,
) -> Result<Value, Error> {
    let body = ComponentsV2Payload::new(vec![container.build()]).into_value();
    http.send_message(channel_id, &body).await
}

pub async fn send_to_channel(
    http: &DiscordHttpClient,
    channel_id: u64,
    title: &str,
    description: &str,
    buttons: Vec<ButtonConfig>,
    image_url: Option<&str>,
) -> Result<Value, Error> {
    let container = create_container(title, description, buttons, image_url);
    send_container_message(http, channel_id, container).await
}

pub async fn respond_with_container(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn respond_component_with_container(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn respond_modal_with_container(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn followup_with_container(
    http: &DiscordHttpClient,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<Value, Error> {
    let body = ComponentsV2Payload::new(vec![container.build()])
        .ephemeral(ephemeral)
        .into_value();
    http.create_followup_message(interaction_token, &body).await
}

pub async fn edit_message_with_container(
    http: &DiscordHttpClient,
    channel_id: u64,
    message_id: u64,
    container: ContainerBuilder,
) -> Result<Value, Error> {
    let body = ComponentsV2Payload::new(vec![container.build()]).into_value();
    http.edit_message(channel_id, message_id, &body).await
}

pub async fn update_component_with_container(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    container: ContainerBuilder,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(vec![container.build()]).into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_UPDATE_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn respond_with_modal(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    modal: ModalBuilder,
) -> Result<(), Error> {
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_MODAL,
        "data": modal.build(),
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn defer_and_followup_container(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    container: ContainerBuilder,
    ephemeral: bool,
) -> Result<Value, Error> {
    let mut flags: u64 = 0;
    if ephemeral {
        flags |= 1 << 6;
    }

    let defer_data = serde_json::json!({
        "type": INTERACTION_RESPONSE_DEFERRED_CHANNEL_MESSAGE,
        "data": { "flags": flags },
    });

    http.create_interaction_response(interaction_id, interaction_token, &defer_data)
        .await?;

    followup_with_container(http, interaction_token, container, ephemeral).await
}

pub async fn send_components_v2(
    http: &DiscordHttpClient,
    channel_id: u64,
    message: ComponentsV2Message,
) -> Result<Value, Error> {
    let body = ComponentsV2Payload::new(message.build()).into_value();
    http.send_message(channel_id, &body).await
}

pub async fn respond_with_components_v2(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    message: ComponentsV2Message,
    ephemeral: bool,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(message.build())
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}

pub async fn respond_component_with_components_v2(
    http: &DiscordHttpClient,
    interaction_id: &str,
    interaction_token: &str,
    message: ComponentsV2Message,
    ephemeral: bool,
) -> Result<(), Error> {
    let data = ComponentsV2Payload::new(message.build())
        .ephemeral(ephemeral)
        .into_value();
    let response = serde_json::json!({
        "type": INTERACTION_RESPONSE_CHANNEL_MESSAGE,
        "data": data,
    });

    http.create_interaction_response(interaction_id, interaction_token, &response)
        .await
}
