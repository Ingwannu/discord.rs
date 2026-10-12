//! The route manifest: every REST route the crate sends, with a stable ID.
//!
//! Each row names one Discord operation: the HTTP method, the path template
//! in the docs' spelling (`{channel.id}`), how the request is authorized,
//! the query parameters the crate can send, the request and response types,
//! and the public methods that send it. Endpoint methods pass their
//! [`RouteId`] to the transport, which takes the method and the
//! authorization from this table instead of from the call site, and
//! `ci/api_drift.py` reads the same rows (it parses this file) to compare the
//! crate with the Discord docs. Changing a route therefore changes one row
//! that both the runtime and the drift report see.
//!
//! [Decision Log] Route manifest
//! - Purpose: one source of route metadata for execution and for the weekly
//!   drift report, and a stable ID per route so that renaming or removing a
//!   method cannot silently drop a route (3.0 phase 5,
//!   docs/design/3.0-types-events-requests.md §4).
//! - Before: `RequestRoute::new(method, path)` derived everything from the
//!   formatted path at runtime, the replay rule matched the path shape of
//!   `POST /channels/{id}/messages`, authorization was guessed from the path
//!   (`request_uses_bot_authorization`), and the drift script recovered
//!   routes by scanning `Method::X, "/literal"` pairs plus a hand-kept list of
//!   helper-built paths.
//! - Alternatives: (1) keep scanning call sites; (2) a generated JSON dump
//!   checked in by a test; (3) a declarative table in Rust that the script
//!   parses as text; (4) a build script.
//! - Chosen: (3). The `route_manifest!` rows generate [`RouteId`] and its
//!   accessors; under `cfg(test)` they also generate a compile check that
//!   every listed type exists and a `MANIFEST` the tests compare with the
//!   goldens. The script reads the rows with one regular
//!   expression per row and checks the other direction (every endpoint
//!   method is listed, and each listed method uses its route).
//! - Why: (1) cannot see helper-built paths or tell two operations on one
//!   path apart (Create DM / Create Group DM, Execute Webhook / Create
//!   Followup Message). (2) can go stale between a code change and the next
//!   test run and needs a regeneration step. (4) costs build time for every
//!   user. A table needs no new tooling, and the compiler rejects a row that
//!   names a type that does not exist.
//! - Trade-offs: the rows have a fixed one-line shape that the script
//!   parses, so a row must stay on one line. Only the method, template, and
//!   authorization exist at runtime (a `match` per accessor); the query,
//!   type, and method lists are test-only text.
//!
//! Row shape (one line each, parsed by `ci/api_drift.py`):
//!
//! ```text
//! Id: METHOD "/docs/{template}" auth, query [names], body Type => Response, api [methods];
//! ```
//!
//! - `auth`: `bot` (the client's bot token), `bearer` (an OAuth2 access token
//!   the caller passes), `token` (the webhook or interaction token in the
//!   path; no `Authorization` header), `none` (a public route), or `client`
//!   (OAuth2 client credentials in a form body).
//! - `query`: the query parameter names the crate can send; `[]` for none.
//! - `body`: the request body type, `()` for none.
//! - `api [..]` lists [`RestClient`](super::RestClient) methods,
//!   `oauth2 [..]` lists [`OAuth2Client`](crate::oauth2::OAuth2Client) methods.
//! - An optional trailing `was [old => new, ..]` records 2.x names removed in
//!   3.0 and the method that replaces each one; the migration table in the
//!   docs is generated from it.

use reqwest::Method;

/// How the transport authorizes a route; see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RouteAuth {
    /// `Authorization: Bot <token>`.
    Bot,
    /// `Authorization: Bearer <access token>` supplied by the caller.
    Bearer,
    /// The secret is the token segment of the path; no header.
    Token,
    /// A public route; no header.
    None,
    /// OAuth2 client credentials in the form body (`OAuth2Client`).
    Client,
}

macro_rules! route_manifest {
    ($(
        $(#[$meta:meta])*
        $id:ident : $method:ident $path:literal $auth:ident,
            query [$($query:tt),* $(,)?],
            body $body:ty => $response:ty,
            $client:ident [$($api:ident),+ $(,)?]
            $(, was [$($was:ident => $now:ident),+ $(,)?])?;
    )+) => {
        /// Stable identity of one REST route. See the module documentation.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum RouteId {
            $($(#[$meta])* $id,)+
        }

        impl RouteId {
            /// Every route, in manifest order.
            #[cfg(test)]
            pub(crate) const ALL: &'static [RouteId] = &[$(RouteId::$id,)+];

            /// The HTTP method this route is sent with.
            pub(crate) fn method(self) -> Method {
                match self {
                    $(RouteId::$id => Method::$method,)+
                }
            }

            /// The path template in the docs' spelling, for diagnostics and
            /// tests; requests use the path the endpoint formatted.
            pub(crate) const fn template(self) -> &'static str {
                match self {
                    $(RouteId::$id => $path,)+
                }
            }

            /// How the transport authorizes this route.
            pub(crate) const fn auth(self) -> RouteAuth {
                match self {
                    $(RouteId::$id => route_auth!($auth),)+
                }
            }

            /// The row's identifier.
            #[cfg(test)]
            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $(RouteId::$id => stringify!($id),)+
                }
            }
        }

        /// The test-only half of a row: everything the runtime does not need.
        #[cfg(test)]
        pub(crate) struct ManifestRow {
            pub(crate) id: RouteId,
            pub(crate) query: &'static [&'static str],
            pub(crate) body: &'static str,
            pub(crate) response: &'static str,
            pub(crate) client: &'static str,
            pub(crate) api: &'static [&'static str],
            pub(crate) was: &'static [(&'static str, &'static str)],
        }

        /// Every row of the manifest, in order.
        #[cfg(test)]
        pub(crate) const MANIFEST: &[ManifestRow] = &[$(
            ManifestRow {
                id: RouteId::$id,
                query: &[$(stringify!($query)),*],
                body: stringify!($body),
                response: stringify!($response),
                client: stringify!($client),
                api: &[$(stringify!($api)),+],
                was: &[$($((stringify!($was), stringify!($now))),+)?],
            },
        )+];

        /// Fails to compile when a row names a type that does not exist. The
        /// method names are checked by `ci/api_drift.py --self-check`
        /// (generic methods cannot be named without their type arguments).
        #[cfg(test)]
        #[expect(dead_code, reason = "a compile-time check, never called")]
        fn manifest_names_resolve() {
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use crate::model::*;
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use crate::command::CommandDefinition;
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use crate::http::FileAttachment;
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use crate::types::Emoji;
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use crate::oauth2::{OAuth2CodeExchange, OAuth2TokenResponse};
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use serde_json::Value;
            #[allow(unused_imports, reason = "each row uses a different subset")]
            use std::collections::HashMap;
            $(
                let _: Option<(&$body, &$response)> = None;
            )+
        }
    };
}

macro_rules! route_auth {
    (bot) => {
        RouteAuth::Bot
    };
    (bearer) => {
        RouteAuth::Bearer
    };
    (token) => {
        RouteAuth::Token
    };
    (none) => {
        RouteAuth::None
    };
    (client) => {
        RouteAuth::Client
    };
}

