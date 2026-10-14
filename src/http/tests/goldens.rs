//! Request goldens for every route of the manifest (3.0 phase 5 B).
//!
//! Each case calls one public endpoint method against a local server that
//! records the request, and renders what went over the wire: method, path
//! with its escaping and query, the `Authorization` header, the content
//! type, and the body bytes (multipart boundaries normalized). The rendering
//! of every case is compared with `src/http/goldens/rest_routes.golden`, and
//! each case is also checked against its manifest row: the method, the path
//! shape of the template, and the authorization kind.
//!
//! Every manifest route needs at least one case, and every case must name a
//! method its row lists, so a route cannot lose coverage when a method is
//! renamed or removed. Regenerate the golden file after an intended change
//! with `DISCORDRS_UPDATE_GOLDENS=1 cargo test --lib http::tests::goldens`
//! and review the diff.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use reqwest::StatusCode;
use serde_json::json;

use super::{sample_command, sample_file, spawn_test_server, PlannedResponse, RecordedRequest};
use crate::bitfield::{message_flags, Permissions};
use crate::error::DiscordError;
use crate::http::routes::{RouteAuth, RouteId, MANIFEST};
use crate::http::{FileAttachment, RestClient};
use crate::model::{self as m, test_id};
use crate::oauth2::{OAuth2Client, OAuth2CodeExchange, OAuth2RefreshToken};
use crate::request as r;

const BOT_TOKEN: &str = "golden-token";
const BEARER: &str = "golden-bearer";
const WEBHOOK_TOKEN: &str = "golden-webhook-token";
const INTERACTION_TOKEN: &str = "golden-interaction-token";
const APPLICATION_ID: u64 = 555;
const GOLDEN_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/http/goldens/rest_routes.golden"
);

type CallFuture = Pin<Box<dyn Future<Output = Result<(), DiscordError>>>>;

/// One golden: the route, the method it calls, and the call itself.
struct Case {
    route: RouteId,
    api: &'static str,
    call: Box<dyn Fn(String) -> CallFuture>,
}

fn rest(base_url: &str) -> RestClient {
    RestClient::new_with_base_url(BOT_TOKEN, APPLICATION_ID, base_url)
}

/// Builds a case. `$c` is a `RestClient` (or, with `oauth2`, an
/// `OAuth2Client`) pointed at the recording server; the body's result is
/// discarded because the server answers `{}`, which most response types do
/// not accept. Only the request matters here.
macro_rules! case {
    ($route:ident, $api:ident, |$c:ident| $body:expr) => {
        Case {
            route: RouteId::$route,
            api: stringify!($api),
            call: Box::new(|base_url: String| {
                Box::pin(async move {
                    let $c = rest(&base_url);
                    $body.await.map(drop)
                })
            }),
        }
    };
    (oauth2 $route:ident, $api:ident, |$c:ident| $body:expr) => {
        Case {
            route: RouteId::$route,
            api: stringify!($api),
            call: Box::new(|base_url: String| {
                Box::pin(async move {
                    let $c = OAuth2Client::new_with_base_url(
                        "golden-client",
                        Some("golden-secret".to_string()),
                        base_url,
                        "http://127.0.0.1:9/oauth2/authorize",
                    );
                    $body.await.map(drop)
                })
            }),
        }
    };
}

fn files() -> Vec<FileAttachment> {
    vec![sample_file("golden.txt", "golden file")]
}

/// Metadata for `files[0]` (the file `files()` sends).
fn upload_0() -> r::MessageAttachment {
    r::MessageAttachment::Upload(
        r::AttachmentUpload::new(m::UploadId::new(0)).filename("golden.txt"),
    )
}

fn metadata() -> HashMap<String, String> {
    HashMap::from([("topic".to_string(), "golden".to_string())])
}

/// A typed message callback (type 4).
fn typed_interaction_response() -> r::InteractionResponse {
    r::InteractionResponse::ChannelMessage(r::InteractionMessage::from("golden"))
}

fn followup() -> r::CreateFollowup {
    r::CreateFollowup::from("golden")
}

fn interaction_edit() -> r::EditInteractionResponse {
    r::EditInteractionResponse::from("golden")
}

fn permission_edit() -> r::EditApplicationCommandPermissions {
    r::EditApplicationCommandPermissions::new([m::ApplicationCommandPermission::role(
        test_id(300),
        true,
    )])
}

