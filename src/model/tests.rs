use crate::model::test_id;
use std::collections::HashMap;

use serde_json::json;

use super::{
    AllowedMentions, ApplicationCommand, ApplicationCommandOptionChoice,
    ApplicationEventWebhookStatus, ApplicationInstallParams, ApplicationIntegrationTypeConfig,
    Attachment, AuthorizingIntegrationOwners, AutocompleteInteraction, BeginGuildPruneRequest,
    Channel, ChannelType, ChatInputCommandInteraction, CommandInteractionData,
    CommandInteractionOption, ComponentId, ComponentInteraction, ComponentInteractionData,
    ComponentType, CreateChannelInvite, CreateDmChannel, CreateGuildBan, CreateGuildChannel,
    CreateGuildRole, CreateGuildSticker, CreateMessage, CreatePoll, CreateStageInstance,
    CreateWebhook, DefaultReaction, DiscordModel, Embed, EmbedField, Entitlement, EntitlementType,
    ForumTag, GatewayBot, GenericId, Guild, GuildId, GuildInstallOwner, GuildScheduledEvent,
    GuildWidget, Integration, Interaction, InteractionCallbackResponse, InteractionContextData,
    InteractionResponseType, InteractionType, Invite, InviteTargetUsersJobStatus,
    InviteTargetUsersJobStatusType, InviteType, Member, Message, MessageComponent,
    MessageContextMenuInteraction, MessageReferenceType, ModalSubmitInteraction,
    ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUserVoiceState, ModifyGuild,
    ModifyGuildMember, ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition,
    ModifyGuildSticker, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyStageInstance,
    ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken, OnboardingMode, OverwriteType,
    PermissionOverwrite, Permissions, PingInteraction, PollAnswer, PollAnswerCount, PollAnswerId,
    PollAnswerVoters, PollLayoutType, PollMedia, PollResults, Presence, Reaction,
    RecurrenceRuleWeekday, ResolvedData, Role, RoleColors, SeparatorSpacing, SessionStartLimit,
    Sku, SkuAccessType, SkuType, StageInstancePrivacyLevel, StickerFormatType, StickerItem,
    Subscription, SubscriptionStatus, ThreadListResponse, ThreadMember, ThreadMetadata, User,
    UserContextMenuInteraction, UserId, WebhookExecuteQuery, WebhookMessageQuery,
    WelcomeScreenChannel,
};
use crate::parsers::V2ModalSubmission;

#[test]
fn snowflake_deserializes_from_string_and_number() {
    let string_value: GenericId = serde_json::from_value(json!("123")).unwrap();
    let number_value: GenericId = serde_json::from_value(json!(123)).unwrap();

    assert_eq!(string_value.get(), 123);
    assert_eq!(number_value.get(), 123);
}

