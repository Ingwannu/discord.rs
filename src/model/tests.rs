use std::collections::HashMap;

use serde_json::json;

use super::{
    AllowedMentions, ApplicationCommand, ApplicationCommandOptionChoice, ApplicationInstallParams,
    ApplicationIntegrationTypeConfig, Attachment, AutocompleteInteraction, BeginGuildPruneRequest,
    Channel, ChannelType, ChatInputCommandInteraction, CommandInteractionData,
    CommandInteractionOption, ComponentInteraction, ComponentInteractionData, CreateChannelInvite,
    CreateDmChannel, CreateGuildBan, CreateGuildChannel, CreateGuildRole, CreateGuildSticker,
    CreateMessage, CreatePoll, CreateStageInstance, CreateWebhook, DefaultReaction, DiscordModel,
    Embed, EmbedField, Entitlement, ForumTag, GatewayBot, Guild, GuildScheduledEvent, Integration,
    Interaction, InteractionCallbackResponse, InteractionContextData, Invite,
    InviteTargetUsersJobStatus, Member, Message, MessageContextMenuInteraction,
    ModalSubmitInteraction, ModifyCurrentApplication, ModifyCurrentMember,
    ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildMember, ModifyGuildOnboarding,
    ModifyGuildRole, ModifyGuildRolePosition, ModifyGuildSticker, ModifyGuildWelcomeScreen,
    ModifyGuildWidgetSettings, ModifyStageInstance, ModifyUserVoiceState, ModifyWebhook,
    ModifyWebhookWithToken, PermissionOverwrite, PermissionsBitField, PingInteraction, PollAnswer,
    PollAnswerCount, PollAnswerVoters, PollMedia, PollResults, Presence, Reaction, Role,
    RoleColors, SessionStartLimit, Sku, Snowflake, StickerItem, Subscription, ThreadListResponse,
    ThreadMember, ThreadMetadata, User, UserContextMenuInteraction, WebhookExecuteQuery,
    WebhookMessageQuery, WelcomeScreenChannel,
};
use crate::parsers::V2ModalSubmission;

#[test]
fn snowflake_deserializes_from_string_and_number() {
    let string_value: Snowflake = serde_json::from_value(json!("123")).unwrap();
    let number_value: Snowflake = serde_json::from_value(json!(123)).unwrap();

    assert_eq!(string_value.as_str(), "123");
    assert_eq!(number_value.as_str(), "123");
}

#[test]
fn permissions_round_trip_through_string_wire_format() {
    let permissions = PermissionsBitField(8);
    let json = serde_json::to_value(permissions).unwrap();
    assert_eq!(json, json!("8"));

    let parsed: PermissionsBitField = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.bits(), 8);
}

#[test]
fn typed_models_keep_wire_shape() {
    let user: User = serde_json::from_value(json!({
        "id": "42",
        "username": "discordrs",
        "global_name": "discordrs"
    }))
    .unwrap();

    let serialized = serde_json::to_value(&user).unwrap();
    assert_eq!(serialized["id"], json!("42"));
    assert_eq!(serialized["username"], json!("discordrs"));
}

#[test]
fn application_command_option_choice_new_serializes_value() {
    let choice = ApplicationCommandOptionChoice::new("Support", "support");
    let serialized = serde_json::to_value(choice).unwrap();

    assert_eq!(serialized["name"], json!("Support"));
    assert_eq!(serialized["value"], json!("support"));
}

#[test]
fn snowflake_timestamp_extracts_creation_time() {
    // Discord Snowflake: timestamp is in the top 42 bits
    let sf = Snowflake::from(1759288472266248192u64);
    let ts = sf.timestamp().expect("should extract timestamp");
    // Should be a reasonable Unix timestamp (after 2020)
    assert!(ts > 1_577_836_800_000u64); // after 2020-01-01
}

#[test]
fn application_command_id_opt_is_none_until_discord_assigns_an_id() {
    let command = ApplicationCommand {
        name: "ping".to_string(),
        description: "Ping".to_string(),
        ..ApplicationCommand::default()
    };

    assert!(command.id_opt().is_none());
    assert_eq!(command.created_at(), None);
}

#[test]
fn application_command_created_at_uses_assigned_id() {
    let command = ApplicationCommand {
        id: Some(Snowflake::from(1759288472266248192u64)),
        name: "ping".to_string(),
        description: "Ping".to_string(),
        ..ApplicationCommand::default()
    };

    assert_eq!(
        command.id_opt().map(Snowflake::as_str),
        Some("1759288472266248192")
    );
    assert!(command.created_at().is_some());
}

#[test]
fn snowflake_helpers_cover_string_numeric_and_error_paths() {
    let snowflake = Snowflake::new("1759288472266248192");
    let parsed = "42".parse::<Snowflake>().unwrap();
    let invalid = Snowflake::new("not-a-number");

    assert_eq!(snowflake.as_str(), "1759288472266248192");
    assert_eq!(snowflake.as_u64(), Some(1_759_288_472_266_248_192));
    assert_eq!(snowflake.to_u64(), Some(1_759_288_472_266_248_192));
    assert_eq!(snowflake.to_string(), "1759288472266248192");
    assert_eq!(parsed.as_str(), "42");
    assert_eq!(invalid.as_u64(), None);
    assert!(snowflake.is_valid());
    assert!(!invalid.is_valid());
    assert!(Snowflake::try_new("42").is_ok());
    assert!(Snowflake::try_new("not-a-number").is_err());

    let error = serde_json::from_value::<Snowflake>(json!(-1)).unwrap_err();
    assert!(error.to_string().contains("snowflake cannot be negative"));
}

