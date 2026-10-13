//! Wire round trips for the polymorphic ID types of phase 3 series 2 step B
//! (docs/design/3.0-types-events-requests.md §1 "다형 ID"): each must read
//! and write exactly the JSON the `GenericId` + raw `type` fields did.
//! Payloads are Discord's documented examples (discord-api-docs `3714862`)
//! where the docs have one.

use serde_json::{json, Value};

use super::{
    test_id, ApplicationCommandPermission, Channel, CommandPermissionTarget, CommandPermissionType,
    CommandPermissionsId, EntitlementOwner, GenericId, GuildApplicationCommandPermissions, GuildId,
    Interaction, MentionableId, MessageComponent, OverwriteTarget, OverwriteType,
    PermissionOverwrite, ResolvedData, SelectDefaultValue,
};

use crate::request::CreateTestEntitlement;

/// Decodes `wire` as `T` and checks that encoding gives `wire` back.
fn round_trip<T>(wire: Value) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let decoded: T = serde_json::from_value(wire.clone()).expect("documented shape decodes");
    assert_eq!(
        serde_json::to_value(&decoded).expect("decoded value encodes"),
        wire
    );
    decoded
}

// --- OverwriteTarget / PermissionOverwrite ---

#[test]
fn overwrites_decode_role_member_and_unknown_types_and_write_them_back() {
    let role: PermissionOverwrite =
        round_trip(json!({ "id": "41771983423143937", "type": 0, "allow": "1024", "deny": "0" }));
    assert_eq!(
        role.target,
        OverwriteTarget::Role(test_id(41771983423143937))
    );
    assert_eq!(role.target.kind(), OverwriteType::ROLE);

    let member: PermissionOverwrite =
        round_trip(json!({ "id": "80351110224678912", "type": 1, "deny": "2048" }));
    assert_eq!(
        member.target,
        OverwriteTarget::Member(test_id(80351110224678912))
    );
    assert_eq!(member.allow, None);

    // A type Discord might add later is kept, not rejected or remapped.
    let future: PermissionOverwrite = round_trip(json!({ "id": "5", "type": 7 }));
    assert_eq!(
        future.target,
        OverwriteTarget::Unknown {
            kind: OverwriteType::new(7),
            id: test_id(5),
        }
    );
    assert_eq!(future.target.id(), test_id::<super::marker::Generic>(5));
}

#[test]
fn overwrites_accept_null_permissions_and_integer_ids_like_before() {
    // `allow`/`deny` may be null in Discord payloads; null reads as absent
    // and is then omitted, as with the 2.x fields.
    let overwrite: PermissionOverwrite = serde_json::from_value(
        json!({ "id": 41771983423143937_u64, "type": 0, "allow": null, "deny": "8" }),
    )
    .expect("integer id and null allow decode");
    assert_eq!(
        serde_json::to_value(overwrite).expect("encodes"),
        json!({ "id": "41771983423143937", "type": 0, "deny": "8" })
    );
    // The 2.x field rejected a string `type`; so does the new one.
    assert!(
        serde_json::from_value::<PermissionOverwrite>(json!({ "id": "1", "type": "role" }))
            .is_err()
    );
}

#[test]
fn channel_overwrites_round_trip_inside_a_documented_channel() {
    let channel: Channel = round_trip(json!({
        "id": "41771983423143937",
        "guild_id": "41771983423143937",
        "name": "general",
        "type": 0,
        "position": 6,
        "permission_overwrites": [
            { "id": "41771983423143937", "type": 0, "deny": "1024" },
            { "id": "80351110224678912", "type": 1, "allow": "1024" }
        ],
        "rate_limit_per_user": 2,
        "nsfw": true,
        "topic": "24/7 chat about how to gank Mike #2",
        "last_message_id": "155117677105512449",
        "parent_id": "399942396007890945",
        "default_auto_archive_duration": 60
    }));
    let overwrites = channel.permission_overwrites.unwrap_or_default();
    let guild_id: GuildId = test_id(41771983423143937);
    // The obfuscated-channel shape: one deny for @everyone (the guild ID).
    assert_eq!(
        overwrites[0].target,
        OverwriteTarget::Role(guild_id.everyone_role())
    );
    assert_eq!(
        overwrites[1].target,
        OverwriteTarget::Member(test_id(80351110224678912))
    );
}

