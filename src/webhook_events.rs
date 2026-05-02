use std::collections::HashMap;

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{Application, Channel, Entitlement, Guild, Snowflake, User};
use crate::types::invalid_data_error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API object for `WebhookPayloadType`.
pub struct WebhookPayloadType(pub u8);

impl WebhookPayloadType {
    /// Public API item `PING`.
    pub const PING: Self = Self(0);
    /// Public API item `EVENT`.
    pub const EVENT: Self = Self(1);
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhookEventPayload`.
pub struct WebhookEventPayload {
    pub version: u8,
    pub application_id: Snowflake,
    pub kind: WebhookPayloadType,
    pub event: Option<WebhookEventBody>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhookEventBody`.
pub struct WebhookEventBody {
    pub kind: String,
    pub timestamp: String,
    pub event: WebhookEvent,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API enum for `WebhookEvent`.
pub enum WebhookEvent {
    /// Discord API enum variant `ApplicationAuthorized`.
    ApplicationAuthorized(ApplicationAuthorizedWebhookEvent),
    /// Discord API enum variant `ApplicationDeauthorized`.
    ApplicationDeauthorized(ApplicationDeauthorizedWebhookEvent),
    /// Discord API enum variant `EntitlementCreate`.
    EntitlementCreate(Entitlement),
    /// Discord API enum variant `EntitlementUpdate`.
    EntitlementUpdate(Entitlement),
    /// Discord API enum variant `EntitlementDelete`.
    EntitlementDelete(Entitlement),
    /// Discord API enum variant `QuestUserEnrollment`.
    QuestUserEnrollment(Value),
    /// Discord API enum variant `LobbyMessageCreate`.
    LobbyMessageCreate(WebhookSocialMessage),
    /// Discord API enum variant `LobbyMessageUpdate`.
    LobbyMessageUpdate(WebhookSocialMessage),
    /// Discord API enum variant `LobbyMessageDelete`.
    LobbyMessageDelete(WebhookDeletedMessage),
    /// Discord API enum variant `GameDirectMessageCreate`.
    GameDirectMessageCreate(WebhookSocialMessage),
    /// Discord API enum variant `GameDirectMessageUpdate`.
    GameDirectMessageUpdate(WebhookSocialMessage),
    /// Discord API enum variant `GameDirectMessageDelete`.
    GameDirectMessageDelete(WebhookSocialMessage),
    /// Discord API enum variant `Unknown`.
    Unknown {
        /// Raw webhook event type.
        kind: String,
        /// Raw event data for unsupported webhook events.
        data: Option<Value>,
    },
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ApplicationAuthorizedWebhookEvent`.
pub struct ApplicationAuthorizedWebhookEvent {
    pub integration_type: Option<u8>,
    pub user: User,
    pub scopes: Vec<String>,
    pub guild: Option<Guild>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ApplicationDeauthorizedWebhookEvent`.
pub struct ApplicationDeauthorizedWebhookEvent {
    pub user: User,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhookSocialMessage`.
pub struct WebhookSocialMessage {
    pub id: Snowflake,
    pub kind: Option<u8>,
    pub content: Option<String>,
    pub lobby_id: Option<Snowflake>,
    pub channel_id: Snowflake,
    pub author: Option<User>,
    pub metadata: Option<HashMap<String, String>>,
    pub flags: Option<u64>,
    pub application_id: Option<Snowflake>,
    pub timestamp: Option<String>,
    pub edited_timestamp: Option<String>,
    pub attachments: Vec<Value>,
    pub components: Vec<Value>,
    pub channel: Option<Channel>,
    pub recipient_id: Option<Snowflake>,
    pub activity: Option<Value>,
    pub application: Option<Application>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhookDeletedMessage`.
pub struct WebhookDeletedMessage {
    pub id: Snowflake,
    pub lobby_id: Option<Snowflake>,
    pub raw: Value,
}

/// Provides the `parse_webhook_event_payload` helper.
pub fn parse_webhook_event_payload(payload: Value) -> Result<WebhookEventPayload, DiscordError> {
    let version = required_u8(&payload, "version")?;
    let application_id = required_snowflake(&payload, "application_id")?;
    let kind = WebhookPayloadType(required_u8(&payload, "type")?);
    let event = if kind == WebhookPayloadType::PING {
        None
    } else {
        Some(parse_webhook_event_body(
            payload
                .get("event")
                .cloned()
                .ok_or_else(|| invalid_data_error("missing webhook event body"))?,
        )?)
    };

    Ok(WebhookEventPayload {
        version,
        application_id,
        kind,
        event,
        raw: payload,
    })
}

fn parse_webhook_event_body(body: Value) -> Result<WebhookEventBody, DiscordError> {
    let kind = required_string(&body, "type")?;
    let timestamp = required_string(&body, "timestamp")?;
    let data = body.get("data").cloned();
    let event = parse_webhook_event(&kind, data.clone())?;

    Ok(WebhookEventBody {
        kind,
        timestamp,
        event,
        raw: body,
    })
}

fn parse_webhook_event(kind: &str, data: Option<Value>) -> Result<WebhookEvent, DiscordError> {
    match kind {
        "APPLICATION_AUTHORIZED" => Ok(WebhookEvent::ApplicationAuthorized(
            parse_application_authorized(required_data(data, kind)?)?,
        )),
        "APPLICATION_DEAUTHORIZED" => Ok(WebhookEvent::ApplicationDeauthorized(
            parse_application_deauthorized(required_data(data, kind)?)?,
        )),
        "ENTITLEMENT_CREATE" => Ok(WebhookEvent::EntitlementCreate(from_data(data, kind)?)),
        "ENTITLEMENT_UPDATE" => Ok(WebhookEvent::EntitlementUpdate(from_data(data, kind)?)),
        "ENTITLEMENT_DELETE" => Ok(WebhookEvent::EntitlementDelete(from_data(data, kind)?)),
        "QUEST_USER_ENROLLMENT" => Ok(WebhookEvent::QuestUserEnrollment(
            data.unwrap_or(Value::Null),
        )),
        "LOBBY_MESSAGE_CREATE" => Ok(WebhookEvent::LobbyMessageCreate(parse_social_message(
            required_data(data, kind)?,
        )?)),
        "LOBBY_MESSAGE_UPDATE" => Ok(WebhookEvent::LobbyMessageUpdate(parse_social_message(
            required_data(data, kind)?,
        )?)),
        "LOBBY_MESSAGE_DELETE" => Ok(WebhookEvent::LobbyMessageDelete(parse_deleted_message(
            required_data(data, kind)?,
        )?)),
        "GAME_DIRECT_MESSAGE_CREATE" => Ok(WebhookEvent::GameDirectMessageCreate(
            parse_social_message(required_data(data, kind)?)?,
        )),
        "GAME_DIRECT_MESSAGE_UPDATE" => Ok(WebhookEvent::GameDirectMessageUpdate(
            parse_social_message(required_data(data, kind)?)?,
        )),
        "GAME_DIRECT_MESSAGE_DELETE" => Ok(WebhookEvent::GameDirectMessageDelete(
            parse_social_message(required_data(data, kind)?)?,
        )),
        _ => Ok(WebhookEvent::Unknown {
            kind: kind.to_string(),
            data,
        }),
    }
}

fn parse_application_authorized(
    data: Value,
) -> Result<ApplicationAuthorizedWebhookEvent, DiscordError> {
    let integration_type = optional_u8(&data, "integration_type")?;
    let user = from_field(&data, "user")?;
    let scopes = data
        .get("scopes")
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .map(|scope| {
                    scope
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| invalid_data_error("invalid webhook scopes entry"))
                })
                .collect::<Result<Vec<_>, DiscordError>>()
        })
        .transpose()?
        .unwrap_or_default();
    let guild = optional_from_field(&data, "guild")?;