impl RouteId {
    /// True for the one route whose `nonce` + `enforce_nonce: true` contract
    /// makes a replay safe (Discord returns the first message for a repeated
    /// nonce); see `ReplayPolicy` in `http.rs`.
    pub(crate) const fn replays_with_nonce(self) -> bool {
        matches!(self, RouteId::CreateMessage)
    }
}

route_manifest! {
    // --- Application -----------------------------------------------------
    GetCurrentApplication: GET "/applications/@me" bot, query [], body () => Application, api [get_current_application];
    EditCurrentApplication: PATCH "/applications/@me" bot, query [], body ModifyCurrentApplication => Application, api [edit_current_application, edit_current_application_from_request];
    GetApplicationActivityInstance: GET "/applications/{application.id}/activity-instances/{instance_id}" bot, query [], body () => ActivityInstance, api [get_application_activity_instance];
    GetApplicationRoleConnectionMetadataRecords: GET "/applications/{application.id}/role-connections/metadata" bot, query [], body () => Vec<ApplicationRoleConnectionMetadata>, api [get_application_role_connection_metadata_records];
    UpdateApplicationRoleConnectionMetadataRecords: PUT "/applications/{application.id}/role-connections/metadata" bot, query [], body [ApplicationRoleConnectionMetadata] => Vec<ApplicationRoleConnectionMetadata>, api [update_application_role_connection_metadata_records];
    GetCurrentBotApplicationInformation: GET "/oauth2/applications/@me" bot, query [], body () => Application, api [get_current_bot_application_information];

    // --- Application identities (Social SDK) --------------------------------
    GetApplicationIdentityProfile: GET "/applications/{application.id}/users/{user.id}/identities/{provider_issued_user_id}/profile" bot, query [], body () => ApplicationIdentityProfile, api [get_application_identity_profile];
    UpdateApplicationIdentityProfile: PATCH "/applications/{application.id}/users/{user.id}/identities/{provider_issued_user_id}/profile" bot, query [], body UpdateApplicationIdentityProfile => Option<ApplicationIdentityProfile>, api [update_application_identity_profile];
    GetApplicationIdentitiesByUserId: GET "/users/{user_id}/application-identities/{application_id}" bot, query [], body () => ApplicationIdentities, api [get_user_application_identities];
    GetApplicationIdentitiesByExternalId: GET "/applications/{application_id}/application-identities/{provider_type}/{provider_issued_user_id}" bot, query [provider_id], body () => ApplicationIdentities, api [get_application_identities_by_external_id];
    DeleteApplicationIdentity: POST "/users/{user_id}/application-identities/{application_id}/{provider_type}/{provider_issued_user_id}/delete" bot, query [provider_id], body () => (), api [delete_application_identity];

    // --- Application commands -------------------------------------------
    GetGlobalApplicationCommands: GET "/applications/{application.id}/commands" bot, query [], body () => Vec<ApplicationCommand>, api [get_global_commands];
    CreateGlobalApplicationCommand: POST "/applications/{application.id}/commands" bot, query [], body CommandDefinition => ApplicationCommand, api [create_global_command];
    BulkOverwriteGlobalApplicationCommands: PUT "/applications/{application.id}/commands" bot, query [], body [CommandDefinition] => Vec<ApplicationCommand>, api [bulk_overwrite_global_commands_typed];
    GetGlobalApplicationCommand: GET "/applications/{application.id}/commands/{command.id}" bot, query [], body () => ApplicationCommand, api [get_global_command];
    EditGlobalApplicationCommand: PATCH "/applications/{application.id}/commands/{command.id}" bot, query [], body Value => ApplicationCommand, api [edit_global_command];
    DeleteGlobalApplicationCommand: DELETE "/applications/{application.id}/commands/{command.id}" bot, query [], body () => (), api [delete_global_command];
    GetGuildApplicationCommands: GET "/applications/{application.id}/guilds/{guild.id}/commands" bot, query [], body () => Vec<ApplicationCommand>, api [get_guild_commands];
    CreateGuildApplicationCommand: POST "/applications/{application.id}/guilds/{guild.id}/commands" bot, query [], body CommandDefinition => ApplicationCommand, api [create_guild_command];
    BulkOverwriteGuildApplicationCommands: PUT "/applications/{application.id}/guilds/{guild.id}/commands" bot, query [], body [CommandDefinition] => Vec<ApplicationCommand>, api [bulk_overwrite_guild_commands_typed];
    GetGuildApplicationCommand: GET "/applications/{application.id}/guilds/{guild.id}/commands/{command.id}" bot, query [], body () => ApplicationCommand, api [get_guild_command];
    EditGuildApplicationCommand: PATCH "/applications/{application.id}/guilds/{guild.id}/commands/{command.id}" bot, query [], body Value => ApplicationCommand, api [edit_guild_command];
    DeleteGuildApplicationCommand: DELETE "/applications/{application.id}/guilds/{guild.id}/commands/{command.id}" bot, query [], body () => (), api [delete_guild_command];
    GetGuildApplicationCommandPermissions: GET "/applications/{application.id}/guilds/{guild.id}/commands/permissions" bot, query [], body () => Vec<GuildApplicationCommandPermissions>, api [get_guild_application_command_permissions];
    GetApplicationCommandPermissions: GET "/applications/{application.id}/guilds/{guild.id}/commands/{command.id}/permissions" bot, query [], body () => GuildApplicationCommandPermissions, api [get_application_command_permissions];
    EditApplicationCommandPermissions: PUT "/applications/{application.id}/guilds/{guild.id}/commands/{command.id}/permissions" bearer, query [], body EditApplicationCommandPermissions => GuildApplicationCommandPermissions, api [edit_application_command_permissions];
    BatchEditApplicationCommandPermissions: PUT "/applications/{application.id}/guilds/{guild.id}/commands/permissions" bearer, query [], body Value => Vec<GuildApplicationCommandPermissions>, api [batch_edit_application_command_permissions];

    // --- Interactions (worker C owns the methods) ----------------------------
    CreateInteractionResponse: POST "/interactions/{interaction.id}/{interaction.token}/callback" token, query [with_response], body InteractionCallbackResponse => Option<InteractionCallbackResult>, api [create_interaction_response_typed, create_interaction_response_json, create_interaction_response_with_files, create_interaction_response_with_result];
    GetOriginalInteractionResponse: GET "/webhooks/{application.id}/{interaction.token}/messages/@original" token, query [], body () => Message, api [get_original_interaction_response, get_original_interaction_response_with_application_id];
    EditOriginalInteractionResponse: PATCH "/webhooks/{application.id}/{interaction.token}/messages/@original" token, query [], body CreateMessage => Message, api [edit_original_interaction_response, edit_original_interaction_response_with_files, edit_original_interaction_response_with_application_id, edit_original_interaction_response_with_application_id_and_files];
    DeleteOriginalInteractionResponse: DELETE "/webhooks/{application.id}/{interaction.token}/messages/@original" token, query [], body () => (), api [delete_original_interaction_response, delete_original_interaction_response_with_application_id];
    CreateFollowupMessage: POST "/webhooks/{application.id}/{interaction.token}" token, query [], body CreateMessage => Message, api [create_followup_message, create_followup_message_with_files, create_followup_message_with_application_id, create_followup_message_with_application_id_and_files, create_followup_message_json, create_followup_message_json_with_application_id];
    EditFollowupMessage: PATCH "/webhooks/{application.id}/{interaction.token}/messages/{message.id}" token, query [], body CreateMessage => Message, api [edit_followup_message, edit_followup_message_with_files, edit_followup_message_with_application_id, edit_followup_message_with_application_id_and_files];
    DeleteFollowupMessage: DELETE "/webhooks/{application.id}/{interaction.token}/messages/{message.id}" token, query [], body () => (), api [delete_followup_message, delete_followup_message_with_application_id];

    // --- Audit log and auto moderation ------------------------------------
    GetGuildAuditLog: GET "/guilds/{guild.id}/audit-logs" bot, query [user_id, action_type, before, after, limit], body () => AuditLog, api [get_guild_audit_log, get_guild_audit_log_typed];
    ListAutoModerationRulesForGuild: GET "/guilds/{guild.id}/auto-moderation/rules" bot, query [], body () => Vec<AutoModerationRule>, api [get_auto_moderation_rules, get_auto_moderation_rules_typed];
    GetAutoModerationRule: GET "/guilds/{guild.id}/auto-moderation/rules/{auto_moderation_rule.id}" bot, query [], body () => AutoModerationRule, api [get_auto_moderation_rule];
    CreateAutoModerationRule: POST "/guilds/{guild.id}/auto-moderation/rules" bot, query [], body Value => AutoModerationRule, api [create_auto_moderation_rule, create_auto_moderation_rule_typed];
    ModifyAutoModerationRule: PATCH "/guilds/{guild.id}/auto-moderation/rules/{auto_moderation_rule.id}" bot, query [], body Value => AutoModerationRule, api [modify_auto_moderation_rule, modify_auto_moderation_rule_typed];
    DeleteAutoModerationRule: DELETE "/guilds/{guild.id}/auto-moderation/rules/{auto_moderation_rule.id}" bot, query [], body () => (), api [delete_auto_moderation_rule];

    // --- Channels ---------------------------------------------------------
    GetChannel: GET "/channels/{channel.id}" bot, query [], body () => Channel, api [get_channel];
    ModifyChannel: PATCH "/channels/{channel.id}" bot, query [], body Value => Channel, api [update_channel, update_channel_typed];
    DeleteChannel: DELETE "/channels/{channel.id}" bot, query [], body () => Channel, api [delete_channel];
    EditChannelPermissions: PUT "/channels/{channel.id}/permissions/{overwrite.id}" bot, query [], body EditChannelPermission => (), api [edit_channel_permissions, edit_channel_permissions_typed];
    DeleteChannelPermission: DELETE "/channels/{channel.id}/permissions/{overwrite.id}" bot, query [], body () => (), api [delete_channel_permission];
    GetChannelInvites: GET "/channels/{channel.id}/invites" bot, query [], body () => Vec<Invite>, api [get_channel_invites];
    CreateChannelInvite: POST "/channels/{channel.id}/invites" bot, query [], body CreateChannelInvite => Invite, api [create_channel_invite, create_channel_invite_typed, create_channel_invite_with_target_users_file];
    FollowAnnouncementChannel: POST "/channels/{channel.id}/followers" bot, query [], body ChannelId => FollowedChannel, api [follow_announcement_channel];
    TriggerTypingIndicator: POST "/channels/{channel.id}/typing" bot, query [], body () => (), api [trigger_typing_indicator];
    GroupDmAddRecipient: PUT "/channels/{channel.id}/recipients/{user.id}" bot, query [], body AddGroupDmRecipient => (), api [add_group_dm_recipient];
    GroupDmRemoveRecipient: DELETE "/channels/{channel.id}/recipients/{user.id}" bot, query [], body () => (), api [remove_group_dm_recipient];
    GetGuildChannels: GET "/guilds/{guild.id}/channels" bot, query [], body () => Vec<Channel>, api [get_guild_channels];
    CreateGuildChannel: POST "/guilds/{guild.id}/channels" bot, query [], body CreateGuildChannel => Channel, api [create_guild_channel, create_guild_channel_typed, create_guild_channel_from_request];
    ModifyGuildChannelPositions: PATCH "/guilds/{guild.id}/channels" bot, query [], body [ModifyGuildChannelPosition] => (), api [modify_guild_channel_positions];
    CreateDm: POST "/users/@me/channels" bot, query [], body CreateDmChannel => Channel, api [create_dm_channel_typed];
    CreateGroupDm: POST "/users/@me/channels" bot, query [], body CreateGroupDmChannel => Channel, api [create_group_dm_channel_typed];
    SetVoiceChannelStatus: PUT "/channels/{channel.id}/voice-status" bot, query [], body SetVoiceChannelStatus => (), api [set_voice_channel_status];

    // --- Threads ----------------------------------------------------------
    StartThreadFromMessage: POST "/channels/{channel.id}/messages/{message.id}/threads" bot, query [], body Value => Channel, api [create_thread_from_message];
    StartThreadWithoutMessage: POST "/channels/{channel.id}/threads" bot, query [], body Value => Channel, api [create_thread];
    JoinThread: PUT "/channels/{channel.id}/thread-members/@me" bot, query [], body () => (), api [join_thread];
    AddThreadMember: PUT "/channels/{channel.id}/thread-members/{user.id}" bot, query [], body () => (), api [add_thread_member];
    LeaveThread: DELETE "/channels/{channel.id}/thread-members/@me" bot, query [], body () => (), api [leave_thread];
    RemoveThreadMember: DELETE "/channels/{channel.id}/thread-members/{user.id}" bot, query [], body () => (), api [remove_thread_member];
    GetThreadMember: GET "/channels/{channel.id}/thread-members/{user.id}" bot, query [with_member], body () => ThreadMember, api [get_thread_member];
    ListThreadMembers: GET "/channels/{channel.id}/thread-members" bot, query [with_member, after, limit], body () => Vec<ThreadMember>, api [list_thread_members, get_thread_members];
    ListPublicArchivedThreads: GET "/channels/{channel.id}/threads/archived/public" bot, query [before, limit], body () => ThreadListResponse, api [list_public_archived_threads, get_public_archived_threads];
    ListPrivateArchivedThreads: GET "/channels/{channel.id}/threads/archived/private" bot, query [before, limit], body () => ThreadListResponse, api [list_private_archived_threads];
    ListJoinedPrivateArchivedThreads: GET "/channels/{channel.id}/users/@me/threads/archived/private" bot, query [before, limit], body () => ThreadListResponse, api [list_joined_private_archived_threads];
    ListActiveGuildThreads: GET "/guilds/{guild.id}/threads/active" bot, query [], body () => ThreadListResponse, api [get_active_guild_threads];

    // --- Messages, reactions, pins, polls (worker A owns the bodies) --------
    GetChannelMessages: GET "/channels/{channel.id}/messages" bot, query [around, before, after, limit], body () => Vec<Message>, api [get_channel_messages, get_channel_messages_paginated];
    GetChannelMessage: GET "/channels/{channel.id}/messages/{message.id}" bot, query [], body () => Message, api [get_message];
    CreateMessage: POST "/channels/{channel.id}/messages" bot, query [], body CreateMessage => Message, api [create_message, create_message_with_files, forward_message, send_message_json];
    CrosspostMessage: POST "/channels/{channel.id}/messages/{message.id}/crosspost" bot, query [], body () => Message, api [crosspost_message];
    EditMessage: PATCH "/channels/{channel.id}/messages/{message.id}" bot, query [], body CreateMessage => Message, api [update_message, update_message_with_files, edit_message_json];
    DeleteMessage: DELETE "/channels/{channel.id}/messages/{message.id}" bot, query [], body () => (), api [delete_message];
    BulkDeleteMessages: POST "/channels/{channel.id}/messages/bulk-delete" bot, query [], body [MessageId] => (), api [bulk_delete_messages];
    SearchGuildMessages: GET "/guilds/{guild.id}/messages/search" bot, query [limit, offset, max_id, min_id, slop, content, channel_id, author_type, author_id, mentions, mentions_role_id, mention_everyone, replied_to_user_id, replied_to_message_id, pinned, has, embed_type, embed_provider, link_hostname, attachment_filename, attachment_extension, sort_by, sort_order, include_nsfw], body () => SearchGuildMessagesResponse, api [search_guild_messages];
    CreateReaction: PUT "/channels/{channel.id}/messages/{message.id}/reactions/{emoji.id}/@me" bot, query [], body () => (), api [add_reaction];
    DeleteOwnReaction: DELETE "/channels/{channel.id}/messages/{message.id}/reactions/{emoji.id}/@me" bot, query [], body () => (), api [remove_reaction];
    DeleteUserReaction: DELETE "/channels/{channel.id}/messages/{message.id}/reactions/{emoji.id}/{user.id}" bot, query [], body () => (), api [remove_user_reaction];
    GetReactions: GET "/channels/{channel.id}/messages/{message.id}/reactions/{emoji.id}" bot, query [type, after, limit], body () => Vec<User>, api [get_reactions, get_reactions_with_query];
    DeleteAllReactions: DELETE "/channels/{channel.id}/messages/{message.id}/reactions" bot, query [], body () => (), api [remove_all_reactions];
    DeleteAllReactionsForEmoji: DELETE "/channels/{channel.id}/messages/{message.id}/reactions/{emoji.id}" bot, query [], body () => (), api [remove_all_reactions_for_emoji];
    GetChannelPins: GET "/channels/{channel.id}/messages/pins" bot, query [before, limit], body () => ChannelPins, api [get_channel_pins];
    PinMessage: PUT "/channels/{channel.id}/messages/pins/{message.id}" bot, query [], body () => (), api [pin_message];
    UnpinMessage: DELETE "/channels/{channel.id}/messages/pins/{message.id}" bot, query [], body () => (), api [unpin_message];
    GetPinnedMessagesDeprecated: GET "/channels/{channel.id}/pins" bot, query [], body () => Vec<Message>, api [get_pinned_messages];
    PinMessageDeprecated: PUT "/channels/{channel.id}/pins/{message.id}" bot, query [], body () => (), api [pin_message_legacy];
    UnpinMessageDeprecated: DELETE "/channels/{channel.id}/pins/{message.id}" bot, query [], body () => (), api [unpin_message_legacy];
    GetAnswerVoters: GET "/channels/{channel.id}/polls/{message.id}/answers/{answer_id}" bot, query [after, limit], body () => PollAnswerVoters, api [get_poll_answer_voters];
    EndPoll: POST "/channels/{channel.id}/polls/{message.id}/expire" bot, query [], body () => Message, api [end_poll];

    // --- Emoji --------------------------------------------------------------
    ListGuildEmojis: GET "/guilds/{guild.id}/emojis" bot, query [], body () => Vec<Emoji>, api [get_guild_emojis, get_guild_emojis_typed];
    GetGuildEmoji: GET "/guilds/{guild.id}/emojis/{emoji.id}" bot, query [], body () => Emoji, api [get_guild_emoji, get_guild_emoji_typed];
    CreateGuildEmoji: POST "/guilds/{guild.id}/emojis" bot, query [], body Value => Emoji, api [create_guild_emoji, create_guild_emoji_typed];
    ModifyGuildEmoji: PATCH "/guilds/{guild.id}/emojis/{emoji.id}" bot, query [], body Value => Emoji, api [modify_guild_emoji, modify_guild_emoji_typed];
    DeleteGuildEmoji: DELETE "/guilds/{guild.id}/emojis/{emoji.id}" bot, query [], body () => (), api [delete_guild_emoji];
    ListApplicationEmojis: GET "/applications/{application.id}/emojis" bot, query [], body () => Vec<Emoji>, api [get_application_emojis, get_application_emojis_typed];
    GetApplicationEmoji: GET "/applications/{application.id}/emojis/{emoji.id}" bot, query [], body () => Emoji, api [get_application_emoji, get_application_emoji_typed];
    CreateApplicationEmoji: POST "/applications/{application.id}/emojis" bot, query [], body Value => Emoji, api [create_application_emoji, create_application_emoji_typed];
    ModifyApplicationEmoji: PATCH "/applications/{application.id}/emojis/{emoji.id}" bot, query [], body Value => Emoji, api [modify_application_emoji, modify_application_emoji_typed];
    DeleteApplicationEmoji: DELETE "/applications/{application.id}/emojis/{emoji.id}" bot, query [], body () => (), api [delete_application_emoji];

    // --- Entitlements and SKUs ------------------------------------------------
    ListSkus: GET "/applications/{application.id}/skus" bot, query [], body () => Vec<Sku>, api [get_skus];
    ListSkuSubscriptions: GET "/skus/{sku.id}/subscriptions" bot, query [before, after, limit, user_id], body () => Vec<Subscription>, api [get_sku_subscriptions];
    GetSkuSubscription: GET "/skus/{sku.id}/subscriptions/{subscription.id}" bot, query [], body () => Subscription, api [get_sku_subscription];
    ListEntitlements: GET "/applications/{application.id}/entitlements" bot, query [user_id, sku_ids, before, after, limit, guild_id, exclude_ended, exclude_deleted], body () => Vec<Entitlement>, api [get_entitlements];
    GetEntitlement: GET "/applications/{application.id}/entitlements/{entitlement.id}" bot, query [], body () => Entitlement, api [get_entitlement];
    ConsumeEntitlement: POST "/applications/{application.id}/entitlements/{entitlement.id}/consume" bot, query [], body () => (), api [consume_entitlement];
    CreateTestEntitlement: POST "/applications/{application.id}/entitlements" bot, query [], body CreateTestEntitlement => Entitlement, api [create_test_entitlement];
    DeleteTestEntitlement: DELETE "/applications/{application.id}/entitlements/{entitlement.id}" bot, query [], body () => (), api [delete_test_entitlement];

    // --- Gateway ----------------------------------------------------------
    GetGateway: GET "/gateway" bot, query [], body () => Gateway, api [get_gateway];
    GetGatewayBot: GET "/gateway/bot" bot, query [], body () => GatewayBot, api [get_gateway_bot];

    // --- Guilds -----------------------------------------------------------
    GetGuild: GET "/guilds/{guild.id}" bot, query [with_counts], body () => Guild, api [get_guild, get_guild_with_query];
    GetGuildPreview: GET "/guilds/{guild.id}/preview" bot, query [], body () => GuildPreview, api [get_guild_preview, get_guild_preview_typed];
    ModifyGuild: PATCH "/guilds/{guild.id}" bot, query [], body ModifyGuild => Guild, api [modify_guild, update_guild, update_guild_typed];
    ModifyGuildIncidentActions: PUT "/guilds/{guild.id}/incident-actions" bot, query [], body ModifyGuildIncidentActions => GuildIncidentsData, api [modify_guild_incident_actions];
    CreateGuild: POST "/guilds" bot, query [], body CreateGuild => Guild, api [create_guild];
    CreateGuildFromTemplate: POST "/guilds/templates/{template.code}" bot, query [], body CreateGuildFromTemplate => Guild, api [create_guild_from_template];
    DeleteGuild: DELETE "/guilds/{guild.id}" bot, query [], body () => (), api [delete_guild];
    ModifyGuildMfaLevel: POST "/guilds/{guild.id}/mfa" bot, query [], body GuildMfaLevel => GuildMfaLevel, api [modify_guild_mfa_level];
    GetGuildIntegrations: GET "/guilds/{guild.id}/integrations" bot, query [], body () => Vec<Integration>, api [get_guild_integrations];
    DeleteGuildIntegration: DELETE "/guilds/{guild.id}/integrations/{integration.id}" bot, query [], body () => (), api [delete_guild_integration];
    GetGuildPruneCount: GET "/guilds/{guild.id}/prune" bot, query [days, include_roles], body () => GuildPruneCount, api [get_guild_prune_count, get_guild_prune_count_typed];
    BeginGuildPrune: POST "/guilds/{guild.id}/prune" bot, query [days, compute_prune_count, include_roles], body BeginGuildPruneRequest => GuildPruneResult, api [begin_guild_prune, begin_guild_prune_typed, begin_guild_prune_with_body, begin_guild_prune_with_request];
    GetGuildVoiceRegions: GET "/guilds/{guild.id}/regions" bot, query [], body () => Vec<VoiceRegion>, api [get_guild_voice_regions];
    GetGuildInvites: GET "/guilds/{guild.id}/invites" bot, query [], body () => Vec<Invite>, api [get_guild_invites];
    GetGuildVanityUrl: GET "/guilds/{guild.id}/vanity-url" bot, query [], body () => VanityUrl, api [get_guild_vanity_url];
    GetGuildWidgetSettings: GET "/guilds/{guild.id}/widget" bot, query [], body () => GuildWidgetSettings, api [get_guild_widget_settings];
    ModifyGuildWidget: PATCH "/guilds/{guild.id}/widget" bot, query [], body ModifyGuildWidgetSettings => GuildWidgetSettings, api [modify_guild_widget_settings, modify_guild_widget_settings_typed, modify_guild_widget];
    GetGuildWidget: GET "/guilds/{guild.id}/widget.json" bot, query [], body () => GuildWidget, api [get_guild_widget, get_guild_widget_typed];
    GetGuildWidgetImage: GET "/guilds/{guild.id}/widget.png" none, query [style], body () => Vec<u8>, api [get_guild_widget_image];
    GetGuildWelcomeScreen: GET "/guilds/{guild.id}/welcome-screen" bot, query [], body () => WelcomeScreen, api [get_guild_welcome_screen];
    ModifyGuildWelcomeScreen: PATCH "/guilds/{guild.id}/welcome-screen" bot, query [], body ModifyGuildWelcomeScreen => WelcomeScreen, api [modify_guild_welcome_screen, modify_guild_welcome_screen_typed, modify_guild_welcome_screen_config];
    GetGuildOnboarding: GET "/guilds/{guild.id}/onboarding" bot, query [], body () => GuildOnboarding, api [get_guild_onboarding];
    ModifyGuildOnboarding: PUT "/guilds/{guild.id}/onboarding" bot, query [], body ModifyGuildOnboarding => GuildOnboarding, api [modify_guild_onboarding, modify_guild_onboarding_typed, modify_guild_onboarding_config];

    // --- Guild templates --------------------------------------------------
    GetGuildTemplate: GET "/guilds/templates/{template.code}" bot, query [], body () => GuildTemplate, api [get_guild_template_by_code];
    GetGuildTemplates: GET "/guilds/{guild.id}/templates" bot, query [], body () => Vec<GuildTemplate>, api [get_guild_templates];
    CreateGuildTemplate: POST "/guilds/{guild.id}/templates" bot, query [], body Value => GuildTemplate, api [create_guild_template];
    SyncGuildTemplate: PUT "/guilds/{guild.id}/templates/{template.code}" bot, query [], body () => GuildTemplate, api [sync_guild_template];
    ModifyGuildTemplate: PATCH "/guilds/{guild.id}/templates/{template.code}" bot, query [], body Value => GuildTemplate, api [modify_guild_template];
    DeleteGuildTemplate: DELETE "/guilds/{guild.id}/templates/{template.code}" bot, query [], body () => GuildTemplate, api [delete_guild_template];

    // --- Members, bans, roles ---------------------------------------------
    GetGuildMember: GET "/guilds/{guild.id}/members/{user.id}" bot, query [], body () => Member, api [get_member];
    ListGuildMembers: GET "/guilds/{guild.id}/members" bot, query [limit, after], body () => Vec<Member>, api [get_guild_members, get_guild_members_with_query];
    SearchGuildMembers: GET "/guilds/{guild.id}/members/search" bot, query [query, limit], body () => Vec<Member>, api [search_guild_members, search_guild_members_with_query];
    AddGuildMember: PUT "/guilds/{guild.id}/members/{user.id}" bot, query [], body AddGuildMember => Option<Member>, api [add_guild_member];
    ModifyGuildMember: PATCH "/guilds/{guild.id}/members/{user.id}" bot, query [], body ModifyGuildMember => (), api [modify_guild_member, modify_guild_member_typed];
    ModifyCurrentMember: PATCH "/guilds/{guild.id}/members/@me" bot, query [], body ModifyCurrentMember => Member, api [modify_current_member];
    ModifyCurrentUserNick: PATCH "/guilds/{guild.id}/members/@me/nick" bot, query [], body Option<String> => (), api [modify_current_member_nick];
    AddGuildMemberRole: PUT "/guilds/{guild.id}/members/{user.id}/roles/{role.id}" bot, query [], body () => (), api [add_guild_member_role];
    RemoveGuildMemberRole: DELETE "/guilds/{guild.id}/members/{user.id}/roles/{role.id}" bot, query [], body () => (), api [remove_guild_member_role];
    RemoveGuildMember: DELETE "/guilds/{guild.id}/members/{user.id}" bot, query [], body () => (), api [remove_guild_member];
    GetGuildBans: GET "/guilds/{guild.id}/bans" bot, query [limit, before, after], body () => Vec<Ban>, api [get_guild_bans, get_guild_bans_with_query];
    GetGuildBan: GET "/guilds/{guild.id}/bans/{user.id}" bot, query [], body () => Ban, api [get_guild_ban];
    CreateGuildBan: PUT "/guilds/{guild.id}/bans/{user.id}" bot, query [], body CreateGuildBan => (), api [create_guild_ban, create_guild_ban_typed];
    RemoveGuildBan: DELETE "/guilds/{guild.id}/bans/{user.id}" bot, query [], body () => (), api [remove_guild_ban];
    BulkGuildBan: POST "/guilds/{guild.id}/bulk-ban" bot, query [], body BulkGuildBanRequest => BulkGuildBanResponse, api [bulk_guild_ban];
    GetGuildRoles: GET "/guilds/{guild.id}/roles" bot, query [], body () => Vec<Role>, api [list_roles];
    GetGuildRole: GET "/guilds/{guild.id}/roles/{role.id}" bot, query [], body () => Role, api [get_guild_role];
    GetGuildRoleMemberCounts: GET "/guilds/{guild.id}/roles/member-counts" bot, query [], body () => HashMap<RoleId, u64>, api [get_guild_role_member_counts];
    CreateGuildRole: POST "/guilds/{guild.id}/roles" bot, query [], body CreateGuildRole => Role, api [create_guild_role, create_role, create_role_typed];
    ModifyGuildRolePositions: PATCH "/guilds/{guild.id}/roles" bot, query [], body [ModifyGuildRolePosition] => Vec<Role>, api [modify_guild_role_positions, modify_guild_role_positions_typed];
    ModifyGuildRole: PATCH "/guilds/{guild.id}/roles/{role.id}" bot, query [], body ModifyGuildRole => Role, api [modify_guild_role, update_role, update_role_typed];
    DeleteGuildRole: DELETE "/guilds/{guild.id}/roles/{role.id}" bot, query [], body () => (), api [delete_role];

    // --- Invites ------------------------------------------------------------
    GetInvite: GET "/invites/{invite.code}" bot, query [with_counts, with_expiration, guild_scheduled_event_id], body () => Invite, api [get_invite, get_invite_with_options];
    DeleteInvite: DELETE "/invites/{invite.code}" bot, query [], body () => Invite, api [delete_invite];
    GetTargetUsers: GET "/invites/{invite.code}/target-users" bot, query [], body () => Vec<u8>, api [get_invite_target_users, get_invite_target_users_csv];
    UpdateTargetUsers: PUT "/invites/{invite.code}/target-users" bot, query [], body FileAttachment => (), api [update_invite_target_users];
    GetTargetUsersJobStatus: GET "/invites/{invite.code}/target-users/job-status" bot, query [], body () => InviteTargetUsersJobStatus, api [get_invite_target_users_job_status];
    AddTargetUser: PUT "/invites/{invite.code}/target-users/{user.id}" bot, query [], body () => (), api [add_invite_target_user];
    RemoveTargetUser: DELETE "/invites/{invite.code}/target-users/{user.id}" bot, query [], body () => (), api [remove_invite_target_user];
    BulkAddTargetUsers: POST "/invites/{invite.code}/target-users/bulk-add" bot, query [], body [UserId] => (), api [bulk_add_invite_target_users];
    BulkDeleteTargetUsers: POST "/invites/{invite.code}/target-users/bulk-delete" bot, query [], body [UserId] => (), api [bulk_delete_invite_target_users];

    // --- Lobbies (Social SDK) ---------------------------------------------
    CreateLobby: POST "/lobbies" bot, query [], body CreateLobby => Lobby, api [create_lobby];
    CreateOrJoinLobby: PUT "/lobbies" bearer, query [], body GetOrCreateLobby => Lobby, api [get_or_create_lobby];
    GetLobby: GET "/lobbies/{lobby.id}" bot, query [], body () => Lobby, api [get_lobby];
    ModifyLobby: PATCH "/lobbies/{lobby.id}" bot, query [], body ModifyLobby => Lobby, api [modify_lobby];
    DeleteLobby: DELETE "/lobbies/{lobby.id}" bot, query [], body () => (), api [delete_lobby];
    AddLobbyMember: PUT "/lobbies/{lobby.id}/members/{user.id}" bot, query [], body AddLobbyMember => LobbyMember, api [add_lobby_member];
    BulkUpdateLobbyMembers: POST "/lobbies/{lobby.id}/members/bulk" bot, query [], body [LobbyMemberUpdate] => Vec<LobbyMember>, api [bulk_update_lobby_members];
    RemoveLobbyMember: DELETE "/lobbies/{lobby.id}/members/{user.id}" bot, query [], body () => (), api [remove_lobby_member];
    LeaveLobby: DELETE "/lobbies/{lobby.id}/members/@me" bearer, query [], body () => (), api [leave_lobby];
    LinkChannelToLobby: PATCH "/lobbies/{lobby.id}/channel-linking" bearer, query [], body LinkLobbyChannel => Lobby, api [link_lobby_channel];
    CreateLobbyChannelInviteForSelf: POST "/lobbies/{lobby.id}/members/@me/invites" bearer, query [], body () => LobbyInvite, api [create_lobby_channel_invite_for_self];
    CreateLobbyChannelInviteForUser: POST "/lobbies/{lobby.id}/members/{user.id}/invites" bot, query [], body () => LobbyInvite, api [create_lobby_channel_invite_for_user];
    SendLobbyMessage: POST "/lobbies/{lobby.id}/messages" bearer, query [], body CreateLobbyMessage => LobbyMessage, api [send_lobby_message];
    GetLobbyMessages: GET "/lobbies/{lobby.id}/messages" bearer, query [limit], body () => Vec<LobbyMessage>, api [get_lobby_messages];
    UpdateLobbyMessageModerationMetadata: PUT "/lobbies/{lobby.id}/messages/{message.id}/moderation-metadata" bot, query [], body HashMap<String, String> => (), api [update_lobby_message_moderation_metadata];

    // --- Scheduled events -----------------------------------------------------
    ListScheduledEventsForGuild: GET "/guilds/{guild.id}/scheduled-events" bot, query [with_user_count], body () => Vec<GuildScheduledEvent>, api [get_guild_scheduled_events, get_guild_scheduled_events_with_query];
    CreateGuildScheduledEvent: POST "/guilds/{guild.id}/scheduled-events" bot, query [], body Value => GuildScheduledEvent, api [create_guild_scheduled_event, create_guild_scheduled_event_typed];
    GetGuildScheduledEvent: GET "/guilds/{guild.id}/scheduled-events/{guild_scheduled_event.id}" bot, query [], body () => GuildScheduledEvent, api [get_guild_scheduled_event];
    ModifyGuildScheduledEvent: PATCH "/guilds/{guild.id}/scheduled-events/{guild_scheduled_event.id}" bot, query [], body Value => GuildScheduledEvent, api [modify_guild_scheduled_event, modify_guild_scheduled_event_typed];
    DeleteGuildScheduledEvent: DELETE "/guilds/{guild.id}/scheduled-events/{guild_scheduled_event.id}" bot, query [], body () => (), api [delete_guild_scheduled_event];
    GetGuildScheduledEventUsers: GET "/guilds/{guild.id}/scheduled-events/{guild_scheduled_event.id}/users" bot, query [limit, with_member, before, after], body () => Vec<GuildScheduledEventUser>, api [get_guild_scheduled_event_users, get_guild_scheduled_event_users_with_query];

    // --- Stickers and soundboard ------------------------------------------
    GetSticker: GET "/stickers/{sticker.id}" bot, query [], body () => Sticker, api [get_sticker];
    ListStickerPacks: GET "/sticker-packs" bot, query [], body () => StickerPackList, api [list_sticker_packs];
    GetStickerPack: GET "/sticker-packs/{pack.id}" bot, query [], body () => StickerPack, api [get_sticker_pack];
    ListGuildStickers: GET "/guilds/{guild.id}/stickers" bot, query [], body () => Vec<Sticker>, api [get_guild_stickers];
    GetGuildSticker: GET "/guilds/{guild.id}/stickers/{sticker.id}" bot, query [], body () => Sticker, api [get_guild_sticker];
    CreateGuildSticker: POST "/guilds/{guild.id}/stickers" bot, query [], body CreateGuildSticker => Sticker, api [create_guild_sticker, create_guild_sticker_typed, create_guild_sticker_from_request];
    ModifyGuildSticker: PATCH "/guilds/{guild.id}/stickers/{sticker.id}" bot, query [], body ModifyGuildSticker => Sticker, api [modify_guild_sticker, modify_guild_sticker_typed, modify_guild_sticker_from_request];
    DeleteGuildSticker: DELETE "/guilds/{guild.id}/stickers/{sticker.id}" bot, query [], body () => (), api [delete_guild_sticker];
    SendSoundboardSound: POST "/channels/{channel.id}/send-soundboard-sound" bot, query [], body Value => (), api [send_soundboard_sound];
    ListDefaultSoundboardSounds: GET "/soundboard-default-sounds" bot, query [], body () => Vec<SoundboardSound>, api [list_default_soundboard_sounds];
    ListGuildSoundboardSounds: GET "/guilds/{guild.id}/soundboard-sounds" bot, query [], body () => SoundboardSoundList, api [list_guild_soundboard_sounds];
    GetGuildSoundboardSound: GET "/guilds/{guild.id}/soundboard-sounds/{sound.id}" bot, query [], body () => SoundboardSound, api [get_guild_soundboard_sound];
    CreateGuildSoundboardSound: POST "/guilds/{guild.id}/soundboard-sounds" bot, query [], body Value => SoundboardSound, api [create_guild_soundboard_sound];
    ModifyGuildSoundboardSound: PATCH "/guilds/{guild.id}/soundboard-sounds/{sound.id}" bot, query [], body Value => SoundboardSound, api [modify_guild_soundboard_sound];
    DeleteGuildSoundboardSound: DELETE "/guilds/{guild.id}/soundboard-sounds/{sound.id}" bot, query [], body () => (), api [delete_guild_soundboard_sound];

    // --- Stage instances and voice ----------------------------------------
    CreateStageInstance: POST "/stage-instances" bot, query [], body CreateStageInstance => StageInstance, api [create_stage_instance, create_stage_instance_typed, create_stage_instance_from_request];
    GetStageInstance: GET "/stage-instances/{channel.id}" bot, query [], body () => StageInstance, api [get_stage_instance];
    ModifyStageInstance: PATCH "/stage-instances/{channel.id}" bot, query [], body ModifyStageInstance => StageInstance, api [modify_stage_instance, modify_stage_instance_typed, modify_stage_instance_from_request];
    DeleteStageInstance: DELETE "/stage-instances/{channel.id}" bot, query [], body () => (), api [delete_stage_instance];
    ListVoiceRegions: GET "/voice/regions" bot, query [], body () => Vec<VoiceRegion>, api [get_voice_regions, get_voice_regions_typed];
    GetCurrentUserVoiceState: GET "/guilds/{guild.id}/voice-states/@me" bot, query [], body () => VoiceState, api [get_current_user_voice_state];
    GetUserVoiceState: GET "/guilds/{guild.id}/voice-states/{user.id}" bot, query [], body () => VoiceState, api [get_user_voice_state];
    ModifyCurrentUserVoiceState: PATCH "/guilds/{guild.id}/voice-states/@me" bot, query [], body ModifyCurrentUserVoiceState => (), api [modify_current_user_voice_state, modify_current_user_voice_state_from_request];
    ModifyUserVoiceState: PATCH "/guilds/{guild.id}/voice-states/{user.id}" bot, query [], body ModifyUserVoiceState => (), api [modify_user_voice_state, modify_user_voice_state_from_request];

    // --- Users ----------------------------------------------------------------
    GetCurrentUser: GET "/users/@me" bot, query [], body () => User, api [get_current_user];
    GetUser: GET "/users/{user.id}" bot, query [], body () => User, api [get_user];
    ModifyCurrentUser: PATCH "/users/@me" bot, query [], body ModifyCurrentUser => User, api [modify_current_user];
    GetCurrentUserGuilds: GET "/users/@me/guilds" bot, query [before, after, limit, with_counts, shard], body () => Vec<CurrentUserGuild>, api [get_current_user_guilds, get_current_user_guilds_with_query, get_current_user_guilds_typed, get_current_user_guilds_typed_with_query];
    GetCurrentUserGuildMember: GET "/users/@me/guilds/{guild.id}/member" bearer, query [], body () => Member, api [get_current_user_guild_member];
    LeaveGuild: DELETE "/users/@me/guilds/{guild.id}" bot, query [], body () => (), api [leave_guild];
    GetCurrentUserConnections: GET "/users/@me/connections" bearer, query [], body () => Vec<UserConnection>, api [get_current_user_connections];
    GetCurrentUserApplicationRoleConnection: GET "/users/@me/applications/{application.id}/role-connection" bearer, query [], body () => UserApplicationRoleConnection, api [get_current_user_application_role_connection];
    UpdateCurrentUserApplicationRoleConnection: PUT "/users/@me/applications/{application.id}/role-connection" bearer, query [], body UpdateUserApplicationRoleConnection => UserApplicationRoleConnection, api [update_current_user_application_role_connection];
    DeleteCurrentUserApplicationRoleConnection: DELETE "/users/@me/applications/{application.id}/role-connection" bearer, query [], body () => (), api [delete_current_user_application_role_connection];
    GetCurrentAuthorizationInformation: GET "/oauth2/@me" bearer, query [], body () => AuthorizationInformation, api [get_current_authorization_information];

    // --- Webhooks -------------------------------------------------------------
    CreateWebhook: POST "/channels/{channel.id}/webhooks" bot, query [], body CreateWebhook => Webhook, api [create_webhook, create_webhook_typed, create_webhook_from_request, create_webhook_raw];
    GetChannelWebhooks: GET "/channels/{channel.id}/webhooks" bot, query [], body () => Vec<Webhook>, api [get_channel_webhooks, get_channel_webhooks_typed, get_channel_webhooks_raw];
    GetGuildWebhooks: GET "/guilds/{guild.id}/webhooks" bot, query [], body () => Vec<Webhook>, api [get_guild_webhooks];
    GetWebhook: GET "/webhooks/{webhook.id}" bot, query [], body () => Webhook, api [get_webhook];
    GetWebhookWithToken: GET "/webhooks/{webhook.id}/{webhook.token}" token, query [], body () => Webhook, api [get_webhook_with_token];
    ModifyWebhook: PATCH "/webhooks/{webhook.id}" bot, query [], body ModifyWebhook => Webhook, api [modify_webhook, modify_webhook_typed, modify_webhook_from_request];
    ModifyWebhookWithToken: PATCH "/webhooks/{webhook.id}/{webhook.token}" token, query [], body ModifyWebhookWithToken => Webhook, api [modify_webhook_with_token, modify_webhook_with_token_typed, modify_webhook_with_token_from_request];
    DeleteWebhook: DELETE "/webhooks/{webhook.id}" bot, query [], body () => (), api [delete_webhook];
    DeleteWebhookWithToken: DELETE "/webhooks/{webhook.id}/{webhook.token}" token, query [], body () => (), api [delete_webhook_with_token];
    ExecuteWebhook: POST "/webhooks/{webhook.id}/{webhook.token}" token, query [wait, thread_id, with_components], body Value => Value, api [execute_webhook, execute_webhook_with_query, execute_webhook_with_files];
    ExecuteSlackCompatibleWebhook: POST "/webhooks/{webhook.id}/{webhook.token}/slack" token, query [wait, thread_id], body Value => Value, api [execute_slack_compatible_webhook];
    ExecuteGitHubCompatibleWebhook: POST "/webhooks/{webhook.id}/{webhook.token}/github" token, query [wait, thread_id], body Value => Value, api [execute_github_compatible_webhook];
    GetWebhookMessage: GET "/webhooks/{webhook.id}/{webhook.token}/messages/{message.id}" token, query [thread_id], body () => Message, api [get_webhook_message, get_webhook_message_with_query];
    EditWebhookMessage: PATCH "/webhooks/{webhook.id}/{webhook.token}/messages/{message.id}" token, query [thread_id, with_components], body CreateMessage => Message, api [edit_webhook_message, edit_webhook_message_with_query, edit_webhook_message_with_files, edit_webhook_message_with_files_and_query];
    DeleteWebhookMessage: DELETE "/webhooks/{webhook.id}/{webhook.token}/messages/{message.id}" token, query [thread_id], body () => (), api [delete_webhook_message, delete_webhook_message_with_query];

    // --- OAuth2 token endpoints (`OAuth2Client`, not `RestClient`) ---------
    OAuth2TokenExchange: POST "/oauth2/token" client, query [], body OAuth2CodeExchange => OAuth2TokenResponse, oauth2 [exchange_code, refresh_token];
    OAuth2TokenRevocation: POST "/oauth2/token/revoke" client, query [], body String => (), oauth2 [revoke_token];
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{RouteAuth, RouteId, MANIFEST};
    use crate::http::paths::{request_uses_bot_authorization, RequestRoute};

    /// `template` with every `{placeholder}` replaced by `value`, except
    /// `{*.token}` placeholders, which become `token`.
    fn instantiate(template: &str, value: &str, token: &str) -> String {
        let mut path = String::new();
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            path.push_str(&rest[..open]);
            let close = rest[open..].find('}').unwrap() + open;
            let name = &rest[open + 1..close];
            path.push_str(if name.ends_with(".token") {
                token
            } else {
                value
            });
            rest = &rest[close + 1..];
        }
        path.push_str(rest);
        path
    }

    #[test]
    fn manifest_rows_are_in_route_id_order() {
        let ids = MANIFEST.iter().map(|row| row.id).collect::<Vec<_>>();
        assert_eq!(ids, RouteId::ALL);
        for row in MANIFEST {
            assert!(!row.api.is_empty(), "{} lists no methods", row.id.name());
            assert!(
                matches!(row.client, "api" | "oauth2"),
                "{}: client {}",
                row.id.name(),
                row.client
            );
        }
    }

    #[test]
    fn rows_describe_plausible_requests() {
        for row in MANIFEST {
            let name = row.id.name();
            // Discord's GET routes take no body; everything is in the path
            // and the query.
            if row.id.method() == reqwest::Method::GET {
                assert_eq!(row.body, "()", "{name}");
            }
            assert!(!row.response.is_empty(), "{name}");
            for parameter in row.query {
                assert!(
                    parameter
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
                    "{name}: query parameter {parameter}"
                );
            }
        }
    }

    #[test]
    fn templates_are_well_formed() {
        for id in RouteId::ALL {
            let template = id.template();
            assert!(template.starts_with('/'), "{template}");
            assert!(
                template.len() == 1 || !template.ends_with('/'),
                "{template}"
            );
            assert!(!template.contains("//"), "{template}");
            assert!(!template.contains('?'), "{template}");
            let opens = template.matches('{').count();
            assert_eq!(opens, template.matches('}').count(), "{template}");
            assert!(!template.contains("{}"), "{template}");
        }
    }

    #[test]
    fn only_distinct_operations_share_a_method_and_template() {
        let mut seen = HashMap::<(String, &str), Vec<&str>>::new();
        for id in RouteId::ALL {
            seen.entry((id.method().to_string(), id.template()))
                .or_default()
                .push(id.name());
        }
        let shared = seen
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(_, mut ids)| {
                ids.sort_unstable();
                ids
            })
            .collect::<Vec<_>>();
        // Create DM and Create Group DM are separate operations on one path.
        assert_eq!(shared, [vec!["CreateDm", "CreateGroupDm"]]);
    }

    #[test]
    fn every_method_belongs_to_one_route() {
        let mut owner = HashMap::<(&str, &str), &str>::new();
        for row in MANIFEST {
            for name in row.api {
                if let Some(previous) = owner.insert((row.client, name), row.id.name()) {
                    panic!("{name} is listed under {previous} and {}", row.id.name());
                }
            }
        }
        let current = MANIFEST
            .iter()
            .flat_map(|row| row.api.iter().map(move |name| (row.client, *name)))
            .collect::<HashSet<_>>();
        for row in MANIFEST {
            for (old, new) in row.was {
                assert!(
                    !current.contains(&(row.client, *old)),
                    "{old} is both current and removed"
                );
                assert!(row.api.contains(new), "{}: {old} => {new}", row.id.name());
            }
        }
    }

    #[test]
    fn authorization_agrees_with_the_raw_path_rule() {
        // Raw requests decide authorization from the path; manifest routes
        // from their row. Both must send the bot token to the same routes.
        for id in RouteId::ALL {
            let path = instantiate(id.template(), "1", "token");
            match id.auth() {
                RouteAuth::Bot | RouteAuth::Bearer => {
                    assert!(request_uses_bot_authorization(&path), "{path}");
                }
                RouteAuth::Token => {
                    assert!(!request_uses_bot_authorization(&path), "{path}");
                }
                RouteAuth::None | RouteAuth::Client => {}
            }
        }
    }

    #[test]
    fn token_routes_keep_their_token_out_of_the_route_key() {
        for id in RouteId::ALL {
            if id.auth() != RouteAuth::Token {
                continue;
            }
            let path = instantiate(id.template(), "1", "secret-token");
            assert!(path.contains("secret-token"), "{path}");
            let route = RequestRoute::new(&id.method(), &path, Some(*id));
            assert!(!route.key.contains("secret-token"), "{}", route.key);
            assert!(
                !route.major_scope.contains("secret-token"),
                "{}",
                route.major_scope
            );
            assert!(route.key.contains(":token-"), "{}", route.key);
        }
    }
}