#[test]
fn permissions_bitfield_helpers_cover_mutation_and_invalid_wire_values() {
    let mut permissions = PermissionsBitField(0b0011);
    assert!(permissions.contains(0b0001));
    assert!(!permissions.contains(0b0100));

    permissions.insert(0b0100);
    assert_eq!(permissions.bits(), 0b0111);
    assert!(permissions.contains(0b0110));

    permissions.remove(0b0010);
    assert_eq!(permissions.bits(), 0b0101);
    assert!(!permissions.contains(0b0010));

    let error = serde_json::from_value::<PermissionsBitField>(json!("oops")).unwrap_err();
    assert!(error.to_string().contains("invalid permission bitfield"));
}

#[test]
fn channel_and_create_message_keep_wire_aliases_and_omit_absent_optionals() {
    let channel = Channel {
        id: Snowflake::from("10"),
        kind: 5,
        name: Some("announcements".to_string()),
        ..Channel::default()
    };
    let message = CreateMessage {
        content: Some("hello".to_string()),
        ..CreateMessage::default()
    };

    let channel_json = serde_json::to_value(&channel).unwrap();
    let message_json = serde_json::to_value(&message).unwrap();

    assert_eq!(channel_json["id"], json!("10"));
    assert_eq!(channel_json["type"], json!(5));
    assert_eq!(channel_json["name"], json!("announcements"));
    assert!(channel_json.get("guild_id").is_none());
    assert!(channel_json.get("topic").is_none());

    assert_eq!(message_json, json!({ "content": "hello" }));
}

#[test]
fn forum_channel_fields_decode_tags_and_default_reaction() {
    let channel: Channel = serde_json::from_value(json!({
        "id": "10",
        "type": 15,
        "available_tags": [{
            "id": "11",
            "name": "Support",
            "moderated": true,
            "emoji_name": "ticket"
        }],
        "applied_tags": ["11"],
        "default_reaction_emoji": { "emoji_name": "ok" },
        "default_thread_rate_limit_per_user": 30,
        "default_sort_order": 1,
        "default_forum_layout": 2
    }))
    .unwrap();

    let tag = &channel.available_tags.as_ref().unwrap()[0];
    assert_eq!(tag.id.as_str(), "11");
    assert_eq!(tag.name, "Support");
    assert!(tag.moderated);
    assert_eq!(tag.emoji_name.as_deref(), Some("ticket"));
    assert_eq!(channel.applied_tags.as_ref().unwrap()[0].as_str(), "11");
    assert_eq!(
        channel
            .default_reaction_emoji
            .as_ref()
            .and_then(|reaction| reaction.emoji_name.as_deref()),
        Some("ok")
    );
    assert_eq!(channel.default_thread_rate_limit_per_user, Some(30));

    let serialized = serde_json::to_value(Channel {
        id: Snowflake::from("20"),
        kind: 15,
        available_tags: Some(vec![ForumTag {
            id: Snowflake::from("21"),
            name: "News".to_string(),
            ..ForumTag::default()
        }]),
        default_reaction_emoji: Some(DefaultReaction {
            emoji_id: Some(Snowflake::from("22")),
            ..DefaultReaction::default()
        }),
        ..Channel::default()
    })
    .unwrap();
    assert_eq!(serialized["available_tags"][0]["name"], json!("News"));
    assert_eq!(
        serialized["default_reaction_emoji"]["emoji_id"],
        json!("22")
    );
}

#[test]
fn message_poll_decodes_and_create_poll_keeps_wire_shape() {
    let message: Message = serde_json::from_value(json!({
        "id": "500",
        "channel_id": "600",
        "poll": {
            "question": {
                "text": "Ship it?"
            },
            "answers": [
                {
                    "answer_id": 1,
                    "poll_media": {
                        "text": "Yes",
                        "emoji": { "name": "yes" }
                    }
                },
                {
                    "answer_id": 2,
                    "poll_media": {
                        "text": "No"
                    }
                }
            ],
            "expiry": "2026-04-30T00:00:00Z",
            "allow_multiselect": true,
            "layout_type": 1,
            "results": {
                "is_finalized": false,
                "answer_counts": [
                    { "id": 1, "count": 3, "me_voted": true },
                    { "id": 2, "count": 1, "me_voted": false }
                ]
            }
        },
        "reactions": [{
            "count": 5,
            "count_details": { "burst": 2, "normal": 3 },
            "me": true,
            "emoji": { "name": "spark" }
        }]
    }))
    .unwrap();

    let poll = message.poll.expect("poll should decode");
    assert_eq!(poll.question.text.as_deref(), Some("Ship it?"));
    assert_eq!(poll.answers[0].answer_id, Some(1));
    assert_eq!(poll.answers[0].poll_media.text.as_deref(), Some("Yes"));
    assert_eq!(
        poll.answers[0]
            .poll_media
            .emoji
            .as_ref()
            .and_then(|emoji| emoji.name.as_deref()),
        Some("yes")
    );
    assert!(poll.allow_multiselect);
    assert_eq!(poll.layout_type, 1);
    let results = poll.results.expect("poll results should decode");
    assert!(!results.is_finalized);
    assert_eq!(results.answer_counts[0].count, 3);
    assert!(results.answer_counts[0].me_voted);
    let reaction = &message.reactions[0];
    assert_eq!(reaction.count_details.as_ref().unwrap().burst, 2);
    assert_eq!(reaction.count_details.as_ref().unwrap().normal, 3);

    let create_message = CreateMessage {
        allowed_mentions: Some(AllowedMentions {
            users: vec![Snowflake::from("42")],
            replied_user: Some(false),
            ..AllowedMentions::default()
        }),
        poll: Some(CreatePoll {
            question: PollMedia {
                text: Some("Pick one".to_string()),
                ..PollMedia::default()
            },
            answers: vec![PollAnswer {
                poll_media: PollMedia {
                    text: Some("A".to_string()),
                    ..PollMedia::default()
                },
                ..PollAnswer::default()
            }],
            duration: Some(24),
            allow_multiselect: Some(false),
            layout_type: Some(1),
        }),
        ..CreateMessage::default()
    };

    assert_eq!(
        serde_json::to_value(&create_message).unwrap(),
        json!({
            "allowed_mentions": {
                "users": ["42"],
                "replied_user": false
            },
            "poll": {
                "question": { "text": "Pick one" },
                "answers": [
                    { "poll_media": { "text": "A" } }
                ],
                "duration": 24,
                "allow_multiselect": false,
                "layout_type": 1
            }
        })
    );

    let _default_results = PollResults {
        answer_counts: vec![PollAnswerCount {
            id: 1,
            count: 0,
            me_voted: false,
        }],
        ..PollResults::default()
    };
}