#[expect(deprecated, reason = "covers deprecated routes while they exist")]
fn cases() -> Vec<Case> {
    vec![
        // --- Application ---------------------------------------------------
        case!(GetCurrentApplication, get_current_application, |c| c
            .get_current_application()),
        case!(EditCurrentApplication, edit_current_application, |c| c
            .edit_current_application(&r::ModifyCurrentApplication {
                description: Some("golden".to_string()),
                role_connections_verification_url: Some(None),
                ..r::ModifyCurrentApplication::default()
            })),
        case!(
            GetApplicationActivityInstance,
            get_application_activity_instance,
            |c| c.get_application_activity_instance("i-1")
        ),
        case!(
            GetApplicationRoleConnectionMetadataRecords,
            get_application_role_connection_metadata_records,
            |c| c.get_application_role_connection_metadata_records()
        ),
        case!(
            UpdateApplicationRoleConnectionMetadataRecords,
            update_application_role_connection_metadata_records,
            |c| c.update_application_role_connection_metadata_records(&[
                m::ApplicationRoleConnectionMetadata {
                    kind: m::RoleConnectionMetadataType::INTEGER_GREATER_THAN_OR_EQUAL,
                    key: "level".to_string(),
                    name: "Level".to_string(),
                    name_localizations: None,
                    description: "At least".to_string(),
                    description_localizations: None,
                }
            ])
        ),
        case!(
            GetCurrentBotApplicationInformation,
            get_current_bot_application_information,
            |c| c.get_current_bot_application_information()
        ),
        // --- Application identities -----------------------------------------
        case!(
            GetApplicationIdentityProfile,
            get_application_identity_profile,
            |c| c.get_application_identity_profile(test_id(42), "player one")
        ),
        case!(
            UpdateApplicationIdentityProfile,
            update_application_identity_profile,
            |c| c.update_application_identity_profile(
                test_id(42),
                "player-1",
                &r::UpdateApplicationIdentityProfile {
                    username: Some("golden".to_string()),
                    ..r::UpdateApplicationIdentityProfile::default()
                }
            )
        ),
        case!(
            GetApplicationIdentitiesByUserId,
            get_user_application_identities,
            |c| c.get_user_application_identities(test_id(42))
        ),
        case!(
            GetApplicationIdentitiesByExternalId,
            get_application_identities_by_external_id,
            |c| c.get_application_identities_by_external_id("steam", "p/1", Some("prov"))
        ),
        case!(
            DeleteApplicationIdentity,
            delete_application_identity,
            |c| c.delete_application_identity(test_id(42), "steam", "p-1", None)
        ),
        // --- Commands -------------------------------------------------------
        case!(GetGlobalApplicationCommands, get_global_commands, |c| c
            .get_global_commands()),
        case!(CreateGlobalApplicationCommand, create_global_command, |c| c
            .create_global_command(&sample_command())),
        case!(
            BulkOverwriteGlobalApplicationCommands,
            bulk_overwrite_global_commands,
            |c| c.bulk_overwrite_global_commands(&[sample_command()])
        ),
        case!(GetGlobalApplicationCommand, get_global_command, |c| c
            .get_global_command(test_id(401))),
        case!(EditGlobalApplicationCommand, edit_global_command, |c| c
            .edit_global_command(
                test_id(401),
                &r::EditApplicationCommand::new()
                    .description("edited")
                    .clear_default_member_permissions()
            )),
        case!(DeleteGlobalApplicationCommand, delete_global_command, |c| c
            .delete_global_command(test_id(401))),
        case!(GetGuildApplicationCommands, get_guild_commands, |c| c
            .get_guild_commands(test_id(200))),
        case!(CreateGuildApplicationCommand, create_guild_command, |c| c
            .create_guild_command(test_id(200), &sample_command())),
        case!(
            BulkOverwriteGuildApplicationCommands,
            bulk_overwrite_guild_commands,
            |c| c.bulk_overwrite_guild_commands(test_id(200), &[sample_command()])
        ),
        case!(GetGuildApplicationCommand, get_guild_command, |c| c
            .get_guild_command(test_id(200), test_id(402))),
        case!(EditGuildApplicationCommand, edit_guild_command, |c| c
            .edit_guild_command(
                test_id(200),
                test_id(402),
                &r::EditApplicationCommand::new()
                    .name("renamed")
                    .default_member_permissions(Permissions::from_bits(8))
            )),
        case!(DeleteGuildApplicationCommand, delete_guild_command, |c| c
            .delete_guild_command(test_id(200), test_id(402))),
        case!(
            GetGuildApplicationCommandPermissions,
            get_guild_application_command_permissions,
            |c| c.get_guild_application_command_permissions(test_id(200))
        ),
        case!(
            GetApplicationCommandPermissions,
            get_application_command_permissions,
            |c| c.get_application_command_permissions(test_id(200), test_id(401))
        ),
        case!(
            EditApplicationCommandPermissions,
            edit_application_command_permissions,
            |c| c.edit_application_command_permissions(
                BEARER,
                test_id(200),
                test_id(401),
                &permission_edit()
            )
        ),
        case!(
            BatchEditApplicationCommandPermissions,
            batch_edit_application_command_permissions,
            |c| c.batch_edit_application_command_permissions(
                BEARER,
                test_id(200),
                &[m::GuildApplicationCommandPermissions {
                    id: m::CommandPermissionsId::Command(test_id(401)),
                    application_id: test_id(APPLICATION_ID),
                    guild_id: test_id(200),
                    permissions: permission_edit().permissions,
                }]
            )
        ),
        // --- Interactions (worker C's methods) ------------------------------
        case!(
            CreateInteractionResponse,
            create_interaction_response_with_files,
            |c| c.create_interaction_response_with_files(
                test_id(778),
                INTERACTION_TOKEN,
                &typed_interaction_response(),
                &files()
            )
        ),
        case!(
            CreateInteractionResponse,
            create_interaction_response_with_result,
            |c| c.create_interaction_response_with_result(
                test_id(778),
                INTERACTION_TOKEN,
                &typed_interaction_response()
            )
        ),
        case!(
            GetOriginalInteractionResponse,
            get_original_interaction_response,
            |c| c.get_original_interaction_response(INTERACTION_TOKEN)
        ),
        case!(
            EditOriginalInteractionResponse,
            edit_original_interaction_response,
            |c| c.edit_original_interaction_response(INTERACTION_TOKEN, &interaction_edit())
        ),
        case!(
            DeleteOriginalInteractionResponse,
            delete_original_interaction_response,
            |c| c.delete_original_interaction_response(INTERACTION_TOKEN)
        ),
        case!(CreateFollowupMessage, create_followup_message, |c| c
            .create_followup_message(INTERACTION_TOKEN, &followup())),
        case!(
            CreateFollowupMessage,
            create_followup_message_with_files,
            |c| c.create_followup_message_with_files(INTERACTION_TOKEN, &followup(), &files())
        ),
        case!(GetFollowupMessage, get_followup_message, |c| c
            .get_followup_message(INTERACTION_TOKEN, "903")),
        case!(EditFollowupMessage, edit_followup_message, |c| c
            .edit_followup_message(
                INTERACTION_TOKEN,
                "903",
                &interaction_edit()
            )),
        case!(DeleteFollowupMessage, delete_followup_message, |c| c
            .delete_followup_message(INTERACTION_TOKEN, "903")),
        case!(
            CreateInteractionResponse,
            create_interaction_response,
            |c| c.create_interaction_response(
                test_id(778),
                INTERACTION_TOKEN,
                &typed_interaction_response()
            )
        ),
        // The `_with_application_id` variants name the application instead of
        // using the client's (555).
        case!(
            GetOriginalInteractionResponse,
            get_original_interaction_response_with_application_id,
            |c| c.get_original_interaction_response_with_application_id(
                test_id(556),
                INTERACTION_TOKEN
            )
        ),
        case!(
            EditOriginalInteractionResponse,
            edit_original_interaction_response_with_files,
            |c| c.edit_original_interaction_response_with_files(
                INTERACTION_TOKEN,
                &interaction_edit(),
                &files()
            )
        ),
        case!(
            EditOriginalInteractionResponse,
            edit_original_interaction_response_with_application_id,
            |c| c.edit_original_interaction_response_with_application_id(
                test_id(556),
                INTERACTION_TOKEN,
                &interaction_edit()
            )
        ),
        case!(
            EditOriginalInteractionResponse,
            edit_original_interaction_response_with_application_id_and_files,
            |c| c.edit_original_interaction_response_with_application_id_and_files(
                test_id(556),
                INTERACTION_TOKEN,
                &interaction_edit(),
                &files()
            )
        ),
        case!(
            DeleteOriginalInteractionResponse,
            delete_original_interaction_response_with_application_id,
            |c| c.delete_original_interaction_response_with_application_id(
                test_id(556),
                INTERACTION_TOKEN
            )
        ),
        case!(
            CreateFollowupMessage,
            create_followup_message_with_application_id,
            |c| c.create_followup_message_with_application_id(
                test_id(556),
                INTERACTION_TOKEN,
                &followup()
            )
        ),
        case!(
            CreateFollowupMessage,
            create_followup_message_with_application_id_and_files,
            |c| c.create_followup_message_with_application_id_and_files(
                test_id(556),
                INTERACTION_TOKEN,
                &followup(),
                &files()
            )
        ),
        case!(EditFollowupMessage, edit_followup_message_with_files, |c| c
            .edit_followup_message_with_files(
                INTERACTION_TOKEN,
                "903",
                &interaction_edit(),
                &files()
            )),
        case!(
            EditFollowupMessage,
            edit_followup_message_with_application_id,
            |c| c.edit_followup_message_with_application_id(
                test_id(556),
                INTERACTION_TOKEN,
                "903",
                &interaction_edit()
            )
        ),
        case!(
            EditFollowupMessage,
            edit_followup_message_with_application_id_and_files,
            |c| c.edit_followup_message_with_application_id_and_files(
                test_id(556),
                INTERACTION_TOKEN,
                "903",
                &interaction_edit(),
                &files()
            )
        ),
        case!(
            GetFollowupMessage,
            get_followup_message_with_application_id,
            |c| c.get_followup_message_with_application_id(test_id(556), INTERACTION_TOKEN, "903")
        ),
        case!(
            DeleteFollowupMessage,
            delete_followup_message_with_application_id,
            |c| c.delete_followup_message_with_application_id(
                test_id(556),
                INTERACTION_TOKEN,
                "903"
            )
        ),
        // --- Audit log and auto moderation ----------------------------------
        case!(GetGuildAuditLog, get_guild_audit_log, |c| c
            .get_guild_audit_log(
                test_id(200),
                &m::AuditLogQuery {
                    user_id: Some(test_id(42)),
                    action_type: Some(m::AuditLogActionType::MEMBER_KICK),
                    before: Some(test_id(950)),
                    after: None,
                    limit: Some(25),
                }
            )),
        case!(
            ListAutoModerationRulesForGuild,
            get_auto_moderation_rules,
            |c| c.get_auto_moderation_rules(test_id(200))
        ),
        case!(GetAutoModerationRule, get_auto_moderation_rule, |c| c
            .get_auto_moderation_rule(test_id(200), test_id(7))),
        case!(CreateAutoModerationRule, create_auto_moderation_rule, |c| c
            .create_auto_moderation_rule(
                test_id(200),
                &r::CreateAutoModerationRule::new(
                    "no spam",
                    m::AutoModerationEventType::MESSAGE_SEND,
                    m::AutoModerationTriggerType::KEYWORD,
                    vec![m::AutoModerationAction {
                        kind: m::AutoModerationActionType::BLOCK_MESSAGE,
                        metadata: None,
                    }],
                )
                .enabled(true)
                .exempt_roles(vec![test_id(9)])
            )),
        case!(ModifyAutoModerationRule, modify_auto_moderation_rule, |c| c
            .modify_auto_moderation_rule(
                test_id(200),
                test_id(7),
                &r::ModifyAutoModerationRule::new().enabled(false)
            )),
        case!(DeleteAutoModerationRule, delete_auto_moderation_rule, |c| c
            .delete_auto_moderation_rule(test_id(200), test_id(7))),
        // --- Channels -------------------------------------------------------
        case!(GetChannel, get_channel, |c| c.get_channel(test_id(100))),
        case!(ModifyChannel, update_channel, |c| c.update_channel(
            test_id(100),
            &r::ModifyChannel::new()
                .name("renamed")
                .clear_topic()
                .rate_limit_per_user(5u64)
                .archived(false)
        )),
        // A forum's tag list: an existing tag kept by id, a new one by name.
        case!(ModifyChannel, update_channel, |c| c.update_channel(
            test_id(102),
            &r::ModifyChannel::new().available_tags(vec![
                r::AvailableForumTag::from(&m::ForumTag {
                    id: test_id(31),
                    name: "bug".to_string(),
                    moderated: true,
                    emoji_id: None,
                    emoji_name: Some("🐛".to_string()),
                }),
                r::AvailableForumTag::new("idea").emoji_id(test_id(32)),
            ])
        )),
        case!(DeleteChannel, delete_channel, |c| c
            .delete_channel(test_id(100))),
        case!(EditChannelPermissions, edit_channel_permissions, |c| c
            .edit_channel_permissions(
                test_id(100),
                m::OverwriteTarget::Role(test_id(9)),
                &r::EditChannelPermission {
                    kind: m::OverwriteType::ROLE,
                    allow: Some(Permissions::from_bits(1024)),
                    deny: None,
                }
            )),
        case!(DeleteChannelPermission, delete_channel_permission, |c| c
            .delete_channel_permission(
                test_id(100),
                m::OverwriteTarget::Member(test_id(42))
            )),
        case!(GetChannelInvites, get_channel_invites, |c| c
            .get_channel_invites(test_id(100))),
        case!(CreateChannelInvite, create_channel_invite, |c| c
            .create_channel_invite(
                test_id(100),
                &r::CreateChannelInvite {
                    max_age: Some(3600),
                    unique: Some(true),
                    ..r::CreateChannelInvite::default()
                }
            )),
        case!(
            CreateChannelInvite,
            create_channel_invite_with_target_users_file,
            |c| c.create_channel_invite_with_target_users_file(
                test_id(100),
                &r::CreateChannelInvite::default(),
                &sample_file("users.csv", "42\n43\n")
            )
        ),
        case!(
            FollowAnnouncementChannel,
            follow_announcement_channel,
            |c| c.follow_announcement_channel(test_id(100), test_id(101))
        ),
        case!(TriggerTypingIndicator, trigger_typing_indicator, |c| c
            .trigger_typing_indicator(test_id(100))),
        case!(GroupDmAddRecipient, add_group_dm_recipient, |c| c
            .add_group_dm_recipient(
                test_id(100),
                test_id(42),
                &r::AddGroupDmRecipient {
                    access_token: "oauth".to_string(),
                    nick: "friend".to_string(),
                }
            )),
        case!(GroupDmRemoveRecipient, remove_group_dm_recipient, |c| c
            .remove_group_dm_recipient(test_id(100), test_id(42))),
        case!(GetGuildChannels, get_guild_channels, |c| c
            .get_guild_channels(test_id(200))),
        case!(CreateGuildChannel, create_guild_channel, |c| c
            .create_guild_channel(
                test_id(200),
                &r::CreateGuildChannel {
                    name: "golden".to_string(),
                    kind: Some(m::ChannelType::GUILD_TEXT),
                    parent_id: Some(None),
                    ..r::CreateGuildChannel::default()
                }
            )),
        case!(CreateGuildChannel, create_guild_channel, |c| c
            .create_guild_channel(
                test_id(200),
                &r::CreateGuildChannel {
                    name: "forum".to_string(),
                    kind: Some(m::ChannelType::GUILD_FORUM),
                    available_tags: Some(Some(vec![r::AvailableForumTag::new("bug")])),
                    ..r::CreateGuildChannel::default()
                }
            )),
        case!(
            ModifyGuildChannelPositions,
            modify_guild_channel_positions,
            |c| c.modify_guild_channel_positions(
                test_id(200),
                &[r::ModifyGuildChannelPosition {
                    id: test_id(100),
                    position: Some(Some(1)),
                    lock_permissions: None,
                    parent_id: Some(None),
                }]
            )
        ),
        case!(CreateDm, create_dm_channel, |c| c.create_dm_channel(
            &r::CreateDmChannel {
                recipient_id: test_id(42),
            }
        )),
        case!(CreateGroupDm, create_group_dm_channel, |c| c
            .create_group_dm_channel(&r::CreateGroupDmChannel {
                access_tokens: vec!["oauth".to_string()],
                nicks: HashMap::from([(test_id(42), "friend".to_string())]),
            })),
        case!(SetVoiceChannelStatus, set_voice_channel_status, |c| c
            .set_voice_channel_status(
                test_id(300),
                &r::SetVoiceChannelStatus {
                    status: Some("golden".to_string()),
                }
            )),
        // --- Threads --------------------------------------------------------
        case!(StartThreadFromMessage, create_thread_from_message, |c| c
            .create_thread_from_message(
                test_id(100),
                test_id(1),
                &r::StartThreadFromMessage::new("thread").auto_archive_duration(60u64)
            )),
        case!(StartThreadWithoutMessage, create_thread, |c| c
            .create_thread(
                test_id(100),
                &r::StartThreadWithoutMessage::new("private")
                    .kind(m::ChannelType::PRIVATE_THREAD)
                    .invitable(false)
            )),
        case!(StartThreadInForumOrMediaChannel, create_forum_thread, |c| c
            .create_forum_thread(
                test_id(700),
                &r::StartForumThread::new("post", r::ForumThreadMessage::new().content("first"))
                    .applied_tags(vec![test_id(5)])
            )),
        case!(
            StartThreadInForumOrMediaChannel,
            create_forum_thread_with_files,
            |c| c.create_forum_thread_with_files(
                test_id(700),
                &r::StartForumThread::new(
                    "post",
                    r::ForumThreadMessage::new()
                        .content("first")
                        .attachments(vec![upload_0()])
                ),
                &files()
            )
        ),
        case!(JoinThread, join_thread, |c| c.join_thread(test_id(900))),
        case!(AddThreadMember, add_thread_member, |c| c
            .add_thread_member(test_id(900), test_id(42))),
        case!(LeaveThread, leave_thread, |c| c.leave_thread(test_id(900))),
        case!(RemoveThreadMember, remove_thread_member, |c| c
            .remove_thread_member(test_id(900), test_id(42))),
        case!(GetThreadMember, get_thread_member, |c| c.get_thread_member(
            test_id(900),
            test_id(42),
            Some(true)
        )),
        case!(ListThreadMembers, list_thread_members, |c| c
            .list_thread_members(
                test_id(900),
                &m::ThreadMemberQuery {
                    with_member: Some(true),
                    after: Some(test_id(41)),
                    limit: Some(50),
                }
            )),
        case!(ListThreadMembers, get_thread_members, |c| c
            .get_thread_members(test_id(900))),
        case!(
            ListPublicArchivedThreads,
            list_public_archived_threads,
            |c| c.list_public_archived_threads(
                test_id(100),
                &m::ArchivedThreadsQuery {
                    before: Some("2026-01-01T00:00:00Z".to_string()),
                    limit: Some(2),
                }
            )
        ),
        case!(
            ListPrivateArchivedThreads,
            list_private_archived_threads,
            |c| c.list_private_archived_threads(test_id(100), &m::ArchivedThreadsQuery::default())
        ),
        case!(
            ListJoinedPrivateArchivedThreads,
            list_joined_private_archived_threads,
            |c| c.list_joined_private_archived_threads(
                test_id(100),
                &m::JoinedArchivedThreadsQuery {
                    before: Some(test_id(800)),
                    limit: Some(3),
                }
            )
        ),
        case!(ListActiveGuildThreads, get_active_guild_threads, |c| c
            .get_active_guild_threads(test_id(200))),
        // --- Messages -------------------------------------------------------
        case!(GetChannelMessages, get_channel_messages, |c| c
            .get_channel_messages(test_id(100), Some(10))),
        case!(GetChannelMessages, get_channel_messages_paginated, |c| c
            .get_channel_messages_paginated(
                test_id(100),
                Some(5),
                Some(test_id(9)),
                None,
                None
            )),
        case!(GetChannelMessage, get_message, |c| c
            .get_message(test_id(100), test_id(1))),
        case!(CreateMessage, create_message, |c| c.create_message(
            test_id(100),
            &r::CreateMessage::new()
                .content("hello <@42>")
                .enforce_nonce("golden-nonce")
                .reply_to(test_id(1))
        )),
        case!(CreateMessage, create_message_with_files, |c| c
            .create_message_with_files(
                test_id(100),
                &r::CreateMessage::new()
                    .content("with a file")
                    .attachments(vec![upload_0()]),
                &files()
            )),
        case!(CreateMessage, forward_message, |c| c.forward_message(
            test_id(101),
            test_id(1),
            test_id(100)
        )),
        case!(CrosspostMessage, crosspost_message, |c| c
            .crosspost_message(test_id(100), test_id(1))),
        case!(EditMessage, edit_message, |c| c.edit_message(
            test_id(100),
            test_id(1),
            &r::EditMessage::new().content("edited").clear_embeds()
        )),
        case!(EditMessage, edit_message_with_files, |c| c
            .edit_message_with_files(
                test_id(100),
                test_id(1),
                &r::EditMessage::new()
                    .attachments(vec![r::MessageAttachment::keep(test_id(77)), upload_0(),]),
                &files()
            )),
        case!(DeleteMessage, delete_message, |c| c
            .delete_message(test_id(100), test_id(1))),
        case!(BulkDeleteMessages, bulk_delete_messages, |c| c
            .bulk_delete_messages(
                test_id(100),
                vec![test_id(1), test_id(2)]
            )),
        case!(SearchGuildMessages, search_guild_messages, |c| c
            .search_guild_messages(
                test_id(200),
                &m::SearchGuildMessagesQuery {
                    content: Some("hello world".to_string()),
                    limit: Some(25),
                    offset: Some(50),
                    max_id: Some(test_id(9)),
                    min_id: Some(test_id(1)),
                    slop: Some(2),
                    author_ids: vec![test_id(42)],
                    pinned: Some(false),
                    sort_by: Some("timestamp".to_string()),
                    sort_order: Some("asc".to_string()),
                    ..m::SearchGuildMessagesQuery::default()
                }
            )),
        case!(CreateReaction, add_reaction, |c| c.add_reaction(
            test_id(100),
            test_id(1),
            "👍"
        )),
        case!(DeleteOwnReaction, remove_reaction, |c| c.remove_reaction(
            test_id(100),
            test_id(1),
            "golden:123"
        )),
        case!(DeleteUserReaction, remove_user_reaction, |c| c
            .remove_user_reaction(
                test_id(100),
                test_id(1),
                "👍",
                test_id(42)
            )),
        case!(GetReactions, get_reactions, |c| c.get_reactions(
            test_id(100),
            test_id(1),
            "👍",
            Some(10),
            Some(test_id(41))
        )),
        case!(GetReactions, get_reactions_with_query, |c| c
            .get_reactions_with_query(
                test_id(100),
                test_id(1),
                "golden:123",
                &m::GetReactionsQuery {
                    kind: Some(m::ReactionType::BURST),
                    ..m::GetReactionsQuery::default()
                }
            )),
        case!(DeleteAllReactions, remove_all_reactions, |c| c
            .remove_all_reactions(test_id(100), test_id(1))),
        case!(
            DeleteAllReactionsForEmoji,
            remove_all_reactions_for_emoji,
            |c| c.remove_all_reactions_for_emoji(test_id(100), test_id(1), "👍")
        ),
        case!(GetChannelPins, get_channel_pins, |c| c.get_channel_pins(
            test_id(100),
            &m::ChannelPinsQuery {
                before: Some("2026-01-01T00:00:00Z".to_string()),
                limit: Some(10),
            }
        )),
        case!(PinMessage, pin_message, |c| c
            .pin_message(test_id(100), test_id(1))),
        case!(UnpinMessage, unpin_message, |c| c
            .unpin_message(test_id(100), test_id(1))),
        case!(GetPinnedMessagesDeprecated, get_pinned_messages, |c| c
            .get_pinned_messages(test_id(100))),
        case!(PinMessageDeprecated, pin_message_legacy, |c| c
            .pin_message_legacy(test_id(100), test_id(1))),
        case!(UnpinMessageDeprecated, unpin_message_legacy, |c| c
            .unpin_message_legacy(test_id(100), test_id(1))),
        case!(GetAnswerVoters, get_poll_answer_voters, |c| c
            .get_poll_answer_voters(
                test_id(100),
                test_id(1),
                m::PollAnswerId::new(2),
                Some(test_id(41)),
                Some(25)
            )),
        case!(EndPoll, end_poll, |c| c.end_poll(test_id(100), test_id(1))),
        // --- Emoji ----------------------------------------------------------
        case!(ListGuildEmojis, get_guild_emojis, |c| c
            .get_guild_emojis(test_id(200))),
        case!(GetGuildEmoji, get_guild_emoji, |c| c
            .get_guild_emoji(test_id(200), test_id(3))),
        case!(CreateGuildEmoji, create_guild_emoji, |c| c
            .create_guild_emoji(
                test_id(200),
                &r::CreateGuildEmoji::new("golden", "data:image/png;base64,AAAA")
                    .roles(vec![test_id(9)])
            )),
        case!(ModifyGuildEmoji, modify_guild_emoji, |c| c
            .modify_guild_emoji(
                test_id(200),
                test_id(3),
                &r::ModifyGuildEmoji::new().name("renamed").clear_roles()
            )),
        case!(DeleteGuildEmoji, delete_guild_emoji, |c| c
            .delete_guild_emoji(test_id(200), test_id(3))),
        case!(ListApplicationEmojis, get_application_emojis, |c| c
            .get_application_emojis()),
        case!(GetApplicationEmoji, get_application_emoji, |c| c
            .get_application_emoji(test_id(4))),
        case!(CreateApplicationEmoji, create_application_emoji, |c| c
            .create_application_emoji(&r::CreateApplicationEmoji::new(
                "golden",
                "data:image/png;base64,AAAA"
            ))),
        case!(ModifyApplicationEmoji, modify_application_emoji, |c| c
            .modify_application_emoji(
                test_id(4),
                &r::ModifyApplicationEmoji::new("renamed")
            )),
        case!(DeleteApplicationEmoji, delete_application_emoji, |c| c
            .delete_application_emoji(test_id(4))),
        // --- Entitlements and SKUs ------------------------------------------
        case!(ListSkus, get_skus, |c| c.get_skus()),
        case!(ListSkuSubscriptions, get_sku_subscriptions, |c| c
            .get_sku_subscriptions(
                test_id(900),
                &m::SubscriptionQuery {
                    before: None,
                    after: Some(test_id(949)),
                    limit: Some(10),
                    user_id: Some(test_id(42)),
                }
            )),
        case!(GetSkuSubscription, get_sku_subscription, |c| c
            .get_sku_subscription(test_id(900), test_id(950))),
        case!(ListEntitlements, get_entitlements, |c| c.get_entitlements(
            &m::EntitlementQuery {
                user_id: Some(test_id(42)),
                sku_ids: vec![test_id(900), test_id(901)],
                before: Some(test_id(990)),
                after: Some(test_id(910)),
                exclude_ended: Some(true),
                ..m::EntitlementQuery::default()
            }
        )),
        case!(GetEntitlement, get_entitlement, |c| c
            .get_entitlement(test_id(901))),
        case!(ConsumeEntitlement, consume_entitlement, |c| c
            .consume_entitlement(test_id(901))),
        case!(CreateTestEntitlement, create_test_entitlement, |c| c
            .create_test_entitlement(&r::CreateTestEntitlement::new(
                test_id(900),
                m::EntitlementOwner::User(test_id(42))
            ))),
        case!(DeleteTestEntitlement, delete_test_entitlement, |c| c
            .delete_test_entitlement(test_id(902))),
        // --- Gateway --------------------------------------------------------
        case!(GetGateway, get_gateway, |c| c.get_gateway()),
        case!(GetGatewayBot, get_gateway_bot, |c| c.get_gateway_bot()),
        // --- Guilds ---------------------------------------------------------
        case!(GetGuild, get_guild, |c| c.get_guild(test_id(200))),
        case!(GetGuild, get_guild_with_query, |c| c.get_guild_with_query(
            test_id(200),
            &m::GetGuildQuery {
                with_counts: Some(true),
            }
        )),
        case!(GetGuildPreview, get_guild_preview, |c| c
            .get_guild_preview(test_id(200))),
        case!(ModifyGuild, modify_guild, |c| c.modify_guild(
            test_id(200),
            &r::ModifyGuild {
                name: Some("golden".to_string()),
                afk_channel_id: Some(None),
                ..r::ModifyGuild::default()
            }
        )),
        case!(
            ModifyGuildIncidentActions,
            modify_guild_incident_actions,
            |c| c.modify_guild_incident_actions(
                test_id(200),
                &r::ModifyGuildIncidentActions {
                    invites_disabled_until: Some("2026-05-01T12:00:00.000000+00:00".to_string()),
                    dms_disabled_until: None,
                }
            )
        ),
        case!(CreateGuild, create_guild, |c| c.create_guild(
            &r::CreateGuild {
                name: "golden".to_string(),
                ..r::CreateGuild::default()
            }
        )),
        case!(CreateGuildFromTemplate, create_guild_from_template, |c| c
            .create_guild_from_template(
                "tmpl",
                &r::CreateGuildFromTemplate {
                    name: "golden".to_string(),
                    icon: None,
                }
            )),
        case!(DeleteGuild, delete_guild, |c| c.delete_guild(test_id(200))),
        case!(ModifyGuildMfaLevel, modify_guild_mfa_level, |c| c
            .modify_guild_mfa_level(test_id(200), m::MfaLevel::ELEVATED)),
        case!(GetGuildIntegrations, get_guild_integrations, |c| c
            .get_guild_integrations(test_id(200))),
        case!(DeleteGuildIntegration, delete_guild_integration, |c| c
            .delete_guild_integration(test_id(200), test_id(33))),
        case!(GetGuildPruneCount, get_guild_prune_count, |c| c
            .get_guild_prune_count(
                test_id(200),
                Some(7),
                &[test_id(9), test_id(10)]
            )),
        case!(BeginGuildPrune, begin_guild_prune, |c| c.begin_guild_prune(
            test_id(200),
            &r::BeginGuildPruneRequest {
                days: Some(7),
                compute_prune_count: Some(false),
                include_roles: Some(vec![test_id(9)]),
                reason: None,
            }
        )),
        case!(GetGuildVoiceRegions, get_guild_voice_regions, |c| c
            .get_guild_voice_regions(test_id(200))),
        case!(GetGuildInvites, get_guild_invites, |c| c
            .get_guild_invites(test_id(200))),
        case!(GetGuildVanityUrl, get_guild_vanity_url, |c| c
            .get_guild_vanity_url(test_id(200))),
        case!(GetGuildWidgetSettings, get_guild_widget_settings, |c| c
            .get_guild_widget_settings(test_id(200))),
        case!(ModifyGuildWidget, modify_guild_widget_settings, |c| c
            .modify_guild_widget_settings(
                test_id(200),
                &r::ModifyGuildWidgetSettings {
                    enabled: Some(true),
                    channel_id: Some(None),
                }
            )),
        case!(GetGuildWidget, get_guild_widget, |c| c
            .get_guild_widget(test_id(200))),
        case!(GetGuildWidgetImage, get_guild_widget_image, |c| c
            .get_guild_widget_image(
                test_id(200),
                Some(m::GuildWidgetImageStyle::Banner2)
            )),
        case!(GetGuildWelcomeScreen, get_guild_welcome_screen, |c| c
            .get_guild_welcome_screen(test_id(200))),
        case!(ModifyGuildWelcomeScreen, modify_guild_welcome_screen, |c| c
            .modify_guild_welcome_screen(
                test_id(200),
                &r::ModifyGuildWelcomeScreen {
                    enabled: Some(Some(true)),
                    description: Some(None),
                    ..r::ModifyGuildWelcomeScreen::default()
                }
            )),
        case!(GetGuildOnboarding, get_guild_onboarding, |c| c
            .get_guild_onboarding(test_id(200))),
        case!(ModifyGuildOnboarding, modify_guild_onboarding, |c| c
            .modify_guild_onboarding(
                test_id(200),
                &r::ModifyGuildOnboarding {
                    default_channel_ids: Some(vec![test_id(100)]),
                    enabled: Some(true),
                    ..r::ModifyGuildOnboarding::default()
                }
            )),
        // --- Guild templates ------------------------------------------------
        case!(GetGuildTemplate, get_guild_template_by_code, |c| c
            .get_guild_template_by_code("tmpl")),
        case!(GetGuildTemplates, get_guild_templates, |c| c
            .get_guild_templates(test_id(200))),
        case!(CreateGuildTemplate, create_guild_template, |c| c
            .create_guild_template(
                test_id(200),
                &r::CreateGuildTemplate::new("golden").description("a template")
            )),
        case!(SyncGuildTemplate, sync_guild_template, |c| c
            .sync_guild_template(test_id(200), "tmpl")),
        case!(ModifyGuildTemplate, modify_guild_template, |c| c
            .modify_guild_template(
                test_id(200),
                "tmpl",
                &r::ModifyGuildTemplate::new().clear_description()
            )),
        case!(DeleteGuildTemplate, delete_guild_template, |c| c
            .delete_guild_template(test_id(200), "tmpl")),
        // --- Members, bans, roles -------------------------------------------
        case!(GetGuildMember, get_member, |c| c
            .get_member(test_id(200), test_id(42))),
        case!(ListGuildMembers, get_guild_members, |c| c
            .get_guild_members(test_id(200), Some(2))),
        case!(ListGuildMembers, get_guild_members_with_query, |c| c
            .get_guild_members_with_query(
                test_id(200),
                &m::GuildMembersQuery {
                    limit: Some(100),
                    after: Some(test_id(41)),
                }
            )),
        case!(SearchGuildMembers, search_guild_members, |c| c
            .search_guild_members(test_id(200), "gold en", Some(5))),
        case!(SearchGuildMembers, search_guild_members_with_query, |c| c
            .search_guild_members_with_query(
                test_id(200),
                &m::SearchGuildMembersQuery {
                    query: "a&b".to_string(),
                    limit: None,
                }
            )),
        case!(AddGuildMember, add_guild_member, |c| c.add_guild_member(
            test_id(200),
            test_id(42),
            &r::AddGuildMember {
                access_token: "oauth".to_string(),
                nick: Some("golden".to_string()),
                ..r::AddGuildMember::default()
            }
        )),
        case!(ModifyGuildMember, modify_guild_member, |c| c
            .modify_guild_member(
                test_id(200),
                test_id(42),
                &r::ModifyGuildMember {
                    nick: Some(None),
                    communication_disabled_until: Some(Some("2026-05-01T00:00:00Z".to_string())),
                    ..r::ModifyGuildMember::default()
                }
            )),
        case!(ModifyCurrentMember, modify_current_member, |c| c
            .modify_current_member(
                test_id(200),
                &r::ModifyCurrentMember {
                    nick: Some(Some("bot".to_string())),
                    ..r::ModifyCurrentMember::default()
                }
            )),
        case!(ModifyCurrentUserNick, modify_current_member_nick, |c| c
            .modify_current_member_nick(test_id(200), None)),
        case!(AddGuildMemberRole, add_guild_member_role, |c| c
            .add_guild_member_role(test_id(200), test_id(42), test_id(9))),
        case!(RemoveGuildMemberRole, remove_guild_member_role, |c| c
            .remove_guild_member_role(
                test_id(200),
                test_id(42),
                test_id(9)
            )),
        case!(RemoveGuildMember, remove_guild_member, |c| c
            .remove_guild_member(test_id(200), test_id(42))),
        case!(GetGuildBans, get_guild_bans, |c| c.get_guild_bans(
            test_id(200),
            Some(2),
            Some(test_id(9))
        )),
        case!(GetGuildBans, get_guild_bans_with_query, |c| c
            .get_guild_bans_with_query(
                test_id(200),
                &m::GuildBansQuery {
                    limit: Some(10),
                    before: None,
                    after: Some(test_id(3)),
                }
            )),
        case!(GetGuildBan, get_guild_ban, |c| c
            .get_guild_ban(test_id(200), test_id(42))),
        case!(CreateGuildBan, create_guild_ban, |c| c.create_guild_ban(
            test_id(200),
            test_id(42),
            &r::CreateGuildBan {
                delete_message_days: None,
                delete_message_seconds: Some(60),
            }
        )),
        case!(RemoveGuildBan, remove_guild_ban, |c| c
            .remove_guild_ban(test_id(200), test_id(42))),
        case!(BulkGuildBan, bulk_guild_ban, |c| c.bulk_guild_ban(
            test_id(200),
            &r::BulkGuildBanRequest {
                user_ids: vec![test_id(42), test_id(43)],
                delete_message_seconds: Some(0),
            }
        )),
        case!(GetGuildRoles, list_roles, |c| c.list_roles(test_id(200))),
        case!(GetGuildRole, get_guild_role, |c| c
            .get_guild_role(test_id(200), test_id(9))),
        case!(
            GetGuildRoleMemberCounts,
            get_guild_role_member_counts,
            |c| c.get_guild_role_member_counts(test_id(200))
        ),
        case!(CreateGuildRole, create_guild_role, |c| c.create_guild_role(
            test_id(200),
            &r::CreateGuildRole {
                name: Some("golden".to_string()),
                permissions: Some(Permissions::from_bits(8)),
                ..r::CreateGuildRole::default()
            }
        )),
        case!(ModifyGuildRolePositions, modify_guild_role_positions, |c| c
            .modify_guild_role_positions(
                test_id(200),
                &[r::ModifyGuildRolePosition::new(test_id(9))]
            )),
        case!(ModifyGuildRole, modify_guild_role, |c| c.modify_guild_role(
            test_id(200),
            test_id(9),
            &r::ModifyGuildRole {
                name: Some(Some("renamed".to_string())),
                icon: Some(None),
                ..r::ModifyGuildRole::default()
            }
        )),
        case!(DeleteGuildRole, delete_role, |c| c
            .delete_role(test_id(200), test_id(9))),
        // --- Invites --------------------------------------------------------
        case!(GetInvite, get_invite, |c| c.get_invite("golden")),
        case!(GetInvite, get_invite_with_options, |c| c
            .get_invite_with_options(
                "golden",
                Some(true),
                Some(false),
                Some(test_id(5))
            )),
        case!(DeleteInvite, delete_invite, |c| c.delete_invite("golden")),
        case!(GetTargetUsers, get_invite_target_users, |c| c
            .get_invite_target_users("golden")),
        case!(GetTargetUsers, get_invite_target_users_csv, |c| c
            .get_invite_target_users_csv("golden")),
        case!(UpdateTargetUsers, update_invite_target_users, |c| c
            .update_invite_target_users(
                "golden",
                &sample_file("users.csv", "42\n")
            )),
        case!(
            GetTargetUsersJobStatus,
            get_invite_target_users_job_status,
            |c| c.get_invite_target_users_job_status("golden")
        ),
        case!(AddTargetUser, add_invite_target_user, |c| c
            .add_invite_target_user("golden", test_id(42))),
        case!(RemoveTargetUser, remove_invite_target_user, |c| c
            .remove_invite_target_user("golden", test_id(42))),
        case!(BulkAddTargetUsers, bulk_add_invite_target_users, |c| c
            .bulk_add_invite_target_users(
                "golden",
                &[test_id(42), test_id(43)]
            )),
        case!(
            BulkDeleteTargetUsers,
            bulk_delete_invite_target_users,
            |c| c.bulk_delete_invite_target_users("golden", &[test_id(42)])
        ),
        // --- Lobbies --------------------------------------------------------
        case!(CreateLobby, create_lobby, |c| c.create_lobby(
            &r::CreateLobby {
                metadata: Some(metadata()),
                idle_timeout_seconds: Some(300),
                ..r::CreateLobby::default()
            }
        )),
        case!(CreateOrJoinLobby, get_or_create_lobby, |c| c
            .get_or_create_lobby(
                BEARER,
                &r::GetOrCreateLobby {
                    secret: "lobby-secret".to_string(),
                    ..r::GetOrCreateLobby::default()
                }
            )),
        case!(GetLobby, get_lobby, |c| c.get_lobby(test_id(600))),
        case!(ModifyLobby, modify_lobby, |c| c.modify_lobby(
            test_id(600),
            &r::ModifyLobby {
                idle_timeout_seconds: Some(60),
                ..r::ModifyLobby::default()
            }
        )),
        case!(DeleteLobby, delete_lobby, |c| c.delete_lobby(test_id(600))),
        case!(AddLobbyMember, add_lobby_member, |c| c.add_lobby_member(
            test_id(600),
            test_id(42),
            &r::AddLobbyMember {
                metadata: Some(metadata()),
                flags: Some(1),
            }
        )),
        case!(BulkUpdateLobbyMembers, bulk_update_lobby_members, |c| c
            .bulk_update_lobby_members(
                test_id(600),
                &[m::LobbyMemberUpdate {
                    id: test_id(42),
                    metadata: None,
                    flags: None,
                    remove_member: Some(true),
                }]
            )),
        case!(RemoveLobbyMember, remove_lobby_member, |c| c
            .remove_lobby_member(test_id(600), test_id(42))),
        case!(LeaveLobby, leave_lobby, |c| c
            .leave_lobby(BEARER, test_id(600))),
        case!(LinkChannelToLobby, link_lobby_channel, |c| c
            .link_lobby_channel(
                BEARER,
                test_id(600),
                &r::LinkLobbyChannel {
                    channel_id: Some(test_id(100)),
                }
            )),
        case!(
            CreateLobbyChannelInviteForSelf,
            create_lobby_channel_invite_for_self,
            |c| c.create_lobby_channel_invite_for_self(BEARER, test_id(600))
        ),
        case!(
            CreateLobbyChannelInviteForUser,
            create_lobby_channel_invite_for_user,
            |c| c.create_lobby_channel_invite_for_user(test_id(600), test_id(42))
        ),
        case!(SendLobbyMessage, send_lobby_message, |c| c
            .send_lobby_message(
                BEARER,
                test_id(600),
                &r::CreateLobbyMessage {
                    content: "hello lobby".to_string(),
                    ..r::CreateLobbyMessage::default()
                }
            )),
        case!(GetLobbyMessages, get_lobby_messages, |c| c
            .get_lobby_messages(BEARER, test_id(600), Some(20))),
        case!(
            UpdateLobbyMessageModerationMetadata,
            update_lobby_message_moderation_metadata,
            |c| c.update_lobby_message_moderation_metadata(test_id(600), test_id(1), &metadata())
        ),
        // --- Scheduled events -----------------------------------------------
        case!(
            ListScheduledEventsForGuild,
            get_guild_scheduled_events,
            |c| c.get_guild_scheduled_events(test_id(200))
        ),
        case!(
            ListScheduledEventsForGuild,
            get_guild_scheduled_events_with_query,
            |c| c.get_guild_scheduled_events_with_query(
                test_id(200),
                &m::GuildScheduledEventsQuery {
                    with_user_count: Some(true),
                }
            )
        ),
        case!(
            CreateGuildScheduledEvent,
            create_guild_scheduled_event,
            |c| c.create_guild_scheduled_event(
                test_id(200),
                &r::CreateGuildScheduledEvent::new(
                    "golden night",
                    m::ScheduledEventEntityType::EXTERNAL,
                    m::ScheduledEventPrivacyLevel::GUILD_ONLY,
                    "2026-05-01T18:00:00Z",
                )
                .scheduled_end_time("2026-05-01T20:00:00Z")
                .entity_metadata(m::GuildScheduledEventEntityMetadata {
                    location: Some("Stage".to_string()),
                })
            )
        ),
        case!(GetGuildScheduledEvent, get_guild_scheduled_event, |c| c
            .get_guild_scheduled_event(test_id(200), test_id(2))),
        case!(
            ModifyGuildScheduledEvent,
            modify_guild_scheduled_event,
            |c| c.modify_guild_scheduled_event(
                test_id(200),
                test_id(2),
                &r::ModifyGuildScheduledEvent::new()
                    .status(m::ScheduledEventStatus::CANCELED)
                    .clear_description()
            )
        ),
        case!(
            DeleteGuildScheduledEvent,
            delete_guild_scheduled_event,
            |c| c.delete_guild_scheduled_event(test_id(200), test_id(2))
        ),
        case!(
            GetGuildScheduledEventUsers,
            get_guild_scheduled_event_users,
            |c| c.get_guild_scheduled_event_users(test_id(200), test_id(2), Some(50))
        ),
        case!(
            GetGuildScheduledEventUsers,
            get_guild_scheduled_event_users_with_query,
            |c| c.get_guild_scheduled_event_users_with_query(
                test_id(200),
                test_id(2),
                &m::GuildScheduledEventUsersQuery {
                    limit: Some(10),
                    with_member: Some(true),
                    before: Some(test_id(43)),
                    after: Some(test_id(41)),
                }
            )
        ),
        // --- Stickers and soundboard ----------------------------------------
        case!(GetSticker, get_sticker, |c| c.get_sticker(test_id(10))),
        case!(ListStickerPacks, list_sticker_packs, |c| c
            .list_sticker_packs()),
        case!(GetStickerPack, get_sticker_pack, |c| c
            .get_sticker_pack(test_id(11))),
        case!(ListGuildStickers, get_guild_stickers, |c| c
            .get_guild_stickers(test_id(200))),
        case!(GetGuildSticker, get_guild_sticker, |c| c
            .get_guild_sticker(test_id(200), test_id(12))),
        case!(CreateGuildSticker, create_guild_sticker, |c| c
            .create_guild_sticker(
                test_id(200),
                &r::CreateGuildSticker {
                    name: "golden".to_string(),
                    description: "a sticker".to_string(),
                    tags: "star".to_string(),
                },
                FileAttachment::new("sticker.png", b"png".to_vec()).with_content_type("image/png")
            )),
        case!(ModifyGuildSticker, modify_guild_sticker, |c| c
            .modify_guild_sticker(
                test_id(200),
                test_id(12),
                &r::ModifyGuildSticker {
                    name: Some("renamed".to_string()),
                    description: Some(None),
                    tags: None,
                }
            )),
        case!(DeleteGuildSticker, delete_guild_sticker, |c| c
            .delete_guild_sticker(test_id(200), test_id(12))),
        case!(SendSoundboardSound, send_soundboard_sound, |c| c
            .send_soundboard_sound(
                test_id(300),
                &r::SendSoundboardSound::new(test_id(1)).source_guild_id(test_id(201))
            )),
        case!(
            ListDefaultSoundboardSounds,
            list_default_soundboard_sounds,
            |c| c.list_default_soundboard_sounds()
        ),
        case!(
            ListGuildSoundboardSounds,
            list_guild_soundboard_sounds,
            |c| c.list_guild_soundboard_sounds(test_id(200))
        ),
        case!(GetGuildSoundboardSound, get_guild_soundboard_sound, |c| c
            .get_guild_soundboard_sound(test_id(200), test_id(2))),
        case!(
            CreateGuildSoundboardSound,
            create_guild_soundboard_sound,
            |c| c.create_guild_soundboard_sound(
                test_id(200),
                &r::CreateGuildSoundboardSound::new("quack", "data:audio/ogg;base64,AAAA")
                    .volume(0.5)
            )
        ),
        case!(
            ModifyGuildSoundboardSound,
            modify_guild_soundboard_sound,
            |c| c.modify_guild_soundboard_sound(
                test_id(200),
                test_id(2),
                &r::ModifyGuildSoundboardSound::new()
                    .name("honk")
                    .clear_emoji_id()
            )
        ),
        case!(
            DeleteGuildSoundboardSound,
            delete_guild_soundboard_sound,
            |c| c.delete_guild_soundboard_sound(test_id(200), test_id(2))
        ),
        // --- Stage instances and voice --------------------------------------
        case!(CreateStageInstance, create_stage_instance, |c| c
            .create_stage_instance(&r::CreateStageInstance::new(
                test_id(400),
                "town hall"
            ))),
        case!(GetStageInstance, get_stage_instance, |c| c
            .get_stage_instance(test_id(400))),
        case!(ModifyStageInstance, modify_stage_instance, |c| c
            .modify_stage_instance(
                test_id(400),
                &r::ModifyStageInstance {
                    topic: Some("golden".to_string()),
                    privacy_level: Some(m::StageInstancePrivacyLevel::GUILD_ONLY),
                }
            )),
        case!(DeleteStageInstance, delete_stage_instance, |c| c
            .delete_stage_instance(test_id(400))),
        case!(ListVoiceRegions, get_voice_regions, |c| c
            .get_voice_regions()),
        case!(
            GetCurrentUserVoiceState,
            get_current_user_voice_state,
            |c| c.get_current_user_voice_state(test_id(200))
        ),
        case!(GetUserVoiceState, get_user_voice_state, |c| c
            .get_user_voice_state(test_id(200), test_id(42))),
        case!(
            ModifyCurrentUserVoiceState,
            modify_current_user_voice_state,
            |c| c.modify_current_user_voice_state(
                test_id(200),
                &r::ModifyCurrentUserVoiceState {
                    suppress: Some(false),
                    request_to_speak_timestamp: Some(None),
                    ..r::ModifyCurrentUserVoiceState::default()
                }
            )
        ),
        case!(ModifyUserVoiceState, modify_user_voice_state, |c| c
            .modify_user_voice_state(
                test_id(200),
                test_id(42),
                &r::ModifyUserVoiceState {
                    channel_id: Some(test_id(400)),
                    suppress: Some(true),
                }
            )),
        // --- Users ----------------------------------------------------------
        case!(GetCurrentUser, get_current_user, |c| c.get_current_user()),
        case!(GetUser, get_user, |c| c.get_user(test_id(42))),
        case!(ModifyCurrentUser, modify_current_user, |c| c
            .modify_current_user(&r::ModifyCurrentUser {
                username: Some("golden".to_string()),
                ..r::ModifyCurrentUser::default()
            })),
        case!(GetCurrentUserGuilds, get_current_user_guilds, |c| c
            .get_current_user_guilds()),
        case!(
            GetCurrentUserGuilds,
            get_current_user_guilds_with_query,
            |c| c.get_current_user_guilds_with_query(&m::CurrentUserGuildsQuery {
                before: None,
                after: Some(test_id(200)),
                limit: Some(1),
                with_counts: Some(true),
                shard: None,
            })
        ),
        case!(
            GetCurrentUserGuildMember,
            get_current_user_guild_member,
            |c| c.get_current_user_guild_member(BEARER, test_id(200))
        ),
        case!(LeaveGuild, leave_guild, |c| c.leave_guild(test_id(200))),
        case!(
            GetCurrentUserConnections,
            get_current_user_connections,
            |c| c.get_current_user_connections(BEARER)
        ),
        case!(
            GetCurrentUserApplicationRoleConnection,
            get_current_user_application_role_connection,
            |c| c.get_current_user_application_role_connection(BEARER)
        ),
        case!(
            UpdateCurrentUserApplicationRoleConnection,
            update_current_user_application_role_connection,
            |c| c.update_current_user_application_role_connection(
                BEARER,
                &r::UpdateUserApplicationRoleConnection {
                    platform_name: Some("Golden".to_string()),
                    ..r::UpdateUserApplicationRoleConnection::default()
                }
            )
        ),
        case!(
            DeleteCurrentUserApplicationRoleConnection,
            delete_current_user_application_role_connection,
            |c| c.delete_current_user_application_role_connection(BEARER)
        ),
        case!(
            GetCurrentAuthorizationInformation,
            get_current_authorization_information,
            |c| c.get_current_authorization_information(BEARER)
        ),
        // --- Webhooks -------------------------------------------------------
        case!(CreateWebhook, create_webhook, |c| c.create_webhook(
            test_id(100),
            &r::CreateWebhook {
                name: "golden".to_string(),
                avatar: Some(None),
            }
        )),
        case!(GetChannelWebhooks, get_channel_webhooks, |c| c
            .get_channel_webhooks(test_id(100))),
        case!(GetGuildWebhooks, get_guild_webhooks, |c| c
            .get_guild_webhooks(test_id(200))),
        case!(GetWebhook, get_webhook, |c| c.get_webhook(test_id(700))),
        case!(GetWebhookWithToken, get_webhook_with_token, |c| c
            .get_webhook_with_token(test_id(700), WEBHOOK_TOKEN)),
        case!(ModifyWebhook, modify_webhook, |c| c.modify_webhook(
            test_id(700),
            &r::ModifyWebhook {
                name: Some("renamed".to_string()),
                channel_id: Some(test_id(101)),
                ..r::ModifyWebhook::default()
            }
        )),
        case!(ModifyWebhookWithToken, modify_webhook_with_token, |c| c
            .modify_webhook_with_token(
                test_id(700),
                WEBHOOK_TOKEN,
                &r::ModifyWebhookWithToken {
                    name: Some("public".to_string()),
                    avatar: Some(None),
                }
            )),
        case!(DeleteWebhook, delete_webhook, |c| c
            .delete_webhook(test_id(700))),
        case!(DeleteWebhookWithToken, delete_webhook_with_token, |c| c
            .delete_webhook_with_token(test_id(700), WEBHOOK_TOKEN)),
        case!(ExecuteWebhook, execute_webhook, |c| c.execute_webhook(
            test_id(700),
            WEBHOOK_TOKEN,
            &r::ExecuteWebhook::new()
                .content("from a webhook")
                .username("Golden")
                .flags(message_flags::SUPPRESS_EMBEDS)
        )),
        case!(ExecuteWebhook, execute_webhook_with_query, |c| c
            .execute_webhook_with_query(
                test_id(700),
                WEBHOOK_TOKEN,
                &m::WebhookExecuteQuery {
                    wait: Some(false),
                    thread_id: Some(test_id(901)),
                    with_components: Some(true),
                },
                &r::ExecuteWebhook::new().content("into a thread")
            )),
        case!(ExecuteWebhook, execute_webhook_with_files, |c| c
            .execute_webhook_with_files(
                test_id(700),
                WEBHOOK_TOKEN,
                &r::ExecuteWebhook::new().attachments(vec![upload_0()]),
                &files()
            )),
        case!(
            ExecuteSlackCompatibleWebhook,
            execute_slack_compatible_webhook,
            |c| c.execute_slack_compatible_webhook(
                test_id(700),
                WEBHOOK_TOKEN,
                &m::WebhookExecuteQuery {
                    wait: Some(true),
                    ..m::WebhookExecuteQuery::default()
                },
                &json!({ "text": "deploy" })
            )
        ),
        case!(
            ExecuteGitHubCompatibleWebhook,
            execute_github_compatible_webhook,
            |c| c.execute_github_compatible_webhook(
                test_id(700),
                WEBHOOK_TOKEN,
                &m::WebhookExecuteQuery::default(),
                &json!({ "action": "opened" })
            )
        ),
        case!(GetWebhookMessage, get_webhook_message, |c| c
            .get_webhook_message(test_id(700), WEBHOOK_TOKEN, "900")),
        case!(GetWebhookMessage, get_webhook_message_with_query, |c| c
            .get_webhook_message_with_query(
                test_id(700),
                WEBHOOK_TOKEN,
                "900",
                &m::WebhookMessageQuery {
                    thread_id: Some(test_id(901)),
                    with_components: Some(true),
                }
            )),
        case!(EditWebhookMessage, edit_webhook_message, |c| c
            .edit_webhook_message(
                test_id(700),
                WEBHOOK_TOKEN,
                "900",
                &r::EditWebhookMessage::new().content("edited")
            )),
        case!(EditWebhookMessage, edit_webhook_message_with_query, |c| c
            .edit_webhook_message_with_query(
                test_id(700),
                WEBHOOK_TOKEN,
                "900",
                &m::WebhookMessageQuery {
                    thread_id: Some(test_id(901)),
                    with_components: Some(true),
                },
                &r::EditWebhookMessage::new().clear_content()
            )),
        case!(EditWebhookMessage, edit_webhook_message_with_files, |c| c
            .edit_webhook_message_with_files(
                test_id(700),
                WEBHOOK_TOKEN,
                "900",
                &r::EditWebhookMessage::new().attachments(vec![upload_0()]),
                &files()
            )),
        case!(
            EditWebhookMessage,
            edit_webhook_message_with_files_and_query,
            |c| c.edit_webhook_message_with_files_and_query(
                test_id(700),
                WEBHOOK_TOKEN,
                "@original",
                &m::WebhookMessageQuery {
                    thread_id: Some(test_id(901)),
                    with_components: None,
                },
                &r::EditWebhookMessage::new(),
                &files()
            )
        ),
        case!(DeleteWebhookMessage, delete_webhook_message, |c| c
            .delete_webhook_message(test_id(700), WEBHOOK_TOKEN, "900")),
        case!(
            DeleteWebhookMessage,
            delete_webhook_message_with_query,
            |c| c.delete_webhook_message_with_query(
                test_id(700),
                WEBHOOK_TOKEN,
                "900",
                &m::WebhookMessageQuery {
                    thread_id: Some(test_id(901)),
                    with_components: Some(true),
                }
            )
        ),
        // --- OAuth2 token endpoints -----------------------------------------
        case!(oauth2 OAuth2TokenExchange, exchange_code, |c| c
            .exchange_code(OAuth2CodeExchange::new("code-1", "https://example.com/cb"))),
        case!(oauth2 OAuth2TokenExchange, refresh_token, |c| c
            .refresh_token(OAuth2RefreshToken::new("refresh-1"))),
        case!(oauth2 OAuth2TokenRevocation, revoke_token, |c| c
            .revoke_token("access-1", Some("access_token"))),
    ]
}