#[test]
fn permissions_round_trip_through_string_wire_format() {
    let permissions = Permissions::from_bits(8);
    let json = serde_json::to_value(permissions).unwrap();
    assert_eq!(json, json!("8"));

    let parsed: Permissions = serde_json::from_value(json).unwrap();
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
fn command_option_values_keep_their_json_type() {
    use super::CommandOptionValue;

    let decode = |json: &str| serde_json::from_str::<CommandOptionValue>(json);
    assert_eq!(
        decode(r#""billing""#).unwrap(),
        CommandOptionValue::String("billing".to_string())
    );
    assert_eq!(decode("6").unwrap(), CommandOptionValue::Integer(6));
    assert_eq!(decode("-6").unwrap(), CommandOptionValue::Integer(-6));
    assert_eq!(decode("0.5").unwrap(), CommandOptionValue::Number(0.5));
    assert_eq!(decode("true").unwrap(), CommandOptionValue::Boolean(true));
    // Discord bounds integers to +-2^53; a u64 past i64 is not an option.
    assert!(decode("18446744073709551615").is_err());
    assert!(decode("null").is_err());
    assert!(decode("{}").is_err());
    assert!(decode("[1]").is_err());

    // A NUMBER option sent without a fraction still reads as a number.
    assert_eq!(decode("5").unwrap().as_f64(), Some(5.0));
    assert_eq!(decode("5").unwrap().as_i64(), Some(5));
    assert_eq!(decode("5.5").unwrap().as_i64(), None);
    assert_eq!(decode(r#""5""#).unwrap().as_i64(), None);
    assert_eq!(decode("true").unwrap().as_bool(), Some(true));
    let user: Option<UserId> = decode(r#""80351110224678912""#).unwrap().as_id();
    assert_eq!(user.map(UserId::get), Some(80_351_110_224_678_912));
    let not_an_id: Option<UserId> = decode(r#""abc""#).unwrap().as_id();
    assert_eq!(not_an_id, None);

    for json in [r#""text""#, "6", "-6", "0.5", "false"] {
        let value = decode(json).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }

    // Choices take any value that fits, and other `Serialize` values
    // through `try_new`.
    assert_eq!(
        ApplicationCommandOptionChoice::new("Six", 6_u32).value,
        CommandOptionValue::Integer(6)
    );
    assert_eq!(
        ApplicationCommandOptionChoice::try_new("Big", 9_007_199_254_740_991_u64)
            .unwrap()
            .value,
        CommandOptionValue::Integer(9_007_199_254_740_991)
    );
    assert!(ApplicationCommandOptionChoice::try_new("List", vec![1]).is_err());
    let choice: ApplicationCommandOptionChoice =
        serde_json::from_value(json!({ "name": "Half", "value": 0.5 })).unwrap();
    assert_eq!(choice.value.as_f64(), Some(0.5));
}

#[test]
fn snowflake_timestamp_extracts_creation_time() {
    // Discord Snowflake: timestamp is in the top 42 bits
    let sf: GenericId = test_id(1759288472266248192);
    let ts = sf.timestamp();
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
        id: Some(test_id(1759288472266248192)),
        name: "ping".to_string(),
        description: "Ping".to_string(),
        ..ApplicationCommand::default()
    };

    assert_eq!(command.id_opt().map(u64::from), Some(1759288472266248192));
    assert!(command.created_at().is_some());
}

#[test]
fn generic_id_construction_is_checked_and_round_trips_as_a_string() {
    let id = GenericId::try_from(1_759_288_472_266_248_192_u64).unwrap();
    let parsed = "42".parse::<GenericId>().unwrap();

    assert_eq!(id.get(), 1_759_288_472_266_248_192);
    assert_eq!(id.to_string(), "1759288472266248192");
    assert_eq!(parsed.get(), 42);
    assert!("not-a-number".parse::<GenericId>().is_err());
    assert!(GenericId::try_from(0_u64).is_err());
    assert_eq!(
        serde_json::to_value(id).unwrap(),
        json!("1759288472266248192")
    );
    assert_eq!(
        serde_json::from_value::<GenericId>(json!(1_759_288_472_266_248_192_u64)).unwrap(),
        id
    );
    assert!(serde_json::from_value::<GenericId>(json!(-1)).is_err());
    assert!(serde_json::from_value::<GenericId>(json!("abc")).is_err());
}

#[test]
fn model_permission_fields_use_the_unified_permissions_type() {
    // The 2.x in-place `insert`/`remove` map to `|=` and `set = set.remove(..)`.
    let mut permissions = Permissions::from_bits(0b0011);
    assert!(permissions.contains(0b0001));
    assert!(!permissions.contains(0b0100));

    permissions |= 0b0100;
    assert_eq!(permissions.bits(), 0b0111);
    assert!(permissions.contains(0b0110));

    permissions = permissions.remove(0b0010);
    assert_eq!(permissions.bits(), 0b0101);
    assert!(!permissions.contains(0b0010));

    // Strings or non-negative integers in; strings out. Unknown bits stay.
    let role: Role = serde_json::from_value(json!({
        "id": "1",
        "name": "r",
        "permissions": "9223372036854775808"
    }))
    .unwrap();
    assert_eq!(role.permissions.map(Permissions::bits), Some(1 << 63));
    assert_eq!(
        serde_json::to_value(&role).unwrap()["permissions"],
        json!("9223372036854775808")
    );
    assert_eq!(
        serde_json::from_value::<Permissions>(json!(8)).unwrap(),
        Permissions::from_bits(8)
    );
    let error = serde_json::from_value::<Permissions>(json!("oops")).unwrap_err();
    assert!(error.to_string().contains("invalid bitfield value"));
    assert!(serde_json::from_value::<Permissions>(json!(-8)).is_err());

    // The deprecated 2.x name is the same type.
    #[expect(deprecated, reason = "checks the deprecated alias")]
    let alias: super::PermissionsBitField = Permissions::from_bits(4);
    assert_eq!(alias.bits(), 4);
}

#[test]
fn channel_and_create_message_keep_wire_aliases_and_omit_absent_optionals() {
    let channel = Channel {
        id: test_id(10),
        kind: ChannelType::GUILD_ANNOUNCEMENT,
        name: Some("announcements".to_string()),
        ..Channel::fixture()
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
    assert_eq!(tag.id.get(), 11);
    assert_eq!(tag.name, "Support");
    assert!(tag.moderated);
    assert_eq!(tag.emoji_name.as_deref(), Some("ticket"));
    assert_eq!(channel.applied_tags.as_ref().unwrap()[0].get(), 11);
    assert_eq!(
        channel
            .default_reaction_emoji
            .as_ref()
            .and_then(|reaction| reaction.emoji_name.as_deref()),
        Some("ok")
    );
    assert_eq!(channel.default_thread_rate_limit_per_user, Some(30));

    let serialized = serde_json::to_value(Channel {
        id: test_id(20),
        kind: ChannelType::GUILD_FORUM,
        available_tags: Some(vec![ForumTag {
            id: test_id(21),
            name: "News".to_string(),
            ..ForumTag::fixture()
        }]),
        default_reaction_emoji: Some(DefaultReaction {
            emoji_id: Some(test_id(22)),
            ..DefaultReaction::default()
        }),
        ..Channel::fixture()
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
            "me_burst": true,
            "emoji": { "name": "spark" },
            "burst_colors": ["#ff8800", "#0088ff"]
        }]
    }))
    .unwrap();

    let poll = message.poll.expect("poll should decode");
    assert_eq!(poll.question.text.as_deref(), Some("Ship it?"));
    assert_eq!(poll.answers[0].answer_id, Some(PollAnswerId::new(1)));
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
    assert_eq!(poll.layout_type, PollLayoutType::DEFAULT);
    let results = poll.results.expect("poll results should decode");
    assert!(!results.is_finalized);
    assert_eq!(results.answer_counts[0].count, 3);
    assert!(results.answer_counts[0].me_voted);
    let reaction = &message.reactions[0];
    assert_eq!(reaction.count_details.as_ref().unwrap().burst, 2);
    assert_eq!(reaction.count_details.as_ref().unwrap().normal, 3);
    assert!(reaction.me_burst);
    assert_eq!(reaction.burst_colors, ["#ff8800", "#0088ff"]);

    let create_message = CreateMessage {
        allowed_mentions: Some(AllowedMentions {
            users: vec![test_id(42)],
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
            layout_type: Some(PollLayoutType::DEFAULT),
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
            id: PollAnswerId::new(1),
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
            .map(|decoration| decoration.sku_id.get()),
        Some(555)
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
            id: test_id(300),
            position: Some(Some(1)),
        },
        ModifyGuildRolePosition {
            id: test_id(301),
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
        roles: Some(Some(vec![test_id(300)])),
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
        permissions: Some(Some(Permissions::from_bits(8))),
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
        kind: Some(ChannelType::GUILD_TEXT),
        topic: Some(Some("read first".to_string())),
        permission_overwrites: Some(Some(vec![PermissionOverwrite {
            target: super::OverwriteTarget::Role(test_id(300)),
            allow: Some(Permissions::from_bits(1024)),
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
            channel_id: test_id(202),
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
        prompts: Some(vec![super::OnboardingPrompt {
            id: test_id(1),
            kind: super::OnboardingPromptType::DROPDOWN,
            options: vec![super::OnboardingPromptOption {
                id: test_id(2),
                channel_ids: vec![test_id(202)],
                role_ids: Vec::new(),
                emoji: None,
                emoji_id: None,
                emoji_name: Some("📚".to_string()),
                emoji_animated: None,
                title: "Reading".to_string(),
                description: None,
            }],
            title: "Pick a topic".to_string(),
            single_select: true,
            required: false,
            in_onboarding: true,
        }]),
        default_channel_ids: Some(vec![test_id(202)]),
        enabled: Some(true),
        mode: Some(OnboardingMode::ONBOARDING_DEFAULT),
    };
    assert_eq!(
        serde_json::to_value(&onboarding).unwrap(),
        json!({
            "prompts": [{
                "id": "1",
                "type": 1,
                "options": [{
                    "id": "2",
                    "channel_ids": ["202"],
                    "role_ids": [],
                    "emoji_name": "📚",
                    "title": "Reading",
                    "description": null
                }],
                "title": "Pick a topic",
                "single_select": true,
                "required": false,
                "in_onboarding": true
            }],
            "default_channel_ids": ["202"],
            "enabled": true,
            "mode": 0
        })
    );

    let prune = BeginGuildPruneRequest {
        days: Some(7),
        compute_prune_count: Some(false),
        include_roles: Some(vec![test_id(300)]),
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
    assert_eq!(sku.kind, SkuType::SUBSCRIPTION);
    assert_eq!(sku.access_type, Some(SkuAccessType::new(1)));
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
    assert_eq!(entitlement.kind, EntitlementType::APPLICATION_SUBSCRIPTION);
    assert_eq!(entitlement.gift_code_flags, Some(0));
    assert_eq!(
        entitlement.subscription_id.map(u64::from),
        Some(1019653835926409216)
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
    assert_eq!(subscription.sku_ids[0].get(), 1158857122189168803);
    assert_eq!(subscription.status, SubscriptionStatus::ACTIVE);

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
    assert_eq!(threads.threads[0].id.get(), 50);
    assert_eq!(threads.members[0].user_id.as_ref().unwrap().get(), 42);

    let thread_member = ThreadMember {
        user_id: Some(test_id(42)),
        member: Some(Member {
            user: Some(std::sync::Arc::new(User {
                id: test_id(42),
                username: "member".to_string(),
                ..User::fixture()
            })),
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
        "interaction": { "id": "9100", "type": 2, "name": "old" },
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

    assert_eq!(message.mention_roles[0].get(), 3000);
    assert_eq!(message.application_id.unwrap().get(), 4000);
    assert_eq!(
        message
            .application
            .as_ref()
            .and_then(|application| application.name.as_deref()),
        Some("Activity App")
    );
    assert_eq!(
        message
            .interaction_metadata
            .as_ref()
            .map(|metadata| metadata.kind),
        Some(InteractionType::APPLICATION_COMMAND)
    );
    assert_eq!(
        message
            .interaction
            .as_ref()
            .map(|interaction| interaction.name.as_str()),
        Some("old")
    );
    assert!(message
        .resolved
        .as_ref()
        .is_some_and(ResolvedData::is_empty));
    assert_eq!(
        message.message_snapshots[0]
            .message
            .components
            .as_ref()
            .and_then(|components| components[0].content.as_deref()),
        Some("snapshot text")
    );
    assert_eq!(message.activity.unwrap().party_id.as_deref(), Some("party"));
    assert!(message
        .role_subscription_data
        .as_ref()
        .is_some_and(|data| data.is_renewal));
    assert_eq!(
        message.message_reference.unwrap().kind,
        Some(MessageReferenceType::FORWARD)
    );
    assert_eq!(message.message_snapshots[0].message.content, "snapshot");
    assert_eq!(
        message.message_snapshots[0].message.sticker_items[0]
            .id
            .get(),
        9000
    );
    assert_eq!(
        message
            .referenced_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("original")
    );
    assert_eq!(message.call.unwrap().participants[0].get(), 42);
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

    assert_eq!(integration.id.get(), 100);
    assert_eq!(integration.kind, "twitch");
    assert_eq!(integration.account.name, "stream");
    assert_eq!(integration.application.as_ref().unwrap().id.get(), 200);
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
    let unfocused = CommandInteractionOption::fixture();
    let focused = CommandInteractionOption {
        focused: Some(true),
        ..CommandInteractionOption::fixture()
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
        id: test_id(99),
        application_id: test_id(77),
        token: "token-123".to_string(),
        ..InteractionContextData::fixture()
    };
    let interaction = Interaction::Component(ComponentInteraction {
        context: context.clone(),
        data: ComponentInteractionData {
            custom_id: "button".to_string(),
            component_type: ComponentType::BUTTON,
            id: None,
            values: vec!["x".to_string()],
            resolved: None,
        },
    });
    let user = User {
        id: test_id(1759288472266248192),
        username: "discordrs".to_string(),
        ..User::fixture()
    };

    assert_eq!(interaction.context().id.get(), 99);
    assert_eq!(interaction.id().get(), 99);
    assert_eq!(interaction.application_id().get(), 77);
    assert_eq!(interaction.token(), "token-123");

    assert_eq!(DiscordModel::id(&user).get(), 1759288472266248192);
    assert_eq!(
        DiscordModel::id_opt(&user).map(u64::from),
        Some(1759288472266248192)
    );
    assert!(DiscordModel::created_at(&user).is_some());
}

#[test]
fn interaction_accessors_cover_all_variants() {
    fn context(id: u64, application_id: u64, token: &str) -> InteractionContextData {
        InteractionContextData {
            id: test_id(id),
            application_id: test_id(application_id),
            token: token.to_string(),
            ..InteractionContextData::fixture()
        }
    }

    let interactions = [
        Interaction::Ping(PingInteraction {
            context: context(1, 10, "ping-token"),
        }),
        Interaction::ChatInputCommand(ChatInputCommandInteraction {
            context: context(2, 20, "chat-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::UserContextMenu(UserContextMenuInteraction {
            context: context(3, 30, "user-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::MessageContextMenu(MessageContextMenuInteraction {
            context: context(4, 40, "message-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::Autocomplete(AutocompleteInteraction {
            context: context(5, 50, "autocomplete-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::Component(ComponentInteraction {
            context: context(6, 60, "component-token"),
            data: ComponentInteractionData::fixture(),
        }),
        Interaction::ModalSubmit(ModalSubmitInteraction {
            context: context(7, 70, "modal-token"),
            submission: V2ModalSubmission {
                custom_id: "modal".to_string(),
                components: vec![],
            },
            resolved: None,
        }),
        Interaction::PrimaryEntryPoint(crate::model::PrimaryEntryPointInteraction {
            context: context(9, 90, "entry-point-token"),
            data: CommandInteractionData::default(),
        }),
        Interaction::Unknown {
            context: context(8, 80, "unknown-token"),
            kind: InteractionType::new(99),
            raw_data: serde_json::value::RawValue::from_string(r#"{"kind":"unknown"}"#.to_string())
                .ok(),
        },
    ];

    let expected = [
        (1, 10, "ping-token"),
        (2, 20, "chat-token"),
        (3, 30, "user-token"),
        (4, 40, "message-token"),
        (5, 50, "autocomplete-token"),
        (6, 60, "component-token"),
        (7, 70, "modal-token"),
        (9, 90, "entry-point-token"),
        (8, 80, "unknown-token"),
    ];

    for (interaction, (id, application_id, token)) in interactions.iter().zip(expected) {
        assert_eq!(interaction.context().id.get(), id);
        assert_eq!(interaction.id().get(), id);
        assert_eq!(interaction.application_id().get(), application_id);
        assert_eq!(interaction.token(), token);
    }
}

#[test]
fn discord_model_trait_returns_ids_for_core_models() {
    let guild = Guild {
        id: test_id(11),
        name: "Guild".to_string(),
        ..Guild::fixture()
    };
    let channel = Channel {
        id: test_id(12),
        kind: ChannelType::GUILD_TEXT,
        ..Channel::fixture()
    };
    let message = Message {
        id: test_id(13),
        channel_id: test_id(99),
        ..Message::fixture()
    };
    let role = Role {
        id: test_id(14),
        name: "Admin".to_string(),
        ..Role::fixture()
    };
    let attachment = Attachment {
        id: test_id(15),
        filename: "file.txt".to_string(),
        ..Attachment::fixture()
    };

    assert_eq!(DiscordModel::id(&guild).get(), 11);
    assert_eq!(DiscordModel::id(&channel).get(), 12);
    assert_eq!(DiscordModel::id(&message).get(), 13);
    assert_eq!(DiscordModel::id(&role).get(), 14);
    assert_eq!(DiscordModel::id(&attachment).get(), 15);
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
        kind: InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        data: None,
    };
    let dm_channel = CreateDmChannel {
        recipient_id: test_id(321),
    };
    let sticker = StickerItem {
        id: test_id(654),
        name: "party".to_string(),
        kind: Some(StickerFormatType::PNG),
    };
    let invite = Invite {
        kind: Some(InviteType::GUILD),
        code: Some("abc".to_string()),
        approximate_member_count: Some(42),
        roles: vec![Role {
            id: test_id(700),
            name: "guest".to_string(),
            ..Role::fixture()
        }],
        ..Invite::default()
    };
    let invite_body = CreateChannelInvite {
        role_ids: Some(vec![test_id(700)]),
        ..CreateChannelInvite::default()
    };
    let invite_job = InviteTargetUsersJobStatus {
        status: InviteTargetUsersJobStatusType::PROCESSING,
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
        channel_id: test_id(400),
        topic: "town hall".to_string(),
        privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
        send_start_notification: Some(true),
        guild_scheduled_event_id: Some(test_id(500)),
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
        privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
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
        channel_id: Some(test_id(200)),
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
        channel_id: Some(test_id(200)),
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
            permissions: Permissions::from_bits(2048),
        }),
        integration_types_config: Some(HashMap::from([(
            "0".to_string(),
            ApplicationIntegrationTypeConfig {
                oauth2_install_params: Some(ApplicationInstallParams {
                    scopes: vec!["applications.commands".to_string()],
                    permissions: Permissions::from_bits(0),
                }),
            },
        )])),
        flags: Some(1 << 14),
        icon: Some(None),
        cover_image: Some(Some("data:image/png;base64,abc".to_string())),
        interactions_endpoint_url: Some(Some("https://example.com/interactions".to_string())),
        tags: Some(vec!["utility".to_string()]),
        event_webhooks_url: Some(None),
        event_webhooks_status: Some(ApplicationEventWebhookStatus::ENABLED),
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
        channel_id: Some(test_id(500)),
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
        thread_id: Some(test_id(600)),
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
        thread_id: Some(test_id(601)),
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
        Some(RecurrenceRuleWeekday::TUESDAY)
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
        user_id: Some(test_id(777)),
        ..Presence::default()
    };
    let numeric_permissions: Permissions = serde_json::from_value(json!(16)).unwrap();

    let embed_json = serde_json::to_value(&embed).unwrap();
    let presence_json = serde_json::to_value(&presence).unwrap();

    assert_eq!(embed_json["title"], json!("Docs"));
    assert_eq!(embed_json["fields"], json!([]));
    assert!(embed_json.get("description").is_none());
    assert_eq!(presence_json, json!({ "user_id": "777" }));
    assert_eq!(numeric_permissions.bits(), 16);
}

#[test]
fn message_components_v2_tree_decodes_typed() {
    let message: Message = serde_json::from_value(json!({
        "id": "1",
        "channel_id": "2",
        "flags": 32768,
        "components": [{
            "type": 17,
            "id": 1,
            "accent_color": 5793266,
            "spoiler": false,
            "components": [
                {
                    "type": 9,
                    "components": [
                        { "type": 10, "content": "# Release 2.2.0" },
                        { "type": 10, "content": "Typed message components" }
                    ],
                    "accessory": {
                        "type": 11,
                        "media": {
                            "url": "https://cdn.example/thumb.png",
                            "proxy_url": "https://proxy.example/thumb.png",
                            "width": 128,
                            "height": 128,
                            "content_type": "image/png"
                        },
                        "description": "cover art"
                    }
                },
                { "type": 14, "divider": true, "spacing": 1 },
                {
                    "type": 12,
                    "items": [{
                        "media": { "url": "https://cdn.example/shot.png" },
                        "description": "screenshot",
                        "spoiler": true
                    }]
                },
                {
                    "type": 13,
                    "file": { "url": "attachment://notes.txt" },
                    "name": "notes.txt",
                    "size": 512
                },
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 2,
                            "style": 1,
                            "label": "Deploy",
                            "custom_id": "deploy",
                            "emoji": { "name": "🚀" }
                        },
                        {
                            "type": 2,
                            "style": 5,
                            "label": "Docs",
                            "url": "https://docs.example",
                            "disabled": false
                        }
                    ]
                },
                {
                    "type": 1,
                    "components": [{
                        "type": 3,
                        "custom_id": "env",
                        "placeholder": "Pick an environment",
                        "min_values": 1,
                        "max_values": 1,
                        "options": [
                            {
                                "label": "Production",
                                "value": "prod",
                                "description": "live traffic",
                                "default": true
                            },
                            { "label": "Staging", "value": "stage" }
                        ]
                    }]
                },
                {
                    "type": 1,
                    "components": [{
                        "type": 8,
                        "custom_id": "log-channel",
                        "channel_types": [0, 5],
                        "default_values": [{ "id": "42", "type": "channel" }]
                    }]
                }
            ]
        }]
    }))
    .unwrap();

    let components = message.components.as_deref().unwrap();
    let container = &components[0];
    assert_eq!(container.kind, ComponentType::CONTAINER);
    assert_eq!(container.id, Some(ComponentId::new(1)));
    assert_eq!(container.accent_color, Some(5_793_266));
    assert_eq!(container.spoiler, Some(false));

    let section = &container.components[0];
    assert_eq!(section.kind, ComponentType::SECTION);
    assert_eq!(
        section.components[0].content.as_deref(),
        Some("# Release 2.2.0")
    );
    let accessory = section.accessory.as_deref().unwrap();
    assert_eq!(accessory.kind, ComponentType::THUMBNAIL);
    let media = accessory.media.as_ref().unwrap();
    assert_eq!(media.url, "https://cdn.example/thumb.png");
    assert_eq!(
        media.proxy_url.as_deref(),
        Some("https://proxy.example/thumb.png")
    );
    assert_eq!(media.width, Some(128));
    assert_eq!(accessory.description.as_deref(), Some("cover art"));

    let separator = &container.components[1];
    assert_eq!(separator.kind, ComponentType::SEPARATOR);
    assert_eq!(separator.divider, Some(true));
    assert_eq!(separator.spacing, Some(SeparatorSpacing::SMALL));

    let gallery = &container.components[2];
    assert_eq!(gallery.kind, ComponentType::MEDIA_GALLERY);
    assert_eq!(gallery.items[0].media.url, "https://cdn.example/shot.png");
    assert_eq!(gallery.items[0].spoiler, Some(true));

    let file = &container.components[3];
    assert_eq!(file.kind, ComponentType::FILE);
    assert_eq!(file.file.as_ref().unwrap().url, "attachment://notes.txt");
    assert_eq!(file.name.as_deref(), Some("notes.txt"));
    assert_eq!(file.size, Some(512));

    let button_row = &container.components[4];
    assert_eq!(button_row.kind, ComponentType::ACTION_ROW);
    assert_eq!(
        button_row.components[0].custom_id.as_deref(),
        Some("deploy")
    );
    assert_eq!(button_row.components[0].style, Some(1));
    assert_eq!(
        button_row.components[0]
            .emoji
            .as_ref()
            .and_then(|emoji| emoji.name.as_deref()),
        Some("🚀")
    );
    assert_eq!(
        button_row.components[1].url.as_deref(),
        Some("https://docs.example")
    );

    let select = &container.components[5].components[0];
    assert_eq!(select.kind, ComponentType::STRING_SELECT);
    assert_eq!(select.placeholder.as_deref(), Some("Pick an environment"));
    assert_eq!(select.min_values, Some(1));
    assert_eq!(select.options.len(), 2);
    assert_eq!(select.options[0].value, "prod");
    assert_eq!(select.options[0].default, Some(true));
    assert_eq!(select.options[1].label, "Staging");

    let channel_select = &container.components[6].components[0];
    assert_eq!(channel_select.kind, ComponentType::CHANNEL_SELECT);
    assert_eq!(
        channel_select.channel_types,
        vec![ChannelType::GUILD_TEXT, ChannelType::GUILD_ANNOUNCEMENT]
    );
    assert_eq!(
        channel_select.default_values[0],
        super::SelectDefaultValue::Channel(test_id(42))
    );
    assert_eq!(channel_select.default_values[0].kind(), "channel");

    // Depth-first iteration visits the whole tree, including the accessory.
    let all: Vec<_> = container.iter().collect();
    assert_eq!(all.len(), 15);
    assert_eq!(
        all.iter()
            .filter(|component| component.kind == ComponentType::BUTTON)
            .count(),
        2
    );
    assert!(all
        .iter()
        .any(|component| component.kind == ComponentType::THUMBNAIL));
}

#[test]
fn message_component_tolerates_unknown_kinds_and_round_trips() {
    // Unknown future component types and fields must never fail decoding.
    let tolerant: MessageComponent = serde_json::from_value(json!({
        "type": 99,
        "brand_new_field": { "nested": true }
    }))
    .unwrap();
    assert_eq!(tolerant.kind, ComponentType::new(99));
    assert!(tolerant.custom_id.is_none());
    assert!(tolerant.components.is_empty());

    // A missing `type` is a decode error since 3.0: 2.x read it as `0`,
    // which is not a component type, and hid malformed payloads.
    assert!(serde_json::from_value::<MessageComponent>(json!({})).is_err());
    // Values past the u8 storage fail instead of wrapping.
    assert!(serde_json::from_value::<MessageComponent>(json!({ "type": 256 })).is_err());

    // Known fields round-trip byte-for-byte through serialization.
    let fixture = json!({
        "type": 1,
        "components": [{
            "type": 2,
            "custom_id": "confirm",
            "style": 3,
            "label": "Confirm",
            "disabled": false
        }, {
            "type": 4,
            "custom_id": "reason",
            "style": 2,
            "min_length": 10,
            "max_length": 200,
            "required": true,
            "value": "because",
            "placeholder": "Why?"
        }]
    });
    let component: MessageComponent = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(&component).unwrap(), fixture);

    // The shared raw `style` reads as the type that matches `kind` only.
    let (button, input) = (&component.components[0], &component.components[1]);
    assert_eq!(button.button_style(), Some(super::ButtonStyle::SUCCESS));
    assert_eq!(button.text_input_style(), None);
    assert_eq!(
        input.text_input_style(),
        Some(super::TextInputStyle::PARAGRAPH)
    );
    assert_eq!(input.button_style(), None);
    assert_eq!(component.button_style(), None);

    // A label wrapping a text input exposes the child via `component`.
    let label: MessageComponent = serde_json::from_value(json!({
        "type": 18,
        "label": "Feedback",
        "description": "Tell us more",
        "component": { "type": 4, "custom_id": "feedback", "style": 2 }
    }))
    .unwrap();
    assert_eq!(label.kind, ComponentType::LABEL);
    assert_eq!(
        label.component.as_deref().map(|child| child.kind),
        Some(ComponentType::TEXT_INPUT)
    );
    assert_eq!(label.iter().count(), 2);
}

#[test]
fn resolved_data_decodes_all_six_maps_with_lookups() {
    let resolved: ResolvedData = serde_json::from_value(json!({
        "users": {
            "42": { "id": "42", "username": "someone", "global_name": "Someone" }
        },
        "members": {
            "42": { "nick": "friend", "roles": ["77"], "joined_at": "2026-01-01T00:00:00Z" }
        },
        "roles": {
            "77": { "id": "77", "name": "Admins", "permissions": "8" }
        },
        "channels": {
            "88": { "id": "88", "type": 11, "name": "release-thread", "parent_id": "89" }
        },
        "messages": {
            "99": { "id": "99", "channel_id": "88", "content": "hello" }
        },
        "attachments": {
            "100": { "id": "100", "filename": "log.txt", "size": 12 }
        }
    }))
    .unwrap();

    assert!(!resolved.is_empty());
    assert_eq!(resolved.user(test_id(42)).unwrap().username, "someone");
    assert_eq!(
        resolved.member(test_id(42)).unwrap().nick.as_deref(),
        Some("friend")
    );
    assert_eq!(resolved.role(test_id(77)).unwrap().name, "Admins");
    let channel = resolved.channel(test_id(88)).unwrap();
    assert_eq!(channel.kind, ChannelType::PUBLIC_THREAD);
    assert_eq!(channel.name.as_deref(), Some("release-thread"));
    assert_eq!(resolved.message(test_id(99)).unwrap().content, "hello");
    assert_eq!(
        resolved.attachment(test_id(100)).unwrap().filename,
        "log.txt"
    );
    assert!(resolved.user(test_id(1)).is_none());

    // Every map defaults to empty when Discord omits it.
    let empty: ResolvedData = serde_json::from_value(json!({})).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn message_interaction_metadata_decodes_with_nested_trigger() {
    let message: Message = serde_json::from_value(json!({
        "id": "1",
        "channel_id": "2",
        "interaction_metadata": {
            "id": "900",
            "type": 5,
            "user": { "id": "42", "username": "someone" },
            "authorizing_integration_owners": { "0": "500", "1": "42" },
            "original_response_message_id": "901",
            "target_user": { "id": "43", "username": "target" },
            "target_message_id": "902",
            "interacted_message_id": "903",
            "triggering_interaction_metadata": {
                "id": "890",
                "type": 3,
                "user": { "id": "42", "username": "someone" },
                "authorizing_integration_owners": {},
                "interacted_message_id": "880"
            }
        },
        "interaction": {
            "id": "900",
            "type": 2,
            "name": "deploy",
            "user": { "id": "42", "username": "someone" }
        }
    }))
    .unwrap();

    let metadata = message.interaction_metadata.unwrap();
    assert_eq!(metadata.id.get(), 900);
    assert_eq!(metadata.kind, InteractionType::MODAL_SUBMIT);
    assert_eq!(metadata.user.as_ref().unwrap().username, "someone");
    assert_eq!(
        metadata
            .authorizing_integration_owners
            .guild_install
            .and_then(GuildInstallOwner::guild_id)
            .map(GuildId::get),
        Some(500)
    );
    assert_eq!(
        metadata
            .authorizing_integration_owners
            .user_install
            .map(UserId::get),
        Some(42)
    );
    assert_eq!(
        metadata.original_response_message_id.map(u64::from),
        Some(901)
    );
    assert_eq!(metadata.target_user.as_ref().unwrap().id.get(), 43);
    assert_eq!(metadata.target_message_id.map(u64::from), Some(902));
    assert_eq!(metadata.interacted_message_id.map(u64::from), Some(903));

    let trigger = metadata.triggering_interaction_metadata.unwrap();
    assert_eq!(trigger.id.get(), 890);
    assert_eq!(trigger.kind, InteractionType::MESSAGE_COMPONENT);
    assert!(trigger.authorizing_integration_owners.is_empty());
    assert_eq!(trigger.interacted_message_id.map(u64::from), Some(880));
    assert!(trigger.triggering_interaction_metadata.is_none());

    let legacy = message.interaction.unwrap();
    assert_eq!(legacy.id.get(), 900);
    assert_eq!(legacy.kind, InteractionType::APPLICATION_COMMAND);
    assert_eq!(legacy.name, "deploy");
    assert_eq!(legacy.user.as_ref().unwrap().id.get(), 42);
}

#[test]
fn command_interaction_resolved_data_is_typed() {
    let data: CommandInteractionData = serde_json::from_value(json!({
        "id": "3",
        "name": "inspect",
        "type": 2,
        "target_id": "42",
        "resolved": {
            "users": { "42": { "id": "42", "username": "target-user" } },
            "members": { "42": { "roles": [] } }
        }
    }))
    .unwrap();

    let resolved = data.resolved.unwrap();
    let target_id = data.target_id.unwrap().cast();
    assert_eq!(
        resolved.user(target_id).map(|user| user.username.as_str()),
        Some("target-user")
    );
    assert!(resolved.member(target_id).is_some());
    assert!(resolved.role(test_id(1)).is_none());
}

#[test]
fn component_embed_link_previews_deserialize_with_their_container() {
    let embed: Embed = serde_json::from_value(json!({
        "type": "components",
        "url": "https://example.com/post",
        "flags": 0,
        "components": [{
            "type": 17,
            "accent_color": 5793266,
            "components": [{ "type": 10, "content": "# Patch Notes" }]
        }]
    }))
    .expect("component embed");
    assert!(embed.is_component_embed());
    assert_eq!(embed.components.len(), 1);
    assert_eq!(embed.components[0].kind, ComponentType::CONTAINER);
    assert_eq!(embed.flags, Some(0));

    let rich: Embed =
        serde_json::from_value(json!({ "type": "rich", "title": "t" })).expect("rich");
    assert!(!rich.is_component_embed());
    let outgoing = serde_json::to_value(Embed {
        title: Some("t".to_string()),
        ..Embed::default()
    })
    .expect("serialize");
    assert!(outgoing.get("components").is_none() && outgoing.get("flags").is_none());
}

#[test]
fn dynamic_profile_fields_round_trip_known_and_unknown_types() {
    use super::DynamicProfileField;

    let raw = json!([
        { "type": 1, "name": "Main", "value": "Mage" },
        { "type": 2, "name": "KDA", "value": 3.5 },
        { "type": 3, "name": "Badge", "value": { "url": "https://cdn.example/b.png" } },
        { "type": 42, "name": "Future", "value": [1, 2] }
    ]);
    let fields: Vec<DynamicProfileField> = serde_json::from_value(raw.clone()).expect("fields");
    assert!(matches!(&fields[0], DynamicProfileField::String { value, .. } if value == "Mage"));
    assert!(
        matches!(&fields[1], DynamicProfileField::Number { value, .. } if (*value - 3.5).abs() < f64::EPSILON)
    );
    assert!(
        matches!(&fields[2], DynamicProfileField::Media { value, .. } if value.url.ends_with("b.png"))
    );
    assert!(matches!(&fields[3], DynamicProfileField::Unknown(_)));
    assert_eq!(fields[3].name().as_deref(), Some("Future"));
    assert_eq!(
        serde_json::to_value(&fields).expect("serialize"),
        raw,
        "unknown types round-trip"
    );

    assert!(serde_json::from_value::<DynamicProfileField>(
        json!({ "type": 2, "name": "x", "value": "nan" })
    )
    .is_err());
}

#[test]
fn channel_obfuscation_is_detected_from_the_flag_not_the_name() {
    let hidden: Channel = serde_json::from_value(json!({
        "id": "1", "type": 0, "name": "___hidden___", "flags": 1 << 17
    }))
    .expect("channel");
    assert!(hidden.is_obfuscated());
    assert!(hidden
        .channel_flags()
        .contains(crate::bitfield::channel_flags::CHANNEL_OBFUSCATED.bits()));

    let named_like_hidden: Channel = serde_json::from_value(json!({
        "id": "2", "type": 0, "name": "___hidden___", "flags": 1 << 21
    }))
    .expect("channel");
    assert!(!named_like_hidden.is_obfuscated());
    assert!(named_like_hidden
        .channel_flags()
        .contains(crate::bitfield::channel_flags::IS_SPOILER_CHANNEL.bits()));
}

#[test]
fn authorizing_integration_owners_keeps_bot_dm_zero_and_unknown_keys() {
    let owners: AuthorizingIntegrationOwners =
        serde_json::from_value(json!({ "0": "0", "1": "42", "7": ["future", 1] })).unwrap();
    assert_eq!(owners.guild_install, Some(GuildInstallOwner::BotDm));
    assert_eq!(owners.user_install.map(UserId::get), Some(42));
    assert_eq!(
        owners.other.get("7").map(|raw| raw.get()),
        Some(r#"["future",1]"#)
    );
    assert!(!owners.is_empty());
    assert_eq!(
        serde_json::to_value(&owners).unwrap(),
        json!({ "0": "0", "1": "42", "7": ["future", 1] })
    );

    let guild: AuthorizingIntegrationOwners =
        serde_json::from_value(json!({ "0": 81384788765712384_u64 })).unwrap();
    assert_eq!(
        guild
            .guild_install
            .and_then(GuildInstallOwner::guild_id)
            .map(GuildId::get),
        Some(81384788765712384)
    );
    assert!(guild.user_install.is_none());
    assert!(AuthorizingIntegrationOwners::default().is_empty());
    assert!(serde_json::from_value::<AuthorizingIntegrationOwners>(json!({ "1": "0" })).is_err());
}

#[test]
fn guild_widget_member_ids_are_opaque_and_may_be_zero() {
    let widget: GuildWidget = serde_json::from_value(json!({
        "id": "1",
        "name": "widget",
        "channels": [],
        "members": [{ "id": "0", "username": "anon" }, { "id": "abc", "username": "x" }]
    }))
    .unwrap();
    assert_eq!(widget.members[0].id.as_str(), "0");
    assert_eq!(widget.members[1].id.as_str(), "abc");
}

#[test]
fn create_guild_placeholders_allow_zero_and_serialize_as_integers() {
    use super::{
        CreateGuild, CreateGuildPlaceholderChannel, CreateGuildPlaceholderId,
        CreateGuildPlaceholderOverwrite, CreateGuildPlaceholderRole,
    };

    let everyone = CreateGuildPlaceholderId::new(0);
    let category = CreateGuildPlaceholderId::new(1);
    let body = CreateGuild {
        name: "guild".to_string(),
        roles: Some(vec![CreateGuildPlaceholderRole {
            id: Some(everyone),
            ..CreateGuildPlaceholderRole::default()
        }]),
        channels: Some(vec![
            CreateGuildPlaceholderChannel {
                id: Some(category),
                name: "general".to_string(),
                kind: Some(ChannelType::GUILD_CATEGORY),
                ..CreateGuildPlaceholderChannel::default()
            },
            CreateGuildPlaceholderChannel {
                id: Some(CreateGuildPlaceholderId::new(2)),
                name: "chat".to_string(),
                parent_id: Some(category),
                permission_overwrites: Some(vec![CreateGuildPlaceholderOverwrite {
                    id: everyone,
                    kind: OverwriteType::ROLE,
                    allow: None,
                    deny: None,
                }]),
                ..CreateGuildPlaceholderChannel::default()
            },
        ]),
        system_channel_id: Some(CreateGuildPlaceholderId::new(2)),
        ..CreateGuild::default()
    };
    let value = serde_json::to_value(&body).unwrap();
    assert_eq!(value["roles"][0]["id"], json!(0));
    assert_eq!(value["channels"][1]["parent_id"], json!(1));
    assert_eq!(
        value["channels"][1]["permission_overwrites"][0]["id"],
        json!(0)
    );
    assert_eq!(value["system_channel_id"], json!(2));
    assert_eq!(everyone.get(), 0);
}

#[test]
fn required_id_request_bodies_have_constructors_instead_of_default() {
    use super::{
        CreateDmChannel, CreateStageInstance, CreateTestEntitlement, LobbyMember,
        LobbyMemberUpdate, ModifyGuildChannelPosition, ModifyGuildRolePosition,
    };

    assert_eq!(
        serde_json::to_value(CreateDmChannel::new(test_id(42))).unwrap(),
        json!({ "recipient_id": "42" })
    );
    assert_eq!(
        serde_json::to_value(ModifyGuildChannelPosition::new(test_id(42))).unwrap(),
        json!({ "id": "42" })
    );
    assert_eq!(
        serde_json::to_value(ModifyGuildRolePosition::new(test_id(42))).unwrap(),
        json!({ "id": "42" })
    );
    assert_eq!(
        serde_json::to_value(CreateTestEntitlement::new(
            test_id(42),
            super::EntitlementOwner::User(test_id(7))
        ))
        .unwrap(),
        json!({ "sku_id": "42", "owner_id": "7", "owner_type": 2 })
    );
    assert_eq!(
        serde_json::to_value(CreateStageInstance::new(test_id(42), "town hall")).unwrap(),
        json!({ "channel_id": "42", "topic": "town hall" })
    );
    assert_eq!(
        serde_json::to_value(LobbyMember::new(test_id(42))).unwrap(),
        json!({ "id": "42" })
    );
    assert_eq!(
        serde_json::to_value(LobbyMemberUpdate::new(test_id(42))).unwrap(),
        json!({ "id": "42" })
    );
}

#[test]
fn component_values_read_strings_and_booleans_directly_and_keep_other_json_as_text() {
    use super::MessageComponentValue;

    let read = |json: &str| serde_json::from_str::<MessageComponentValue>(json).unwrap();
    assert_eq!(read(r#""text""#), MessageComponentValue::from("text"));
    assert_eq!(read("false"), MessageComponentValue::Bool(false));
    for json in ["3", "-3", "1.5", r#"[1,"a"]"#, r#"{"level":3}"#, "null"] {
        let value = read(json);
        match &value {
            MessageComponentValue::Other(raw) => assert_eq!(raw.get(), json),
            other => panic!("{json}: read as {other:?}"),
        }
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(value.as_str(), None);
        assert_eq!(value.as_bool(), None);
    }
    assert_ne!(read("3"), read("4"));
    assert_eq!(read("[1]"), read("[1]"));

    // Through a Value (the 2.x decode path) the same holds.
    let component: MessageComponent =
        serde_json::from_value(json!({ "type": 99, "value": { "level": 3 } })).unwrap();
    assert_eq!(
        component.value.as_ref().map(|value| match value {
            MessageComponentValue::Other(raw) => raw.get().to_string(),
            other => format!("{other:?}"),
        }),
        Some(r#"{"level":3}"#.to_string())
    );
    // `null` and an absent key both read as no value (see the decision log
    // on `MessageComponent::value`).
    let empty_radio: MessageComponent =
        serde_json::from_value(json!({ "type": 21, "custom_id": "r", "value": null })).unwrap();
    assert_eq!(empty_radio.value, None);
}

#[test]
fn display_name_styles_decode_on_users_and_members_and_tolerate_partial_objects() {
    use super::{DisplayNameEffect, DisplayNameFont, Member, User};

    let user: User = serde_json::from_value(json!({
        "id": "1",
        "username": "styled",
        "display_name_styles": { "font_id": 3, "effect_id": 4, "colors": [16777215] }
    }))
    .unwrap();
    let styles = user.display_name_styles.expect("styles");
    assert_eq!(styles.font_id, Some(DisplayNameFont::SAKURA));
    assert_eq!(styles.effect_id, Some(DisplayNameEffect::TOON));
    assert_eq!(styles.colors, [0x00FF_FFFF]);

    // A value this version has no name for stays open, and a missing field
    // does not fail the user that carries the object.
    let member: Member = serde_json::from_value(json!({
        "roles": [],
        "display_name_styles": { "font_id": 99, "colors": [] }
    }))
    .unwrap();
    let styles = member.display_name_styles.expect("styles");
    assert_eq!(styles.font_id, Some(DisplayNameFont::new(99)));
    assert_eq!(styles.effect_id, None);
    let user: User = serde_json::from_value(
        json!({ "id": "2", "username": "plain", "display_name_styles": null }),
    )
    .unwrap();
    assert!(user.display_name_styles.is_none());
}