#[test]
fn guild_member_decodes_current_profile_fields() {
    let member: Member = serde_json::from_value(json!({
        "user": { "id": "42", "username": "profiled" },
        "roles": ["100"],
        "avatar": "guild_avatar",
        "banner": "guild_banner",
        "avatar_decoration_data": {
            "asset": "decoration_asset",
            "sku_id": "555"
        },
        "collectibles": {
            "nameplate": {
                "sku_id": "777",
                "asset": "nameplate_asset",
                "label": "Champion",
                "palette": "violet"
            }
        },
        "flags": 1
    }))
    .unwrap();

    assert_eq!(member.banner.as_deref(), Some("guild_banner"));
    assert_eq!(
        member
            .avatar_decoration_data
            .as_ref()
            .map(|decoration| decoration.sku_id.as_str()),
        Some("555")
    );
    assert_eq!(
        member
            .collectibles
            .as_ref()
            .and_then(|collectibles| collectibles.nameplate.as_ref())
            .map(|nameplate| nameplate.label.as_str()),
        Some("Champion")
    );

    let serialized = serde_json::to_value(&member).unwrap();
    assert_eq!(serialized["banner"], json!("guild_banner"));
    assert_eq!(
        serialized["avatar_decoration_data"]["asset"],
        json!("decoration_asset")
    );
    assert_eq!(
        serialized["collectibles"]["nameplate"]["palette"],
        json!("violet")
    );
}

#[test]
fn guild_role_position_body_keeps_nullable_wire_shape() {
    let positions = vec![
        ModifyGuildRolePosition {
            id: Snowflake::from("300"),
            position: Some(Some(1)),
        },
        ModifyGuildRolePosition {
            id: Snowflake::from("301"),
            position: Some(None),
        },
    ];

    assert_eq!(
        serde_json::to_value(&positions).unwrap(),
        json!([
            { "id": "300", "position": 1 },
            { "id": "301", "position": null }
        ])
    );
}

#[test]
fn guild_request_bodies_keep_nullable_wire_shape() {
    let member = ModifyGuildMember {
        nick: Some(None),
        roles: Some(Some(vec![Snowflake::from("300")])),
        mute: Some(Some(false)),
        deaf: None,
        channel_id: Some(None),
        communication_disabled_until: Some(Some("2026-05-01T00:00:00Z".to_string())),
        flags: Some(Some(1)),
    };
    assert_eq!(
        serde_json::to_value(&member).unwrap(),
        json!({
            "nick": null,
            "roles": ["300"],
            "mute": false,
            "channel_id": null,
            "communication_disabled_until": "2026-05-01T00:00:00Z",
            "flags": 1
        })
    );

    let current_member = ModifyCurrentMember {
        nick: Some(Some("bot".to_string())),
        banner: Some(None),
        avatar: None,
        bio: Some(Some("shipping".to_string())),
    };
    assert_eq!(
        serde_json::to_value(&current_member).unwrap(),
        json!({
            "nick": "bot",
            "banner": null,
            "bio": "shipping"
        })
    );

    let ban = CreateGuildBan {
        delete_message_days: None,
        delete_message_seconds: Some(60),
    };
    assert_eq!(
        serde_json::to_value(&ban).unwrap(),
        json!({ "delete_message_seconds": 60 })
    );

    let create_role = CreateGuildRole {
        name: Some("gradient".to_string()),
        colors: Some(RoleColors {
            primary_color: 11127295,
            secondary_color: Some(16759788),
            tertiary_color: Some(16761760),
        }),
        icon: Some(None),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&create_role).unwrap(),
        json!({
            "name": "gradient",
            "colors": {
                "primary_color": 11127295,
                "secondary_color": 16759788,
                "tertiary_color": 16761760
            },
            "icon": null
        })
    );

    let modify_role = ModifyGuildRole {
        name: Some(None),
        permissions: Some(Some(PermissionsBitField(8))),
        colors: Some(None),
        unicode_emoji: Some(None),
        mentionable: Some(Some(true)),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&modify_role).unwrap(),
        json!({
            "name": null,
            "permissions": "8",
            "colors": null,
            "unicode_emoji": null,
            "mentionable": true
        })
    );
}

