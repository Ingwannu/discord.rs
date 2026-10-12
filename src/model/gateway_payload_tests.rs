use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::model::{test_id, Embed};

fn guild_create_payload() -> serde_json::Value {
    json!({
        "id": "10",
        "name": "guild",
        "roles": [{ "id": "11", "name": "role", "permissions": "0" }],
        "features": ["COMMUNITY"],
        "member_count": 2,
        "channels": [
            { "id": "20", "type": 0, "name": "general" },
            { "id": "21", "type": 2, "guild_id": "99", "name": "voice" }
        ],
        "threads": [{ "id": "22", "type": 11, "name": "thread" }],
        "members": [
            { "user": { "id": "30", "username": "a" }, "roles": ["11"] },
            { "user": { "id": "31", "username": "b" }, "roles": [] }
        ],
        "presences": [
            { "user": { "id": "30" }, "status": "online" },
            { "user_id": "31", "status": "idle" }
        ],
        "voice_states": [{ "user_id": "30", "channel_id": "21", "session_id": "s" }],
        "emojis": [{ "id": "40", "name": "emoji" }],
        "stickers": [],
        // A complete scheduled event: `Guild::guild_scheduled_events` is
        // typed, so a partial one would fail the dispatch.
        "guild_scheduled_events": [{
            "id": "50",
            "guild_id": "10",
            "name": "raid night",
            "scheduled_start_time": "2026-10-11T20:00:00+00:00",
            "privacy_level": 2,
            "status": 1,
            "entity_type": 2,
            "channel_id": "21"
        }]
    })
}

fn assert_normalized(created: &GuildCreate) {
    assert_eq!(created.id(), test_id(10));
    assert_eq!(created.guild.name, "guild");
    assert!(created.guild.roles.is_empty(), "roles move to `roles`");
    assert_eq!(created.roles.len(), 1);
    assert_eq!(created.guild.member_count, Some(2));
    assert!(created.guild.channels.is_empty());
    assert!(created.guild.members.is_empty());
    assert!(created.guild.presences.is_empty());
    assert!(created.guild.guild_scheduled_events.is_empty());

    let channel_guilds: Vec<_> = created
        .channels
        .iter()
        .chain(created.threads.iter())
        .map(|channel| channel.guild_id)
        .collect();
    // A channel that names a guild keeps it; the others get this guild.
    assert_eq!(
        channel_guilds,
        vec![Some(test_id(10)), Some(test_id(99)), Some(test_id(10))]
    );
    assert_eq!(created.voice_states[0].guild_id, Some(test_id(10)));

    assert_eq!(created.members.len(), 2);
    assert_eq!(
        created.members[0].user.as_ref().map(|user| user.id),
        Some(test_id(30))
    );
    let presence_users: Vec<_> = created
        .presences
        .iter()
        .map(|presence| presence.user_id)
        .collect();
    assert_eq!(presence_users, vec![Some(test_id(30)), Some(test_id(31))]);
    assert_eq!(created.emojis.len(), 1);
    assert_eq!(created.guild_scheduled_events.len(), 1);
}

/// The fast path's result for a payload that must be an available guild.
fn decode_available(text: &str) -> Result<GuildCreate, serde_json::Error> {
    GuildCreatePayload::from_json_str(text).map(|payload| match payload {
        GuildCreatePayload::Available(guild) => *guild,
        GuildCreatePayload::Unavailable(guild) => panic!("unexpected {guild:?}"),
    })
}