    Ok(ApplicationAuthorizedWebhookEvent {
        integration_type,
        user,
        scopes,
        guild,
        raw: data,
    })
}

fn parse_application_deauthorized(
    data: Value,
) -> Result<ApplicationDeauthorizedWebhookEvent, DiscordError> {
    Ok(ApplicationDeauthorizedWebhookEvent {
        user: from_field(&data, "user")?,
        raw: data,
    })
}

fn parse_social_message(data: Value) -> Result<WebhookSocialMessage, DiscordError> {
    Ok(WebhookSocialMessage {
        id: required_snowflake(&data, "id")?,
        kind: optional_u8(&data, "type")?,
        content: optional_string(&data, "content")?,
        lobby_id: optional_snowflake(&data, "lobby_id")?,
        channel_id: required_snowflake(&data, "channel_id")?,
        author: optional_from_field(&data, "author")?,
        metadata: optional_from_field(&data, "metadata")?,
        flags: optional_u64(&data, "flags")?,
        application_id: optional_snowflake(&data, "application_id")?,
        timestamp: optional_string(&data, "timestamp")?,
        edited_timestamp: optional_string(&data, "edited_timestamp")?,
        attachments: optional_value_array(&data, "attachments")?,
        components: optional_value_array(&data, "components")?,
        channel: optional_from_field(&data, "channel")?,
        recipient_id: optional_snowflake(&data, "recipient_id")?,
        activity: data.get("activity").cloned(),
        application: optional_from_field(&data, "application")?,
        raw: data,
    })
}

fn parse_deleted_message(data: Value) -> Result<WebhookDeletedMessage, DiscordError> {
    Ok(WebhookDeletedMessage {
        id: required_snowflake(&data, "id")?,
        lobby_id: optional_snowflake(&data, "lobby_id")?,
        raw: data,
    })
}

fn required_data(data: Option<Value>, kind: &str) -> Result<Value, DiscordError> {
    data.ok_or_else(|| invalid_data_error(format!("missing webhook data for {kind}")))
}

fn from_data<T>(data: Option<Value>, kind: &str) -> Result<T, DiscordError>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_value(required_data(data, kind)?)?)
}