#[test]
fn guild_admin_request_bodies_keep_nullable_wire_shape() {
    let guild = ModifyGuild {
        name: Some("renamed".to_string()),
        afk_channel_id: Some(None),
        features: Some(vec!["COMMUNITY".to_string()]),
        description: Some(None),
        premium_progress_bar_enabled: Some(true),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&guild).unwrap(),
        json!({
            "name": "renamed",
            "afk_channel_id": null,
            "features": ["COMMUNITY"],
            "description": null,
            "premium_progress_bar_enabled": true
        })
    );

    let channel = CreateGuildChannel {
        name: "rules".to_string(),
        kind: Some(ChannelType::Text as u8),
        topic: Some(Some("read first".to_string())),
        permission_overwrites: Some(Some(vec![PermissionOverwrite {
            id: Snowflake::from("300"),
            kind: 0,
            allow: Some(PermissionsBitField(1024)),
            deny: None,
        }])),
        parent_id: Some(None),
        default_reaction_emoji: Some(None),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&channel).unwrap(),
        json!({
            "name": "rules",
            "type": 0,
            "topic": "read first",
            "permission_overwrites": [{
                "id": "300",
                "type": 0,
                "allow": "1024"
            }],
            "parent_id": null,
            "default_reaction_emoji": null
        })
    );

    let widget = ModifyGuildWidgetSettings {
        enabled: Some(true),
        channel_id: Some(None),
    };
    assert_eq!(
        serde_json::to_value(&widget).unwrap(),
        json!({ "enabled": true, "channel_id": null })
    );

    let welcome_screen = ModifyGuildWelcomeScreen {
        enabled: Some(Some(true)),
        welcome_channels: Some(Some(vec![WelcomeScreenChannel {
            channel_id: Snowflake::from("202"),
            description: "Start here".to_string(),
            emoji_id: None,
            emoji_name: Some("wave".to_string()),
        }])),
        description: Some(None),
    };
    assert_eq!(
        serde_json::to_value(&welcome_screen).unwrap(),
        json!({
            "enabled": true,
            "welcome_channels": [{
                "channel_id": "202",
                "description": "Start here",
                "emoji_name": "wave"
            }],
            "description": null
        })
    );

    let onboarding = ModifyGuildOnboarding {
        prompts: Some(vec![json!({ "id": "1", "title": "Pick a topic" })]),
        default_channel_ids: Some(vec![Snowflake::from("202")]),
        enabled: Some(true),
        mode: Some(0),
    };
    assert_eq!(
        serde_json::to_value(&onboarding).unwrap(),
        json!({
            "prompts": [{ "id": "1", "title": "Pick a topic" }],
            "default_channel_ids": ["202"],
            "enabled": true,
            "mode": 0
        })
    );

    let prune = BeginGuildPruneRequest {
        days: Some(7),
        compute_prune_count: Some(false),
        include_roles: Some(vec![Snowflake::from("300")]),
        reason: None,
    };
    assert_eq!(
        serde_json::to_value(&prune).unwrap(),
        json!({
            "days": 7,
            "compute_prune_count": false,
            "include_roles": ["300"]
        })
    );
}

#[test]
fn monetization_poll_and_thread_response_models_decode() {
    let sku: Sku = serde_json::from_value(json!({
        "id": "1088510058284990888",
        "type": 5,
        "dependent_sku_id": null,
        "application_id": "788708323867885999",
        "manifest_labels": null,
        "access_type": 1,
        "name": "Test Premium",
        "features": [],
        "release_date": null,
        "premium": false,
        "slug": "test-premium",
        "flags": 128,
        "show_age_gate": false
    }))
    .unwrap();
    assert_eq!(sku.kind, 5);
    assert_eq!(sku.access_type, Some(1));
    assert_eq!(sku.premium, Some(false));
    assert_eq!(sku.show_age_gate, Some(false));
    assert!(sku.features.is_empty());

    let entitlement: Entitlement = serde_json::from_value(json!({
        "id": "1019653849998299136",
        "sku_id": "1019475255913222144",
        "application_id": "1019370614521200640",
        "user_id": "771129655544643584",
        "promotion_id": null,
        "type": 8,
        "deleted": false,
        "gift_code_flags": 0,
        "consumed": false,
        "starts_at": "2022-09-14T17:00:18.704163+00:00",
        "ends_at": "2022-10-14T17:00:18.704163+00:00",
        "guild_id": "1015034326372454400",
        "subscription_id": "1019653835926409216"
    }))
    .unwrap();
    assert_eq!(entitlement.kind, 8);
    assert_eq!(entitlement.gift_code_flags, Some(0));
    assert_eq!(
        entitlement.subscription_id.as_ref().map(Snowflake::as_str),
        Some("1019653835926409216")
    );

    let subscription: Subscription = serde_json::from_value(json!({
        "id": "1278078770116427839",
        "user_id": "1088605110638227537",
        "sku_ids": ["1158857122189168803"],
        "entitlement_ids": ["1"],
        "renewal_sku_ids": null,
        "current_period_start": "2024-08-27T19:48:44.406602+00:00",
        "current_period_end": "2024-09-27T19:48:44.406602+00:00",
        "status": 0,
        "canceled_at": null
    }))
    .unwrap();
    assert_eq!(subscription.sku_ids[0].as_str(), "1158857122189168803");
    assert_eq!(subscription.status, 0);

    let voters: PollAnswerVoters = serde_json::from_value(json!({
        "users": [{ "id": "42", "username": "voter" }]
    }))
    .unwrap();
    assert_eq!(voters.users[0].username, "voter");

    let threads: ThreadListResponse = serde_json::from_value(json!({
        "threads": [{ "id": "50", "type": 11, "name": "thread" }],
        "members": [{ "id": "50", "user_id": "42", "join_timestamp": "2026-04-29T00:00:00Z", "flags": 0 }],
        "has_more": false
    }))
    .unwrap();
    assert_eq!(threads.threads[0].id.as_str(), "50");
    assert_eq!(threads.members[0].user_id.as_ref().unwrap().as_str(), "42");

    let thread_member = ThreadMember {
        user_id: Some(Snowflake::from("42")),
        member: Some(Member {
            user: Some(User {
                id: Snowflake::from("42"),
                username: "member".to_string(),
                ..User::default()
            }),
            ..Member::default()
        }),
        ..ThreadMember::default()
    };
    assert_eq!(
        thread_member
            .member
            .as_ref()
            .and_then(|member| member.user.as_ref())
            .map(|user| user.username.as_str()),
        Some("member")
    );
}