/// A deprecated forward and its replacement, called with the same arguments.
macro_rules! forward {
    ($route:ident, $old:ident => $new:ident ($($arg:expr),* $(,)?)) => {
        (
            case!($route, $old, |c| c.$old($($arg),*)),
            case!($route, $new, |c| c.$new($($arg),*)),
        )
    };
}

#[expect(
    deprecated,
    reason = "compares each deprecated forward with its replacement"
)]
fn forwards() -> Vec<(Case, Case)> {
    vec![
        forward!(CreateGuildChannel, create_guild_channel_from_request => create_guild_channel(
            test_id(200),
            &r::CreateGuildChannel {
                name: "golden".to_string(),
                ..r::CreateGuildChannel::default()
            }
        )),
        forward!(CreateDm, create_dm_channel_typed => create_dm_channel(&r::CreateDmChannel {
            recipient_id: test_id(42),
        })),
        forward!(CreateGroupDm, create_group_dm_channel_typed => create_group_dm_channel(
            &r::CreateGroupDmChannel {
                access_tokens: vec!["oauth".to_string()],
                nicks: HashMap::new(),
            }
        )),
        forward!(EditChannelPermissions, edit_channel_permissions_typed => edit_channel_permissions(
            test_id(100),
            m::OverwriteTarget::Member(test_id(42)),
            &r::EditChannelPermission::for_target(m::OverwriteTarget::Member(test_id(42)))
        )),
        forward!(CreateChannelInvite, create_channel_invite_typed => create_channel_invite(
            test_id(100),
            &r::CreateChannelInvite::default()
        )),
        forward!(GetGuildPreview, get_guild_preview_typed => get_guild_preview(test_id(200))),
        forward!(GetGuildPruneCount, get_guild_prune_count_typed => get_guild_prune_count(
            test_id(200),
            Some(3),
            &[]
        )),
        forward!(BeginGuildPrune, begin_guild_prune_with_request => begin_guild_prune(
            test_id(200),
            &r::BeginGuildPruneRequest::default()
        )),
        forward!(ModifyGuildWidget, modify_guild_widget => modify_guild_widget_settings(
            test_id(200),
            &r::ModifyGuildWidgetSettings::default()
        )),
        forward!(GetGuildWidget, get_guild_widget_typed => get_guild_widget(test_id(200))),
        forward!(ModifyGuildWelcomeScreen, modify_guild_welcome_screen_config => modify_guild_welcome_screen(
            test_id(200),
            &r::ModifyGuildWelcomeScreen::default()
        )),
        forward!(ModifyGuildOnboarding, modify_guild_onboarding_config => modify_guild_onboarding(
            test_id(200),
            &r::ModifyGuildOnboarding::default()
        )),
        forward!(ModifyGuildRolePositions, modify_guild_role_positions_typed => modify_guild_role_positions(
            test_id(200),
            &[r::ModifyGuildRolePosition::new(test_id(9))]
        )),
        forward!(ListAutoModerationRulesForGuild, get_auto_moderation_rules_typed => get_auto_moderation_rules(
            test_id(200)
        )),
        forward!(GetGuildAuditLog, get_guild_audit_log_typed => get_guild_audit_log(
            test_id(200),
            &m::AuditLogQuery::default()
        )),
        forward!(ListGuildEmojis, get_guild_emojis_typed => get_guild_emojis(test_id(200))),
        forward!(GetGuildEmoji, get_guild_emoji_typed => get_guild_emoji(test_id(200), test_id(3))),
        forward!(ListApplicationEmojis, get_application_emojis_typed => get_application_emojis()),
        forward!(GetApplicationEmoji, get_application_emoji_typed => get_application_emoji(test_id(4))),
        forward!(CreateGuildSticker, create_guild_sticker_from_request => create_guild_sticker(
            test_id(200),
            &r::CreateGuildSticker {
                name: "golden".to_string(),
                description: String::new(),
                tags: "star".to_string(),
            },
            FileAttachment::new("sticker.png", b"png".to_vec())
        )),
        forward!(ModifyGuildSticker, modify_guild_sticker_from_request => modify_guild_sticker(
            test_id(200),
            test_id(12),
            &r::ModifyGuildSticker::default()
        )),
        forward!(CreateStageInstance, create_stage_instance_from_request => create_stage_instance(
            &r::CreateStageInstance::new(test_id(400), "town hall")
        )),
        forward!(ModifyStageInstance, modify_stage_instance_from_request => modify_stage_instance(
            test_id(400),
            &r::ModifyStageInstance::default()
        )),
        forward!(ListVoiceRegions, get_voice_regions_typed => get_voice_regions()),
        forward!(ModifyCurrentUserVoiceState, modify_current_user_voice_state_from_request => modify_current_user_voice_state(
            test_id(200),
            &r::ModifyCurrentUserVoiceState::default()
        )),
        forward!(ModifyUserVoiceState, modify_user_voice_state_from_request => modify_user_voice_state(
            test_id(200),
            test_id(42),
            &r::ModifyUserVoiceState::default()
        )),
        forward!(EditCurrentApplication, edit_current_application_from_request => edit_current_application(
            &r::ModifyCurrentApplication::default()
        )),
        forward!(BulkOverwriteGlobalApplicationCommands, bulk_overwrite_global_commands_typed => bulk_overwrite_global_commands(
            &[sample_command()]
        )),
        forward!(BulkOverwriteGuildApplicationCommands, bulk_overwrite_guild_commands_typed => bulk_overwrite_guild_commands(
            test_id(200),
            &[sample_command()]
        )),
        forward!(GetCurrentUserGuilds, get_current_user_guilds_typed => get_current_user_guilds()),
        forward!(GetCurrentUserGuilds, get_current_user_guilds_typed_with_query => get_current_user_guilds_with_query(
            &m::CurrentUserGuildsQuery::default()
        )),
        forward!(CreateWebhook, create_webhook_from_request => create_webhook(
            test_id(100),
            &r::CreateWebhook {
                name: "golden".to_string(),
                avatar: None,
            }
        )),
        forward!(GetChannelWebhooks, get_channel_webhooks_typed => get_channel_webhooks(test_id(100))),
        forward!(ModifyWebhook, modify_webhook_from_request => modify_webhook(
            test_id(700),
            &r::ModifyWebhook::default()
        )),
        forward!(ModifyWebhookWithToken, modify_webhook_with_token_from_request => modify_webhook_with_token(
            test_id(700),
            WEBHOOK_TOKEN,
            &r::ModifyWebhookWithToken::default()
        )),
    ]
}

