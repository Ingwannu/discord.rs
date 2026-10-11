use std::sync::Arc;

use serde_json::{json, Value};

use super::*;
use crate::error::DiscordError;
use crate::model::{
    test_id, AutoModerationActionType, ChannelType, GuildCreatePayload, Id, IdMarker, Interaction,
    Patch, SubscriptionStatus,
};

/// The 2.x-style test entry: decodes a `Value` payload and reports a
/// payload that does not fit its type as an error, as the old decoder did.
fn decode_event(name: &str, payload: Value) -> Result<Event, DiscordError> {
    let event = super::decode_event(name, &payload.to_string())?;
    if let EventKind::DecodeFailed(error) = event.kind() {
        return Err(DiscordError::from(error.to_string()));
    }
    Ok(event)
}

/// The raw payload of `event` as a `Value`, for comparisons.
fn raw_json(event: &Event) -> Option<Value> {
    event
        .raw()
        .map(|raw| serde_json::from_str(raw.get()).expect("raw payloads are JSON"))
}

fn snowflake<M: IdMarker>(id: &str) -> Id<M> {
    id.parse().unwrap()
}

fn raw_box(json: &str) -> Box<RawValue> {
    RawValue::from_string(json.to_string()).unwrap()
}

fn metadata(name: &str) -> EventMetadata {
    EventMetadata::new(name, Instant::now())
}

#[test]
fn event_and_kind_stay_within_their_layout_budget() {
    // Design gate (docs/design/3.0-types-events-requests.md, phase 4):
    // every delivery moves an `Arc<Event>`, and these sizes bound what the
    // journal, queues, and handler futures hold per event.
    assert!(
        std::mem::size_of::<EventKind>() <= 64,
        "EventKind is {} bytes",
        std::mem::size_of::<EventKind>()
    );
    assert!(
        std::mem::size_of::<Event>() <= 192,
        "Event is {} bytes",
        std::mem::size_of::<Event>()
    );
}

#[test]
fn known_events_keep_raw_only_when_asked() {
    let payload = r#"{"id":"2","channel_id":"1","content":"hi"}"#;
    let dropped = Event::decode(metadata("MESSAGE_CREATE"), raw_box(payload), false);
    assert!(matches!(dropped.kind(), EventKind::MessageCreate(_)));
    assert!(dropped.raw().is_none());

    let kept = Event::decode(metadata("MESSAGE_CREATE"), raw_box(payload), true);
    assert!(matches!(kept.kind(), EventKind::MessageCreate(_)));
    assert_eq!(kept.raw().map(RawValue::get), Some(payload));
}