#[test]
fn message_resource_extended_fields_decode() {
    let message: Message = serde_json::from_value(json!({
        "id": "1000",
        "channel_id": "2000",
        "content": "forwarded",
        "mention_roles": ["3000"],
        "application_id": "4000",
        "application": { "id": "4000", "name": "Activity App" },
        "activity": { "type": 1, "party_id": "party" },
        "role_subscription_data": {
            "role_subscription_listing_id": "5000",
            "tier_name": "Gold",
            "total_months_subscribed": 7,
            "is_renewal": true
        },
        "message_reference": {
            "type": 1,
            "message_id": "6000",
            "channel_id": "7000"
        },
        "message_snapshots": [{
            "message": {
                "type": 0,
                "content": "snapshot",
                "timestamp": "2026-05-01T00:00:00.000000+00:00",
                "flags": 16384,
                "mentions": [{ "id": "42", "username": "mentioned" }],
                "mention_roles": ["8000"],
                "sticker_items": [{
                    "id": "9000",
                    "name": "ship",
                    "format_type": 1
                }],
                "components": [{ "type": 10, "content": "snapshot text" }]
            }
        }],
        "referenced_message": {
            "id": "6000",
            "channel_id": "7000",
            "content": "original"
        },
        "interaction_metadata": { "id": "9100", "type": 2 },
        "interaction": { "id": "9100", "name": "old" },
        "resolved": { "users": {} },
        "call": {
            "participants": ["42"],
            "ended_timestamp": null
        },
        "shared_client_theme": {
            "colors": ["5865F2", "7258F2"],
            "gradient_angle": 45,
            "base_mix": 58,
            "base_theme": 1
        }
    }))
    .unwrap();

    assert_eq!(message.mention_roles[0].as_str(), "3000");
    assert_eq!(message.application_id.unwrap().as_str(), "4000");
    assert_eq!(message.activity.unwrap().party_id.as_deref(), Some("party"));
    assert!(message
        .role_subscription_data
        .as_ref()
        .is_some_and(|data| data.is_renewal));
    assert_eq!(message.message_reference.unwrap().kind, Some(1));
    assert_eq!(message.message_snapshots[0].message.content, "snapshot");
    assert_eq!(
        message.message_snapshots[0].message.sticker_items[0]
            .id
            .as_str(),
        "9000"
    );
    assert_eq!(
        message
            .referenced_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("original")
    );
    assert_eq!(message.call.unwrap().participants[0].as_str(), "42");
    assert_eq!(
        message.shared_client_theme.unwrap().colors,
        vec!["5865F2".to_string(), "7258F2".to_string()]
    );
}

#[test]
fn integration_model_decodes_core_guild_integration_shape() {
    let integration: Integration = serde_json::from_value(json!({
        "id": "100",
        "name": "Twitch",
        "type": "twitch",
        "enabled": true,
        "account": { "id": "abc", "name": "stream" },
        "application": {
            "id": "200",
            "name": "App",
            "description": "integration app"
        },
        "scopes": ["bot"]
    }))
    .unwrap();

    assert_eq!(integration.id.as_str(), "100");
    assert_eq!(integration.kind, "twitch");
    assert_eq!(integration.account.name, "stream");
    assert_eq!(integration.application.as_ref().unwrap().id.as_str(), "200");
}

#[test]
fn embed_field_and_focus_helpers_follow_default_and_true_branches() {
    let default_field = EmbedField {
        name: "Name".to_string(),
        value: "Value".to_string(),
        ..EmbedField::default()
    };
    let inline_field = EmbedField {
        inline: true,
        ..default_field.clone()
    };
    let unfocused = CommandInteractionOption::default();
    let focused = CommandInteractionOption {
        focused: Some(true),
        ..CommandInteractionOption::default()
    };

    let default_json = serde_json::to_value(&default_field).unwrap();
    let inline_json = serde_json::to_value(&inline_field).unwrap();

    assert!(default_json.get("inline").is_none());
    assert_eq!(inline_json["inline"], json!(true));
    assert!(!unfocused.is_focused());
    assert!(focused.is_focused());
}