/// Sends one case against a fresh recording server and returns what the
/// server saw (`None` when nothing was sent, for example because the body
/// failed to encode).
async fn record(case: &Case) -> Option<RecordedRequest> {
    let (base_url, captured, server) =
        spawn_test_server(vec![PlannedResponse::json(StatusCode::OK, json!({}))]).await;
    // The `{}` answer does not decode into most response types; only the
    // request matters, so the result is discarded on purpose.
    (case.call)(base_url).await.ok();
    server.abort();
    let request = captured.lock().ok()?.first().cloned();
    request
}

/// The boundary reqwest generated for a multipart request, so the rendering
/// can replace it.
fn boundary(request: &RecordedRequest) -> Option<String> {
    request
        .header("content-type")?
        .split("boundary=")
        .nth(1)
        .map(str::to_string)
}

fn render(case: &Case, request: Option<&RecordedRequest>) -> String {
    let mut out = format!("### {} · {}\n", case.route.name(), case.api);
    let Some(request) = request else {
        out.push_str("(no request)\n");
        return out;
    };
    out.push_str(&format!("{} {}\n", request.method, request.path));
    out.push_str(&format!(
        "authorization: {}\n",
        request.header("authorization").unwrap_or("-")
    ));
    let content_type = request.header("content-type").unwrap_or("-");
    let boundary = boundary(request);
    match &boundary {
        Some(boundary) => {
            out.push_str(&format!(
                "content-type: {}\n",
                content_type.replace(boundary.as_str(), "BOUNDARY")
            ));
        }
        None => out.push_str(&format!("content-type: {content_type}\n")),
    }
    let body = match &boundary {
        Some(boundary) => request.body.replace(boundary.as_str(), "BOUNDARY"),
        None => request.body.clone(),
    };
    let body = body.replace("\r\n", "\n");
    if body.is_empty() {
        out.push_str("body: -\n");
    } else {
        out.push_str("body:\n");
        out.push_str(&body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// True when `path` (no query) has `template`'s literal segments and one
/// segment per placeholder.
fn matches_template(template: &str, path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    let template_segments = template.split('/').collect::<Vec<_>>();
    let path_segments = path.split('/').collect::<Vec<_>>();
    template_segments.len() == path_segments.len()
        && template_segments
            .iter()
            .zip(&path_segments)
            .all(|(template, path)| {
                if template.starts_with('{') && template.ends_with('}') {
                    !path.is_empty()
                } else {
                    template == path
                }
            })
}

#[tokio::test]
async fn every_route_matches_its_golden_request() {
    let cases = cases();
    let mut rendered = String::new();
    let mut problems = Vec::new();
    for case in &cases {
        let request = record(case).await;
        // A blank line between cases, none after the last (`git diff --check`).
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&render(case, request.as_ref()));
        let Some(request) = request else {
            problems.push(format!(
                "{} · {}: nothing was sent",
                case.route.name(),
                case.api
            ));
            continue;
        };
        if request.method != case.route.method().as_str() {
            problems.push(format!(
                "{} · {}: sent {} but the manifest says {}",
                case.route.name(),
                case.api,
                request.method,
                case.route.method()
            ));
        }
        if !matches_template(case.route.template(), &request.path) {
            problems.push(format!(
                "{} · {}: path {} does not fit {}",
                case.route.name(),
                case.api,
                request.path,
                case.route.template()
            ));
        }
        let authorization = request.header("authorization");
        let expected = match case.route.auth() {
            RouteAuth::Bot => Some(format!("Bot {BOT_TOKEN}")),
            RouteAuth::Bearer => Some(format!("Bearer {BEARER}")),
            RouteAuth::Token | RouteAuth::Interaction | RouteAuth::None | RouteAuth::Client => None,
        };
        if authorization != expected.as_deref() {
            problems.push(format!(
                "{} · {}: authorization {:?}, manifest expects {:?}",
                case.route.name(),
                case.api,
                authorization,
                expected
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    if std::env::var_os("DISCORDRS_UPDATE_GOLDENS").is_some() {
        std::fs::write(GOLDEN_FILE, &rendered).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(GOLDEN_FILE).unwrap_or_default();
    if golden != rendered {
        let first = golden
            .lines()
            .zip(rendered.lines())
            .position(|(golden, rendered)| golden != rendered)
            .unwrap_or(0);
        panic!(
            "the requests differ from {GOLDEN_FILE} (first difference at line {}):\n  golden:   {:?}\n  rendered: {:?}\nRegenerate with DISCORDRS_UPDATE_GOLDENS=1 if the change is intended.",
            first + 1,
            golden.lines().nth(first),
            rendered.lines().nth(first)
        );
    }
}

#[test]
fn every_route_and_listed_method_has_a_golden() {
    let cases = cases();
    let covered_routes = cases.iter().map(|case| case.route).collect::<HashSet<_>>();
    let missing_routes = RouteId::ALL
        .iter()
        .filter(|route| !covered_routes.contains(route))
        .map(|route| route.name())
        .collect::<Vec<_>>();
    assert!(
        missing_routes.is_empty(),
        "routes without a golden: {missing_routes:?}"
    );

    for case in &cases {
        let row = MANIFEST.iter().find(|row| row.id == case.route).unwrap();
        assert!(
            row.api.contains(&case.api),
            "{} is not listed under {}",
            case.api,
            case.route.name()
        );
    }

    // Every method a row lists has its own golden, except the crate-private
    // callback senders behind the public interaction response methods.
    let called = cases
        .iter()
        .map(|case| (case.route, case.api))
        .collect::<HashSet<_>>();
    let exempt = [
        "post_interaction_callback",
        "post_interaction_callback_with_result",
    ];
    let uncovered = MANIFEST
        .iter()
        .flat_map(|row| row.api.iter().map(move |api| (row.id, *api)))
        .filter(|(route, api)| !called.contains(&(*route, *api)) && !exempt.contains(api))
        .map(|(route, api)| format!("{} · {api}", route.name()))
        .collect::<Vec<_>>();
    assert!(
        uncovered.is_empty(),
        "methods without a golden: {uncovered:?}"
    );
}

#[tokio::test]
async fn deprecated_forwards_send_what_their_replacement_sends() {
    let forwards = forwards();
    let mut forwarded = HashSet::new();
    for (old, new) in &forwards {
        let old_request = record(old).await;
        let new_request = record(new).await;
        assert!(old_request.is_some(), "{} sent nothing", old.api);
        // Everything but the header line (which names the method).
        let body = |case: &Case, request: Option<&RecordedRequest>| {
            render(case, request)
                .split_once('\n')
                .map(|(_, rest)| rest.to_string())
                .unwrap_or_default()
        };
        assert_eq!(
            body(old, old_request.as_ref()),
            body(new, new_request.as_ref()),
            "{} and {} differ",
            old.api,
            new.api
        );
        forwarded.insert(old.api);
    }

    // Every deprecated forward the manifest records under `was` is compared.
    let recorded = MANIFEST
        .iter()
        .flat_map(|row| row.was.iter().map(|(old, _)| *old))
        .collect::<HashSet<_>>();
    for api in &forwarded {
        assert!(recorded.contains(api), "{api} is not recorded under `was`");
    }
}

#[test]
fn templates_match_their_own_shape_only() {
    assert!(matches_template("/channels/{channel.id}", "/channels/1"));
    assert!(matches_template(
        "/channels/{channel.id}/messages",
        "/channels/1/messages?limit=2"
    ));
    assert!(!matches_template(
        "/channels/{channel.id}",
        "/channels/1/messages"
    ));
    assert!(!matches_template(
        "/channels/{channel.id}/pins",
        "/channels/1/messages"
    ));
    assert!(!matches_template("/channels/{channel.id}", "/channels/"));
}