#[test]
fn guild_create_reads_an_unavailable_guild_without_making_one_up() {
    for text in [
        r#"{"id":"10","unavailable":true}"#,
        r#"{ "unavailable" : true , "id" : "10" }"#,
        // Fields beside `unavailable: true` are not trusted as a guild.
        r#"{"id":"10","name":"g","unavailable":true,"members":[]}"#,
    ] {
        let payload = GuildCreatePayload::from_json_str(text).unwrap();
        assert!(
            matches!(payload, GuildCreatePayload::Unavailable(UnavailableGuild { id }) if id == test_id(10)),
            "{text}: {payload:?}"
        );
        assert_eq!(payload.id(), test_id(10));
        assert!(payload.available().is_none());
    }

    let available =
        GuildCreatePayload::from_json_str(r#"{"id":"10","name":"g","unavailable":false}"#).unwrap();
    assert_eq!(
        available.available().map(|guild| guild.guild.name.as_str()),
        Some("g")
    );
    let missing_id = GuildCreatePayload::from_json_str(r#"{"unavailable":true}"#).unwrap_err();
    assert!(
        missing_id.to_string().contains("missing field `id`"),
        "{missing_id}"
    );
    assert!(GuildCreatePayload::from_json_str(r#"{"id":"x","unavailable":true}"#).is_err());
}

#[test]
fn guild_create_shares_presences_and_roles_without_spare_capacity() {
    // The cache keeps these allocations for as long as the guild and its
    // members are cached, so the decoder's doubling slack must not stay;
    // roles are separate `Arc`s, not a list inside the shared guild.
    let payload = json!({
        "id": "10",
        "name": "guild",
        "roles": (11..16).map(|id| json!({ "id": id.to_string(), "name": "r" })).collect::<Vec<_>>(),
        "presences": [{
            "user": { "id": "30" },
            "status": "online",
            "activities": [{ "name": "Custom Status", "type": 4 }]
        }]
    });
    let created = decode_available(&payload.to_string()).unwrap();
    let activities = created.presences[0].activities.as_ref().unwrap();
    assert_eq!((activities.len(), activities.capacity()), (1, 1));
    assert_eq!(created.roles.len(), 5);
    assert!(created.guild.roles.is_empty());
    assert!(created
        .roles
        .iter()
        .all(|role| Arc::strong_count(role) == 1));
}

#[test]
fn guild_create_fast_path_matches_the_guild_conversion() {
    let payload = guild_create_payload();
    let fast = decode_available(&payload.to_string()).unwrap();
    let slow = GuildCreate::from(serde_json::from_value::<Guild>(payload).unwrap());
    assert_normalized(&fast);
    assert_normalized(&slow);
    assert_eq!(
        serde_json::to_value(fast.to_guild()).unwrap(),
        serde_json::to_value(slow.to_guild()).unwrap()
    );
    // Freshly decoded units are not shared with anything yet.
    assert!(fast
        .members
        .iter()
        .all(|member| Arc::strong_count(member) == 1));
}

#[test]
fn guild_create_to_guild_puts_every_collection_back() {
    let payload = guild_create_payload();
    let created = decode_available(&payload.to_string()).unwrap();
    let guild = created.to_guild();
    assert_eq!(guild.channels.len(), 2);
    assert_eq!(guild.threads.len(), 1);
    assert_eq!(guild.members.len(), 2);
    assert_eq!(guild.presences.len(), 2);
    assert_eq!(guild.voice_states.len(), 1);
    assert_eq!(guild.emojis.len(), 1);
    assert_eq!(guild.guild_scheduled_events.len(), 1);
}

#[test]
fn guild_create_fast_path_accepts_escaped_keys_and_rejects_bad_payloads() {
    let escaped = r#"{"id":"10","name":"guild","members":[{"roles":[]}]}"#;
    let created = decode_available(escaped).unwrap();
    assert_eq!(created.guild.name, "guild");
    assert_eq!(created.members.len(), 1);

    let missing_name = decode_available(r#"{"id":"10"}"#).unwrap_err();
    assert!(missing_name.to_string().contains("name"), "{missing_name}");
    assert!(decode_available(r#"{"id":"10","name":"g","members":{}}"#).is_err());
    assert!(decode_available(r#"{"id":"10","name":"g"} trailing"#).is_err());
    assert!(decode_available("[]").is_err());
}

fn cached_message() -> Message {
    serde_json::from_value(json!({
        "id": "2",
        "channel_id": "1",
        "guild_id": "9",
        "author": { "id": "5", "username": "author", "global_name": "Author", "avatar": "a" },
        "content": "before",
        "edited_timestamp": "2026-01-01T00:00:00Z",
        "pinned": true,
        "flags": 4,
        "embeds": [{ "title": "embed" }],
        "mention_roles": ["7"],
        "components": [{ "type": 1, "components": [] }]
    }))
    .unwrap()
}

fn update(value: serde_json::Value) -> MessageUpdate {
    serde_json::from_value(value).unwrap()
}

#[test]
fn message_update_distinguishes_missing_null_empty_false_and_zero() {
    let mut message = cached_message();
    let patch = update(json!({
        "id": "2",
        "channel_id": "1",
        "edited_timestamp": null,
        "embeds": [],
        "pinned": false,
        "flags": 0
    }));
    assert!(patch.content.is_missing());
    assert_eq!(patch.edited_timestamp, NullablePatch::null());
    assert_eq!(patch.pinned, Patch::Value(Nullable::Value(false)));
    assert_eq!(patch.flags, Patch::Value(Nullable::Value(0)));
    assert!(patch.apply_to(&mut message));

    assert_eq!(message.content, "before", "missing keeps");
    assert_eq!(message.guild_id, Some(test_id(9)), "missing keeps");
    assert_eq!(message.mention_roles, vec![test_id(7)], "missing keeps");
    assert!(message.components.is_some(), "missing keeps");
    assert_eq!(message.edited_timestamp, None, "null clears");
    assert!(message.embeds.is_empty(), "[] replaces with empty");
    assert_eq!(message.pinned, Some(false), "false is a value");
    assert_eq!(message.flags, Some(0), "0 is a value");
}

#[test]
fn message_update_replaces_present_values_and_clears_nullable_lists() {
    let mut message = cached_message();
    let patch = update(json!({
        "id": "2",
        "channel_id": "1",
        "content": "after",
        "embeds": [{ "title": "new" }],
        "components": null,
        "mention_roles": []
    }));
    assert!(patch.apply_to(&mut message));
    assert_eq!(message.content, "after");
    assert_eq!(
        message
            .embeds
            .first()
            .and_then(|embed: &Embed| embed.title.as_deref()),
        Some("new")
    );
    assert!(message.components.is_none());
    assert!(message.mention_roles.is_empty());
}

#[test]
fn message_update_rejects_null_for_non_nullable_fields() {
    let error = serde_json::from_value::<MessageUpdate>(json!({
        "id": "2",
        "channel_id": "1",
        "content": null
    }))
    .unwrap_err();
    assert!(error.to_string().contains("string"), "{error}");
    assert!(serde_json::from_value::<MessageUpdate>(json!({
        "id": "2",
        "channel_id": "1",
        "embeds": null
    }))
    .is_err());
}

#[test]
fn message_update_merges_a_nested_partial_author() {
    let mut message = cached_message();
    let patch = update(json!({
        "id": "2",
        "channel_id": "1",
        "author": { "id": "5", "global_name": null, "avatar": "b" }
    }));
    assert!(
        patch.author_user().is_none(),
        "no username, so not a full user"
    );
    assert!(patch.apply_to(&mut message));
    let author = message.author.as_ref().unwrap();
    assert_eq!(author.username, "author", "missing keeps");
    assert_eq!(author.global_name, None, "null clears");
    assert_eq!(author.avatar.as_deref(), Some("b"), "value replaces");

    // A partial for another user without a username cannot replace the
    // cached author: nothing is made up.
    let other = update(json!({ "id": "2", "channel_id": "1", "author": { "id": "6" } }));
    assert!(other.apply_to(&mut message));
    assert_eq!(
        message.author.as_ref().map(|user| user.id),
        Some(test_id(5))
    );

    // A complete author fills an empty slot.
    message.author = None;
    let complete = update(json!({
        "id": "2",
        "channel_id": "1",
        "author": { "id": "6", "username": "new" }
    }));
    assert_eq!(
        complete.author_user().map(|user| user.username),
        Some("new".to_string())
    );
    assert!(complete.apply_to(&mut message));
    assert_eq!(
        message.author.as_ref().map(|user| user.id),
        Some(test_id(6))
    );

    let cleared = update(json!({ "id": "2", "channel_id": "1", "author": null }));
    assert!(cleared.apply_to(&mut message));
    assert!(message.author.is_none());
}

#[test]
fn message_update_ignores_another_message() {
    let mut message = cached_message();
    let mut patch = MessageUpdate::new(test_id(3), test_id(1));
    patch.content = Patch::Value("other".to_string());
    assert!(!patch.targets(&message));
    assert!(!patch.apply_to(&mut message));
    assert_eq!(message.content, "before");

    let mut wrong_channel = MessageUpdate::new(test_id(2), test_id(8));
    wrong_channel.content = Patch::Value("other".to_string());
    assert!(!wrong_channel.apply_to(&mut message));
    assert_eq!(message.content, "before");
}

#[test]
fn message_update_serializes_only_sent_fields() {
    let mut patch = MessageUpdate::new(test_id(2), test_id(1));
    patch.edited_timestamp = NullablePatch::null();
    patch.embeds = Patch::Value(Vec::new());
    patch.author = Patch::Value(Nullable::Value(PartialUser::new(test_id(5))));
    assert_eq!(
        serde_json::to_value(&patch).unwrap(),
        json!({
            "id": "2",
            "channel_id": "1",
            "author": { "id": "5" },
            "edited_timestamp": null,
            "embeds": []
        })
    );
}

#[test]
fn partial_user_applies_only_to_the_same_user() {
    let mut user = User {
        id: test_id(5),
        username: "name".to_string(),
        global_name: Some("Name".to_string()),
        ..User::fixture()
    };
    let mut partial = PartialUser::new(test_id(6));
    partial.username = Patch::Value("other".to_string());
    assert!(!partial.apply_to(&mut user));
    assert_eq!(user.username, "name");

    let mut same = PartialUser::new(test_id(5));
    same.global_name = NullablePatch::null();
    same.bot = Patch::Value(Nullable::Value(false));
    assert!(same.apply_to(&mut user));
    assert_eq!(user.global_name, None);
    assert_eq!(user.bot, Some(false));
    assert_eq!(user.username, "name");
    assert!(same.to_user().is_none());
}

#[test]
fn partial_users_patch_display_name_styles() {
    let mut user: User = serde_json::from_value(json!({ "id": "5", "username": "u" })).unwrap();
    let set: PartialUser = serde_json::from_value(json!({
        "id": "5",
        "display_name_styles": { "font_id": 11, "effect_id": 1, "colors": [1] }
    }))
    .unwrap();
    assert!(set.apply_to(&mut user));
    assert!(user.display_name_styles.is_some());

    let omitted: PartialUser = serde_json::from_value(json!({ "id": "5" })).unwrap();
    assert!(omitted.apply_to(&mut user));
    assert!(user.display_name_styles.is_some(), "omitted keeps");

    let cleared: PartialUser =
        serde_json::from_value(json!({ "id": "5", "display_name_styles": null })).unwrap();
    assert!(cleared.apply_to(&mut user));
    assert!(user.display_name_styles.is_none(), "null clears");
}

#[test]
fn member_updates_keep_omitted_fields_and_refuse_other_users() {
    let mut member: Member = serde_json::from_value(json!({
        "user": { "id": "5", "username": "before" },
        "roles": ["1"],
        "nick": "nick",
        "deaf": true,
        "avatar": "a1"
    }))
    .unwrap();
    let update: MemberUpdate = serde_json::from_value(json!({
        "guild_id": "9",
        "user": { "id": "5", "username": "after" },
        "roles": ["2", "3"],
        "avatar": null
    }))
    .unwrap();
    assert!(update.nick.is_missing() && update.deaf.is_missing());
    assert!(update.apply_to(&mut member));
    assert_eq!(member.roles, [test_id(2), test_id(3)]);
    assert_eq!(member.nick.as_deref(), Some("nick"), "omitted keeps");
    assert_eq!(member.deaf, Some(true));
    assert_eq!(member.avatar, None, "null clears");
    assert!(Arc::ptr_eq(member.user.as_ref().unwrap(), &update.user));

    let other = MemberUpdate::new(
        test_id(9),
        serde_json::from_value::<User>(json!({ "id": "6", "username": "x" })).unwrap(),
    );
    assert!(!other.apply_to(&mut member));
    assert_eq!(member.user.as_ref().unwrap().id, test_id(5));
}
