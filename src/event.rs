use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    Channel, Guild, Interaction, Member, Message, Role, Snowflake, User, VoiceServerUpdate,
    VoiceState,
};
use crate::parsers::parse_interaction;
use crate::types::Emoji;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadyApplication {
    pub id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadyPayload {
    pub user: User,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<ReadyApplication>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_gateway_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ReadyEvent {
    pub data: ReadyPayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct GuildEvent {
    pub guild: Guild,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuildDeletePayload {
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct GuildDeleteEvent {
    pub data: GuildDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct ChannelEvent {
    pub channel: Channel,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct MemberEvent {
    pub guild_id: Snowflake,
    pub member: Member,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberRemovePayload {
    pub guild_id: Snowflake,
    pub user: User,
}

#[derive(Clone, Debug)]
pub struct MemberRemoveEvent {
    pub data: MemberRemovePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct RoleEvent {
    pub guild_id: Snowflake,
    pub role: Role,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoleDeletePayload {
    pub guild_id: Snowflake,
    pub role_id: Snowflake,
}

#[derive(Clone, Debug)]
pub struct RoleDeleteEvent {
    pub data: RoleDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct MessageEvent {
    pub message: Message,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageDeletePayload {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
}

#[derive(Clone, Debug)]
pub struct MessageDeleteEvent {
    pub data: MessageDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct InteractionEvent {
    pub interaction: Interaction,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct VoiceStateEvent {
    pub state: VoiceState,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct VoiceServerEvent {
    pub data: VoiceServerUpdate,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct BulkMessageDeleteEvent {
    pub ids: Vec<Snowflake>,
    pub channel_id: Snowflake,
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct ChannelPinsUpdateEvent {
    pub channel_id: Snowflake,
    pub guild_id: Option<Snowflake>,
    pub last_pin_timestamp: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct GuildBanEvent {
    pub guild_id: Snowflake,
    pub user: User,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct GuildEmojisUpdateEvent {
    pub guild_id: Snowflake,
    pub emojis: Vec<Emoji>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct InviteEvent {
    pub guild_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub code: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct ReactionEvent {
    pub user_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub emoji: Option<Emoji>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct ReactionRemoveAllEvent {
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct TypingStartEvent {
    pub channel_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub user_id: Option<Snowflake>,
    pub timestamp: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct PresenceUpdateEvent {
    pub user_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub status: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct WebhooksUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub struct GuildIntegrationsUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
pub enum Event {
    Ready(ReadyEvent),
    GuildCreate(GuildEvent),
    GuildUpdate(GuildEvent),
    GuildDelete(GuildDeleteEvent),
    ChannelCreate(ChannelEvent),
    ChannelUpdate(ChannelEvent),
    ChannelDelete(ChannelEvent),
    MemberAdd(MemberEvent),
    MemberUpdate(MemberEvent),
    MemberRemove(MemberRemoveEvent),
    RoleCreate(RoleEvent),
    RoleUpdate(RoleEvent),
    RoleDelete(RoleDeleteEvent),
    MessageCreate(MessageEvent),
    MessageUpdate(MessageEvent),
    MessageDelete(MessageDeleteEvent),
    MessageDeleteBulk(BulkMessageDeleteEvent),
    ChannelPinsUpdate(ChannelPinsUpdateEvent),
    GuildBanAdd(GuildBanEvent),
    GuildBanRemove(GuildBanEvent),
    GuildEmojisUpdate(GuildEmojisUpdateEvent),
    GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent),
    WebhooksUpdate(WebhooksUpdateEvent),
    InviteCreate(InviteEvent),
    InviteDelete(InviteEvent),
    MessageReactionAdd(ReactionEvent),
    MessageReactionRemove(ReactionEvent),
    MessageReactionRemoveAll(ReactionRemoveAllEvent),
    TypingStart(TypingStartEvent),
    PresenceUpdate(PresenceUpdateEvent),
    InteractionCreate(InteractionEvent),
    VoiceStateUpdate(VoiceStateEvent),
    VoiceServerUpdate(VoiceServerEvent),
    Unknown { kind: String, raw: Value },
}

impl Event {
    pub fn kind(&self) -> &str {
        match self {
            Event::Ready(_) => "READY",
            Event::GuildCreate(_) => "GUILD_CREATE",
            Event::GuildUpdate(_) => "GUILD_UPDATE",
            Event::GuildDelete(_) => "GUILD_DELETE",
            Event::ChannelCreate(_) => "CHANNEL_CREATE",
            Event::ChannelUpdate(_) => "CHANNEL_UPDATE",
            Event::ChannelDelete(_) => "CHANNEL_DELETE",
            Event::MemberAdd(_) => "GUILD_MEMBER_ADD",
            Event::MemberUpdate(_) => "GUILD_MEMBER_UPDATE",
            Event::MemberRemove(_) => "GUILD_MEMBER_REMOVE",
            Event::RoleCreate(_) => "GUILD_ROLE_CREATE",
            Event::RoleUpdate(_) => "GUILD_ROLE_UPDATE",
            Event::RoleDelete(_) => "GUILD_ROLE_DELETE",
            Event::MessageCreate(_) => "MESSAGE_CREATE",
            Event::MessageUpdate(_) => "MESSAGE_UPDATE",
            Event::MessageDelete(_) => "MESSAGE_DELETE",
            Event::MessageDeleteBulk(_) => "MESSAGE_DELETE_BULK",
            Event::ChannelPinsUpdate(_) => "CHANNEL_PINS_UPDATE",
            Event::GuildBanAdd(_) => "GUILD_BAN_ADD",
            Event::GuildBanRemove(_) => "GUILD_BAN_REMOVE",
            Event::GuildEmojisUpdate(_) => "GUILD_EMOJIS_UPDATE",
            Event::GuildIntegrationsUpdate(_) => "GUILD_INTEGRATIONS_UPDATE",
            Event::WebhooksUpdate(_) => "WEBHOOKS_UPDATE",
            Event::InviteCreate(_) => "INVITE_CREATE",
            Event::InviteDelete(_) => "INVITE_DELETE",
            Event::MessageReactionAdd(_) => "MESSAGE_REACTION_ADD",
            Event::MessageReactionRemove(_) => "MESSAGE_REACTION_REMOVE",
            Event::MessageReactionRemoveAll(_) => "MESSAGE_REACTION_REMOVE_ALL",
            Event::TypingStart(_) => "TYPING_START",
            Event::PresenceUpdate(_) => "PRESENCE_UPDATE",
            Event::InteractionCreate(_) => "INTERACTION_CREATE",
            Event::VoiceStateUpdate(_) => "VOICE_STATE_UPDATE",
            Event::VoiceServerUpdate(_) => "VOICE_SERVER_UPDATE",
            Event::Unknown { kind, .. } => kind.as_str(),
        }
    }

    pub fn raw(&self) -> &Value {
        match self {
            Event::Ready(event) => &event.raw,
            Event::GuildCreate(event) | Event::GuildUpdate(event) => &event.raw,
            Event::GuildDelete(event) => &event.raw,
            Event::ChannelCreate(event)
            | Event::ChannelUpdate(event)
            | Event::ChannelDelete(event) => &event.raw,
            Event::MemberAdd(event) | Event::MemberUpdate(event) => &event.raw,
            Event::MemberRemove(event) => &event.raw,
            Event::RoleCreate(event) | Event::RoleUpdate(event) => &event.raw,
            Event::RoleDelete(event) => &event.raw,
            Event::MessageCreate(event) | Event::MessageUpdate(event) => &event.raw,
            Event::MessageDelete(event) => &event.raw,
            Event::MessageDeleteBulk(event) => &event.raw,
            Event::ChannelPinsUpdate(event) => &event.raw,
            Event::GuildBanAdd(event) | Event::GuildBanRemove(event) => &event.raw,
            Event::GuildEmojisUpdate(event) => &event.raw,
            Event::GuildIntegrationsUpdate(event) => &event.raw,
            Event::WebhooksUpdate(event) => &event.raw,
            Event::InviteCreate(event) | Event::InviteDelete(event) => &event.raw,
            Event::MessageReactionAdd(event) | Event::MessageReactionRemove(event) => &event.raw,
            Event::MessageReactionRemoveAll(event) => &event.raw,
            Event::TypingStart(event) => &event.raw,
            Event::PresenceUpdate(event) => &event.raw,
            Event::InteractionCreate(event) => &event.raw,
            Event::VoiceStateUpdate(event) => &event.raw,
            Event::VoiceServerUpdate(event) => &event.raw,
            Event::Unknown { raw, .. } => raw,
        }
    }
}

pub fn decode_event(event_name: &str, data: Value) -> Result<Event, DiscordError> {
    let event = match event_name {
        "READY" => Event::Ready(ReadyEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_CREATE" => Event::GuildCreate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_UPDATE" => Event::GuildUpdate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_DELETE" => Event::GuildDelete(GuildDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_CREATE" => Event::ChannelCreate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_UPDATE" => Event::ChannelUpdate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_DELETE" => Event::ChannelDelete(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_ADD" => Event::MemberAdd(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_UPDATE" => Event::MemberUpdate(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_REMOVE" => Event::MemberRemove(MemberRemoveEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_ROLE_CREATE" => Event::RoleCreate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_UPDATE" => Event::RoleUpdate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_DELETE" => Event::RoleDelete(RoleDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_CREATE" => Event::MessageCreate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_UPDATE" => Event::MessageUpdate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE" => Event::MessageDelete(MessageDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE_BULK" => {
            let ids: Vec<Snowflake> =
                serde_json::from_value(data.get("ids").cloned().unwrap_or(Value::Null))?;
            Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                channel_id: read_required_snowflake(&data, "channel_id")?,
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                ids,
                raw: data,
            })
        }
        "CHANNEL_PINS_UPDATE" => Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
            channel_id: read_required_snowflake(&data, "channel_id")?,
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            last_pin_timestamp: data
                .get("last_pin_timestamp")
                .and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "GUILD_BAN_ADD" => Event::GuildBanAdd(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_BAN_REMOVE" => Event::GuildBanRemove(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_EMOJIS_UPDATE" => Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            emojis: serde_json::from_value(data.get("emojis").cloned().unwrap_or(Value::Null))
                .unwrap_or_default(),
            raw: data,
        }),
        "GUILD_INTEGRATIONS_UPDATE" => {
            Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                raw: data,
            })
        }
        "WEBHOOKS_UPDATE" => Event::WebhooksUpdate(WebhooksUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "INVITE_CREATE" => Event::InviteCreate(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "INVITE_DELETE" => Event::InviteDelete(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "MESSAGE_REACTION_ADD" => Event::MessageReactionAdd(ReactionEvent {
            user_id: data
                .get("user_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            message_id: data
                .get("message_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            emoji: data
                .get("emoji")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "MESSAGE_REACTION_REMOVE" => Event::MessageReactionRemove(ReactionEvent {
            user_id: data
                .get("user_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            message_id: data
                .get("message_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            emoji: data
                .get("emoji")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "MESSAGE_REACTION_REMOVE_ALL" => Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            message_id: data
                .get("message_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "TYPING_START" => Event::TypingStart(TypingStartEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            user_id: data
                .get("user_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            timestamp: data.get("timestamp").and_then(|v| v.as_u64()),
            raw: data,
        }),
        "PRESENCE_UPDATE" => Event::PresenceUpdate(PresenceUpdateEvent {
            user_id: data
                .pointer("/user/id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .map(Snowflake::new),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            status: data
                .get("status")
                .and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "INTERACTION_CREATE" => Event::InteractionCreate(InteractionEvent {
            interaction: parse_interaction(&data)?,
            raw: data,
        }),
        "VOICE_STATE_UPDATE" => Event::VoiceStateUpdate(VoiceStateEvent {
            state: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "VOICE_SERVER_UPDATE" => Event::VoiceServerUpdate(VoiceServerEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        _ => Event::Unknown {
            kind: event_name.to_string(),
            raw: data,
        },
    };

    Ok(event)
}

fn read_required_snowflake(value: &Value, field: &str) -> Result<Snowflake, DiscordError> {
    let Some(raw) = value.get(field) else {
        return Err(format!("missing field {field}").into());
    };

    serde_json::from_value(raw.clone()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::error::DiscordError;
    use crate::model::{
        Channel, Guild, Interaction, InteractionContextData, Member, Message, PingInteraction,
        Role, Snowflake, User, VoiceServerUpdate, VoiceState,
    };
    use crate::types::Emoji;

    fn snowflake(id: &str) -> Snowflake {
        Snowflake::new(id)
    }

    fn raw(kind: &str) -> Value {
        json!({ "kind": kind })
    }

    fn user(id: &str, username: &str) -> User {
        User {
            id: snowflake(id),
            username: username.to_string(),
            ..Default::default()
        }
    }

    fn guild(id: &str, name: &str) -> Guild {
        Guild {
            id: snowflake(id),
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn channel(id: &str) -> Channel {
        Channel {
            id: snowflake(id),
            kind: 0,
            ..Default::default()
        }
    }

    fn member(id: &str, username: &str) -> Member {
        Member {
            user: Some(user(id, username)),
            ..Default::default()
        }
    }

    fn role(id: &str, name: &str) -> Role {
        Role {
            id: snowflake(id),
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn message(id: &str, channel_id: &str, content: &str) -> Message {
        Message {
            id: snowflake(id),
            channel_id: snowflake(channel_id),
            content: content.to_string(),
            ..Default::default()
        }
    }

    fn interaction_context() -> InteractionContextData {
        InteractionContextData {
            id: snowflake("400"),
            application_id: snowflake("401"),
            token: "token".to_string(),
            ..Default::default()
        }
    }

    fn assert_kind_and_raw(event: Event, expected_kind: &str) {
        assert_eq!(event.kind(), expected_kind);
        assert_eq!(event.raw(), &raw(expected_kind));
    }

    #[test]
    fn decode_message_create_event_returns_typed_payload() {
        let raw = json!({
            "id": "2",
            "channel_id": "1",
            "content": "hello",
            "mentions": [],
            "attachments": []
        });
        let event = decode_event("MESSAGE_CREATE", raw.clone()).unwrap();

        match event {
            Event::MessageCreate(message) => {
                assert_eq!(message.message.content, "hello");
                assert_eq!(message.raw, raw);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_handles_optional_field_fallbacks() {
        let emojis_update = decode_event(
            "GUILD_EMOJIS_UPDATE",
            json!({
                "guild_id": "1",
                "emojis": {}
            }),
        )
        .unwrap();
        match emojis_update {
            Event::GuildEmojisUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("1"));
                assert!(event.emojis.is_empty());
                assert_eq!(event.raw, json!({"guild_id": "1", "emojis": {}}));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let webhooks_update = decode_event(
            "WEBHOOKS_UPDATE",
            json!({
                "guild_id": {},
                "channel_id": {}
            }),
        )
        .unwrap();
        match webhooks_update {
            Event::WebhooksUpdate(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let invite_create = decode_event(
            "INVITE_CREATE",
            json!({
                "guild_id": {},
                "channel_id": {},
                "code": 42
            }),
        )
        .unwrap();
        match invite_create {
            Event::InviteCreate(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
                assert_eq!(event.code, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let invite_delete = decode_event(
            "INVITE_DELETE",
            json!({
                "guild_id": {},
                "channel_id": {},
                "code": 42
            }),
        )
        .unwrap();
        match invite_delete {
            Event::InviteDelete(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
                assert_eq!(event.code, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let pins_update = decode_event(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": "2",
                "guild_id": {},
                "last_pin_timestamp": 123
            }),
        )
        .unwrap();
        match pins_update {
            Event::ChannelPinsUpdate(event) => {
                assert_eq!(event.channel_id, snowflake("2"));
                assert_eq!(event.guild_id, None);
                assert_eq!(event.last_pin_timestamp, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let typing_start = decode_event(
            "TYPING_START",
            json!({
                "channel_id": {},
                "guild_id": {},
                "user_id": {},
                "timestamp": "later"
            }),
        )
        .unwrap();
        match typing_start {
            Event::TypingStart(event) => {
                assert_eq!(event.channel_id, None);
                assert_eq!(event.guild_id, None);
                assert_eq!(event.user_id, None);
                assert_eq!(event.timestamp, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let presence_update = decode_event(
            "PRESENCE_UPDATE",
            json!({
                "guild_id": {},
                "status": 1,
                "user": { "id": 9 }
            }),
        )
        .unwrap();
        match presence_update {
            Event::PresenceUpdate(event) => {
                assert_eq!(event.user_id, None);
                assert_eq!(event.guild_id, None);
                assert_eq!(event.status, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let integrations_update = decode_event(
            "GUILD_INTEGRATIONS_UPDATE",
            json!({
                "guild_id": {}
            }),
        )
        .unwrap();
        match integrations_update {
            Event::GuildIntegrationsUpdate(event) => {
                assert_eq!(event.guild_id, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_reads_nested_and_required_payloads() {
        let member_add = decode_event(
            "GUILD_MEMBER_ADD",
            json!({
                "guild_id": "100",
                "user": {
                    "id": "200",
                    "username": "member"
                }
            }),
        )
        .unwrap();
        match member_add {
            Event::MemberAdd(event) => {
                assert_eq!(event.guild_id, snowflake("100"));
                assert_eq!(
                    event
                        .member
                        .user
                        .as_ref()
                        .map(|user| user.username.as_str()),
                    Some("member")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let role_create = decode_event(
            "GUILD_ROLE_CREATE",
            json!({
                "guild_id": "100",
                "role": {
                    "id": "300",
                    "name": "mods"
                }
            }),
        )
        .unwrap();
        match role_create {
            Event::RoleCreate(event) => {
                assert_eq!(event.guild_id, snowflake("100"));
                assert_eq!(event.role.id, snowflake("300"));
                assert_eq!(event.role.name, "mods");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let bulk_delete = decode_event(
            "MESSAGE_DELETE_BULK",
            json!({
                "ids": ["10", "11"],
                "channel_id": "12",
                "guild_id": "13"
            }),
        )
        .unwrap();
        match bulk_delete {
            Event::MessageDeleteBulk(event) => {
                assert_eq!(event.ids, vec![snowflake("10"), snowflake("11")]);
                assert_eq!(event.channel_id, snowflake("12"));
                assert_eq!(event.guild_id, Some(snowflake("13")));
                assert_eq!(
                    event.raw,
                    json!({
                        "ids": ["10", "11"],
                        "channel_id": "12",
                        "guild_id": "13"
                    })
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let pins_update = decode_event(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": "14",
                "last_pin_timestamp": "2024-01-01T00:00:00Z"
            }),
        )
        .unwrap();
        match pins_update {
            Event::ChannelPinsUpdate(event) => {
                assert_eq!(event.channel_id, snowflake("14"));
                assert_eq!(
                    event.last_pin_timestamp.as_deref(),
                    Some("2024-01-01T00:00:00Z")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let typing_start = decode_event(
            "TYPING_START",
            json!({
                "channel_id": "15",
                "guild_id": "16",
                "user_id": "17",
                "timestamp": 12345
            }),
        )
        .unwrap();
        match typing_start {
            Event::TypingStart(event) => {
                assert_eq!(event.channel_id, Some(snowflake("15")));
                assert_eq!(event.guild_id, Some(snowflake("16")));
                assert_eq!(event.user_id, Some(snowflake("17")));
                assert_eq!(event.timestamp, Some(12345));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let presence_update = decode_event(
            "PRESENCE_UPDATE",
            json!({
                "guild_id": "18",
                "status": "online",
                "user": { "id": "19" }
            }),
        )
        .unwrap();
        match presence_update {
            Event::PresenceUpdate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("18")));
                assert_eq!(event.user_id, Some(snowflake("19")));
                assert_eq!(event.status.as_deref(), Some("online"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_reports_required_field_errors_and_preserves_unknown_events() {
        let missing_guild_id = decode_event(
            "GUILD_MEMBER_ADD",
            json!({
                "user": {
                    "id": "20",
                    "username": "member"
                }
            }),
        )
        .unwrap_err();
        match missing_guild_id {
            DiscordError::Model { message } => assert_eq!(message, "missing field guild_id"),
            other => panic!("unexpected error: {other:?}"),
        }

        let invalid_guild_id = decode_event(
            "GUILD_ROLE_CREATE",
            json!({
                "guild_id": {},
                "role": {
                    "id": "21",
                    "name": "mods"
                }
            }),
        )
        .unwrap_err();
        assert!(
            matches!(invalid_guild_id, DiscordError::Json(message) if message.contains("snowflake"))
        );

        let missing_ids = decode_event(
            "MESSAGE_DELETE_BULK",
            json!({
                "channel_id": "22"
            }),
        )
        .unwrap_err();
        assert!(matches!(missing_ids, DiscordError::Json(_)));

        let raw = json!({ "x": 1 });
        let unknown = decode_event("SOMETHING_NEW", raw.clone()).unwrap();
        match unknown {
            Event::Unknown {
                kind,
                raw: event_raw,
            } => {
                assert_eq!(kind, "SOMETHING_NEW");
                assert_eq!(event_raw, raw);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn event_kind_and_raw_cover_remaining_variants() {
        let cases = vec![
            (
                "READY",
                Event::Ready(ReadyEvent {
                    data: ReadyPayload {
                        user: user("1", "ready"),
                        session_id: "session".to_string(),
                        application: Some(ReadyApplication { id: snowflake("2") }),
                        resume_gateway_url: Some("wss://gateway.discord.test".to_string()),
                    },
                    raw: raw("READY"),
                }),
            ),
            (
                "GUILD_CREATE",
                Event::GuildCreate(GuildEvent {
                    guild: guild("10", "guild-create"),
                    raw: raw("GUILD_CREATE"),
                }),
            ),
            (
                "GUILD_UPDATE",
                Event::GuildUpdate(GuildEvent {
                    guild: guild("11", "guild-update"),
                    raw: raw("GUILD_UPDATE"),
                }),
            ),
            (
                "GUILD_DELETE",
                Event::GuildDelete(GuildDeleteEvent {
                    data: GuildDeletePayload {
                        id: snowflake("12"),
                        unavailable: Some(true),
                    },
                    raw: raw("GUILD_DELETE"),
                }),
            ),
            (
                "CHANNEL_CREATE",
                Event::ChannelCreate(ChannelEvent {
                    channel: channel("13"),
                    raw: raw("CHANNEL_CREATE"),
                }),
            ),
            (
                "CHANNEL_UPDATE",
                Event::ChannelUpdate(ChannelEvent {
                    channel: channel("14"),
                    raw: raw("CHANNEL_UPDATE"),
                }),
            ),
            (
                "CHANNEL_DELETE",
                Event::ChannelDelete(ChannelEvent {
                    channel: channel("15"),
                    raw: raw("CHANNEL_DELETE"),
                }),
            ),
            (
                "GUILD_MEMBER_UPDATE",
                Event::MemberUpdate(MemberEvent {
                    guild_id: snowflake("16"),
                    member: member("17", "member-update"),
                    raw: raw("GUILD_MEMBER_UPDATE"),
                }),
            ),
            (
                "GUILD_MEMBER_REMOVE",
                Event::MemberRemove(MemberRemoveEvent {
                    data: MemberRemovePayload {
                        guild_id: snowflake("18"),
                        user: user("19", "member-remove"),
                    },
                    raw: raw("GUILD_MEMBER_REMOVE"),
                }),
            ),
            (
                "GUILD_ROLE_UPDATE",
                Event::RoleUpdate(RoleEvent {
                    guild_id: snowflake("20"),
                    role: role("21", "role-update"),
                    raw: raw("GUILD_ROLE_UPDATE"),
                }),
            ),
            (
                "GUILD_ROLE_DELETE",
                Event::RoleDelete(RoleDeleteEvent {
                    data: RoleDeletePayload {
                        guild_id: snowflake("22"),
                        role_id: snowflake("23"),
                    },
                    raw: raw("GUILD_ROLE_DELETE"),
                }),
            ),
            (
                "MESSAGE_UPDATE",
                Event::MessageUpdate(MessageEvent {
                    message: message("24", "25", "updated"),
                    raw: raw("MESSAGE_UPDATE"),
                }),
            ),
            (
                "MESSAGE_DELETE",
                Event::MessageDelete(MessageDeleteEvent {
                    data: MessageDeletePayload {
                        id: snowflake("26"),
                        channel_id: snowflake("27"),
                        guild_id: Some(snowflake("28")),
                    },
                    raw: raw("MESSAGE_DELETE"),
                }),
            ),
            (
                "GUILD_BAN_ADD",
                Event::GuildBanAdd(GuildBanEvent {
                    guild_id: snowflake("29"),
                    user: user("30", "ban-add"),
                    raw: raw("GUILD_BAN_ADD"),
                }),
            ),
            (
                "GUILD_BAN_REMOVE",
                Event::GuildBanRemove(GuildBanEvent {
                    guild_id: snowflake("31"),
                    user: user("32", "ban-remove"),
                    raw: raw("GUILD_BAN_REMOVE"),
                }),
            ),
            (
                "MESSAGE_REACTION_ADD",
                Event::MessageReactionAdd(ReactionEvent {
                    user_id: Some(snowflake("33")),
                    channel_id: Some(snowflake("34")),
                    message_id: Some(snowflake("35")),
                    guild_id: Some(snowflake("36")),
                    emoji: Some(Emoji::unicode("🔥")),
                    raw: raw("MESSAGE_REACTION_ADD"),
                }),
            ),
            (
                "MESSAGE_REACTION_REMOVE",
                Event::MessageReactionRemove(ReactionEvent {
                    user_id: Some(snowflake("37")),
                    channel_id: Some(snowflake("38")),
                    message_id: Some(snowflake("39")),
                    guild_id: Some(snowflake("40")),
                    emoji: Some(Emoji::unicode("🔥")),
                    raw: raw("MESSAGE_REACTION_REMOVE"),
                }),
            ),
            (
                "MESSAGE_REACTION_REMOVE_ALL",
                Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
                    channel_id: Some(snowflake("41")),
                    message_id: Some(snowflake("42")),
                    guild_id: Some(snowflake("43")),
                    raw: raw("MESSAGE_REACTION_REMOVE_ALL"),
                }),
            ),
            (
                "INTERACTION_CREATE",
                Event::InteractionCreate(InteractionEvent {
                    interaction: Interaction::Ping(PingInteraction {
                        context: interaction_context(),
                    }),
                    raw: raw("INTERACTION_CREATE"),
                }),
            ),
            (
                "VOICE_STATE_UPDATE",
                Event::VoiceStateUpdate(VoiceStateEvent {
                    state: VoiceState {
                        guild_id: Some(snowflake("44")),
                        channel_id: Some(snowflake("45")),
                        user_id: Some(snowflake("46")),
                        ..Default::default()
                    },
                    raw: raw("VOICE_STATE_UPDATE"),
                }),
            ),
            (
                "VOICE_SERVER_UPDATE",
                Event::VoiceServerUpdate(VoiceServerEvent {
                    data: VoiceServerUpdate {
                        guild_id: snowflake("47"),
                        token: "voice-token".to_string(),
                        endpoint: Some("wss://voice.discord.test".to_string()),
                    },
                    raw: raw("VOICE_SERVER_UPDATE"),
                }),
            ),
        ];

        for (kind, event) in cases {
            assert_kind_and_raw(event, kind);
        }
    }
}