#[test]
fn overwrite_target_conversions_and_wire_parts_agree() {
    let role = OverwriteTarget::from(test_id::<super::marker::Role>(3));
    let member = OverwriteTarget::from(test_id::<super::marker::User>(4));
    assert_eq!(
        role,
        OverwriteTarget::from_wire(OverwriteType::ROLE, test_id(3))
    );
    assert_eq!(
        member,
        OverwriteTarget::from_wire(OverwriteType::MEMBER, test_id(4))
    );
    assert_eq!(
        (member.kind(), member.id()),
        (OverwriteType::MEMBER, test_id(4))
    );
    assert_eq!(
        PermissionOverwrite::new(role),
        PermissionOverwrite {
            target: role,
            allow: None,
            deny: None,
        }
    );
    assert_eq!(
        crate::request::EditChannelPermission::for_target(member).kind,
        OverwriteType::MEMBER
    );
}

// --- Command permissions ---

#[test]
fn a_lone_command_permission_decodes_without_guild_constants() {
    let role: ApplicationCommandPermission =
        round_trip(json!({ "id": "500", "type": 1, "permission": false }));
    assert_eq!(role.target, CommandPermissionTarget::Role(test_id(500)));
    let user: ApplicationCommandPermission =
        round_trip(json!({ "id": "7", "type": 2, "permission": true }));
    assert_eq!(user.target, CommandPermissionTarget::User(test_id(7)));
    let channel: ApplicationCommandPermission =
        round_trip(json!({ "id": "499", "type": 3, "permission": true }));
    assert_eq!(
        channel.target,
        CommandPermissionTarget::Channel(test_id(499))
    );
    let future: ApplicationCommandPermission =
        round_trip(json!({ "id": "8", "type": 9, "permission": true }));
    assert_eq!(
        future.target,
        CommandPermissionTarget::Unknown {
            kind: CommandPermissionType::new(9),
            id: test_id(8),
        }
    );
}

#[test]
fn guild_command_permissions_resolve_everyone_all_channels_and_application_scope() {
    // `id` equal to `application_id`: application-wide permissions.
    let wire = json!({
        "id": "1000",
        "application_id": "1000",
        "guild_id": "500",
        "permissions": [
            { "id": "500", "type": 1, "permission": false },
            { "id": "499", "type": 3, "permission": false },
            { "id": "300", "type": 1, "permission": true },
            { "id": "498", "type": 3, "permission": true },
            { "id": "7", "type": 2, "permission": true }
        ]
    });
    let permissions: GuildApplicationCommandPermissions = round_trip(wire);
    let guild_id: GuildId = test_id(500);
    assert_eq!(
        permissions.id,
        CommandPermissionsId::Application(test_id(1000))
    );
    let targets: Vec<_> = permissions.permissions.iter().map(|p| p.target).collect();
    assert_eq!(
        targets,
        vec![
            CommandPermissionTarget::Everyone(guild_id),
            CommandPermissionTarget::AllChannels(guild_id),
            CommandPermissionTarget::Role(test_id(300)),
            CommandPermissionTarget::Channel(test_id(498)),
            CommandPermissionTarget::User(test_id(7)),
        ]
    );

    // A command entry, and a missing `permissions` array, as before.
    let command: GuildApplicationCommandPermissions = serde_json::from_value(
        json!({ "id": "1001", "application_id": "1000", "guild_id": "500" }),
    )
    .expect("permissions default to empty");
    assert_eq!(command.id, CommandPermissionsId::Command(test_id(1001)));
    assert!(command.permissions.is_empty());
    assert_eq!(
        serde_json::to_value(&command).expect("encodes"),
        json!({ "id": "1001", "application_id": "1000", "guild_id": "500", "permissions": [] })
    );
}