fn from_field<T>(data: &Value, field: &str) -> Result<T, DiscordError>
where
    T: serde::de::DeserializeOwned,
{
    let value = data
        .get(field)
        .cloned()
        .ok_or_else(|| invalid_data_error(format!("missing webhook field {field}")))?;
    Ok(serde_json::from_value(value)?)
}

fn optional_from_field<T>(data: &Value, field: &str) -> Result<Option<T>, DiscordError>
where
    T: serde::de::DeserializeOwned,
{
    match data.get(field) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
    }
}

fn required_snowflake(data: &Value, field: &str) -> Result<Snowflake, DiscordError> {
    from_field(data, field)
}

fn optional_snowflake(data: &Value, field: &str) -> Result<Option<Snowflake>, DiscordError> {
    optional_from_field(data, field)
}

fn required_string(data: &Value, field: &str) -> Result<String, DiscordError> {
    data.get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| invalid_data_error(format!("missing or invalid webhook field {field}")))
}

fn optional_string(data: &Value, field: &str) -> Result<Option<String>, DiscordError> {
    match data.get(field) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| invalid_data_error(format!("invalid webhook field {field}"))),
    }
}

fn required_u8(data: &Value, field: &str) -> Result<u8, DiscordError> {
    let value = optional_u64(data, field)?
        .ok_or_else(|| invalid_data_error(format!("missing webhook field {field}")))?;
    u8::try_from(value).map_err(|_| invalid_data_error(format!("webhook field {field} exceeds u8")))
}

