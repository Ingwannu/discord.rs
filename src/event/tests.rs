use serde_json::{json, Value};

use super::*;
use crate::error::DiscordError;
use crate::model::{
    Channel, Entitlement, Guild, Integration, IntegrationAccount, Interaction,
    InteractionContextData, Member, Message, PingInteraction, Role, Snowflake, SoundboardSound,
    StageInstance, Subscription, User, VoiceServerUpdate, VoiceState,
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
            "user": { "id": {} }
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
            "user": {
                "id": "19",
                "username": "present"
            },
            "activities": [{
                "name": "Building",
                "type": 0
            }],
            "client_status": {
                "desktop": "online",
                "web": "idle"
            }
        }),
    )
    .unwrap();
    match presence_update {
        Event::PresenceUpdate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("18")));
            assert_eq!(event.user_id, Some(snowflake("19")));
            assert_eq!(event.status.as_deref(), Some("online"));
            assert_eq!(
                event
                    .user
                    .as_ref()
                    .and_then(|user| user.username.as_deref()),
                Some("present")
            );
            assert_eq!(event.activities[0].name, "Building");
            assert_eq!(
                event
                    .client_status
                    .as_ref()
                    .and_then(|status| status.desktop.as_deref()),
                Some("online")
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_additional_typed_gateway_payloads() {
    match decode_event(
        "GUILD_CREATE",
        json!({
            "id": "1",
            "name": "discordrs",
            "roles": []
        }),
    )
    .unwrap()
    {
        Event::GuildCreate(event) => {
            assert_eq!(event.guild.id, snowflake("1"));
            assert_eq!(event.guild.name, "discordrs");
        }
        other => panic!("unexpected guild event: {other:?}"),
    }

    match decode_event(
        "CHANNEL_CREATE",
        json!({
            "id": "2",
            "type": 0,
            "name": "general"
        }),
    )
    .unwrap()
    {
        Event::ChannelCreate(event) => {
            assert_eq!(event.channel.id, snowflake("2"));
            assert_eq!(event.channel.name.as_deref(), Some("general"));
        }
        other => panic!("unexpected channel event: {other:?}"),
    }

    match decode_event(
        "GUILD_MEMBER_REMOVE",
        json!({
            "guild_id": "3",
            "user": {
                "id": "4",
                "username": "member"
            }
        }),
    )
    .unwrap()
    {
        Event::MemberRemove(event) => {
            assert_eq!(event.data.guild_id, snowflake("3"));
            assert_eq!(event.data.user.id, snowflake("4"));
        }
        other => panic!("unexpected member removal event: {other:?}"),
    }

    match decode_event(
        "GUILD_ROLE_DELETE",
        json!({
            "guild_id": "5",
            "role_id": "6"
        }),
    )
    .unwrap()
    {
        Event::RoleDelete(event) => {
            assert_eq!(event.data.guild_id, snowflake("5"));
            assert_eq!(event.data.role_id, snowflake("6"));
        }
        other => panic!("unexpected role delete event: {other:?}"),
    }
}

#[test]
fn decode_event_exposes_common_fields_for_newer_gateway_payloads() {
    match decode_event(
        "GUILD_SCHEDULED_EVENT_CREATE",
        json!({
            "id": "700",
            "guild_id": "701",
            "channel_id": "702",
            "creator_id": "703",
            "name": "Launch",
            "description": "Release stream",
            "scheduled_start_time": "2026-04-30T01:00:00Z",
            "scheduled_end_time": "2026-04-30T02:00:00Z",
            "privacy_level": 2,
            "status": 1,
            "entity_type": 2,
            "entity_id": "704",
            "entity_metadata": { "location": "voice" },
            "user_count": 42,
            "image": "cover"
        }),
    )
    .unwrap()
    {
        Event::GuildScheduledEventCreate(event) => {
            assert_eq!(event.id, Some(snowflake("700")));
            assert_eq!(event.guild_id, Some(snowflake("701")));
            assert_eq!(event.channel_id, Some(snowflake("702")));
            assert_eq!(event.creator_id, Some(snowflake("703")));
            assert_eq!(event.name.as_deref(), Some("Launch"));
            assert_eq!(event.description.as_deref(), Some("Release stream"));
            assert_eq!(
                event.scheduled_start_time.as_deref(),
                Some("2026-04-30T01:00:00Z")
            );
            assert_eq!(event.status, Some(1));
            assert_eq!(event.entity_type, Some(2));
            assert_eq!(event.entity_id, Some(snowflake("704")));
            assert_eq!(event.entity_metadata, Some(json!({ "location": "voice" })));
            assert_eq!(event.user_count, Some(42));
            assert_eq!(event.image.as_deref(), Some("cover"));
        }
        other => panic!("unexpected scheduled event: {other:?}"),
    }

    match decode_event(
        "AUTO_MODERATION_RULE_CREATE",
        json!({
            "id": "710",
            "guild_id": "711",
            "name": "Keyword Filter",
            "creator_id": "712",
            "event_type": 1,
            "trigger_type": 1,
            "trigger_metadata": { "keyword_filter": ["bad"] },
            "actions": [{ "type": 1 }],
            "enabled": true,
            "exempt_roles": ["713"],
            "exempt_channels": ["714"]
        }),
    )
    .unwrap()
    {
        Event::AutoModerationRuleCreate(event) => {
            assert_eq!(event.id, Some(snowflake("710")));
            assert_eq!(event.guild_id, Some(snowflake("711")));
            assert_eq!(event.name.as_deref(), Some("Keyword Filter"));
            assert_eq!(event.creator_id, Some(snowflake("712")));
            assert_eq!(event.event_type, Some(1));
            assert_eq!(event.trigger_type, Some(1));
            assert_eq!(
                event.trigger_metadata,
                Some(json!({ "keyword_filter": ["bad"] }))
            );
            assert_eq!(event.actions, vec![json!({ "type": 1 })]);
            assert_eq!(event.enabled, Some(true));
            assert_eq!(event.exempt_roles, vec![snowflake("713")]);
            assert_eq!(event.exempt_channels, vec![snowflake("714")]);
        }
        other => panic!("unexpected auto moderation rule event: {other:?}"),
    }

    match decode_event(
        "AUTO_MODERATION_ACTION_EXECUTION",
        json!({
            "guild_id": "720",
            "action": { "type": 2, "metadata": { "channel_id": "721" } },
            "rule_id": "722",
            "rule_trigger_type": 1,
            "user_id": "723",
            "channel_id": "724",
            "message_id": "725",
            "alert_system_message_id": "726",
            "content": "blocked text",
            "matched_keyword": "blocked",
            "matched_content": "blocked"
        }),
    )
    .unwrap()
    {
        Event::AutoModerationActionExecution(event) => {
            assert_eq!(event.guild_id, Some(snowflake("720")));
            assert_eq!(
                event.action,
                Some(json!({ "type": 2, "metadata": { "channel_id": "721" } }))
            );
            assert_eq!(event.rule_id, Some(snowflake("722")));
            assert_eq!(event.rule_trigger_type, Some(1));
            assert_eq!(event.user_id, Some(snowflake("723")));
            assert_eq!(event.channel_id, Some(snowflake("724")));
            assert_eq!(event.message_id, Some(snowflake("725")));
            assert_eq!(event.alert_system_message_id, Some(snowflake("726")));
            assert_eq!(event.content.as_deref(), Some("blocked text"));
            assert_eq!(event.matched_keyword.as_deref(), Some("blocked"));
            assert_eq!(event.matched_content.as_deref(), Some("blocked"));
        }
        other => panic!("unexpected auto moderation action event: {other:?}"),
    }

    match decode_event(
        "GUILD_AUDIT_LOG_ENTRY_CREATE",
        json!({
            "guild_id": "730",
            "id": "731",
            "user_id": "732",
            "target_id": "733",
            "action_type": 22,
            "changes": [{ "key": "nick", "new_value": "new" }],
            "options": { "delete_member_days": "1" },
            "reason": "cleanup"
        }),
    )
    .unwrap()
    {
        Event::GuildAuditLogEntryCreate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("730")));
            assert_eq!(event.id, Some(snowflake("731")));
            assert_eq!(event.user_id, Some(snowflake("732")));
            assert_eq!(event.target_id, Some(snowflake("733")));
            assert_eq!(event.action_type, Some(22));
            assert_eq!(
                event.changes,
                Some(vec![json!({ "key": "nick", "new_value": "new" })])
            );
            assert_eq!(event.options, Some(json!({ "delete_member_days": "1" })));
            assert_eq!(event.reason.as_deref(), Some("cleanup"));
            assert_eq!(
                event
                    .entry
                    .as_ref()
                    .and_then(|entry| entry.id.as_ref())
                    .map(Snowflake::as_str),
                Some("731")
            );
        }
        other => panic!("unexpected audit log entry event: {other:?}"),
    }

    match decode_event(
        "USER_UPDATE",
        json!({
            "id": "740",
            "username": "bot"
        }),
    )
    .unwrap()
    {
        Event::UserUpdate(event) => {
            assert_eq!(event.user.id, snowflake("740"));
            assert_eq!(event.user.username, "bot");
        }
        other => panic!("unexpected user update event: {other:?}"),
    }

    let rate_limited = decode_event(
        "RATE_LIMITED",
        json!({
            "opcode": 8,
            "retry_after": 12.5,
            "meta": {
                "guild_id": "750",
                "nonce": "members-1"
            }
        }),
    )
    .unwrap();
    assert_eq!(rate_limited.kind(), "RATE_LIMITED");
    match rate_limited {
        Event::RateLimited(event) => {
            assert_eq!(event.opcode, Some(8));
            assert_eq!(event.retry_after, Some(12.5));
            assert_eq!(
                event.meta,
                Some(json!({ "guild_id": "750", "nonce": "members-1" }))
            );
        }
        other => panic!("unexpected rate limited event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_voice_ban_reaction_and_interaction_variants() {
    match decode_event(
        "VOICE_STATE_UPDATE",
        json!({
            "guild_id": "1",
            "channel_id": "2",
            "user_id": "3"
        }),
    )
    .unwrap()
    {
        Event::VoiceStateUpdate(event) => {
            assert_eq!(event.state.guild_id, Some(snowflake("1")));
            assert_eq!(event.state.channel_id, Some(snowflake("2")));
            assert_eq!(event.state.user_id, Some(snowflake("3")));
        }
        other => panic!("unexpected voice state event: {other:?}"),
    }

    match decode_event(
        "VOICE_SERVER_UPDATE",
        json!({
            "guild_id": "4",
            "token": "voice-token",
            "endpoint": "wss://voice.discord.test"
        }),
    )
    .unwrap()
    {
        Event::VoiceServerUpdate(event) => {
            assert_eq!(event.data.guild_id, snowflake("4"));
            assert_eq!(event.data.token, "voice-token");
            assert_eq!(
                event.data.endpoint.as_deref(),
                Some("wss://voice.discord.test")
            );
        }
        other => panic!("unexpected voice server event: {other:?}"),
    }

    match decode_event(
        "GUILD_BAN_ADD",
        json!({
            "guild_id": "7",
            "user": {
                "id": "8",
                "username": "banned"
            }
        }),
    )
    .unwrap()
    {
        Event::GuildBanAdd(event) => {
            assert_eq!(event.guild_id, snowflake("7"));
            assert_eq!(event.user.username, "banned");
        }
        other => panic!("unexpected guild ban event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_REACTION_ADD",
        json!({
            "user_id": "9",
            "channel_id": "10",
            "message_id": "11",
            "guild_id": "12",
            "member": {
                "user": {
                    "id": "9",
                    "username": "reactor"
                }
            },
            "message_author_id": "99",
            "burst": true,
            "burst_colors": ["#ff0000", "#00ff00"],
            "type": 1,
            "emoji": {
                "name": "?뵦"
            }
        }),
    )
    .unwrap()
    {
        Event::MessageReactionAdd(event) => {
            assert_eq!(event.user_id, Some(snowflake("9")));
            assert_eq!(event.channel_id, Some(snowflake("10")));
            assert_eq!(event.message_id, Some(snowflake("11")));
            assert_eq!(event.guild_id, Some(snowflake("12")));
            assert_eq!(
                event
                    .member
                    .as_ref()
                    .and_then(|member| member.user.as_ref())
                    .map(|user| user.username.as_str()),
                Some("reactor")
            );
            assert_eq!(event.message_author_id, Some(snowflake("99")));
            assert_eq!(event.burst, Some(true));
            assert_eq!(event.burst_colors, vec!["#ff0000", "#00ff00"]);
            assert_eq!(event.reaction_type, Some(1));
            assert_eq!(
                event.emoji.and_then(|emoji| emoji.name),
                Some("?뵦".to_string())
            );
        }
        other => panic!("unexpected reaction event: {other:?}"),
    }

    match decode_event(
        "INTERACTION_CREATE",
        json!({
            "id": "13",
            "application_id": "14",
            "token": "interaction-token",
            "type": 1
        }),
    )
    .unwrap()
    {
        Event::InteractionCreate(event) => {
            assert!(matches!(event.interaction, Interaction::Ping(_)));
        }
        other => panic!("unexpected interaction event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_remaining_success_variants() {
    match decode_event(
        "READY",
        json!({
            "user": {
                "id": "50",
                "username": "ready"
            },
            "session_id": "session-50"
        }),
    )
    .unwrap()
    {
        Event::Ready(event) => {
            assert_eq!(event.data.user.id, snowflake("50"));
            assert_eq!(event.data.session_id, "session-50");
            assert!(event.data.application.is_none());
            assert!(event.data.resume_gateway_url.is_none());
        }
        other => panic!("unexpected ready event: {other:?}"),
    }

    match decode_event(
        "GUILD_UPDATE",
        json!({
            "id": "51",
            "name": "guild-update",
            "roles": []
        }),
    )
    .unwrap()
    {
        Event::GuildUpdate(event) => {
            assert_eq!(event.guild.id, snowflake("51"));
            assert_eq!(event.guild.name, "guild-update");
        }
        other => panic!("unexpected guild update event: {other:?}"),
    }

    match decode_event(
        "GUILD_DELETE",
        json!({
            "id": "52"
        }),
    )
    .unwrap()
    {
        Event::GuildDelete(event) => {
            assert_eq!(event.data.id, snowflake("52"));
            assert_eq!(event.data.unavailable, None);
        }
        other => panic!("unexpected guild delete event: {other:?}"),
    }

    match decode_event(
        "CHANNEL_UPDATE",
        json!({
            "id": "53",
            "type": 0
        }),
    )
    .unwrap()
    {
        Event::ChannelUpdate(event) => {
            assert_eq!(event.channel.id, snowflake("53"));
            assert_eq!(event.channel.kind, 0);
        }
        other => panic!("unexpected channel update event: {other:?}"),
    }

    match decode_event(
        "CHANNEL_DELETE",
        json!({
            "id": "54",
            "type": 0
        }),
    )
    .unwrap()
    {
        Event::ChannelDelete(event) => {
            assert_eq!(event.channel.id, snowflake("54"));
            assert_eq!(event.channel.kind, 0);
        }
        other => panic!("unexpected channel delete event: {other:?}"),
    }

    match decode_event(
        "GUILD_MEMBER_UPDATE",
        json!({
            "guild_id": "55",
            "user": {
                "id": "56",
                "username": "member-update"
            }
        }),
    )
    .unwrap()
    {
        Event::MemberUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("55"));
            assert_eq!(
                event
                    .member
                    .user
                    .as_ref()
                    .map(|user| user.username.as_str()),
                Some("member-update")
            );
        }
        other => panic!("unexpected member update event: {other:?}"),
    }

    match decode_event(
        "GUILD_ROLE_UPDATE",
        json!({
            "guild_id": "57",
            "role": {
                "id": "58",
                "name": "role-update"
            }
        }),
    )
    .unwrap()
    {
        Event::RoleUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("57"));
            assert_eq!(event.role.id, snowflake("58"));
            assert_eq!(event.role.name, "role-update");
        }
        other => panic!("unexpected role update event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_UPDATE",
        json!({
            "id": "59",
            "channel_id": "60",
            "content": "edited",
            "mentions": [],
            "attachments": []
        }),
    )
    .unwrap()
    {
        Event::MessageUpdate(event) => {
            assert_eq!(event.message.id, snowflake("59"));
            assert_eq!(event.message.channel_id, snowflake("60"));
            assert_eq!(event.message.content, "edited");
        }
        other => panic!("unexpected message update event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_DELETE",
        json!({
            "id": "61",
            "channel_id": "62"
        }),
    )
    .unwrap()
    {
        Event::MessageDelete(event) => {
            assert_eq!(event.data.id, snowflake("61"));
            assert_eq!(event.data.channel_id, snowflake("62"));
            assert_eq!(event.data.guild_id, None);
        }
        other => panic!("unexpected message delete event: {other:?}"),
    }

    match decode_event(
        "GUILD_BAN_REMOVE",
        json!({
            "guild_id": "63",
            "user": {
                "id": "64",
                "username": "ban-remove"
            }
        }),
    )
    .unwrap()
    {
        Event::GuildBanRemove(event) => {
            assert_eq!(event.guild_id, snowflake("63"));
            assert_eq!(event.user.id, snowflake("64"));
            assert_eq!(event.user.username, "ban-remove");
        }
        other => panic!("unexpected guild ban remove event: {other:?}"),
    }

    match decode_event(
        "GUILD_EMOJIS_UPDATE",
        json!({
            "guild_id": "65"
        }),
    )
    .unwrap()
    {
        Event::GuildEmojisUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("65"));
            assert!(event.emojis.is_empty());
        }
        other => panic!("unexpected guild emojis update event: {other:?}"),
    }

    match decode_event(
        "GUILD_INTEGRATIONS_UPDATE",
        json!({
            "guild_id": "66"
        }),
    )
    .unwrap()
    {
        Event::GuildIntegrationsUpdate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("66")));
        }
        other => panic!("unexpected integrations update event: {other:?}"),
    }

    match decode_event(
        "WEBHOOKS_UPDATE",
        json!({
            "guild_id": "67",
            "channel_id": "68"
        }),
    )
    .unwrap()
    {
        Event::WebhooksUpdate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("67")));
            assert_eq!(event.channel_id, Some(snowflake("68")));
        }
        other => panic!("unexpected webhooks update event: {other:?}"),
    }

    match decode_event(
        "INVITE_DELETE",
        json!({
            "guild_id": "69",
            "channel_id": "70",
            "code": "invite-code"
        }),
    )
    .unwrap()
    {
        Event::InviteDelete(event) => {
            assert_eq!(event.guild_id, Some(snowflake("69")));
            assert_eq!(event.channel_id, Some(snowflake("70")));
            assert_eq!(event.code.as_deref(), Some("invite-code"));
        }
        other => panic!("unexpected invite delete event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_REACTION_REMOVE",
        json!({
            "user_id": "71",
            "channel_id": "72",
            "message_id": "73",
            "guild_id": "74",
            "emoji": {
                "name": "x"
            }
        }),
    )
    .unwrap()
    {
        Event::MessageReactionRemove(event) => {
            assert_eq!(event.user_id, Some(snowflake("71")));
            assert_eq!(event.channel_id, Some(snowflake("72")));
            assert_eq!(event.message_id, Some(snowflake("73")));
            assert_eq!(event.guild_id, Some(snowflake("74")));
            assert_eq!(
                event.emoji.and_then(|emoji| emoji.name),
                Some("x".to_string())
            );
        }
        other => panic!("unexpected reaction remove event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_REACTION_REMOVE_ALL",
        json!({
            "channel_id": "75",
            "message_id": "76",
            "guild_id": "77"
        }),
    )
    .unwrap()
    {
        Event::MessageReactionRemoveAll(event) => {
            assert_eq!(event.channel_id, Some(snowflake("75")));
            assert_eq!(event.message_id, Some(snowflake("76")));
            assert_eq!(event.guild_id, Some(snowflake("77")));
        }
        other => panic!("unexpected reaction remove all event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_success_payloads_with_present_optional_fields() {
    match decode_event(
        "READY",
        json!({
            "user": {
                "id": "80",
                "username": "ready-plus"
            },
            "session_id": "session-80",
            "application": {
                "id": "81"
            },
            "resume_gateway_url": "wss://gateway.discord.test"
        }),
    )
    .unwrap()
    {
        Event::Ready(event) => {
            assert_eq!(event.data.user.id, snowflake("80"));
            assert_eq!(
                event.data.application.map(|app| app.id),
                Some(snowflake("81"))
            );
            assert_eq!(
                event.data.resume_gateway_url.as_deref(),
                Some("wss://gateway.discord.test")
            );
        }
        other => panic!("unexpected ready event: {other:?}"),
    }

    match decode_event(
        "GUILD_DELETE",
        json!({
            "id": "82",
            "unavailable": true
        }),
    )
    .unwrap()
    {
        Event::GuildDelete(event) => {
            assert_eq!(event.data.id, snowflake("82"));
            assert_eq!(event.data.unavailable, Some(true));
        }
        other => panic!("unexpected guild delete event: {other:?}"),
    }

    match decode_event(
        "MESSAGE_DELETE",
        json!({
            "id": "83",
            "channel_id": "84",
            "guild_id": "85"
        }),
    )
    .unwrap()
    {
        Event::MessageDelete(event) => {
            assert_eq!(event.data.id, snowflake("83"));
            assert_eq!(event.data.channel_id, snowflake("84"));
            assert_eq!(event.data.guild_id, Some(snowflake("85")));
        }
        other => panic!("unexpected message delete event: {other:?}"),
    }

    match decode_event(
        "CHANNEL_PINS_UPDATE",
        json!({
            "channel_id": "86",
            "guild_id": "87",
            "last_pin_timestamp": "2024-06-01T00:00:00Z"
        }),
    )
    .unwrap()
    {
        Event::ChannelPinsUpdate(event) => {
            assert_eq!(event.channel_id, snowflake("86"));
            assert_eq!(event.guild_id, Some(snowflake("87")));
            assert_eq!(
                event.last_pin_timestamp.as_deref(),
                Some("2024-06-01T00:00:00Z")
            );
        }
        other => panic!("unexpected channel pins update event: {other:?}"),
    }

    match decode_event(
        "GUILD_EMOJIS_UPDATE",
        json!({
            "guild_id": "88",
            "emojis": [
                {
                    "name": "wave"
                }
            ]
        }),
    )
    .unwrap()
    {
        Event::GuildEmojisUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("88"));
            assert_eq!(event.emojis.len(), 1);
            assert_eq!(event.emojis[0].name.as_deref(), Some("wave"));
        }
        other => panic!("unexpected guild emojis update event: {other:?}"),
    }

    match decode_event(
        "INVITE_CREATE",
        json!({
            "guild_id": "89",
            "channel_id": "90",
            "code": "invite-create"
        }),
    )
    .unwrap()
    {
        Event::InviteCreate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("89")));
            assert_eq!(event.channel_id, Some(snowflake("90")));
            assert_eq!(event.code.as_deref(), Some("invite-create"));
        }
        other => panic!("unexpected invite create event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_new_gateway_surface_variants() {
    match decode_event(
        "GUILD_MEMBERS_CHUNK",
        json!({
            "guild_id": "1",
            "members": [{
                "user": { "id": "2", "username": "member" },
                "roles": ["3"]
            }],
            "chunk_index": 0,
            "chunk_count": 1,
            "not_found": ["4"],
            "presences": [{
                "user_id": "2",
                "status": "online",
                "activities": [{ "name": "Testing", "type": 0 }]
            }],
            "nonce": "abc"
        }),
    )
    .unwrap()
    {
        Event::GuildMembersChunk(event) => {
            assert_eq!(event.data.guild_id.as_str(), "1");
            assert_eq!(event.data.members.len(), 1);
            assert_eq!(event.data.not_found[0].as_str(), "4");
            assert_eq!(
                event.data.presences.unwrap()[0]
                    .activities
                    .as_ref()
                    .unwrap()[0]
                    .name,
                "Testing"
            );
            assert_eq!(event.data.nonce.as_deref(), Some("abc"));
        }
        other => panic!("unexpected guild members chunk event: {other:?}"),
    }

    match decode_event("RESUMED", json!({ "trace": [] })).unwrap() {
        Event::Resumed(event) => assert_eq!(event.raw["trace"], json!([])),
        other => panic!("unexpected resumed event: {other:?}"),
    }

    match decode_event(
        "VOICE_CHANNEL_STATUS_UPDATE",
        json!({
            "guild_id": "1",
            "channel_id": "2",
            "status": "Live"
        }),
    )
    .unwrap()
    {
        Event::VoiceChannelStatusUpdate(event) => {
            assert_eq!(event.channel_id.unwrap().as_str(), "2");
            assert_eq!(event.status.as_deref(), Some("Live"));
        }
        other => panic!("unexpected voice channel status event: {other:?}"),
    }

    let raw_channel_info = json!({
        "guild_id": "1",
        "channels": [{
            "id": "2",
            "status": "Live",
            "voice_start_time": "2026-05-01T00:00:00.000000+00:00"
        }]
    });
    let event = decode_event("CHANNEL_INFO", raw_channel_info.clone()).unwrap();
    assert_eq!(event.kind(), "CHANNEL_INFO");
    assert_eq!(event.raw(), &raw_channel_info);
    match event {
        Event::ChannelInfo(event) => {
            assert_eq!(event.guild_id.as_str(), "1");
            assert_eq!(event.channels[0].id.as_str(), "2");
            assert_eq!(event.channels[0].status.as_deref(), Some("Live"));
            assert_eq!(
                event.channels[0].voice_start_time.as_deref(),
                Some("2026-05-01T00:00:00.000000+00:00")
            );
        }
        other => panic!("unexpected channel info event: {other:?}"),
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
                emoji: Some(Emoji::unicode("?뵦")),
                raw: raw("MESSAGE_REACTION_ADD"),
                ..ReactionEvent::default()
            }),
        ),
        (
            "MESSAGE_REACTION_REMOVE",
            Event::MessageReactionRemove(ReactionEvent {
                user_id: Some(snowflake("37")),
                channel_id: Some(snowflake("38")),
                message_id: Some(snowflake("39")),
                guild_id: Some(snowflake("40")),
                emoji: Some(Emoji::unicode("?뵦")),
                raw: raw("MESSAGE_REACTION_REMOVE"),
                ..ReactionEvent::default()
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

#[test]
fn event_kind_and_raw_cover_missing_variants() {
    let cases = vec![
        (
            "GUILD_MEMBER_ADD",
            Event::MemberAdd(MemberEvent {
                guild_id: snowflake("80"),
                member: member("81", "member-add"),
                raw: raw("GUILD_MEMBER_ADD"),
            }),
        ),
        (
            "GUILD_ROLE_CREATE",
            Event::RoleCreate(RoleEvent {
                guild_id: snowflake("82"),
                role: role("83", "role-create"),
                raw: raw("GUILD_ROLE_CREATE"),
            }),
        ),
        (
            "MESSAGE_CREATE",
            Event::MessageCreate(MessageEvent {
                message: message("84", "85", "created"),
                raw: raw("MESSAGE_CREATE"),
            }),
        ),
        (
            "MESSAGE_DELETE_BULK",
            Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                ids: vec![snowflake("86"), snowflake("87")],
                channel_id: snowflake("88"),
                guild_id: Some(snowflake("89")),
                raw: raw("MESSAGE_DELETE_BULK"),
            }),
        ),
        (
            "CHANNEL_PINS_UPDATE",
            Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
                channel_id: snowflake("90"),
                guild_id: Some(snowflake("91")),
                last_pin_timestamp: Some("2024-07-01T00:00:00Z".to_string()),
                raw: raw("CHANNEL_PINS_UPDATE"),
            }),
        ),
        (
            "GUILD_EMOJIS_UPDATE",
            Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
                guild_id: snowflake("92"),
                emojis: vec![Emoji::unicode("wave")],
                raw: raw("GUILD_EMOJIS_UPDATE"),
            }),
        ),
        (
            "GUILD_INTEGRATIONS_UPDATE",
            Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
                guild_id: Some(snowflake("93")),
                raw: raw("GUILD_INTEGRATIONS_UPDATE"),
            }),
        ),
        (
            "ENTITLEMENT_CREATE",
            Event::EntitlementCreate(EntitlementEvent {
                entitlement: Entitlement {
                    id: snowflake("930"),
                    sku_id: snowflake("931"),
                    application_id: snowflake("932"),
                    kind: 8,
                    deleted: false,
                    ..Entitlement::default()
                },
                raw: raw("ENTITLEMENT_CREATE"),
            }),
        ),
        (
            "SUBSCRIPTION_CREATE",
            Event::SubscriptionCreate(SubscriptionEvent {
                subscription: Subscription {
                    id: snowflake("940"),
                    user_id: snowflake("941"),
                    current_period_start: "2026-04-01T00:00:00Z".to_string(),
                    current_period_end: "2026-05-01T00:00:00Z".to_string(),
                    status: 0,
                    ..Subscription::default()
                },
                raw: raw("SUBSCRIPTION_CREATE"),
            }),
        ),
        (
            "INTEGRATION_CREATE",
            Event::IntegrationCreate(IntegrationEvent {
                guild_id: Some(snowflake("942")),
                integration: Integration {
                    id: snowflake("943"),
                    name: "integration".to_string(),
                    kind: "discord".to_string(),
                    account: IntegrationAccount {
                        id: "account".to_string(),
                        name: "account".to_string(),
                    },
                    ..Integration::default()
                },
                raw: raw("INTEGRATION_CREATE"),
            }),
        ),
        (
            "INTEGRATION_DELETE",
            Event::IntegrationDelete(IntegrationDeleteEvent {
                id: Some(snowflake("944")),
                guild_id: Some(snowflake("945")),
                application_id: Some(snowflake("946")),
                raw: raw("INTEGRATION_DELETE"),
            }),
        ),
        (
            "GUILD_SOUNDBOARD_SOUND_CREATE",
            Event::GuildSoundboardSoundCreate(SoundboardSoundEvent {
                sound: SoundboardSound {
                    name: "quack".to_string(),
                    sound_id: snowflake("933"),
                    guild_id: Some(snowflake("934")),
                    volume: 1.0,
                    available: true,
                    ..SoundboardSound::default()
                },
                raw: raw("GUILD_SOUNDBOARD_SOUND_CREATE"),
            }),
        ),
        (
            "GUILD_SOUNDBOARD_SOUND_DELETE",
            Event::GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent {
                sound_id: snowflake("935"),
                guild_id: snowflake("936"),
                raw: raw("GUILD_SOUNDBOARD_SOUND_DELETE"),
            }),
        ),
        (
            "SOUNDBOARD_SOUNDS",
            Event::SoundboardSounds(SoundboardSoundsEvent {
                guild_id: snowflake("937"),
                soundboard_sounds: vec![SoundboardSound {
                    name: "quack".to_string(),
                    sound_id: snowflake("938"),
                    volume: 1.0,
                    available: true,
                    ..SoundboardSound::default()
                }],
                raw: raw("SOUNDBOARD_SOUNDS"),
            }),
        ),
        (
            "ENTITLEMENT_UPDATE",
            Event::EntitlementUpdate(EntitlementEvent {
                entitlement: Entitlement {
                    id: snowflake("939"),
                    sku_id: snowflake("940"),
                    application_id: snowflake("941"),
                    kind: 8,
                    deleted: false,
                    ..Entitlement::default()
                },
                raw: raw("ENTITLEMENT_UPDATE"),
            }),
        ),
        (
            "ENTITLEMENT_DELETE",
            Event::EntitlementDelete(EntitlementEvent {
                entitlement: Entitlement {
                    id: snowflake("942"),
                    sku_id: snowflake("943"),
                    application_id: snowflake("944"),
                    kind: 8,
                    deleted: true,
                    ..Entitlement::default()
                },
                raw: raw("ENTITLEMENT_DELETE"),
            }),
        ),
        (
            "SUBSCRIPTION_UPDATE",
            Event::SubscriptionUpdate(SubscriptionEvent {
                subscription: Subscription {
                    id: snowflake("945"),
                    user_id: snowflake("946"),
                    current_period_start: "2026-04-01T00:00:00Z".to_string(),
                    current_period_end: "2026-05-01T00:00:00Z".to_string(),
                    status: 1,
                    ..Subscription::default()
                },
                raw: raw("SUBSCRIPTION_UPDATE"),
            }),
        ),
        (
            "SUBSCRIPTION_DELETE",
            Event::SubscriptionDelete(SubscriptionEvent {
                subscription: Subscription {
                    id: snowflake("947"),
                    user_id: snowflake("948"),
                    current_period_start: "2026-04-01T00:00:00Z".to_string(),
                    current_period_end: "2026-05-01T00:00:00Z".to_string(),
                    status: 2,
                    ..Subscription::default()
                },
                raw: raw("SUBSCRIPTION_DELETE"),
            }),
        ),
        (
            "INTEGRATION_UPDATE",
            Event::IntegrationUpdate(IntegrationEvent {
                guild_id: Some(snowflake("949")),
                integration: Integration {
                    id: snowflake("950"),
                    name: "updated-integration".to_string(),
                    kind: "discord".to_string(),
                    account: IntegrationAccount {
                        id: "account".to_string(),
                        name: "account".to_string(),
                    },
                    ..Integration::default()
                },
                raw: raw("INTEGRATION_UPDATE"),
            }),
        ),
        (
            "GUILD_SOUNDBOARD_SOUND_UPDATE",
            Event::GuildSoundboardSoundUpdate(SoundboardSoundEvent {
                sound: SoundboardSound {
                    name: "updated".to_string(),
                    sound_id: snowflake("951"),
                    guild_id: Some(snowflake("952")),
                    volume: 1.0,
                    available: true,
                    ..SoundboardSound::default()
                },
                raw: raw("GUILD_SOUNDBOARD_SOUND_UPDATE"),
            }),
        ),
        (
            "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
            Event::GuildSoundboardSoundsUpdate(SoundboardSoundsEvent {
                guild_id: snowflake("953"),
                soundboard_sounds: Vec::new(),
                raw: raw("GUILD_SOUNDBOARD_SOUNDS_UPDATE"),
            }),
        ),
        (
            "WEBHOOKS_UPDATE",
            Event::WebhooksUpdate(WebhooksUpdateEvent {
                guild_id: Some(snowflake("94")),
                channel_id: Some(snowflake("95")),
                raw: raw("WEBHOOKS_UPDATE"),
            }),
        ),
        (
            "INVITE_CREATE",
            Event::InviteCreate(InviteEvent {
                guild_id: Some(snowflake("96")),
                channel_id: Some(snowflake("97")),
                code: Some("invite-create".to_string()),
                raw: raw("INVITE_CREATE"),
            }),
        ),
        (
            "INVITE_DELETE",
            Event::InviteDelete(InviteEvent {
                guild_id: Some(snowflake("98")),
                channel_id: Some(snowflake("99")),
                code: Some("invite-delete".to_string()),
                raw: raw("INVITE_DELETE"),
            }),
        ),
        (
            "MESSAGE_POLL_VOTE_ADD",
            Event::MessagePollVoteAdd(PollVoteEvent {
                user_id: Some(snowflake("980")),
                channel_id: Some(snowflake("981")),
                message_id: Some(snowflake("982")),
                guild_id: Some(snowflake("983")),
                answer_id: Some(1),
                raw: raw("MESSAGE_POLL_VOTE_ADD"),
            }),
        ),
        (
            "MESSAGE_POLL_VOTE_REMOVE",
            Event::MessagePollVoteRemove(PollVoteEvent {
                user_id: Some(snowflake("984")),
                channel_id: Some(snowflake("985")),
                message_id: Some(snowflake("986")),
                guild_id: Some(snowflake("987")),
                answer_id: Some(2),
                raw: raw("MESSAGE_POLL_VOTE_REMOVE"),
            }),
        ),
        (
            "TYPING_START",
            Event::TypingStart(TypingStartEvent {
                channel_id: Some(snowflake("100")),
                guild_id: Some(snowflake("101")),
                user_id: Some(snowflake("102")),
                timestamp: Some(123),
                raw: raw("TYPING_START"),
            }),
        ),
        (
            "PRESENCE_UPDATE",
            Event::PresenceUpdate(PresenceUpdateEvent {
                user_id: Some(snowflake("103")),
                guild_id: Some(snowflake("104")),
                status: Some("idle".to_string()),
                raw: raw("PRESENCE_UPDATE"),
                ..PresenceUpdateEvent::default()
            }),
        ),
        (
            "THREAD_CREATE",
            Event::ThreadCreate(ThreadEvent {
                thread: channel("110"),
                raw: raw("THREAD_CREATE"),
            }),
        ),
        (
            "THREAD_UPDATE",
            Event::ThreadUpdate(ThreadEvent {
                thread: channel("111"),
                raw: raw("THREAD_UPDATE"),
            }),
        ),
        (
            "THREAD_DELETE",
            Event::ThreadDelete(ThreadEvent {
                thread: channel("112"),
                raw: raw("THREAD_DELETE"),
            }),
        ),
        (
            "THREAD_LIST_SYNC",
            Event::ThreadListSync(ThreadListSyncEvent {
                guild_id: Some(snowflake("113")),
                threads: vec![channel("114")],
                raw: raw("THREAD_LIST_SYNC"),
            }),
        ),
        (
            "THREAD_MEMBER_UPDATE",
            Event::ThreadMemberUpdate(ThreadMemberUpdateEvent {
                guild_id: Some(snowflake("115")),
                thread_id: Some(snowflake("116")),
                raw: raw("THREAD_MEMBER_UPDATE"),
            }),
        ),
        (
            "THREAD_MEMBERS_UPDATE",
            Event::ThreadMembersUpdate(ThreadMembersUpdateEvent {
                guild_id: Some(snowflake("117")),
                thread_id: Some(snowflake("118")),
                added_members: Some(vec![json!({"id": "119"})]),
                removed_member_ids: Some(vec![snowflake("120")]),
                member_count: Some(2),
                raw: raw("THREAD_MEMBERS_UPDATE"),
            }),
        ),
        (
            "MESSAGE_REACTION_REMOVE_EMOJI",
            Event::MessageReactionRemoveEmoji(ReactionRemoveEmojiEvent {
                channel_id: Some(snowflake("121")),
                message_id: Some(snowflake("122")),
                guild_id: Some(snowflake("123")),
                emoji: Some(Emoji::unicode("wave")),
                raw: raw("MESSAGE_REACTION_REMOVE_EMOJI"),
            }),
        ),
        (
            "GUILD_STICKERS_UPDATE",
            Event::GuildStickersUpdate(GuildStickersUpdateEvent {
                guild_id: Some(snowflake("124")),
                stickers: Vec::new(),
                raw: raw("GUILD_STICKERS_UPDATE"),
            }),
        ),
        (
            "GUILD_SCHEDULED_EVENT_UPDATE",
            Event::GuildScheduledEventUpdate(ScheduledEvent {
                id: Some(snowflake("125")),
                guild_id: Some(snowflake("126")),
                raw: raw("GUILD_SCHEDULED_EVENT_UPDATE"),
                ..ScheduledEvent::default()
            }),
        ),
        (
            "GUILD_SCHEDULED_EVENT_DELETE",
            Event::GuildScheduledEventDelete(ScheduledEvent {
                id: Some(snowflake("127")),
                guild_id: Some(snowflake("128")),
                raw: raw("GUILD_SCHEDULED_EVENT_DELETE"),
                ..ScheduledEvent::default()
            }),
        ),
        (
            "GUILD_SCHEDULED_EVENT_USER_ADD",
            Event::GuildScheduledEventUserAdd(GuildScheduledEventUserEvent {
                guild_scheduled_event_id: snowflake("129"),
                user_id: snowflake("130"),
                guild_id: snowflake("131"),
                member: None,
                user: None,
                raw: raw("GUILD_SCHEDULED_EVENT_USER_ADD"),
            }),
        ),
        (
            "GUILD_SCHEDULED_EVENT_USER_REMOVE",
            Event::GuildScheduledEventUserRemove(GuildScheduledEventUserEvent {
                guild_scheduled_event_id: snowflake("132"),
                user_id: snowflake("133"),
                guild_id: snowflake("134"),
                member: None,
                user: None,
                raw: raw("GUILD_SCHEDULED_EVENT_USER_REMOVE"),
            }),
        ),
        (
            "STAGE_INSTANCE_UPDATE",
            Event::StageInstanceUpdate(StageInstanceEvent {
                stage_instance: StageInstance {
                    id: snowflake("135"),
                    guild_id: snowflake("136"),
                    channel_id: snowflake("137"),
                    topic: "updated".to_string(),
                    privacy_level: 2,
                    ..StageInstance::default()
                },
                raw: raw("STAGE_INSTANCE_UPDATE"),
            }),
        ),
        (
            "STAGE_INSTANCE_DELETE",
            Event::StageInstanceDelete(StageInstanceEvent {
                stage_instance: StageInstance {
                    id: snowflake("138"),
                    guild_id: snowflake("139"),
                    channel_id: snowflake("140"),
                    topic: "deleted".to_string(),
                    privacy_level: 2,
                    ..StageInstance::default()
                },
                raw: raw("STAGE_INSTANCE_DELETE"),
            }),
        ),
        (
            "VOICE_CHANNEL_EFFECT_SEND",
            Event::VoiceChannelEffectSend(VoiceChannelEffectEvent {
                channel_id: Some(snowflake("141")),
                guild_id: Some(snowflake("142")),
                user_id: Some(snowflake("143")),
                emoji: Some(Emoji::unicode("wave")),
                animation_type: Some(1),
                animation_id: Some(2),
                sound_id: Some(snowflake("144")),
                sound_volume: Some(0.5),
                raw: raw("VOICE_CHANNEL_EFFECT_SEND"),
            }),
        ),
        (
            "VOICE_CHANNEL_START_TIME_UPDATE",
            Event::VoiceChannelStartTimeUpdate(VoiceChannelStartTimeUpdateEvent {
                channel_id: Some(snowflake("145")),
                guild_id: Some(snowflake("146")),
                voice_channel_start_time: Some("2026-05-02T00:00:00Z".to_string()),
                raw: raw("VOICE_CHANNEL_START_TIME_UPDATE"),
            }),
        ),
        (
            "VOICE_CHANNEL_STATUS_UPDATE",
            Event::VoiceChannelStatusUpdate(VoiceChannelStatusUpdateEvent {
                channel_id: Some(snowflake("147")),
                guild_id: Some(snowflake("148")),
                status: Some("live".to_string()),
                raw: raw("VOICE_CHANNEL_STATUS_UPDATE"),
            }),
        ),
        (
            "CHANNEL_INFO",
            Event::ChannelInfo(ChannelInfoEvent {
                guild_id: snowflake("149"),
                channels: vec![ChannelInfoChannel {
                    id: snowflake("150"),
                    status: Some("live".to_string()),
                    voice_start_time: Some("2026-05-02T00:00:00Z".to_string()),
                    raw: raw("CHANNEL_INFO_CHANNEL"),
                }],
                raw: raw("CHANNEL_INFO"),
            }),
        ),
        (
            "RATE_LIMITED",
            Event::RateLimited(RateLimitedEvent {
                opcode: Some(8),
                retry_after: Some(1.5),
                meta: Some(json!({"nonce": "members"})),
                raw: raw("RATE_LIMITED"),
            }),
        ),
        (
            "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
            Event::ApplicationCommandPermissionsUpdate(ApplicationCommandPermissionsUpdateEvent {
                id: Some(snowflake("151")),
                application_id: Some(snowflake("152")),
                guild_id: Some(snowflake("153")),
                permissions: vec![json!({"id": "154", "type": 1, "permission": true})],
                raw: raw("APPLICATION_COMMAND_PERMISSIONS_UPDATE"),
            }),
        ),
        (
            "AUTO_MODERATION_RULE_UPDATE",
            Event::AutoModerationRuleUpdate(AutoModerationEvent {
                id: Some(snowflake("155")),
                guild_id: Some(snowflake("156")),
                name: Some("updated".to_string()),
                creator_id: None,
                event_type: None,
                trigger_type: None,
                trigger_metadata: None,
                actions: Vec::new(),
                enabled: None,
                exempt_roles: Vec::new(),
                exempt_channels: Vec::new(),
                action: None,
                rule_id: None,
                rule_trigger_type: None,
                user_id: None,
                channel_id: None,
                message_id: None,
                alert_system_message_id: None,
                content: None,
                matched_keyword: None,
                matched_content: None,
                raw: raw("AUTO_MODERATION_RULE_UPDATE"),
            }),
        ),
        (
            "AUTO_MODERATION_RULE_DELETE",
            Event::AutoModerationRuleDelete(AutoModerationEvent {
                id: Some(snowflake("157")),
                guild_id: Some(snowflake("158")),
                name: Some("deleted".to_string()),
                creator_id: None,
                event_type: None,
                trigger_type: None,
                trigger_metadata: None,
                actions: Vec::new(),
                enabled: None,
                exempt_roles: Vec::new(),
                exempt_channels: Vec::new(),
                action: None,
                rule_id: None,
                rule_trigger_type: None,
                user_id: None,
                channel_id: None,
                message_id: None,
                alert_system_message_id: None,
                content: None,
                matched_keyword: None,
                matched_content: None,
                raw: raw("AUTO_MODERATION_RULE_DELETE"),
            }),
        ),
        (
            "SOMETHING_NEW",
            Event::Unknown {
                kind: "SOMETHING_NEW".to_string(),
                raw: raw("SOMETHING_NEW"),
            },
        ),
    ];

    for (kind, event) in cases {
        assert_kind_and_raw(event, kind);
    }
}