#[test]
fn command_permission_constants_serialize_to_the_documented_ids() {
    let guild_id: GuildId = test_id(500);
    assert_eq!(
        serde_json::to_value(ApplicationCommandPermission::everyone(guild_id, false))
            .expect("encodes"),
        json!({ "id": "500", "type": 1, "permission": false })
    );
    assert_eq!(
        serde_json::to_value(ApplicationCommandPermission::all_channels(guild_id, true))
            .expect("encodes"),
        json!({ "id": "499", "type": 3, "permission": true })
    );
    // Guild ID 1 has no valid all-channels constant (it would be 0).
    let impossible = ApplicationCommandPermission::all_channels(test_id(1), true);
    assert!(impossible.target.id().is_err());
    assert!(serde_json::to_value(impossible).is_err());
}

#[test]
fn in_guild_only_rewrites_the_constants_of_that_guild() {
    let guild_id: GuildId = test_id(500);
    let other_role = CommandPermissionTarget::Role(test_id(501));
    assert_eq!(other_role.in_guild(guild_id), other_role);
    let user_with_guild_number = CommandPermissionTarget::User(test_id(500));
    assert_eq!(
        user_with_guild_number.in_guild(guild_id),
        user_with_guild_number
    );
    assert_eq!(
        CommandPermissionTarget::Channel(test_id(499)).in_guild(test_id(9)),
        CommandPermissionTarget::Channel(test_id(499))
    );
    assert_eq!(
        CommandPermissionTarget::Everyone(guild_id).kind(),
        CommandPermissionType::ROLE
    );
    assert_eq!(
        CommandPermissionTarget::AllChannels(guild_id).kind(),
        CommandPermissionType::CHANNEL
    );
    assert_eq!(
        CommandPermissionsId::from_wire(test_id(5), test_id(6)),
        CommandPermissionsId::Command(test_id(5))
    );
    assert_eq!(
        CommandPermissionsId::Application(test_id(6)).id(),
        test_id::<super::marker::Generic>(6)
    );
}

#[test]
fn the_permissions_update_event_is_decoded_with_its_guild_and_application() {
    let payload = json!({
        "id": "1000",
        "application_id": "1000",
        "guild_id": "500",
        "permissions": [
            { "id": "500", "type": 1, "permission": false },
            { "id": "499", "type": 3, "permission": true }
        ]
    });
    let event = crate::event::decode_event(
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
        &payload.to_string(),
    )
    .expect("event decodes");
    let crate::event::EventKind::ApplicationCommandPermissionsUpdate(event) = event.kind() else {
        panic!("unexpected event: {event:?}");
    };
    assert_eq!(
        event.id,
        Some(CommandPermissionsId::Application(test_id(1000)))
    );
    assert_eq!(
        event.permissions[0].target,
        CommandPermissionTarget::Everyone(test_id(500))
    );
    assert_eq!(
        event.permissions[1].target,
        CommandPermissionTarget::AllChannels(test_id(500))
    );

    // Without `application_id` the `id` is taken as a command, and without
    // `guild_id` the targets stay plain.
    let bare = crate::event::decode_event(
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
        r#"{ "id": "1000", "permissions": [{ "id": "500", "type": 1, "permission": true }] }"#,
    )
    .expect("event decodes");
    let crate::event::EventKind::ApplicationCommandPermissionsUpdate(bare) = bare.kind() else {
        panic!("unexpected event: {bare:?}");
    };
    assert_eq!(bare.id, Some(CommandPermissionsId::Command(test_id(1000))));
    assert_eq!(
        bare.permissions[0].target,
        CommandPermissionTarget::Role(test_id(500))
    );
}

// --- SelectDefaultValue / MentionableId ---