fn optional_u8(data: &Value, field: &str) -> Result<Option<u8>, DiscordError> {
    optional_u64(data, field)?
        .map(u8::try_from)
        .transpose()
        .map_err(|_| invalid_data_error(format!("webhook field {field} exceeds u8")))
}

fn optional_u64(data: &Value, field: &str) -> Result<Option<u64>, DiscordError> {
    match data.get(field) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid_data_error(format!("invalid webhook field {field}"))),
    }
}

fn optional_value_array(data: &Value, field: &str) -> Result<Vec<Value>, DiscordError> {
    match data.get(field) {
        Some(Value::Null) | None => Ok(Vec::new()),
        Some(Value::Array(values)) => Ok(values.clone()),
        Some(_) => Err(invalid_data_error(format!("invalid webhook field {field}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user_payload(id: &str, username: &str) -> Value {
        json!({
            "id": id,
            "username": username,
            "discriminator": "0000"
        })
    }

    fn entitlement_payload() -> Value {
        json!({
            "id": "10",
            "sku_id": "20",
            "application_id": "30",
            "user_id": "40",
            "type": 4,
            "deleted": false,
            "consumed": false
        })
    }

    #[test]
    fn parses_ping_and_authorization_webhook_events() {
        let ping = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "123",
            "type": 0
        }))
        .unwrap();
        assert_eq!(ping.kind, WebhookPayloadType::PING);
        assert!(ping.event.is_none());

        let authorized = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "123",
            "type": 1,
            "event": {
                "type": "APPLICATION_AUTHORIZED",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": {
                    "integration_type": 1,
                    "scopes": ["applications.commands"],
                    "user": user_payload("42", "installer")
                }
            }
        }))
        .unwrap();

        let event = authorized.event.unwrap();
        assert_eq!(event.kind, "APPLICATION_AUTHORIZED");
        match event.event {
            WebhookEvent::ApplicationAuthorized(data) => {
                assert_eq!(data.integration_type, Some(1));
                assert_eq!(data.user.username, "installer");
                assert_eq!(data.scopes, vec!["applications.commands".to_string()]);
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }
    }

    #[test]
    fn parses_entitlement_and_social_message_webhook_events() {
        let entitlement = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "ENTITLEMENT_CREATE",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": entitlement_payload()
            }
        }))
        .unwrap();
        match entitlement.event.unwrap().event {
            WebhookEvent::EntitlementCreate(entitlement) => {
                assert_eq!(entitlement.id.as_str(), "10");
                assert_eq!(entitlement.user_id.unwrap().as_str(), "40");
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }

        let lobby_message = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "LOBBY_MESSAGE_CREATE",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": {
                    "id": "50",
                    "type": 0,
                    "content": "welcome",
                    "lobby_id": "60",
                    "channel_id": "60",
                    "author": user_payload("42", "sender"),
                    "metadata": { "mood": "ready" },
                    "flags": 65536,
                    "application_id": "30"
                }
            }
        }))
        .unwrap();
        match lobby_message.event.unwrap().event {
            WebhookEvent::LobbyMessageCreate(message) => {
                assert_eq!(message.id.as_str(), "50");
                assert_eq!(message.lobby_id.unwrap().as_str(), "60");
                assert_eq!(
                    message.metadata.unwrap().get("mood").map(String::as_str),
                    Some("ready")
                );
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }
    }

    #[test]
    fn parses_delete_and_preserves_unknown_webhook_events() {
        let deleted = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "LOBBY_MESSAGE_DELETE",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": {
                    "id": "70",
                    "lobby_id": "60"
                }
            }
        }))
        .unwrap();
        match deleted.event.unwrap().event {
            WebhookEvent::LobbyMessageDelete(message) => {
                assert_eq!(message.id.as_str(), "70");
                assert_eq!(message.lobby_id.unwrap().as_str(), "60");
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }

        let unknown = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "NEW_EVENT",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": { "id": "80" }
            }
        }))
        .unwrap();
        match unknown.event.unwrap().event {
            WebhookEvent::Unknown { kind, data } => {
                assert_eq!(kind, "NEW_EVENT");
                assert_eq!(data.unwrap()["id"], json!("80"));
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }
    }

    #[test]
    fn parses_deauthorization_entitlement_updates_and_game_dm_events() {
        let deauthorized = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "APPLICATION_DEAUTHORIZED",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": {
                    "user": user_payload("42", "leaver")
                }
            }
        }))
        .unwrap();
        match deauthorized.event.unwrap().event {
            WebhookEvent::ApplicationDeauthorized(data) => {
                assert_eq!(data.user.username, "leaver");
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }

        for event_type in ["ENTITLEMENT_UPDATE", "ENTITLEMENT_DELETE"] {
            let parsed = parse_webhook_event_payload(json!({
                "version": 1,
                "application_id": "30",
                "type": 1,
                "event": {
                    "type": event_type,
                    "timestamp": "2026-05-01T00:00:00.000000+00:00",
                    "data": entitlement_payload()
                }
            }))
            .unwrap();
            match parsed.event.unwrap().event {
                WebhookEvent::EntitlementUpdate(entitlement)
                | WebhookEvent::EntitlementDelete(entitlement) => {
                    assert_eq!(entitlement.sku_id.as_str(), "20");
                }
                other => panic!("unexpected webhook event: {other:?}"),
            }
        }

        for event_type in [
            "GAME_DIRECT_MESSAGE_CREATE",
            "GAME_DIRECT_MESSAGE_UPDATE",
            "GAME_DIRECT_MESSAGE_DELETE",
        ] {
            let parsed = parse_webhook_event_payload(json!({
                "version": 1,
                "application_id": "30",
                "type": 1,
                "event": {
                    "type": event_type,
                    "timestamp": "2026-05-01T00:00:00.000000+00:00",
                    "data": {
                        "id": "90",
                        "type": 0,
                        "content": "ready",
                        "channel_id": "91",
                        "author": user_payload("42", "friend"),
                        "recipient_id": "92",
                        "attachments": [],
                        "components": []
                    }
                }
            }))
            .unwrap();
            match parsed.event.unwrap().event {
                WebhookEvent::GameDirectMessageCreate(message)
                | WebhookEvent::GameDirectMessageUpdate(message)
                | WebhookEvent::GameDirectMessageDelete(message) => {
                    assert_eq!(message.channel_id.as_str(), "91");
                    assert_eq!(message.recipient_id.unwrap().as_str(), "92");
                }
                other => panic!("unexpected webhook event: {other:?}"),
            }
        }
    }

    #[test]
    fn handles_quest_event_and_reports_invalid_webhook_shapes() {
        let quest = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "QUEST_USER_ENROLLMENT",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": { "quest_id": "q" }
            }
        }))
        .unwrap();
        match quest.event.unwrap().event {
            WebhookEvent::QuestUserEnrollment(data) => {
                assert_eq!(data["quest_id"], json!("q"));
            }
            other => panic!("unexpected webhook event: {other:?}"),
        }

        let missing_event = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1
        }))
        .unwrap_err();
        assert!(missing_event
            .to_string()
            .contains("missing webhook event body"));

        let bad_type = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 300,
            "event": {
                "type": "NEW_EVENT",
                "timestamp": "2026-05-01T00:00:00.000000+00:00"
            }
        }))
        .unwrap_err();
        assert!(bad_type
            .to_string()
            .contains("webhook field type exceeds u8"));

        let bad_message = parse_webhook_event_payload(json!({
            "version": 1,
            "application_id": "30",
            "type": 1,
            "event": {
                "type": "LOBBY_MESSAGE_CREATE",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "data": {
                    "id": "50",
                    "type": "wrong",
                    "channel_id": "60"
                }
            }
        }))
        .unwrap_err();
        assert!(bad_message
            .to_string()
            .contains("invalid webhook field type"));
    }
}