#[test]
fn decode_event_handles_entitlement_and_soundboard_events() {
    let entitlement = decode_event(
        "ENTITLEMENT_UPDATE",
        json!({
            "id": "1",
            "sku_id": "2",
            "application_id": "3",
            "type": 8,
            "deleted": false,
            "consumed": false
        }),
    )
    .unwrap();
    match entitlement {
        Event::EntitlementUpdate(event) => {
            assert_eq!(event.entitlement.sku_id.as_str(), "2");
            assert!(!event.entitlement.deleted);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let subscription = decode_event(
        "SUBSCRIPTION_UPDATE",
        json!({
            "id": "30",
            "user_id": "31",
            "sku_ids": ["32"],
            "entitlement_ids": ["33"],
            "current_period_start": "2026-04-01T00:00:00Z",
            "current_period_end": "2026-05-01T00:00:00Z",
            "status": 1
        }),
    )
    .unwrap();
    match subscription {
        Event::SubscriptionUpdate(event) => {
            assert_eq!(event.subscription.id.as_str(), "30");
            assert_eq!(event.subscription.status, 1);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let integration = decode_event(
        "INTEGRATION_CREATE",
        json!({
            "id": "40",
            "guild_id": "41",
            "name": "integration",
            "type": "discord",
            "enabled": true,
            "account": { "id": "acc", "name": "account" }
        }),
    )
    .unwrap();
    match integration {
        Event::IntegrationCreate(event) => {
            assert_eq!(event.guild_id.unwrap().as_str(), "41");
            assert_eq!(event.integration.id.as_str(), "40");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let integration_delete = decode_event(
        "INTEGRATION_DELETE",
        json!({
            "id": "40",
            "guild_id": "41",
            "application_id": "42"
        }),
    )
    .unwrap();
    match integration_delete {
        Event::IntegrationDelete(event) => {
            assert_eq!(event.id.unwrap().as_str(), "40");
            assert_eq!(event.application_id.unwrap().as_str(), "42");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let poll_vote = decode_event(
        "MESSAGE_POLL_VOTE_REMOVE",
        json!({
            "user_id": "50",
            "channel_id": "51",
            "message_id": "52",
            "guild_id": "53",
            "answer_id": 2
        }),
    )
    .unwrap();
    match poll_vote {
        Event::MessagePollVoteRemove(event) => {
            assert_eq!(event.user_id.unwrap().as_str(), "50");
            assert_eq!(event.answer_id, Some(2));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let sound = decode_event(
        "GUILD_SOUNDBOARD_SOUND_UPDATE",
        json!({
            "name": "quack",
            "sound_id": "10",
            "volume": 1.0,
            "guild_id": "20",
            "available": true
        }),
    )
    .unwrap();
    match sound {
        Event::GuildSoundboardSoundUpdate(event) => {
            assert_eq!(event.sound.sound_id.as_str(), "10");
            assert_eq!(event.sound.guild_id.unwrap().as_str(), "20");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let delete = decode_event(
        "GUILD_SOUNDBOARD_SOUND_DELETE",
        json!({
            "sound_id": "10",
            "guild_id": "20"
        }),
    )
    .unwrap();
    match delete {
        Event::GuildSoundboardSoundDelete(event) => {
            assert_eq!(event.sound_id.as_str(), "10");
            assert_eq!(event.guild_id.as_str(), "20");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let sounds = decode_event(
        "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
        json!({
            "guild_id": "20",
            "soundboard_sounds": [{
                "name": "quack",
                "sound_id": "10",
                "volume": 1.0,
                "available": true
            }]
        }),
    )
    .unwrap();
    match sounds {
        Event::GuildSoundboardSoundsUpdate(event) => {
            assert_eq!(event.guild_id.as_str(), "20");
            assert_eq!(event.soundboard_sounds.len(), 1);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_structural_decoder_branches() {
    fn assert_decodes_with_raw(kind: &str, payload: Value) {
        let event = decode_event(kind, payload.clone()).expect(kind);
        assert_eq!(event.kind(), kind);
        assert_eq!(event.raw(), &payload);
    }

    for kind in ["THREAD_CREATE", "THREAD_UPDATE", "THREAD_DELETE"] {
        assert_decodes_with_raw(kind, json!({ "id": "210", "type": 0 }));
    }

    assert_decodes_with_raw(
        "THREAD_LIST_SYNC",
        json!({
            "guild_id": "211",
            "threads": [{ "id": "212", "type": 0 }]
        }),
    );
    assert_decodes_with_raw(
        "THREAD_MEMBER_UPDATE",
        json!({
            "guild_id": "213",
            "id": "214"
        }),
    );
    assert_decodes_with_raw(
        "THREAD_MEMBERS_UPDATE",
        json!({
            "guild_id": "215",
            "id": "216",
            "added_members": [{ "id": "217" }],
            "removed_member_ids": ["218"],
            "member_count": 2
        }),
    );
    assert_decodes_with_raw(
        "MESSAGE_REACTION_REMOVE_EMOJI",
        json!({
            "channel_id": "219",
            "message_id": "220",
            "guild_id": "221",
            "emoji": { "name": "wave" }
        }),
    );
    assert_decodes_with_raw(
        "GUILD_STICKERS_UPDATE",
        json!({
            "guild_id": "222",
            "stickers": [{
                "id": "223",
                "name": "wave",
                "tags": "wave",
                "type": 1,
                "format_type": 1
            }]
        }),
    );

    for kind in ["ENTITLEMENT_CREATE", "ENTITLEMENT_DELETE"] {
        assert_decodes_with_raw(
            kind,
            json!({
                "id": "224",
                "sku_id": "225",
                "application_id": "226",
                "type": 8,
                "deleted": false
            }),
        );
    }

    for kind in ["SUBSCRIPTION_CREATE", "SUBSCRIPTION_DELETE"] {
        assert_decodes_with_raw(
            kind,
            json!({
                "id": "227",
                "user_id": "228",
                "sku_ids": ["229"],
                "entitlement_ids": ["230"],
                "current_period_start": "2026-05-02T00:00:00Z",
                "current_period_end": "2026-06-02T00:00:00Z",
                "status": 1
            }),
        );
    }

    for kind in ["INTEGRATION_CREATE", "INTEGRATION_UPDATE"] {
        assert_decodes_with_raw(
            kind,
            json!({
                "id": "231",
                "name": "GitHub",
                "type": "github",
                "guild_id": "232",
                "account": {
                    "id": "acct",
                    "name": "account"
                }
            }),
        );
    }
    assert_decodes_with_raw(
        "INTEGRATION_DELETE",
        json!({
            "id": "233",
            "guild_id": "234",
            "application_id": "235"
        }),
    );
    assert_decodes_with_raw(
        "GUILD_SOUNDBOARD_SOUND_CREATE",
        json!({
            "name": "ding",
            "sound_id": "236",
            "volume": 1.0,
            "guild_id": "237",
            "available": true
        }),
    );
    assert_decodes_with_raw(
        "STAGE_INSTANCE_CREATE",
        json!({
            "id": "238",
            "guild_id": "239",
            "channel_id": "240",
            "topic": "town hall",
            "privacy_level": 2
        }),
    );
    assert_decodes_with_raw(
        "VOICE_CHANNEL_EFFECT_SEND",
        json!({
            "channel_id": "241",
            "guild_id": "242",
            "user_id": "243",
            "emoji": { "name": "sparkles" },
            "animation_type": 1,
            "animation_id": 2,
            "sound_id": "244",
            "sound_volume": 0.5
        }),
    );
    assert_decodes_with_raw(
        "VOICE_CHANNEL_START_TIME_UPDATE",
        json!({
            "channel_id": "245",
            "guild_id": "246",
            "voice_channel_start_time": "2026-05-02T00:00:00Z"
        }),
    );
    assert_decodes_with_raw(
        "VOICE_CHANNEL_STATUS_UPDATE",
        json!({
            "channel_id": "247",
            "guild_id": "248",
            "status": "live"
        }),
    );
    assert_decodes_with_raw(
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
        json!({
            "id": "249",
            "application_id": "250",
            "guild_id": "251",
            "permissions": [{ "id": "252", "type": 1, "permission": true }]
        }),
    );
    assert_decodes_with_raw(
        "AUTO_MODERATION_RULE_CREATE",
        json!({
            "id": "253",
            "guild_id": "254",
            "name": "links",
            "creator_id": "255",
            "event_type": 1,
            "trigger_type": 1,
            "actions": [{ "type": 1 }],
            "enabled": true,
            "exempt_roles": ["256"],
            "exempt_channels": ["257"]
        }),
    );
    assert_decodes_with_raw(
        "AUTO_MODERATION_ACTION_EXECUTION",
        json!({
            "guild_id": "258",
            "action": { "type": 1 },
            "rule_id": "259",
            "rule_trigger_type": 1,
            "user_id": "260",
            "channel_id": "261",
            "message_id": "262",
            "alert_system_message_id": "263",
            "content": "blocked",
            "matched_keyword": "bad",
            "matched_content": "bad link"
        }),
    );
    assert_decodes_with_raw(
        "GUILD_AUDIT_LOG_ENTRY_CREATE",
        json!({
            "id": "264",
            "guild_id": "265",
            "user_id": "266",
            "target_id": "267",
            "action_type": 1,
            "changes": [],
            "options": { "count": "1" },
            "reason": "review"
        }),
    );
}