#[test]
fn select_default_values_round_trip_every_documented_type() {
    let user: SelectDefaultValue = round_trip(json!({ "id": "80351110224678912", "type": "user" }));
    assert_eq!(user, SelectDefaultValue::User(test_id(80351110224678912)));
    let role: SelectDefaultValue = round_trip(json!({ "id": "41771983423143936", "type": "role" }));
    assert_eq!(role, SelectDefaultValue::Role(test_id(41771983423143936)));
    let channel: SelectDefaultValue =
        round_trip(json!({ "id": "41771983423143937", "type": "channel" }));
    assert_eq!(
        channel,
        SelectDefaultValue::Channel(test_id(41771983423143937))
    );
    assert_eq!(channel.kind(), "channel");
    assert_eq!(
        channel.id(),
        test_id::<super::marker::Generic>(41771983423143937)
    );

    let future: SelectDefaultValue = round_trip(json!({ "id": "9", "type": "thread" }));
    assert_eq!(
        future,
        SelectDefaultValue::Unknown {
            kind: "thread".to_string(),
            id: test_id(9),
        }
    );
    // The 2.x field defaulted a missing `type` to ""; so does Unknown.
    let missing: SelectDefaultValue =
        serde_json::from_value(json!({ "id": "9" })).expect("missing type decodes");
    assert_eq!(missing.kind(), "");
    assert_eq!(
        serde_json::to_value(missing).expect("encodes"),
        json!({ "id": "9", "type": "" })
    );
}

#[test]
fn received_select_components_and_builders_share_the_wire_shape() {
    let wire = json!({
        "type": 8,
        "custom_id": "channels",
        "default_values": [{ "id": "41771983423143937", "type": "channel" }]
    });
    let component: MessageComponent = round_trip(wire);
    assert_eq!(
        component.default_values,
        vec![SelectDefaultValue::Channel(test_id(41771983423143937))]
    );

    let built = crate::builders::SelectMenuBuilder::mentionable("who")
        .default_mentionable(MentionableId::User(test_id(7)))
        .default_mentionable(MentionableId::from(test_id::<super::marker::Role>(8)))
        .build();
    assert_eq!(
        serde_json::to_value(built).unwrap()["default_values"],
        json!([{ "id": "7", "type": "user" }, { "id": "8", "type": "role" }])
    );
}

#[test]
fn resolved_data_tells_mentionable_users_from_roles() {
    let resolved: ResolvedData = serde_json::from_value(json!({
        "users": { "7": { "id": "7", "username": "wumpus" } },
        "members": { "9": { "roles": [] } },
        "roles": { "8": { "id": "8", "name": "mods", "color": 0, "hoist": false,
                          "position": 1, "permissions": "0", "managed": false,
                          "mentionable": true } }
    }))
    .expect("resolved data decodes");
    let value = |raw: u64| -> GenericId { test_id(raw) };
    assert_eq!(
        resolved.mentionable(value(7)),
        Some(MentionableId::User(test_id(7)))
    );
    // A member entry alone (no user object) still marks a user.
    assert_eq!(
        resolved.mentionable(value(9)),
        Some(MentionableId::User(test_id(9)))
    );
    assert_eq!(
        resolved.mentionable(value(8)),
        Some(MentionableId::Role(test_id(8)))
    );
    assert_eq!(resolved.mentionable(value(10)), None);
    assert_eq!(MentionableId::Role(test_id(8)).id(), value(8));
}

// --- EntitlementOwner ---

#[test]
fn test_entitlement_bodies_keep_owner_id_and_owner_type() {
    // The documented example (a guild subscription). Request bodies are
    // only encoded, so the check is one-way.
    let guild = CreateTestEntitlement::new(
        test_id(999184799365857331),
        EntitlementOwner::Guild(test_id(847184799365857999)),
    );
    assert_eq!(
        serde_json::to_value(guild).unwrap(),
        json!({
            "sku_id": "999184799365857331",
            "owner_id": "847184799365857999",
            "owner_type": 1
        })
    );
    let user = CreateTestEntitlement::new(
        test_id(1),
        EntitlementOwner::from(test_id::<super::marker::User>(2)),
    );
    assert_eq!(
        serde_json::to_value(user).unwrap(),
        json!({ "sku_id": "1", "owner_id": "2", "owner_type": 2 })
    );
    assert_eq!(user.owner.kind(), super::EntitlementOwnerType::USER);
    assert_eq!(
        EntitlementOwner::from(test_id::<super::marker::Guild>(3)).id(),
        test_id::<super::marker::Generic>(3)
    );
}