#[test]
fn unknown_and_failed_dispatches_always_keep_raw_and_stay_distinct() {
    let unknown = Event::decode(metadata("SOMETHING_NEW"), raw_box(r#"{"x":1}"#), false);
    assert!(matches!(unknown.kind(), EventKind::Unknown));
    assert_eq!(unknown.name(), "SOMETHING_NEW");
    assert_eq!(unknown.raw().map(RawValue::get), Some(r#"{"x":1}"#));

    let failed = Event::decode(
        metadata("GUILD_MEMBER_ADD"),
        raw_box(r#"{"user":{}}"#),
        false,
    );
    let EventKind::DecodeFailed(error) = failed.kind() else {
        panic!("expected a decode failure, got {failed:?}");
    };
    assert!(error.message().contains("guild_id"), "{error}");
    assert_eq!(error.to_string(), error.message());
    assert_eq!(failed.raw().map(RawValue::get), Some(r#"{"user":{}}"#));
}

#[test]
fn debug_output_never_shows_the_raw_payload() {
    let secret = r#"{"id":"1","application_id":"2","token":"very-secret-token","type":1}"#;
    let event = Event::decode(metadata("SOMETHING_NEW"), raw_box(secret), true);
    let debug = format!("{event:?}");
    assert!(!debug.contains("very-secret-token"), "{debug}");
    assert!(
        debug.contains(&format!("<{} bytes>", secret.len())),
        "{debug}"
    );

    let without_raw = Event::decode(metadata("RESUMED"), raw_box("null"), false);
    assert!(format!("{without_raw:?}").contains("raw: None"));
}

#[test]
fn metadata_records_shard_sequence_and_receive_time() {
    let received_at = Instant::now();
    let metadata = EventMetadata::new("MESSAGE_CREATE", received_at)
        .with_shard(3, 0)
        .with_sequence(42);
    assert_eq!(metadata.name(), "MESSAGE_CREATE");
    assert_eq!(metadata.shard(), (3, 1), "a zero shard count is one");
    assert_eq!(metadata.sequence(), Some(42));
    assert_eq!(metadata.received_at(), received_at);
    assert_eq!(EventMetadata::new("X", received_at).sequence(), None);

    let event = Event::decode(metadata.clone(), raw_box("null"), false);
    assert_eq!(event.metadata(), &metadata);
    assert!(matches!(event.into_kind(), EventKind::DecodeFailed(_)));
}

#[test]
fn dispatch_names_are_interned_from_one_sorted_table() {
    assert!(
        decode::KNOWN_DISPATCHES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "the dispatch table must stay sorted for the binary search"
    );
    for name in decode::KNOWN_DISPATCHES {
        assert!(matches!(intern(name), Cow::Borrowed(_)), "{name}");
        // Every known name has a decoder: an empty object either decodes or
        // fails to decode, but is never `Unknown`.
        let event = Event::decode(metadata(name), raw_box("{}"), false);
        assert!(!matches!(event.kind(), EventKind::Unknown), "{name}");
    }
    assert!(matches!(intern("NOT_A_DISPATCH"), Cow::Owned(_)));
}

fn intern(name: &str) -> Cow<'static, str> {
    decode::intern_dispatch_name(name)
}

#[test]
fn decode_event_rejects_text_that_is_not_json() {
    assert!(super::decode_event("MESSAGE_CREATE", "{not json").is_err());
}

#[test]
fn shared_payloads_are_arcs_and_member_events_share_their_member() {
    let created = Event::decode(
        metadata("GUILD_MEMBER_ADD"),
        raw_box(r#"{"guild_id":"1","user":{"id":"2","username":"a"},"roles":[]}"#),
        false,
    );
    let EventKind::MemberAdd(member) = created.kind() else {
        panic!("unexpected {created:?}");
    };
    assert_eq!(member.guild_id, snowflake("1"));
    assert_eq!(Arc::strong_count(&member.member), 1);
    let shared = Arc::clone(&member.member);
    assert_eq!(Arc::strong_count(&shared), 2);
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

    match event.kind() {
        EventKind::MessageCreate(message) => {
            assert_eq!(message.content, "hello");
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
    match emojis_update.kind() {
        EventKind::GuildEmojisUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("1"));
            assert!(event.emojis.is_empty());
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
    match webhooks_update.kind() {
        EventKind::WebhooksUpdate(event) => {
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
    match invite_create.kind() {
        EventKind::InviteCreate(event) => {
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
    match invite_delete.kind() {
        EventKind::InviteDelete(event) => {
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
    match pins_update.kind() {
        EventKind::ChannelPinsUpdate(event) => {
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
    match typing_start.kind() {
        EventKind::TypingStart(event) => {
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
    match presence_update.kind() {
        EventKind::PresenceUpdate(event) => {
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
    match integrations_update.kind() {
        EventKind::GuildIntegrationsUpdate(event) => {
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
    match member_add.kind() {
        EventKind::MemberAdd(event) => {
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
    match role_create.kind() {
        EventKind::RoleCreate(event) => {
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
    match bulk_delete.kind() {
        EventKind::MessageDeleteBulk(event) => {
            assert_eq!(event.ids, vec![snowflake("10"), snowflake("11")]);
            assert_eq!(event.channel_id, snowflake("12"));
            assert_eq!(event.guild_id, Some(snowflake("13")));
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
    match pins_update.kind() {
        EventKind::ChannelPinsUpdate(event) => {
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
    match typing_start.kind() {
        EventKind::TypingStart(event) => {
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
    match presence_update.kind() {
        EventKind::PresenceUpdate(event) => {
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
    .kind()
    {
        EventKind::GuildCreate(GuildCreatePayload::Available(event)) => {
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
    .kind()
    {
        EventKind::ChannelCreate(event) => {
            assert_eq!(event.id, snowflake("2"));
            assert_eq!(event.name.as_deref(), Some("general"));
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
    .kind()
    {
        EventKind::MemberRemove(event) => {
            assert_eq!(event.guild_id, snowflake("3"));
            assert_eq!(event.user.id, snowflake("4"));
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
    .kind()
    {
        EventKind::RoleDelete(event) => {
            assert_eq!(event.guild_id, snowflake("5"));
            assert_eq!(event.role_id, snowflake("6"));
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
    .kind()
    {
        EventKind::GuildScheduledEventCreate(event) => {
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
            assert_eq!(event.status, Some(ScheduledEventStatus::SCHEDULED));
            assert_eq!(event.entity_type, Some(ScheduledEventEntityType::VOICE));
            assert_eq!(event.entity_id, Some(snowflake("704")));
            assert_eq!(
                event
                    .entity_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.location.as_deref()),
                Some("voice")
            );
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
    .kind()
    {
        EventKind::AutoModerationRuleCreate(event) => {
            assert_eq!(event.id, Some(snowflake("710")));
            assert_eq!(event.guild_id, Some(snowflake("711")));
            assert_eq!(event.name.as_deref(), Some("Keyword Filter"));
            assert_eq!(event.creator_id, Some(snowflake("712")));
            assert_eq!(
                event.event_type,
                Some(AutoModerationEventType::MESSAGE_SEND)
            );
            assert_eq!(event.trigger_type, Some(AutoModerationTriggerType::KEYWORD));
            let trigger_metadata = event
                .trigger_metadata
                .as_ref()
                .expect("trigger metadata should decode");
            assert_eq!(trigger_metadata.keyword_filter, vec!["bad".to_string()]);
            assert_eq!(event.actions.len(), 1);
            assert_eq!(
                event.actions[0].kind,
                AutoModerationActionType::BLOCK_MESSAGE
            );
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
    .kind()
    {
        EventKind::AutoModerationActionExecution(event) => {
            assert_eq!(event.guild_id, Some(snowflake("720")));
            let action = event.action.as_ref().expect("action should decode");
            assert_eq!(action.kind, AutoModerationActionType::SEND_ALERT_MESSAGE);
            assert_eq!(
                action.metadata.as_ref().and_then(|m| m.channel_id),
                Some(snowflake("721"))
            );
            assert_eq!(event.rule_id, Some(snowflake("722")));
            assert_eq!(
                event.rule_trigger_type,
                Some(AutoModerationTriggerType::KEYWORD)
            );
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
    .kind()
    {
        EventKind::GuildAuditLogEntryCreate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("730")));
            assert_eq!(event.id, Some(snowflake("731")));
            assert_eq!(event.user_id, Some(snowflake("732")));
            assert_eq!(event.target_id, Some(snowflake("733")));
            assert_eq!(event.action_type, Some(AuditLogActionType::MEMBER_BAN_ADD));
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
                    .and_then(|entry| entry.id)
                    .map(u64::from),
                Some(731)
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
    .kind()
    {
        EventKind::UserUpdate(event) => {
            assert_eq!(event.id, snowflake("740"));
            assert_eq!(event.username, "bot");
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
    assert_eq!(rate_limited.name(), "RATE_LIMITED");
    match rate_limited.kind() {
        EventKind::RateLimited(event) => {
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
    .kind()
    {
        EventKind::VoiceStateUpdate(event) => {
            assert_eq!(event.guild_id, Some(snowflake("1")));
            assert_eq!(event.channel_id, Some(snowflake("2")));
            assert_eq!(event.user_id, Some(snowflake("3")));
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
    .kind()
    {
        EventKind::VoiceServerUpdate(event) => {
            assert_eq!(event.guild_id, snowflake("4"));
            assert_eq!(event.token, "voice-token");
            assert_eq!(event.endpoint.as_deref(), Some("wss://voice.discord.test"));
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
    .kind()
    {
        EventKind::GuildBanAdd(event) => {
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
    .kind()
    {
        EventKind::MessageReactionAdd(event) => {
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
            assert_eq!(event.reaction_type, Some(ReactionType::BURST));
            assert_eq!(
                event.emoji.as_ref().and_then(|emoji| emoji.name.clone()),
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
    .kind()
    {
        EventKind::InteractionCreate(event) => {
            assert!(matches!(**event, Interaction::Ping(_)));
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
    .kind()
    {
        EventKind::Ready(event) => {
            assert_eq!(event.user.id, snowflake("50"));
            assert_eq!(event.session_id, "session-50");
            assert!(event.application.is_none());
            assert!(event.resume_gateway_url.is_none());
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
    .kind()
    {
        EventKind::GuildUpdate(event) => {
            assert_eq!(event.id, snowflake("51"));
            assert_eq!(event.name, "guild-update");
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
    .kind()
    {
        EventKind::GuildDelete(event) => {
            assert_eq!(event.id, snowflake("52"));
            assert_eq!(event.unavailable, None);
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
    .kind()
    {
        EventKind::ChannelUpdate(event) => {
            assert_eq!(event.id, snowflake("53"));
            assert_eq!(event.kind, ChannelType::GUILD_TEXT);
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
    .kind()
    {
        EventKind::ChannelDelete(event) => {
            assert_eq!(event.id, snowflake("54"));
            assert_eq!(event.kind, ChannelType::GUILD_TEXT);
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
    .kind()
    {
        EventKind::MemberUpdate(event) => {
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
    .kind()
    {
        EventKind::RoleUpdate(event) => {
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
    .kind()
    {
        EventKind::MessageUpdate(event) => {
            assert_eq!(event.id, snowflake("59"));
            assert_eq!(event.channel_id, snowflake("60"));
            assert_eq!(event.content, Patch::Value("edited".to_string()));
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
    .kind()
    {
        EventKind::MessageDelete(event) => {
            assert_eq!(event.id, snowflake("61"));
            assert_eq!(event.channel_id, snowflake("62"));
            assert_eq!(event.guild_id, None);
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
    .kind()
    {
        EventKind::GuildBanRemove(event) => {
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
    .kind()
    {
        EventKind::GuildEmojisUpdate(event) => {
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
    .kind()
    {
        EventKind::GuildIntegrationsUpdate(event) => {
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
    .kind()
    {
        EventKind::WebhooksUpdate(event) => {
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
    .kind()
    {
        EventKind::InviteDelete(event) => {
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
    .kind()
    {
        EventKind::MessageReactionRemove(event) => {
            assert_eq!(event.user_id, Some(snowflake("71")));
            assert_eq!(event.channel_id, Some(snowflake("72")));
            assert_eq!(event.message_id, Some(snowflake("73")));
            assert_eq!(event.guild_id, Some(snowflake("74")));
            assert_eq!(
                event.emoji.as_ref().and_then(|emoji| emoji.name.clone()),
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
    .kind()
    {
        EventKind::MessageReactionRemoveAll(event) => {
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
    .kind()
    {
        EventKind::Ready(event) => {
            assert_eq!(event.user.id, snowflake("80"));
            assert_eq!(
                event.application.as_ref().map(|app| app.id),
                Some(snowflake("81"))
            );
            assert_eq!(
                event.resume_gateway_url.as_deref(),
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
    .kind()
    {
        EventKind::GuildDelete(event) => {
            assert_eq!(event.id, snowflake("82"));
            assert_eq!(event.unavailable, Some(true));
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
    .kind()
    {
        EventKind::MessageDelete(event) => {
            assert_eq!(event.id, snowflake("83"));
            assert_eq!(event.channel_id, snowflake("84"));
            assert_eq!(event.guild_id, Some(snowflake("85")));
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
    .kind()
    {
        EventKind::ChannelPinsUpdate(event) => {
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
    .kind()
    {
        EventKind::GuildEmojisUpdate(event) => {
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
    .kind()
    {
        EventKind::InviteCreate(event) => {
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
    .kind()
    {
        EventKind::GuildMembersChunk(event) => {
            assert_eq!(event.guild_id.get(), 1);
            assert_eq!(event.members.len(), 1);
            assert_eq!(event.not_found[0].get(), 4);
            assert_eq!(
                event.presences.as_ref().unwrap()[0]
                    .activities
                    .as_ref()
                    .unwrap()[0]
                    .name,
                "Testing"
            );
            assert_eq!(event.nonce.as_deref(), Some("abc"));
        }
        other => panic!("unexpected guild members chunk event: {other:?}"),
    }

    let resumed = decode_event("RESUMED", json!({ "trace": [] })).unwrap();
    assert!(matches!(resumed.kind(), EventKind::Resumed));
    assert_eq!(raw_json(&resumed), Some(json!({ "trace": [] })));

    match decode_event(
        "VOICE_CHANNEL_STATUS_UPDATE",
        json!({
            "guild_id": "1",
            "channel_id": "2",
            "status": "Live"
        }),
    )
    .unwrap()
    .kind()
    {
        EventKind::VoiceChannelStatusUpdate(event) => {
            assert_eq!(event.channel_id.unwrap().get(), 2);
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
    assert_eq!(event.name(), "CHANNEL_INFO");
    assert_eq!(raw_json(&event), Some(raw_channel_info.clone()));
    match event.kind() {
        EventKind::ChannelInfo(event) => {
            assert_eq!(event.guild_id.get(), 1);
            assert_eq!(event.channels[0].id.get(), 2);
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
    assert!(
        missing_guild_id
            .to_string()
            .contains("missing field `guild_id`"),
        "{missing_guild_id}"
    );

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
        invalid_guild_id.to_string().contains("snowflake"),
        "{invalid_guild_id}"
    );

    let missing_ids = decode_event(
        "MESSAGE_DELETE_BULK",
        json!({
            "channel_id": "22"
        }),
    )
    .unwrap_err();
    assert!(missing_ids.to_string().contains("ids"), "{missing_ids}");

    let raw = json!({ "x": 1 });
    let unknown = decode_event("SOMETHING_NEW", raw.clone()).unwrap();
    assert!(matches!(unknown.kind(), EventKind::Unknown));
    assert_eq!(unknown.name(), "SOMETHING_NEW");
    assert_eq!(raw_json(&unknown), Some(raw));
}

#[test]
fn field_map_events_read_escaped_and_repeated_keys_like_the_2x_decoder() {
    // Low-volume lenient events read a field map: escaped keys match, the
    // last of a repeated key wins, and a bad lenient field reads as `None`.
    let event = super::decode_event(
        "WEBHOOKS_UPDATE",
        r#"{"guild\u005fid":"1","channel_id":"2","channel_id":"3"}"#,
    )
    .unwrap();
    let EventKind::WebhooksUpdate(update) = event.kind() else {
        panic!("{:?}", event.kind());
    };
    assert_eq!(update.guild_id, Some(snowflake("1")));
    assert_eq!(update.channel_id, Some(snowflake("3")));

    let event =
        super::decode_event("WEBHOOKS_UPDATE", r#"{"guild_id":{},"channel_id":"2"}"#).unwrap();
    let EventKind::WebhooksUpdate(update) = event.kind() else {
        panic!("{:?}", event.kind());
    };
    assert_eq!(update.guild_id, None);
    assert_eq!(update.channel_id, Some(snowflake("2")));

    // Required fields and the payload shape still fail the event.
    for (name, payload, message) in [
        (
            "MESSAGE_DELETE_BULK",
            r#"{"ids":["1"]}"#,
            "missing field `channel_id`",
        ),
        (
            "MESSAGE_DELETE_BULK",
            r#"{"ids":"1","channel_id":"2"}"#,
            "invalid type",
        ),
        ("WEBHOOKS_UPDATE", r#"["1","2"]"#, "expected a JSON object"),
        ("RATE_LIMITED", r#"{"meta":1e400}"#, "number out of range"),
    ] {
        let event = super::decode_event(name, payload).unwrap();
        let EventKind::DecodeFailed(error) = event.kind() else {
            panic!("{name} {payload}: {:?}", event.kind());
        };
        assert!(error.message().contains(message), "{name}: {error}");
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
    match entitlement.kind() {
        EventKind::EntitlementUpdate(event) => {
            assert_eq!(event.sku_id.get(), 2);
            assert!(!event.deleted);
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
    match subscription.kind() {
        EventKind::SubscriptionUpdate(event) => {
            assert_eq!(event.id.get(), 30);
            assert_eq!(event.status, SubscriptionStatus::INACTIVE);
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
    match integration.kind() {
        EventKind::IntegrationCreate(event) => {
            assert_eq!(event.guild_id.unwrap().get(), 41);
            assert_eq!(event.integration.id.get(), 40);
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
    match integration_delete.kind() {
        EventKind::IntegrationDelete(event) => {
            assert_eq!(event.id.unwrap().get(), 40);
            assert_eq!(event.application_id.unwrap().get(), 42);
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
    match poll_vote.kind() {
        EventKind::MessagePollVoteRemove(event) => {
            assert_eq!(event.user_id.unwrap().get(), 50);
            assert_eq!(event.answer_id, Some(crate::model::PollAnswerId::new(2)));
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
    match sound.kind() {
        EventKind::GuildSoundboardSoundUpdate(event) => {
            assert_eq!(event.sound_id.get(), 10);
            assert_eq!(event.guild_id.unwrap().get(), 20);
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
    match delete.kind() {
        EventKind::GuildSoundboardSoundDelete(event) => {
            assert_eq!(event.sound_id.get(), 10);
            assert_eq!(event.guild_id.get(), 20);
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
    match sounds.kind() {
        EventKind::GuildSoundboardSoundsUpdate(event) => {
            assert_eq!(event.guild_id.get(), 20);
            assert_eq!(event.soundboard_sounds.len(), 1);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn decode_event_covers_structural_decoder_branches() {
    fn assert_decodes_with_raw(kind: &str, payload: Value) {
        let event = decode_event(kind, payload.clone()).expect(kind);
        assert_eq!(event.name(), kind);
        assert_eq!(raw_json(&event), Some(payload));
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

#[test]
fn invite_create_decodes_full_typed_fields() {
    let event = decode_event(
        "INVITE_CREATE",
        json!({
            "guild_id": "300",
            "channel_id": "301",
            "code": "abc123",
            "inviter": { "id": "302", "username": "inviter" },
            "uses": 0,
            "max_uses": 5,
            "max_age": 3600,
            "temporary": true,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": "2026-01-01T01:00:00Z",
            "target_type": 1,
            "target_user": { "id": "303", "username": "streamer" }
        }),
    )
    .unwrap();

    match event.kind() {
        EventKind::InviteCreate(invite) => {
            assert_eq!(invite.code.as_deref(), Some("abc123"));
            assert_eq!(
                invite.inviter.as_ref().map(|user| user.id),
                Some(snowflake("302"))
            );
            assert_eq!(invite.uses, Some(0));
            assert_eq!(invite.max_uses, Some(5));
            assert_eq!(invite.max_age, Some(3600));
            assert_eq!(invite.temporary, Some(true));
            assert_eq!(invite.created_at.as_deref(), Some("2026-01-01T00:00:00Z"));
            assert_eq!(invite.expires_at.as_deref(), Some("2026-01-01T01:00:00Z"));
            assert_eq!(invite.target_type, Some(InviteTargetType::STREAM));
            assert_eq!(
                invite.target_user.as_ref().map(|user| user.id),
                Some(snowflake("303"))
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn thread_events_decode_new_typed_fields() {
    let created = decode_event(
        "THREAD_CREATE",
        json!({ "id": "310", "type": 11, "newly_created": true }),
    )
    .unwrap();
    match created.kind() {
        EventKind::ThreadCreate(thread) => assert_eq!(thread.newly_created, Some(true)),
        other => panic!("unexpected event: {other:?}"),
    }

    let list_sync = decode_event(
        "THREAD_LIST_SYNC",
        json!({
            "guild_id": "311",
            "channel_ids": ["312", "313"],
            "threads": [{ "id": "314", "type": 11 }],
            "members": [{ "id": "314", "user_id": "315", "flags": 0 }]
        }),
    )
    .unwrap();
    match list_sync.kind() {
        EventKind::ThreadListSync(sync) => {
            assert_eq!(sync.channel_ids, vec![snowflake("312"), snowflake("313")]);
            assert_eq!(sync.threads.len(), 1);
            assert_eq!(sync.members.len(), 1);
            assert_eq!(sync.members[0].user_id, Some(snowflake("315")));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let member_update = decode_event(
        "THREAD_MEMBER_UPDATE",
        json!({
            "id": "316",
            "guild_id": "317",
            "user_id": "318",
            "join_timestamp": "2026-01-01T00:00:00Z",
            "flags": 4
        }),
    )
    .unwrap();
    match member_update.kind() {
        EventKind::ThreadMemberUpdate(update) => {
            assert_eq!(update.thread_id, Some(snowflake("316")));
            assert_eq!(update.user_id, Some(snowflake("318")));
            assert_eq!(
                update.join_timestamp.as_deref(),
                Some("2026-01-01T00:00:00Z")
            );
            assert_eq!(update.flags, Some(4));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let members_update = decode_event(
        "THREAD_MEMBERS_UPDATE",
        json!({
            "id": "319",
            "guild_id": "320",
            "member_count": 3,
            "added_members": [{ "id": "319", "user_id": "321", "flags": 0 }],
            "removed_member_ids": ["322"]
        }),
    )
    .unwrap();
    match members_update.kind() {
        EventKind::ThreadMembersUpdate(update) => {
            let added = update
                .added_members
                .as_ref()
                .expect("added members should decode");
            assert_eq!(added.len(), 1);
            assert_eq!(added[0].user_id, Some(snowflake("321")));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn decode_event_types_command_permissions_and_scheduled_event_metadata() {
    let permissions_update = decode_event(
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
        json!({
            "id": "300",
            "application_id": "301",
            "guild_id": "302",
            "permissions": [
                { "id": "303", "type": 1, "permission": true },
                { "id": "304", "type": 2, "permission": false },
                { "id": "305", "type": 3, "permission": true }
            ]
        }),
    )
    .unwrap();
    match permissions_update.kind() {
        EventKind::ApplicationCommandPermissionsUpdate(event) => {
            use crate::model::{CommandPermissionTarget, CommandPermissionsId};

            assert_eq!(event.id, Some(CommandPermissionsId::Command(test_id(300))));
            assert_eq!(event.permissions.len(), 3);
            assert_eq!(
                event.permissions[0].target,
                CommandPermissionTarget::Role(test_id(303))
            );
            assert!(event.permissions[0].permission);
            assert_eq!(
                event.permissions[1].target,
                CommandPermissionTarget::User(test_id(304))
            );
            assert!(!event.permissions[1].permission);
            assert_eq!(
                event.permissions[2].target,
                CommandPermissionTarget::Channel(test_id(305))
            );
            // The raw payload stays readable alongside the typed view.
            assert_eq!(
                raw_json(&permissions_update).unwrap()["permissions"][0]["id"],
                json!("303")
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    // Malformed permission entries fall back to an empty list instead of
    // failing the whole event decode.
    match decode_event(
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
        json!({
            "id": "300",
            "permissions": [{ "id": 12.5 }]
        }),
    )
    .unwrap()
    .kind()
    {
        EventKind::ApplicationCommandPermissionsUpdate(event) => {
            assert!(event.permissions.is_empty());
        }
        other => panic!("unexpected event: {other:?}"),
    }

    match decode_event(
        "GUILD_SCHEDULED_EVENT_UPDATE",
        json!({
            "id": "310",
            "guild_id": "311",
            "entity_type": 3,
            "entity_metadata": { "location": "Berlin HQ" }
        }),
    )
    .unwrap()
    .kind()
    {
        EventKind::GuildScheduledEventUpdate(event) => {
            assert_eq!(
                event
                    .entity_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.location.as_deref()),
                Some("Berlin HQ")
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    // A null or shape-shifted entity_metadata decodes to None.
    match decode_event(
        "GUILD_SCHEDULED_EVENT_UPDATE",
        json!({
            "id": "310",
            "entity_metadata": null
        }),
    )
    .unwrap()
    .kind()
    {
        EventKind::GuildScheduledEventUpdate(event) => {
            assert!(event.entity_metadata.is_none());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