#[test]
fn interaction_accessors_and_discord_model_trait_delegate_to_context_and_ids() {
    let context = InteractionContextData {
        id: Snowflake::from("99"),
        application_id: Snowflake::from("77"),
        token: "token-123".to_string(),
        ..InteractionContextData::default()
    };
    let interaction = Interaction::Component(ComponentInteraction {
        context: context.clone(),
        data: ComponentInteractionData {
            custom_id: "button".to_string(),
            component_type: 2,
            values: vec!["x".to_string()],
        },
    });
    let user = User {
        id: Snowflake::from(1759288472266248192u64),
        username: "discordrs".to_string(),
        ..User::default()
    };

    assert_eq!(interaction.context().id.as_str(), "99");
    assert_eq!(interaction.id().as_str(), "99");
    assert_eq!(interaction.application_id().as_str(), "77");
    assert_eq!(interaction.token(), "token-123");

    assert_eq!(DiscordModel::id(&user).as_str(), "1759288472266248192");
    assert_eq!(
        DiscordModel::id_opt(&user).map(Snowflake::as_str),
        Some("1759288472266248192")
    );
    assert!(DiscordModel::created_at(&user).is_some());
}

#[test]
fn interaction_accessors_cover_all_variants() {
    fn context(id: &str, application_id: &str, token: &str) -> InteractionContextData {
        InteractionContextData {
            id: Snowflake::from(id),
            application_id: Snowflake::from(application_id),
            token: token.to_string(),
            ..InteractionContextData::default()
        }
    }

    let interactions = [
        Interaction::Ping(PingInteraction {
            context: context("1", "10", "ping-token"),
        }),
        Interaction::ChatInputCommand(ChatInputCommandInteraction {
            context: context("2", "20", "chat-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::UserContextMenu(UserContextMenuInteraction {
            context: context("3", "30", "user-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::MessageContextMenu(MessageContextMenuInteraction {
            context: context("4", "40", "message-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::Autocomplete(AutocompleteInteraction {
            context: context("5", "50", "autocomplete-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::Component(ComponentInteraction {
            context: context("6", "60", "component-token"),
            data: ComponentInteractionData::default(),
        }),
        Interaction::ModalSubmit(ModalSubmitInteraction {
            context: context("7", "70", "modal-token"),
            submission: V2ModalSubmission {
                custom_id: "modal".to_string(),
                components: vec![],
            },
        }),
        Interaction::Unknown {
            context: context("8", "80", "unknown-token"),
            kind: 99,
            raw_data: json!({ "kind": "unknown" }),
        },
    ];

    let expected = [
        ("1", "10", "ping-token"),
        ("2", "20", "chat-token"),
        ("3", "30", "user-token"),
        ("4", "40", "message-token"),
        ("5", "50", "autocomplete-token"),
        ("6", "60", "component-token"),
        ("7", "70", "modal-token"),
        ("8", "80", "unknown-token"),
    ];

    for (interaction, (id, application_id, token)) in interactions.iter().zip(expected) {
        assert_eq!(interaction.context().id.as_str(), id);
        assert_eq!(interaction.id().as_str(), id);
        assert_eq!(interaction.application_id().as_str(), application_id);
        assert_eq!(interaction.token(), token);
    }
}

#[test]
fn discord_model_trait_returns_ids_for_core_models() {
    let guild = Guild {
        id: Snowflake::from("11"),
        name: "Guild".to_string(),
        ..Guild::default()
    };
    let channel = Channel {
        id: Snowflake::from("12"),
        kind: 0,
        ..Channel::default()
    };
    let message = Message {
        id: Snowflake::from("13"),
        channel_id: Snowflake::from("99"),
        ..Message::default()
    };
    let role = Role {
        id: Snowflake::from("14"),
        name: "Admin".to_string(),
        ..Role::default()
    };
    let attachment = Attachment {
        id: Snowflake::from("15"),
        filename: "file.txt".to_string(),
        ..Attachment::default()
    };

    assert_eq!(DiscordModel::id(&guild).as_str(), "11");
    assert_eq!(DiscordModel::id(&channel).as_str(), "12");
    assert_eq!(DiscordModel::id(&message).as_str(), "13");
    assert_eq!(DiscordModel::id(&role).as_str(), "14");
    assert_eq!(DiscordModel::id(&attachment).as_str(), "15");
}

#[test]
fn serde_defaults_fill_missing_fields_for_core_models() {
    let member: Member = serde_json::from_value(json!({})).unwrap();
    let message: Message = serde_json::from_value(json!({
        "id": "55",
        "channel_id": "66"
    }))
    .unwrap();
    let reaction: Reaction = serde_json::from_value(json!({})).unwrap();
    let component: ComponentInteractionData = serde_json::from_value(json!({
        "custom_id": "menu",
        "component_type": 3
    }))
    .unwrap();
    let thread_metadata: ThreadMetadata = serde_json::from_value(json!({})).unwrap();

    assert!(member.roles.is_empty());
    assert_eq!(message.content, "");
    assert!(message.mentions.is_empty());
    assert!(message.attachments.is_empty());
    assert!(message.embeds.is_empty());
    assert!(message.reactions.is_empty());
    assert_eq!(reaction.count, 0);
    assert!(!reaction.me);
    assert!(reaction.emoji.is_none());
    assert!(component.values.is_empty());
    assert!(!thread_metadata.archived);
    assert!(!thread_metadata.locked);
    assert!(thread_metadata.auto_archive_duration.is_none());
}

#[test]
fn simple_payload_models_keep_wire_aliases_and_omit_absent_optionals() {
    let callback = InteractionCallbackResponse {
        kind: 4,
        ..InteractionCallbackResponse::default()
    };
    let dm_channel = CreateDmChannel {
        recipient_id: Snowflake::from("321"),
    };
    let sticker = StickerItem {
        id: Snowflake::from("654"),
        name: "party".to_string(),
        kind: Some(1),
    };
    let invite = Invite {
        kind: Some(0),
        code: Some("abc".to_string()),
        approximate_member_count: Some(42),
        roles: vec![Role {
            id: Snowflake::from("700"),
            name: "guest".to_string(),
            ..Role::default()
        }],
        ..Invite::default()
    };
    let invite_body = CreateChannelInvite {
        role_ids: Some(vec![Snowflake::from("700")]),
        ..CreateChannelInvite::default()
    };
    let invite_job = InviteTargetUsersJobStatus {
        status: 1,
        total_users: 100,
        processed_users: 41,
        created_at: "2025-01-08T12:00:00.000000+00:00".to_string(),
        completed_at: None,
        error_message: None,
    };
    let gateway = GatewayBot {
        url: "wss://gateway.discord.gg".to_string(),
        shards: 2,
        session_start_limit: SessionStartLimit {
            total: 1000,
            remaining: 999,
            reset_after: 60_000,
            max_concurrency: 1,
        },
    };

    assert_eq!(
        serde_json::to_value(&callback).unwrap(),
        json!({ "type": 4 })
    );
    assert_eq!(
        serde_json::to_value(&dm_channel).unwrap(),
        json!({ "recipient_id": "321" })
    );
    assert_eq!(
        serde_json::to_value(&sticker).unwrap(),
        json!({ "id": "654", "name": "party", "format_type": 1 })
    );
    assert_eq!(
        serde_json::to_value(&invite).unwrap(),
        json!({
            "type": 0,
            "code": "abc",
            "approximate_member_count": 42,
            "roles": [{
                "id": "700",
                "name": "guest"
            }]
        })
    );
    assert_eq!(
        serde_json::to_value(&invite_body).unwrap(),
        json!({ "role_ids": ["700"] })
    );
    assert_eq!(
        serde_json::to_value(&invite_job).unwrap(),
        json!({
            "status": 1,
            "total_users": 100,
            "processed_users": 41,
            "created_at": "2025-01-08T12:00:00.000000+00:00"
        })
    );
    assert_eq!(
        serde_json::to_value(&gateway).unwrap()["session_start_limit"]["remaining"],
        999
    );
}

#[test]
fn stage_instance_request_bodies_keep_wire_shape() {
    let create = CreateStageInstance {
        channel_id: Snowflake::from("400"),
        topic: "town hall".to_string(),
        privacy_level: Some(2),
        send_start_notification: Some(true),
        guild_scheduled_event_id: Some(Snowflake::from("500")),
    };
    assert_eq!(
        serde_json::to_value(&create).unwrap(),
        json!({
            "channel_id": "400",
            "topic": "town hall",
            "privacy_level": 2,
            "send_start_notification": true,
            "guild_scheduled_event_id": "500"
        })
    );

    let modify = ModifyStageInstance {
        privacy_level: Some(2),
    };
    assert_eq!(
        serde_json::to_value(&modify).unwrap(),
        json!({ "privacy_level": 2 })
    );
}

#[test]
fn sticker_request_bodies_keep_wire_shape() {
    let create = CreateGuildSticker {
        name: "wave".to_string(),
        description: "Waves hello".to_string(),
        tags: "wave,hello".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&create).unwrap(),
        json!({
            "name": "wave",
            "description": "Waves hello",
            "tags": "wave,hello"
        })
    );

    let modify = ModifyGuildSticker {
        name: Some("wave2".to_string()),
        description: Some(None),
        tags: Some("wave".to_string()),
    };
    assert_eq!(
        serde_json::to_value(&modify).unwrap(),
        json!({
            "name": "wave2",
            "description": null,
            "tags": "wave"
        })
    );
}

#[test]
fn voice_state_request_bodies_keep_wire_shape() {
    let current = ModifyCurrentUserVoiceState {
        channel_id: Some(Snowflake::from("200")),
        suppress: Some(false),
        request_to_speak_timestamp: Some(None),
    };
    assert_eq!(
        serde_json::to_value(&current).unwrap(),
        json!({
            "channel_id": "200",
            "suppress": false,
            "request_to_speak_timestamp": null
        })
    );

    let user = ModifyUserVoiceState {
        channel_id: Some(Snowflake::from("200")),
        suppress: Some(true),
    };
    assert_eq!(
        serde_json::to_value(&user).unwrap(),
        json!({
            "channel_id": "200",
            "suppress": true
        })
    );
}

#[test]
fn current_application_request_body_keeps_wire_shape() {
    let body = ModifyCurrentApplication {
        description: Some("updated".to_string()),
        role_connections_verification_url: Some(None),
        install_params: Some(ApplicationInstallParams {
            scopes: vec!["bot".to_string(), "applications.commands".to_string()],
            permissions: PermissionsBitField(2048),
        }),
        integration_types_config: Some(HashMap::from([(
            "0".to_string(),
            ApplicationIntegrationTypeConfig {
                oauth2_install_params: Some(ApplicationInstallParams {
                    scopes: vec!["applications.commands".to_string()],
                    permissions: PermissionsBitField(0),
                }),
            },
        )])),
        flags: Some(1 << 14),
        icon: Some(None),
        cover_image: Some(Some("data:image/png;base64,abc".to_string())),
        interactions_endpoint_url: Some(Some("https://example.com/interactions".to_string())),
        tags: Some(vec!["utility".to_string()]),
        event_webhooks_url: Some(None),
        event_webhooks_status: Some(2),
        event_webhooks_types: Some(vec!["APPLICATION_AUTHORIZED".to_string()]),
        ..ModifyCurrentApplication::default()
    };

    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        json!({
            "description": "updated",
            "role_connections_verification_url": null,
            "install_params": {
                "scopes": ["bot", "applications.commands"],
                "permissions": "2048"
            },
            "integration_types_config": {
                "0": {
                    "oauth2_install_params": {
                        "scopes": ["applications.commands"],
                        "permissions": "0"
                    }
                }
            },
            "flags": 16384,
            "icon": null,
            "cover_image": "data:image/png;base64,abc",
            "interactions_endpoint_url": "https://example.com/interactions",
            "tags": ["utility"],
            "event_webhooks_url": null,
            "event_webhooks_status": 2,
            "event_webhooks_types": ["APPLICATION_AUTHORIZED"]
        })
    );
}

#[test]
fn webhook_request_bodies_and_queries_keep_wire_shape() {
    let create = CreateWebhook {
        name: "deployments".to_string(),
        avatar: Some(None),
    };
    assert_eq!(
        serde_json::to_value(&create).unwrap(),
        json!({
            "name": "deployments",
            "avatar": null
        })
    );

    let modify = ModifyWebhook {
        name: Some("ops".to_string()),
        avatar: Some(Some("data:image/png;base64,abc".to_string())),
        channel_id: Some(Snowflake::from("500")),
    };
    assert_eq!(
        serde_json::to_value(&modify).unwrap(),
        json!({
            "name": "ops",
            "avatar": "data:image/png;base64,abc",
            "channel_id": "500"
        })
    );

    let token_modify = ModifyWebhookWithToken {
        name: Some("public".to_string()),
        avatar: Some(None),
    };
    assert_eq!(
        serde_json::to_value(&token_modify).unwrap(),
        json!({
            "name": "public",
            "avatar": null
        })
    );

    let query = WebhookExecuteQuery {
        wait: Some(false),
        thread_id: Some(Snowflake::from("600")),
        with_components: Some(true),
    };
    assert_eq!(
        serde_json::to_value(&query).unwrap(),
        json!({
            "wait": false,
            "thread_id": "600",
            "with_components": true
        })
    );

    let message_query = WebhookMessageQuery {
        thread_id: Some(Snowflake::from("601")),
        with_components: Some(false),
    };
    assert_eq!(
        serde_json::to_value(&message_query).unwrap(),
        json!({
            "thread_id": "601",
            "with_components": false
        })
    );
}

#[test]
fn scheduled_event_recurrence_and_reaction_emoji_are_typed() {
    let event: GuildScheduledEvent = serde_json::from_value(json!({
        "id": "1",
        "guild_id": "2",
        "name": "standup",
        "scheduled_start_time": "2026-04-29T00:00:00.000000+00:00",
        "privacy_level": 2,
        "status": 1,
        "entity_type": 3,
        "entity_metadata": { "location": "voice" },
        "recurrence_rule": {
            "start": "2026-04-29T00:00:00.000000+00:00",
            "frequency": 2,
            "interval": 1,
            "by_weekday": [1],
            "by_n_weekday": [{ "n": 1, "day": 1 }]
        }
    }))
    .unwrap();
    let reaction: Reaction = serde_json::from_value(json!({
        "count": 2,
        "me": true,
        "emoji": { "id": "10", "name": "party", "animated": true }
    }))
    .unwrap();

    assert_eq!(
        event
            .entity_metadata
            .as_ref()
            .and_then(|metadata| metadata.location.as_deref()),
        Some("voice")
    );
    assert_eq!(
        event
            .recurrence_rule
            .as_ref()
            .and_then(|rule| rule.by_n_weekday.as_ref())
            .and_then(|weekdays| weekdays.first())
            .map(|weekday| weekday.day),
        Some(1)
    );
    assert_eq!(
        reaction
            .emoji
            .as_ref()
            .and_then(|emoji| emoji.name.as_deref()),
        Some("party")
    );
}

#[test]
fn embed_presence_and_permissions_cover_optional_and_numeric_serde_paths() {
    let embed = Embed {
        title: Some("Docs".to_string()),
        ..Embed::default()
    };
    let presence = Presence {
        user_id: Some(Snowflake::from("777")),
        ..Presence::default()
    };
    let numeric_permissions: PermissionsBitField = serde_json::from_value(json!(16)).unwrap();
    let invalid_timestamp = Snowflake::new("not-a-number");

    let embed_json = serde_json::to_value(&embed).unwrap();
    let presence_json = serde_json::to_value(&presence).unwrap();

    assert_eq!(embed_json["title"], json!("Docs"));
    assert_eq!(embed_json["fields"], json!([]));
    assert!(embed_json.get("description").is_none());
    assert_eq!(presence_json, json!({ "user_id": "777" }));
    assert_eq!(numeric_permissions.bits(), 16);
    assert_eq!(invalid_timestamp.timestamp(), None);
}