// --- Context menu targets ---

#[test]
fn context_menu_interactions_type_their_target_by_command_kind() {
    // Trimmed from the documented user and message command interactions.
    let user_command = json!({
        "application_id": "775799577604522054",
        "channel_id": "772908445358620702",
        "data": {
            "id": "866818195033292850",
            "name": "context-menu-user-2",
            "resolved": {
                "users": {
                    "809850198683418695": {
                        "id": "809850198683418695",
                        "username": "VoltyDemo",
                        "discriminator": "7302",
                        "avatar": "afc428077119df8aabbbd84b0dc90c74",
                        "bot": true,
                        "public_flags": 0
                    }
                }
            },
            "target_id": "809850198683418695",
            "type": 2
        },
        "guild_id": "772904309264089089",
        "id": "867794291820986368",
        "token": "UNIQUE_TOKEN",
        "type": 2,
        "version": 1
    });
    let Interaction::UserContextMenu(interaction) =
        crate::parsers::parse_interaction(&user_command).expect("user command parses")
    else {
        panic!("type 2 data is a user command");
    };
    assert_eq!(interaction.target_id(), Some(test_id(809850198683418695)));
    assert_eq!(
        interaction.target_user().map(|user| user.username.as_str()),
        Some("VoltyDemo")
    );

    let message_command = json!({
        "application_id": "775799577604522054",
        "channel_id": "772908445358620702",
        "data": {
            "id": "866818195033292851",
            "name": "context-menu-message-2",
            "resolved": {
                "messages": {
                    "867793854505943041": {
                        "id": "867793854505943041",
                        "channel_id": "772908445358620702",
                        "content": "some message",
                        "author": { "id": "167348773423415296", "username": "ian" },
                        "timestamp": "2021-07-22T15:42:57.744000+00:00",
                        "type": 0
                    }
                }
            },
            "target_id": "867793854505943041",
            "type": 3
        },
        "guild_id": "772904309264089089",
        "id": "867793873336926249",
        "token": "UNIQUE_TOKEN",
        "type": 2,
        "version": 1
    });
    let Interaction::MessageContextMenu(interaction) =
        crate::parsers::parse_interaction(&message_command).expect("message command parses")
    else {
        panic!("type 3 data is a message command");
    };
    assert_eq!(interaction.target_id(), Some(test_id(867793854505943041)));
    assert_eq!(
        interaction
            .target_message()
            .map(|message| message.content.as_str()),
        Some("some message")
    );
}

#[test]
fn constructors_build_the_matching_variants() {
    assert_eq!(
        SelectDefaultValue::role(test_id(1)),
        SelectDefaultValue::Role(test_id(1))
    );
    assert_eq!(
        SelectDefaultValue::channel(test_id(2)),
        SelectDefaultValue::Channel(test_id(2))
    );
    assert_eq!(
        SelectDefaultValue::from(MentionableId::from(test_id::<super::marker::User>(3))),
        SelectDefaultValue::user(test_id(3))
    );
    assert_eq!(
        MentionableId::User(test_id(3)).id(),
        test_id::<super::marker::Generic>(3)
    );
    assert_eq!(
        ApplicationCommandPermission::channel(test_id(4), false).target,
        CommandPermissionTarget::Channel(test_id(4))
    );
    assert_eq!(
        ApplicationCommandPermission::user(test_id(5), true),
        ApplicationCommandPermission::new(CommandPermissionTarget::User(test_id(5)), true)
    );
}
