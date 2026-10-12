use crate::model::{
    test_id, AuditLogActionType, ChannelType, CommandType, InteractionResponseType, MfaLevel,
    OnboardingMode, OverwriteType, RoleConnectionMetadataType, ScheduledEventEntityType,
    ScheduledEventPrivacyLevel, ScheduledEventStatus, SkuAccessType, StageInstancePrivacyLevel,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::body::{decode_json_body, header_string, json_body, parse_body_value};
use super::client_config::default_http_client;
use super::paths::RequestRoute;
use super::paths::{
    archived_threads_query, audit_log_query, configured_application_id, current_user_guilds_query,
    execute_webhook_path, followup_webhook_path, get_guild_query, global_commands_path,
    guild_bans_query, guild_members_query, interaction_callback_path, invite_query,
    joined_archived_threads_query, poll_answer_voters_query, rate_limit_route_key,
    request_uses_bot_authorization, search_guild_members_query, subscription_query,
    thread_member_query, validate_token_path_segment,
};
use super::rate_limit::RateLimitState;
use super::{
    discord_api_error, discord_rate_limit_error, sleep_for_retry_after, FileAttachment, RawRequest,
    RestClient, RouteId,
};
use crate::bitfield::Permissions;
use crate::command::CommandDefinition;
use crate::error::DiscordError;
use crate::model::{
    AddGroupDmRecipient, AddGuildMember, AddLobbyMember, AllowedMentions,
    ApplicationCommandPermission, ApplicationRoleConnectionMetadata, ArchivedThreadsQuery,
    AuditLogQuery, AutoModerationAction, AutoModerationActionType, AutoModerationEventType,
    AutoModerationTriggerType, BeginGuildPruneRequest, BulkGuildBanRequest, ChannelPinsQuery,
    CommandPermissionsId, CreateChannelInvite, CreateGuildBan, CreateGuildChannel, CreateGuildRole,
    CreateGuildSticker, CreateLobby, CreateMessage, CreateStageInstance, CreateTestEntitlement,
    CreateWebhook, CurrentUserGuildsQuery, EditApplicationCommandPermissions,
    EditChannelPermission, EntitlementOwner, EntitlementQuery, GetGuildQuery,
    GuildApplicationCommandPermissions, GuildBansQuery, GuildMembersQuery, GuildWidgetImageStyle,
    InteractionCallbackResponse, JoinedArchivedThreadsQuery, LinkLobbyChannel, LobbyMember,
    LobbyMemberUpdate, ModifyCurrentApplication, ModifyCurrentMember, ModifyCurrentUser,
    ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildChannelPosition,
    ModifyGuildIncidentActions, ModifyGuildMember, ModifyGuildOnboarding, ModifyGuildRole,
    ModifyGuildRolePosition, ModifyGuildSticker, ModifyGuildWelcomeScreen,
    ModifyGuildWidgetSettings, ModifyLobby, ModifyStageInstance, ModifyUserVoiceState,
    ModifyWebhook, ModifyWebhookWithToken, OverwriteTarget, PollAnswerId, RoleColors,
    SearchGuildMembersQuery, SearchGuildMessagesQuery, SetVoiceChannelStatus, SubscriptionQuery,
    ThreadMemberQuery, UpdateUserApplicationRoleConnection, WebhookExecuteQuery,
    WebhookMessageQuery, WelcomeScreenChannel,
};
use crate::request::{
    CreateApplicationEmoji, CreateAutoModerationRule, CreateGuildEmoji, CreateGuildScheduledEvent,
    CreateGuildSoundboardSound, CreateGuildTemplate, EditApplicationCommand,
    ModifyApplicationEmoji, ModifyAutoModerationRule, ModifyChannel, ModifyGuildEmoji,
    ModifyGuildScheduledEvent, ModifyGuildSoundboardSound, ModifyGuildTemplate,
    SendSoundboardSound, StartThreadFromMessage, StartThreadWithoutMessage,
};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[derive(serde::Serialize)]
struct InvalidJsonBody {
    map: HashMap<Vec<u8>, String>,
}

fn route(method: Method, path: &str) -> RequestRoute {
    RequestRoute::new(&method, path, None)
}

/// The `RequestRoute` an endpoint sending manifest route `id` produces.
fn routed(id: RouteId, path: &str) -> RequestRoute {
    RequestRoute::new(&id.method(), path, Some(id))
}

#[derive(Debug)]
struct PlannedResponse {
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: String,
}

impl PlannedResponse {
    fn json(status: StatusCode, body: serde_json::Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    }

    fn text(status: StatusCode, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: body.into(),
        }
    }

    fn empty(status: StatusCode) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

impl RecordedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

async fn read_recorded_request(stream: &mut tokio::net::TcpStream) -> RecordedRequest {
    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        let mut chunk = [0u8; 2048];
        let read = stream.read(&mut chunk).await.expect("read request");
        assert!(read > 0, "client disconnected before sending request");
        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none() {
            if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
                header_end = Some(index + 4);
                let header_text = String::from_utf8_lossy(&buffer[..index]).to_string();
                content_length = header_text
                    .split("\r\n")
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
            }
        }

        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                let header_text = String::from_utf8_lossy(&buffer[..end - 4]).to_string();
                let mut lines = header_text.split("\r\n");
                let request_line = lines.next().expect("request line");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().expect("method").to_string();
                let path = parts.next().expect("path").to_string();
                let headers = lines
                    .filter_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
                    })
                    .collect::<HashMap<_, _>>();
                let body = String::from_utf8_lossy(&buffer[end..end + content_length]).to_string();

                return RecordedRequest {
                    method,
                    path,
                    headers,
                    body,
                };
            }
        }
    }
}

async fn write_planned_response(stream: &mut tokio::net::TcpStream, response: PlannedResponse) {
    let status_text = response.status.canonical_reason().unwrap_or("OK");
    let mut raw = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status.as_u16(),
        status_text,
        response.body.len()
    );
    for (name, value) in response.headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str("\r\n");

    stream
        .write_all(raw.as_bytes())
        .await
        .expect("write headers");
    if !response.body.is_empty() {
        stream
            .write_all(response.body.as_bytes())
            .await
            .expect("write body");
    }
}

async fn spawn_test_server(
    responses: Vec<PlannedResponse>,
) -> (String, Arc<Mutex<Vec<RecordedRequest>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_for_task = Arc::clone(&captured);

    let task = tokio::spawn(async move {
        for response in responses {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let request = read_recorded_request(&mut stream).await;
            captured_for_task
                .lock()
                .expect("capture mutex")
                .push(request);
            write_planned_response(&mut stream, response).await;
        }
    });

    (base_url, captured, task)
}

fn message_payload(id: &str, channel_id: &str, content: &str) -> serde_json::Value {
    json!({
        "id": id,
        "channel_id": channel_id,
        "content": content
    })
}

fn channel_payload(id: &str, kind: u8, name: Option<&str>) -> serde_json::Value {
    let mut channel = json!({
        "id": id,
        "type": kind
    });
    if let Some(name) = name {
        channel["name"] = json!(name);
    }
    channel
}

fn lobby_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "application_id": "555",
        "metadata": {
            "topic": "redstone"
        },
        "members": [{
            "id": "42",
            "metadata": {
                "role": "host"
            },
            "flags": 1
        }],
        "linked_channel": channel_payload("700", 0, Some("lobby"))
    })
}

fn guild_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name
    })
}

fn current_user_guild_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "banner": "banner_hash",
        "owner": false,
        "permissions": "8",
        "features": ["COMMUNITY"],
        "approximate_member_count": 3268,
        "approximate_presence_count": 784
    })
}

fn user_payload(id: &str, username: &str) -> serde_json::Value {
    json!({
        "id": id,
        "username": username,
        "discriminator": "0000"
    })
}

fn member_payload(user_id: &str, username: &str) -> serde_json::Value {
    json!({
        "user": {
            "id": user_id,
            "username": username,
            "discriminator": "0000",
            "bot": false
        },
        "roles": []
    })
}

fn webhook_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "type": 1,
        "name": name,
        "channel_id": "100",
        "token": "token"
    })
}

fn sticker_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": "sticker",
        "tags": "tag",
        "type": 2,
        "format_type": 1
    })
}

fn stage_payload(channel_id: &str) -> serde_json::Value {
    json!({
        "id": "9000",
        "guild_id": "200",
        "channel_id": channel_id,
        "topic": "town hall",
        "privacy_level": 2
    })
}

fn voice_state_payload(user_id: &str, channel_id: &str) -> serde_json::Value {
    json!({
        "guild_id": "200",
        "channel_id": channel_id,
        "user_id": user_id,
        "session_id": "voice-session",
        "deaf": false,
        "mute": false,
        "self_deaf": false,
        "self_mute": false,
        "self_video": false,
        "suppress": true
    })
}

fn welcome_screen_payload() -> serde_json::Value {
    json!({
        "description": "welcome",
        "welcome_channels": [{
            "channel_id": "300",
            "description": "rules",
            "emoji_name": "wave"
        }]
    })
}

fn onboarding_payload() -> serde_json::Value {
    json!({
        "guild_id": "200",
        "prompts": [],
        "default_channel_ids": ["300"],
        "enabled": true,
        "mode": 1
    })
}

fn incidents_payload() -> serde_json::Value {
    json!({
        "invites_disabled_until": "2026-05-01T12:00:00.000000+00:00",
        "dms_disabled_until": null,
        "dm_spam_detected_at": null,
        "raid_detected_at": "2026-05-01T11:00:00.000000+00:00"
    })
}

fn template_payload(code: &str) -> serde_json::Value {
    json!({
        "code": code,
        "name": "template",
        "usage_count": 0,
        "created_at": "2026-01-01T00:00:00.000000+00:00",
        "updated_at": "2026-01-01T00:00:00.000000+00:00"
    })
}

fn scheduled_event_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "guild_id": "200",
        "channel_id": "300",
        "creator_id": "400",
        "name": "community night",
        "description": "games",
        "scheduled_start_time": "2026-05-01T00:00:00.000000+00:00",
        "scheduled_end_time": "2026-05-01T01:00:00.000000+00:00",
        "privacy_level": 2,
        "status": 1,
        "entity_type": 2,
        "entity_metadata": { "location": "Stage" },
        "recurrence_rule": {
            "start": "2026-05-01T00:00:00.000000+00:00",
            "frequency": 2,
            "interval": 1
        },
        "user_count": 5
    })
}

fn sku_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "type": 5,
        "application_id": "555",
        "name": "Premium",
        "slug": "premium",
        "flags": 128,
        "dependent_sku_id": null,
        "manifest_labels": null,
        "access_type": 1,
        "features": [],
        "release_date": null,
        "premium": false,
        "show_age_gate": false
    })
}

fn entitlement_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "sku_id": "900",
        "application_id": "555",
        "user_id": "777",
        "promotion_id": null,
        "type": 8,
        "deleted": false,
        "gift_code_flags": 0,
        "consumed": false,
        "starts_at": "2026-01-01T00:00:00.000000+00:00",
        "ends_at": "2026-02-01T00:00:00.000000+00:00",
        "guild_id": "200",
        "subscription_id": "950"
    })
}

fn subscription_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "user_id": "777",
        "sku_ids": ["900"],
        "entitlement_ids": ["901"],
        "current_period_start": "2026-04-01T00:00:00.000000+00:00",
        "current_period_end": "2026-05-01T00:00:00.000000+00:00",
        "status": 0,
        "canceled_at": null
    })
}

fn soundboard_payload(id: &str) -> serde_json::Value {
    json!({
        "name": "quack",
        "sound_id": id,
        "volume": 1.0,
        "emoji_name": "duck",
        "guild_id": "200",
        "available": true
    })
}

fn role_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name
    })
}

fn emoji_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "animated": false
    })
}

fn sample_auto_moderation_rule(name: &str) -> CreateAutoModerationRule {
    CreateAutoModerationRule::new(
        name,
        AutoModerationEventType::MESSAGE_SEND,
        AutoModerationTriggerType::KEYWORD,
        vec![AutoModerationAction {
            kind: AutoModerationActionType::BLOCK_MESSAGE,
            metadata: None,
        }],
    )
}

fn auto_moderation_rule_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "guild_id": "200",
        "name": "block bad words",
        "creator_id": "42",
        "event_type": 1,
        "trigger_type": 1,
        "trigger_metadata": {
            "keyword_filter": ["bad"],
            "allow_list": []
        },
        "actions": [{ "type": 1 }],
        "enabled": true,
        "exempt_roles": [],
        "exempt_channels": []
    })
}

fn guild_preview_payload() -> serde_json::Value {
    json!({
        "id": "200",
        "name": "preview guild",
        "emojis": [],
        "features": ["COMMUNITY"],
        "approximate_member_count": 100,
        "approximate_presence_count": 10,
        "description": "typed preview",
        "stickers": []
    })
}

fn command_payload(id: &str, name: &str, description: &str) -> serde_json::Value {
    json!({
        "id": id,
        "type": 1,
        "name": name,
        "description": description
    })
}

fn application_payload(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "description": "application",
        "bot_public": true,
        "bot_require_code_grant": false
    })
}

fn role_connection_metadata_payload(key: &str) -> serde_json::Value {
    json!({
        "type": 1,
        "key": key,
        "name": "Score",
        "description": "User score"
    })
}

fn gateway_payload() -> serde_json::Value {
    json!({
        "url": "wss://gateway.discord.gg",
        "shards": 1,
        "session_start_limit": {
            "total": 10,
            "remaining": 9,
            "reset_after": 1000,
            "max_concurrency": 1
        }
    })
}

fn assert_request_basics(
    request: &RecordedRequest,
    method: &str,
    path: &str,
    expected_authorization: Option<&str>,
) {
    assert_eq!(request.method, method);
    assert_eq!(request.path, path);
    assert_eq!(request.header("authorization"), expected_authorization);
    assert_eq!(
        request.header("user-agent"),
        Some(concat!(
            "DiscordBot (discordrs, ",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
}

fn assert_multipart_request(
    request: &RecordedRequest,
    method: &str,
    path: &str,
    expected_authorization: Option<&str>,
) {
    assert_eq!(request.method, method);
    assert_eq!(request.path, path);
    assert_eq!(request.header("authorization"), expected_authorization);
    assert_eq!(
        request.header("user-agent"),
        Some(concat!(
            "DiscordBot (discordrs, ",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
    );
    assert!(
        request
            .header("content-type")
            .is_some_and(|value| value.starts_with("multipart/form-data; boundary=")),
        "expected multipart content-type, got {:?}",
        request.header("content-type")
    );
    assert!(request.body.contains(r#"name="payload_json""#));
    assert!(request.body.contains(r#"name="files[0]""#));
}

fn assert_named_file_multipart_request(
    request: &RecordedRequest,
    method: &str,
    path: &str,
    expected_authorization: Option<&str>,
    field_name: &str,
    expect_payload_json: bool,
) {
    assert_eq!(request.method, method);
    assert_eq!(request.path, path);
    assert_eq!(request.header("authorization"), expected_authorization);
    assert_eq!(
        request.header("user-agent"),
        Some(concat!(
            "DiscordBot (discordrs, ",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
    );
    assert!(
        request
            .header("content-type")
            .is_some_and(|value| value.starts_with("multipart/form-data; boundary=")),
        "expected multipart content-type, got {:?}",
        request.header("content-type")
    );
    assert_eq!(
        request.body.contains(r#"name="payload_json""#),
        expect_payload_json
    );
    assert!(request.body.contains(&format!(r#"name="{field_name}""#)));
}

fn sample_command() -> CommandDefinition {
    CommandDefinition {
        kind: Some(CommandType::CHAT_INPUT),
        name: "ping".to_string(),
        description: "pong".to_string(),
        ..CommandDefinition::default()
    }
}

fn sample_message() -> CreateMessage {
    CreateMessage {
        content: Some("hello".to_string()),
        ..CreateMessage::default()
    }
}

fn sample_interaction_response() -> InteractionCallbackResponse {
    InteractionCallbackResponse {
        kind: InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
        data: Some(json!({ "content": "ack" })),
    }
}

fn sample_followup() -> crate::request::CreateFollowup {
    crate::request::CreateFollowup::from("hello")
}

fn sample_edit() -> crate::request::EditInteractionResponse {
    crate::request::EditInteractionResponse::from("hello")
}

fn sample_file(name: &str, data: &str) -> FileAttachment {
    FileAttachment::new(name, data.as_bytes().to_vec()).with_content_type("text/plain")
}

fn assert_model_error_contains(error: DiscordError, expected: &str) {
    match error {
        DiscordError::Model { message } => {
            assert!(
                message.contains(expected),
                "expected `{expected}` in `{message}`"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn configured_application_id_rejects_zero() {
    let error = configured_application_id(None).unwrap_err();
    assert!(error.to_string().contains("application_id must be set"));
}

#[test]
fn global_commands_path_rejects_zero_application_id() {
    let error = global_commands_path(None).unwrap_err();
    assert!(error.to_string().contains("application_id must be set"));
}

#[test]
fn global_commands_path_uses_configured_application_id() {
    assert_eq!(
        global_commands_path(Some(test_id(123))).unwrap(),
        "/applications/123/commands"
    );
}

#[test]
fn followup_webhook_path_uses_explicit_application_id() {
    let path = followup_webhook_path(test_id(123), "token", None).unwrap();
    assert_eq!(path, "/webhooks/123/token");
}

#[test]
fn edit_followup_webhook_path_includes_message_id() {
    let path = followup_webhook_path(test_id(123), "token", Some("456")).unwrap();
    assert_eq!(path, "/webhooks/123/token/messages/456");
}

#[test]
fn original_interaction_response_path_uses_original_message_marker() {
    let path = followup_webhook_path(test_id(123), "token", Some("@original")).unwrap();
    assert_eq!(path, "/webhooks/123/token/messages/@original");
}

#[test]
fn followup_webhook_path_rejects_empty_or_unsafe_segments() {
    let token_error = followup_webhook_path(test_id(123), "", None).unwrap_err();
    assert!(token_error.to_string().contains("interaction_token"));

    let token_separator_error =
        followup_webhook_path(test_id(123), "token/part", None).unwrap_err();
    assert!(token_separator_error
        .to_string()
        .contains("interaction_token"));

    let message_error = followup_webhook_path(test_id(123), "token", Some("bad/id")).unwrap_err();
    assert!(message_error.to_string().contains("message_id"));
}

#[test]
fn interaction_callback_path_rejects_unsafe_tokens() {
    let error = interaction_callback_path(test_id(123), "bad/token").unwrap_err();
    assert!(error.to_string().contains("interaction_token"));
}

#[test]
fn interaction_and_webhook_paths_accept_safe_segments() {
    assert_eq!(
        interaction_callback_path(test_id(123), "safe-token").unwrap(),
        "/interactions/123/safe-token/callback"
    );
    assert_eq!(
        execute_webhook_path(test_id(456), "safe-token").unwrap(),
        "/webhooks/456/safe-token?wait=true"
    );
}

#[test]
fn execute_webhook_path_rejects_unsafe_tokens() {
    let error = execute_webhook_path(test_id(123), "bad/token").unwrap_err();
    assert!(error.to_string().contains("webhook_token"));
}

#[test]
fn request_uses_bot_authorization_skips_tokenized_callback_paths() {
    assert!(request_uses_bot_authorization("/channels/123/messages"));
    assert!(!request_uses_bot_authorization("/webhooks/123/token"));
    assert!(!request_uses_bot_authorization(
        "/interactions/123/token/callback"
    ));
    assert!(!request_uses_bot_authorization(
        "/webhooks/123/token/messages/@original"
    ));
}

#[test]
fn discord_api_error_preserves_status_and_code() {
    let error = discord_api_error(
        StatusCode::BAD_REQUEST,
        r#"{"code":50035,"message":"Invalid Form Body"}"#,
    );

    match error {
        DiscordError::Api {
            status,
            code,
            message,
        } => {
            assert_eq!(status, 400);
            assert_eq!(code, Some(50035));
            assert_eq!(message, "Invalid Form Body");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn discord_rate_limit_error_preserves_route_and_retry_after() {
    let error = discord_rate_limit_error(
        "POST:webhooks/123/token",
        r#"{"message":"You are being rate limited.","retry_after":2.5,"global":false}"#,
    );

    match error {
        DiscordError::RateLimit { route, retry_after } => {
            assert_eq!(route, "POST:webhooks/123/token");
            assert_eq!(retry_after, 2.5);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn discord_api_error_uses_string_and_object_fallback_messages() {
    match discord_api_error(StatusCode::BAD_REQUEST, r#""plain string""#) {
        DiscordError::Api { message, .. } => {
            assert_eq!(message, "plain string");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    match discord_api_error(StatusCode::BAD_REQUEST, r#"{"code":7}"#) {
        DiscordError::Api { code, message, .. } => {
            assert_eq!(code, Some(7));
            assert_eq!(message, r#"{"code":7}"#);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn rate_limit_route_key_preserves_major_parameters() {
    assert_eq!(
        rate_limit_route_key(&Method::GET, "/channels/123/messages/456"),
        "GET:channels/123/messages/:id"
    );
    assert_eq!(
        rate_limit_route_key(&Method::GET, "/guilds/789/members/456"),
        "GET:guilds/789/members/:id"
    );
    let webhook_key = rate_limit_route_key(&Method::GET, "/webhooks/111/222/messages/333");
    assert!(
        webhook_key.starts_with("GET:webhooks/111/:token-")
            && webhook_key.ends_with("/messages/:id"),
        "webhook token must be digested, got {webhook_key}"
    );
    assert!(!webhook_key.contains("/222/"));
}

#[test]
fn rate_limit_route_key_digests_tokens_with_stable_per_token_identity() {
    let first = rate_limit_route_key(&Method::POST, "/webhooks/1/secret-token-a?wait=true");
    let again = rate_limit_route_key(&Method::POST, "/webhooks/1/secret-token-a");
    let other = rate_limit_route_key(&Method::POST, "/webhooks/1/secret-token-b");
    assert!(
        !first.contains("secret-token-a"),
        "token leaked into {first}"
    );
    assert_eq!(first, again, "the same token must map to the same key");
    assert_ne!(first, other, "different tokens must keep separate keys");

    let callback = rate_limit_route_key(
        &Method::POST,
        "/interactions/42/interaction-secret/callback",
    );
    assert!(callback.starts_with("POST:interactions/:id/:token-"));
    assert!(callback.ends_with("/callback"));
    assert!(!callback.contains("interaction-secret"));
}

#[test]
fn request_route_major_scope_combines_major_names_ids_and_tokens() {
    let scope = |method: Method, path: &str| route(method, path).major_scope;
    assert_eq!(
        scope(Method::GET, "/channels/123/messages/456"),
        "channels/123"
    );
    assert_eq!(
        scope(Method::POST, "/applications/1/guilds/2/commands/3"),
        "applications/1/guilds/2"
    );
    assert_eq!(scope(Method::GET, "/users/@me"), "");
    let webhook = scope(Method::POST, "/webhooks/9/tok");
    assert!(webhook.starts_with("webhooks/9/:token-"), "got {webhook}");
    assert!(!webhook.contains("tok/") && !webhook.ends_with("/tok"));
    let callback = scope(Method::POST, "/interactions/7/tok/callback");
    assert!(
        callback.starts_with("interactions/:id/:token-"),
        "got {callback}"
    );
}

#[test]
fn only_the_create_message_route_replays_with_a_nonce() {
    let replaying = RouteId::ALL
        .iter()
        .copied()
        .filter(|id| id.replays_with_nonce())
        .collect::<Vec<_>>();
    assert_eq!(replaying, [RouteId::CreateMessage]);
    assert_eq!(RouteId::CreateMessage.method(), Method::POST);
    assert_eq!(
        RouteId::CreateMessage.template(),
        "/channels/{channel.id}/messages"
    );
    // The route identity comes from the endpoint, never from the path: a
    // raw request to the same path carries none.
    assert_eq!(route(Method::POST, "/channels/123/messages").id, None);
    assert_eq!(
        routed(RouteId::CreateMessage, "/channels/123/messages").id,
        Some(RouteId::CreateMessage)
    );
}

#[test]
fn route_token_and_scrub_token_use_the_exact_request_secret() {
    use super::paths::{route_token, scrub_token};

    assert_eq!(route_token("/webhooks/1/secret?wait=true"), Some("secret"));
    assert_eq!(route_token("/interactions/2/itok/callback"), Some("itok"));
    assert_eq!(route_token("/channels/3/messages"), None);
    assert_eq!(route_token("/webhooks/1"), None);
    assert_eq!(
        scrub_token("Unknown route /webhooks/1/secret/messages", Some("secret")),
        "Unknown route /webhooks/1/:token/messages"
    );
    assert_eq!(scrub_token("unchanged", None), "unchanged");
}

#[test]
fn redact_path_tokens_handles_api_prefix_and_query() {
    use super::paths::redact_path_tokens;

    assert_eq!(
        redact_path_tokens("/api/v10/webhooks/1/secret/messages/@original?thread_id=5"),
        "/api/v10/webhooks/1/:token/messages/@original?…"
    );
    assert_eq!(
        redact_path_tokens("/interactions/2/secret/callback"),
        "/interactions/2/:token/callback"
    );
    assert_eq!(
        redact_path_tokens("/channels/3/messages"),
        "/channels/3/messages"
    );
    assert_eq!(
        redact_path_tokens("/channels/3/webhooks"),
        "/channels/3/webhooks"
    );
    assert_eq!(
        redact_path_tokens("/proxy/webhooks/gateway/api/v10/webhooks/1/secret"),
        "/proxy/webhooks/gateway/api/v10/webhooks/1/:token",
        "a custom base containing a resource word must not shift the mask"
    );
}

#[test]
fn rate_limit_route_key_keeps_application_and_guild_major_ids() {
    assert_eq!(
        rate_limit_route_key(&Method::POST, "/applications/123/guilds/456/commands/789"),
        "POST:applications/123/guilds/456/commands/:id"
    );
}

#[test]
fn rate_limit_state_reports_wait_duration() {
    let state = RateLimitState::default();
    let channel = route(Method::GET, "/channels/1");
    state
        .blocked_until
        .lock()
        .unwrap()
        .insert(channel.key.clone(), Instant::now() + Duration::from_secs(1));

    assert!(state.wait_duration(&channel).is_some());
}

#[test]
fn rate_limit_state_keeps_major_parameters_distinct() {
    let state = RateLimitState::default();
    let blocked_route = route(Method::GET, "/channels/123/messages/456");
    let other_route = route(Method::GET, "/channels/999/messages/456");

    state.blocked_until.lock().unwrap().insert(
        blocked_route.key.clone(),
        Instant::now() + Duration::from_secs(1),
    );

    assert!(state.wait_duration(&blocked_route).is_some());
    assert!(state.wait_duration(&other_route).is_none());
}

#[test]
fn parse_helpers_handle_empty_invalid_and_string_body_shapes() {
    assert_eq!(parse_body_value(String::new()), serde_json::Value::Null);
    assert_eq!(
        parse_body_value(String::from("plain text")),
        serde_json::Value::String(String::from("plain text"))
    );
    assert_eq!(
        parse_body_value(String::from(r#"{"message":"ok"}"#)),
        serde_json::json!({ "message": "ok" })
    );

    let header = HeaderValue::from_static("bucket-1");
    assert_eq!(header_string(Some(&header)), Some(String::from("bucket-1")));
    assert_eq!(header_string(None), None);
}

/// Bodies the transport can hand to the decoder: Discord JSON, the shapes a
/// proxy or an outage produces, invalid UTF-8, and JSON edge cases where a
/// typed parse and a `Value` parse differ (duplicate keys, numbers outside
/// `f64`).
const DECODE_CORPUS: &[&[u8]] = &[
    b"",
    b" ",
    b"null",
    b"{}",
    b"[]",
    b"42",
    b"\"just a string\"",
    br#"{"id":"1","name":"general"}"#,
    br#"[{"id":"1"},{"id":"2","name":null}]"#,
    br#"{"id":"1","id":"2"}"#,
    br#"{"id":"1","extra":1e400}"#,
    b"{\"id\":\"caf\xc3\xa9\"}",
    b"{\"id\":\"\xff\"}",
    b"\xff\xfe",
    b"\xffnot json",
    b"plain text",
    b"<html><body>502 Bad Gateway</body></html>",
    b"{\"truncated\":",
    b"[1,2,3] trailing",
];

/// The decoder before 3.0 phase 4: copy into a lossy `String`, parse it
/// into a `Value`, then decode `T` from the `Value`.
fn decode_like_before<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, DiscordError> {
    serde_json::from_value(parse_body_value(String::from_utf8_lossy(body).into_owned()))
        .map_err(Into::into)
}

/// The bodies whose result changed when `decode_json_body` moved from the
/// `Value` reader to the text reader (3.0 phase 4), with the type they
/// changed for. Every other (body, type) pair must decode exactly as before.
const CHANGED_BY_TEXT_READER: &[(&[u8], &str)] = &[
    // A duplicated key is a "duplicate field" error for a struct instead
    // of the last value winning.
    (br#"{"id":"1","id":"2"}"#, "DecodeProbe"),
    (br#"{"id":"1","id":"2"}"#, "Option<DecodeProbe>"),
    // A number out of `f64` range fails where it is read, instead of the
    // whole body becoming a JSON string, so every target sees an object now
    // and an ignored field no longer matters ("*": every type).
    (br#"{"id":"1","extra":1e400}"#, "*"),
];

fn assert_decodes_like_before<T>(body: &[u8], type_name: &str)
where
    T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let shown = String::from_utf8_lossy(body);
    let changed = CHANGED_BY_TEXT_READER
        .iter()
        .any(|&(changed, name)| changed == body && (name == type_name || name == "*"));
    match (decode_like_before::<T>(body), decode_json_body::<T>(body)) {
        _ if changed => {}
        (Ok(before), Ok(now)) => assert_eq!(before, now, "body {shown:?} as {type_name}"),
        (Err(before), Err(now)) => {
            // The text reader adds the position: "... at line 1 column 7".
            let (before, now) = (before.to_string(), now.to_string());
            assert!(
                now.starts_with(&before),
                "body {shown:?} as {type_name}: before {before:?}, now {now:?}"
            );
        }
        (before, now) => panic!("body {shown:?} as {type_name}: before {before:?}, now {now:?}"),
    }
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct DecodeProbe {
    id: String,
    name: Option<String>,
}

/// `RestClient::request::<Value>` (a raw request) reads a body like 2.x's
/// untyped `request`: empty is `null`, non-JSON text is a string, and
/// invalid UTF-8 is replaced first. The one difference is the documented
/// phase 4 change for every decoder: JSON with a number out of `f64` range
/// is an error instead of being returned as a string.
#[test]
fn raw_value_responses_match_the_lossy_string_parser() {
    for body in DECODE_CORPUS {
        let before = parse_body_value(String::from_utf8_lossy(body).into_owned());
        match decode_json_body::<serde_json::Value>(body) {
            Ok(value) => assert_eq!(value, before, "body {:?}", String::from_utf8_lossy(body)),
            Err(error) => {
                assert_eq!(*body, br#"{"id":"1","extra":1e400}"#);
                assert!(error.to_string().contains("number out of range"), "{error}");
            }
        }
    }
}

#[test]
fn decode_json_body_matches_the_previous_decoder_except_the_documented_changes() {
    for body in DECODE_CORPUS {
        assert_decodes_like_before::<serde_json::Value>(body, "Value");
        assert_decodes_like_before::<Option<serde_json::Value>>(body, "Option<Value>");
        assert_decodes_like_before::<()>(body, "()");
        assert_decodes_like_before::<String>(body, "String");
        assert_decodes_like_before::<DecodeProbe>(body, "DecodeProbe");
        assert_decodes_like_before::<Vec<DecodeProbe>>(body, "Vec<DecodeProbe>");
        assert_decodes_like_before::<Option<DecodeProbe>>(body, "Option<DecodeProbe>");
    }
    // The cases the corpus exists for, spelled out.
    assert_eq!(
        decode_json_body::<serde_json::Value>(b"").unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        decode_json_body::<String>(b"plain text").unwrap(),
        "plain text"
    );
    assert_eq!(
        decode_json_body::<String>(b"[1,2,3] trailing").unwrap(),
        "[1,2,3] trailing"
    );
    let duplicate = decode_json_body::<DecodeProbe>(br#"{"id":"1","id":"2"}"#).unwrap_err();
    assert!(
        duplicate.to_string().contains("duplicate field `id`"),
        "{duplicate}"
    );
    assert_eq!(
        decode_json_body::<DecodeProbe>(br#"{"id":"1","extra":1e400}"#).unwrap(),
        DecodeProbe {
            id: String::from("1"),
            name: None
        }
    );
    assert_eq!(
        decode_json_body::<DecodeProbe>(b"{\"id\":\"\xff\"}").unwrap(),
        DecodeProbe {
            id: String::from("\u{fffd}"),
            name: None
        }
    );
    let html = decode_json_body::<DecodeProbe>(b"<html>").unwrap_err();
    assert!(matches!(html, DiscordError::Json(_)));
    assert!(html.to_string().contains("line 1 column"), "{html}");
}

#[tokio::test]
async fn typed_shims_decode_text_empty_and_no_content_success_bodies_like_before() {
    let (base_url, requests, server) = spawn_test_server(vec![
        PlannedResponse::text(StatusCode::OK, "plain text"),
        PlannedResponse::empty(StatusCode::OK),
        PlannedResponse::text(StatusCode::OK, "<html>proxy page</html>"),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::CREATED, json!({ "id": "7", "name": null })),
        PlannedResponse::text(StatusCode::OK, "plain text"),
    ])
    .await;
    let client = RestClient::new_with_base_url("decode-token", 123, base_url);
    let text: serde_json::Value = client.request(RawRequest::get("/users/@me")).await.unwrap();
    assert_eq!(text, json!("plain text"));
    let empty: serde_json::Value = client.request(RawRequest::get("/users/@me")).await.unwrap();
    assert_eq!(empty, serde_json::Value::Null);
    let error = client
        .request::<DecodeProbe>(RawRequest::get("/users/@me"))
        .await
        .unwrap_err();
    // The same error as before, plus the text reader's position.
    assert!(
        error.to_string().starts_with(
            &decode_like_before::<DecodeProbe>(b"<html>proxy page</html>")
                .unwrap_err()
                .to_string()
        ),
        "{error}"
    );
    let no_content: Option<DecodeProbe> = client
        .send_optional(
            RouteId::AddGuildMember,
            "/guilds/1/members/2",
            json_body(&json!({})),
        )
        .await
        .unwrap();
    assert_eq!(no_content, None);
    let created: Option<DecodeProbe> = client
        .send_optional(
            RouteId::AddGuildMember,
            "/guilds/1/members/2",
            json_body(&json!({})),
        )
        .await
        .unwrap();
    assert_eq!(
        created,
        Some(DecodeProbe {
            id: String::from("7"),
            name: None
        })
    );
    let untyped: serde_json::Value = client.request(RawRequest::get("/users/@me")).await.unwrap();
    assert_eq!(untyped, json!("plain text"));

    server.await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 6);
}

#[tokio::test]
async fn multipart_shim_serialization_errors_return_before_network() {
    let client = RestClient::new_with_base_url("serialize-token", 123, "http://127.0.0.1:9");
    let body = InvalidJsonBody {
        map: HashMap::from([(vec![1, 2, 3], "bad".to_string())]),
    };
    let files = [FileAttachment::new("a.txt", b"a".to_vec())];

    let error = client
        .request::<serde_json::Value>(
            RawRequest::post("/channels/1/messages").multipart(&body, &files),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, DiscordError::Json(message) if message.contains("key must be a string")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn validate_token_path_segment_handles_original_marker_and_control_characters() {
    validate_token_path_segment("message_id", "@original", true).unwrap();
    validate_token_path_segment("token", "safe-token", false).unwrap();
    validate_token_path_segment("token", "abc.DEF_123-456~xyz", false).unwrap();

    let backslash = validate_token_path_segment("token", r"bad\token", false).unwrap_err();
    assert!(backslash.to_string().contains("token"));

    let query = validate_token_path_segment("token", "bad?token", false).unwrap_err();
    assert!(query.to_string().contains("token"));

    let marker = validate_token_path_segment("token", "@original", false).unwrap_err();
    assert!(marker.to_string().contains("unsafe"));

    let control = validate_token_path_segment("token", "bad\ntoken", false).unwrap_err();
    assert!(control.to_string().contains("unsafe"));
}

#[tokio::test]
async fn invite_helpers_reject_unsafe_codes_before_network() {
    let client = RestClient::new_with_base_url("poc-token", 123, "http://127.0.0.1:9");

    for code in [
        "../channels/111/messages/222",
        r"..\channels\111\messages\222",
        "abc?with_counts=true",
        "abc#fragment",
        "line\nbreak",
    ] {
        assert!(matches!(
            client.get_invite(code).await,
            Err(DiscordError::Model { .. })
        ));
        assert!(matches!(
            client
                .get_invite_with_options(code, Some(true), None, None)
                .await,
            Err(DiscordError::Model { .. })
        ));
        assert!(matches!(
            client.delete_invite(code).await,
            Err(DiscordError::Model { .. })
        ));
        assert!(matches!(
            client.get_invite_target_users(code).await,
            Err(DiscordError::Model { .. })
        ));
        assert!(matches!(
            client
                .update_invite_target_users(code, &sample_file("users.csv", "user_id\n1\n"))
                .await,
            Err(DiscordError::Model { .. })
        ));
        assert!(matches!(
            client.get_invite_target_users_job_status(code).await,
            Err(DiscordError::Model { .. })
        ));
    }
}

#[tokio::test]
async fn typed_request_serialization_errors_return_json_errors_before_network() {
    let client = RestClient::new_with_base_url("serialize-token", 123, "http://127.0.0.1:9");
    let body = InvalidJsonBody {
        map: HashMap::from([(vec![1, 2, 3], "bad".to_string())]),
    };

    match client
        .request::<serde_json::Value>(RawRequest::patch("/guilds/1/members/2").json(&body))
        .await
        .unwrap_err()
    {
        DiscordError::Json(message) => {
            assert!(message.contains("key must be a string"));
        }
        other => panic!("unexpected serialization error: {other:?}"),
    }
}

#[test]
fn authorization_and_error_helpers_cover_query_and_fallback_cases() {
    assert!(request_uses_bot_authorization(
        "/channels/123/messages?wait=true"
    ));
    assert!(!request_uses_bot_authorization(
        "/webhooks/123/token?wait=true"
    ));
    assert!(request_uses_bot_authorization("/webhooks/123"));
    assert!(!request_uses_bot_authorization(
        "/webhooks/123/token/messages/456"
    ));

    match discord_api_error(StatusCode::BAD_REQUEST, "plain body") {
        DiscordError::Api {
            status,
            code,
            message,
        } => {
            assert_eq!(status, 400);
            assert_eq!(code, None);
            assert_eq!(message, "plain body");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    match discord_rate_limit_error("GET:channels/123", r#"{"message":"limited"}"#) {
        DiscordError::RateLimit { route, retry_after } => {
            assert_eq!(route, "GET:channels/123");
            assert_eq!(retry_after, 1.0);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    assert_eq!(
        rate_limit_route_key(&Method::PATCH, "/channels/123/messages/456?wait=true"),
        "PATCH:channels/123/messages/:id"
    );
}

#[test]
fn new_coverage_query_helpers_build_expected_paths() {
    assert_eq!(
        thread_member_query(&ThreadMemberQuery {
            with_member: Some(true),
            after: Some(test_id(10)),
            limit: Some(25),
        }),
        "?with_member=true&after=10&limit=25"
    );
    assert_eq!(
        archived_threads_query(&ArchivedThreadsQuery {
            before: Some("2026-04-29T00:00:00Z".to_string()),
            limit: Some(50),
        }),
        "?before=2026-04-29T00%3A00%3A00Z&limit=50"
    );
    assert_eq!(
        joined_archived_threads_query(&JoinedArchivedThreadsQuery {
            before: Some(test_id(11)),
            limit: Some(10),
        }),
        "?before=11&limit=10"
    );
    assert_eq!(
        subscription_query(&SubscriptionQuery {
            before: Some(test_id(20)),
            after: Some(test_id(21)),
            limit: Some(100),
            user_id: Some(test_id(22)),
        }),
        "?before=20&after=21&limit=100&user_id=22"
    );
    assert_eq!(
        invite_query(Some(true), Some(false), Some(test_id(30))),
        "?with_counts=true&with_expiration=false&guild_scheduled_event_id=30"
    );
    assert_eq!(
        poll_answer_voters_query(Some(test_id(40)), Some(15)),
        "?after=40&limit=15"
    );
    assert_eq!(
        audit_log_query(&AuditLogQuery {
            user_id: Some(test_id(50)),
            action_type: Some(AuditLogActionType::VOICE_CHANNEL_STATUS_CREATE),
            before: Some(test_id(60)),
            after: Some(test_id(55)),
            limit: Some(25),
        }),
        "?user_id=50&action_type=192&before=60&after=55&limit=25"
    );
    assert_eq!(
        current_user_guilds_query(&CurrentUserGuildsQuery {
            before: Some(test_id(70)),
            after: Some(test_id(65)),
            limit: Some(50),
            with_counts: Some(true),
            shard: None,
        }),
        "?before=70&after=65&limit=50&with_counts=true"
    );
    assert_eq!(
        current_user_guilds_query(&CurrentUserGuildsQuery {
            limit: Some(200),
            shard: Some(3),
            ..CurrentUserGuildsQuery::default()
        }),
        "?limit=200&shard=3",
        "large-bot-sharding apps must be able to send the shard param"
    );
    assert_eq!(
        get_guild_query(&GetGuildQuery {
            with_counts: Some(true),
        }),
        "?with_counts=true"
    );
    assert_eq!(
        guild_bans_query(&GuildBansQuery {
            limit: Some(100),
            before: Some(test_id(90)),
            after: Some(test_id(10)),
        }),
        "?limit=100&before=90&after=10"
    );
    assert_eq!(
        guild_members_query(&GuildMembersQuery {
            limit: Some(100),
            after: Some(test_id(80)),
        }),
        "?limit=100&after=80"
    );
    assert_eq!(
        search_guild_members_query(&SearchGuildMembersQuery {
            query: "alice & bob".to_string(),
            limit: Some(5),
        }),
        "?query=alice+%26+bob&limit=5"
    );
}

#[test]
fn rate_limit_state_observe_tracks_buckets_and_global_limits() {
    let state = RateLimitState::default();
    let messages = route(Method::POST, "/channels/123/messages");
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-bucket", HeaderValue::from_static("bucket-42"));
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
    headers.insert("x-ratelimit-reset-after", HeaderValue::from_static("0.05"));

    state.observe(&messages, &headers, StatusCode::OK, "");
    assert!(state
        .route_buckets
        .lock()
        .unwrap()
        .contains_key(&messages.key));
    assert!(state.wait_duration(&messages).is_some());

    let global_headers = HeaderMap::new();
    state.observe(
        &messages,
        &global_headers,
        StatusCode::TOO_MANY_REQUESTS,
        r#"{"retry_after":0.05,"global":true}"#,
    );
    assert!(state
        .wait_duration(&route(Method::GET, "/anything"))
        .is_some());
}

#[test]
fn rate_limit_state_scopes_shared_bucket_hash_by_major_parameter() {
    // Discord's bucket hash excludes major parameters: routes that report
    // the same hash share a limit only within the same channel/guild/webhook.
    let state = RateLimitState::default();
    let route_a = route(Method::GET, "/channels/123/messages");
    let route_b = route(Method::POST, "/channels/123/messages");
    let other_channel = route(Method::POST, "/channels/456/messages");

    let mut bucket_headers = HeaderMap::new();
    bucket_headers.insert(
        "x-ratelimit-bucket",
        HeaderValue::from_static("shared-bucket"),
    );

    state.observe(&route_a, &bucket_headers, StatusCode::OK, "");
    state.observe(&route_b, &bucket_headers, StatusCode::OK, "");
    state.observe(&other_channel, &bucket_headers, StatusCode::OK, "");
    state.observe(
        &route_a,
        &HeaderMap::new(),
        StatusCode::TOO_MANY_REQUESTS,
        r#"{"retry_after":0.05,"global":false}"#,
    );

    assert!(state.wait_duration(&route_a).is_some());
    assert!(
        state.wait_duration(&route_b).is_some(),
        "same bucket and channel must share the block"
    );
    assert!(
        state.wait_duration(&other_channel).is_none(),
        "another channel reporting the same hash must stay independent"
    );
    assert_eq!(state.block_key_for(&route_a), state.block_key_for(&route_b));
    assert_ne!(
        state.block_key_for(&route_a),
        state.block_key_for(&other_channel)
    );
}

#[test]
fn rate_limit_state_survives_malformed_wait_values() {
    let route = route(Method::GET, "/channels/1/messages");
    for value in ["inf", "-inf", "NaN", "-5", "1e300"] {
        let state = RateLimitState::default();
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        headers.insert(
            "x-ratelimit-reset-after",
            HeaderValue::from_str(value).expect("header value"),
        );
        state.observe(&route, &headers, StatusCode::OK, "");

        let mut limited = HeaderMap::new();
        limited.insert(
            "retry-after",
            HeaderValue::from_str(value).expect("header value"),
        );
        state.observe(
            &route,
            &limited,
            StatusCode::TOO_MANY_REQUESTS,
            "<html>banned</html>",
        );
        let _ = state.wait_duration(&route);
    }
}

#[test]
fn wait_conversion_clamps_instead_of_panicking() {
    use super::rate_limit::{retry_after_seconds, wait_duration_from_seconds, MAX_SERVER_WAIT};

    assert_eq!(wait_duration_from_seconds(f64::NAN), None);
    assert_eq!(wait_duration_from_seconds(-1.0), Some(Duration::ZERO));
    assert_eq!(
        wait_duration_from_seconds(f64::INFINITY),
        Some(MAX_SERVER_WAIT)
    );
    assert_eq!(wait_duration_from_seconds(1e300), Some(MAX_SERVER_WAIT));
    assert_eq!(
        wait_duration_from_seconds(1.5),
        Some(Duration::from_millis(1500))
    );

    let mut headers = HeaderMap::new();
    headers.insert("retry-after", HeaderValue::from_static("NaN"));
    headers.insert("x-ratelimit-reset-after", HeaderValue::from_static("2.5"));
    assert_eq!(
        retry_after_seconds(&headers, "<html></html>"),
        2.5,
        "a NaN source must fall through to the next one"
    );
    let mut infinite = HeaderMap::new();
    infinite.insert("retry-after", HeaderValue::from_static("inf"));
    assert_eq!(
        retry_after_seconds(&infinite, ""),
        MAX_SERVER_WAIT.as_secs_f64()
    );
}

#[test]
fn rate_limit_state_ignores_expired_route_and_global_blocks() {
    let state = RateLimitState::default();
    state.blocked_until.lock().unwrap().insert(
        route(Method::GET, "/channels/123/messages").key,
        Instant::now(),
    );
    *state.global_blocked_until.lock().unwrap() = Some(Instant::now());

    assert!(state
        .wait_duration(&route(Method::GET, "/channels/123/messages"))
        .is_none());
    assert!(state
        .wait_duration(&route(Method::GET, "/anything"))
        .is_none());
    assert!(state.blocked_until.lock().unwrap().is_empty());
    assert!(state.global_blocked_until.lock().unwrap().is_none());
}

#[test]
fn rate_limit_state_cleans_stale_bucket_metadata() {
    let state = RateLimitState::default();
    let messages = route(Method::GET, "/channels/1/messages");
    state
        .route_buckets
        .lock()
        .unwrap()
        .insert(messages.key.clone(), "bucket-old".to_string());
    state.bucket_last_seen.lock().unwrap().insert(
        "bucket-old".to_string(),
        Instant::now() - super::RATE_LIMIT_BUCKET_RETENTION - Duration::from_secs(1),
    );

    assert!(state.wait_duration(&messages).is_none());
    assert!(state.route_buckets.lock().unwrap().is_empty());
    assert!(state.bucket_last_seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sleep_for_retry_after_waits_without_panicking() {
    let start = Instant::now();
    sleep_for_retry_after(0.01).await;
    assert!(start.elapsed() >= Duration::from_millis(5));
}

#[tokio::test]
async fn channel_message_file_helpers_send_multipart_payloads() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, message_payload("701", "100", "created")),
        PlannedResponse::json(StatusCode::OK, message_payload("702", "100", "updated")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("file-token", 123, base_url);
    let body = sample_message();
    let files = vec![sample_file("hello.txt", "hello file")];

    assert_eq!(
        client
            .create_message_with_files(test_id(100), &body, &files)
            .await
            .expect("create message with files")
            .content,
        "created"
    );
    assert_eq!(
        client
            .update_message_with_files(test_id(100), test_id(701), &body, &files,)
            .await
            .expect("update message with files")
            .content,
        "updated"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 2);
    assert_multipart_request(
        &requests[0],
        "POST",
        "/channels/100/messages",
        Some("Bot file-token"),
    );
    assert!(requests[0].body.contains(r#"{"content":"hello"}"#));
    assert!(requests[0].body.contains(r#"filename="hello.txt""#));
    assert!(requests[0].body.contains("Content-Type: text/plain"));
    assert!(requests[0].body.contains("hello file"));
    assert_multipart_request(
        &requests[1],
        "PATCH",
        "/channels/100/messages/701",
        Some("Bot file-token"),
    );
    assert!(requests[1].body.contains(r#"filename="hello.txt""#));
}

#[tokio::test]
async fn tokenized_file_helpers_send_multipart_without_bot_authorization() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!({ "id": "800" })),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, message_payload("801", "500", "followup")),
        PlannedResponse::json(StatusCode::OK, message_payload("802", "500", "original")),
        PlannedResponse::json(
            StatusCode::OK,
            message_payload("803", "500", "followup-edit"),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("bot-token", 123, base_url);
    let files = vec![sample_file("tokenized.txt", "tokenized file")];

    assert_eq!(
        client
            .execute_webhook_with_files(
                test_id(777),
                "token",
                &json!({ "content": "webhook" }),
                &files,
            )
            .await
            .expect("execute webhook with files")["id"],
        json!("800")
    );
    client
        .create_interaction_response_with_files(
            test_id(778),
            "token",
            &crate::request::InteractionResponse::reply("ack"),
            &files,
        )
        .await
        .expect("create interaction response with files");
    assert_eq!(
        client
            .create_followup_message_with_files("token", &sample_followup(), &files)
            .await
            .expect("create followup with files")
            .content,
        "followup"
    );
    assert_eq!(
        client
            .edit_original_interaction_response_with_files("token", &sample_edit(), &files)
            .await
            .expect("edit original with files")
            .content,
        "original"
    );
    assert_eq!(
        client
            .edit_followup_message_with_files("token", "55", &sample_edit(), &files)
            .await
            .expect("edit followup with files")
            .content,
        "followup-edit"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 5);
    assert_multipart_request(&requests[0], "POST", "/webhooks/777/token?wait=true", None);
    assert_multipart_request(
        &requests[1],
        "POST",
        "/interactions/778/token/callback",
        None,
    );
    assert_multipart_request(&requests[2], "POST", "/webhooks/123/token", None);
    assert_multipart_request(
        &requests[3],
        "PATCH",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_multipart_request(
        &requests[4],
        "PATCH",
        "/webhooks/123/token/messages/55",
        None,
    );
    assert!(requests[0].body.contains(r#"{"content":"webhook"}"#));
    assert!(requests[1].body.contains(r#""type":4"#));
    assert!(requests[4].body.contains("tokenized file"));
}

#[tokio::test]
async fn webhook_message_crud_uses_tokenized_message_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "webhook")),
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "edited")),
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "edited-file")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("bot-token", 123, base_url);
    let body = sample_message();
    let files = vec![sample_file("webhook.txt", "webhook file")];

    assert_eq!(
        client
            .get_webhook_message(test_id(777), "token", "900")
            .await
            .expect("get webhook message")
            .content,
        "webhook"
    );
    assert_eq!(
        client
            .edit_webhook_message(test_id(777), "token", "900", &body)
            .await
            .expect("edit webhook message")
            .content,
        "edited"
    );
    assert_eq!(
        client
            .edit_webhook_message_with_files(test_id(777), "token", "900", &body, &files,)
            .await
            .expect("edit webhook message with files")
            .content,
        "edited-file"
    );
    client
        .delete_webhook_message(test_id(777), "token", "900")
        .await
        .expect("delete webhook message");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);
    assert_request_basics(
        &requests[0],
        "GET",
        "/webhooks/777/token/messages/900",
        None,
    );
    assert_request_basics(
        &requests[1],
        "PATCH",
        "/webhooks/777/token/messages/900",
        None,
    );
    assert_multipart_request(
        &requests[2],
        "PATCH",
        "/webhooks/777/token/messages/900",
        None,
    );
    assert_request_basics(
        &requests[3],
        "DELETE",
        "/webhooks/777/token/messages/900",
        None,
    );
    assert!(requests[2].body.contains("webhook file"));
}

#[tokio::test]
async fn webhook_message_query_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "threaded")),
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "edited")),
        PlannedResponse::json(StatusCode::OK, message_payload("900", "500", "edited-file")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("bot-token", 123, base_url);
    let body = sample_message();
    let files = vec![sample_file("webhook.txt", "webhook file")];
    let thread_query = WebhookMessageQuery {
        thread_id: Some(test_id(42)),
        with_components: None,
    };
    let edit_query = WebhookMessageQuery {
        thread_id: Some(test_id(42)),
        with_components: Some(true),
    };

    assert_eq!(
        client
            .get_webhook_message_with_query(test_id(777), "token", "900", &thread_query)
            .await
            .expect("get webhook message with query")
            .content,
        "threaded"
    );
    assert_eq!(
        client
            .edit_webhook_message_with_query(test_id(777), "token", "900", &edit_query, &body,)
            .await
            .expect("edit webhook message with query")
            .content,
        "edited"
    );
    assert_eq!(
        client
            .edit_webhook_message_with_files_and_query(
                test_id(777),
                "token",
                "900",
                &edit_query,
                &body,
                &files,
            )
            .await
            .expect("edit webhook message with files and query")
            .content,
        "edited-file"
    );
    client
        .delete_webhook_message_with_query(test_id(777), "token", "900", &thread_query)
        .await
        .expect("delete webhook message with query");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);
    assert_request_basics(
        &requests[0],
        "GET",
        "/webhooks/777/token/messages/900?thread_id=42",
        None,
    );
    assert_request_basics(
        &requests[1],
        "PATCH",
        "/webhooks/777/token/messages/900?thread_id=42&with_components=true",
        None,
    );
    assert_multipart_request(
        &requests[2],
        "PATCH",
        "/webhooks/777/token/messages/900?thread_id=42&with_components=true",
        None,
    );
    assert_request_basics(
        &requests[3],
        "DELETE",
        "/webhooks/777/token/messages/900?thread_id=42",
        None,
    );
    assert!(requests[2].body.contains("webhook file"));
}

#[tokio::test]
async fn webhook_management_and_compatible_execute_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!({ "id": "700", "type": 1 })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "700", "type": 1 })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "700", "type": 1 })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "800", "content": "query" })),
        PlannedResponse::json(StatusCode::OK, json!({ "ok": "slack" })),
        PlannedResponse::json(StatusCode::OK, json!({ "ok": "github" })),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("webhook-token", 123, base_url);
    let query = WebhookExecuteQuery {
        wait: Some(false),
        thread_id: Some(test_id(42)),
        with_components: Some(true),
    };
    let compatible_query = WebhookExecuteQuery {
        wait: Some(true),
        thread_id: Some(test_id(43)),
        with_components: Some(true),
    };

    assert_eq!(
        client
            .create_webhook(
                test_id(500),
                &CreateWebhook {
                    name: "deployments".to_string(),
                    avatar: Some(None),
                },
            )
            .await
            .expect("create webhook")
            .id
            .map(u64::from),
        Some(700)
    );
    client
        .modify_webhook(
            test_id(700),
            &ModifyWebhook {
                name: Some("ops".to_string()),
                avatar: Some(None),
                channel_id: Some(test_id(501)),
            },
        )
        .await
        .expect("modify webhook");
    client
        .modify_webhook_with_token(
            test_id(700),
            "token",
            &ModifyWebhookWithToken {
                name: Some("public".to_string()),
                avatar: Some(None),
            },
        )
        .await
        .expect("modify webhook with token");
    assert_eq!(
        client
            .execute_webhook_with_query(
                test_id(700),
                "token",
                &query,
                &json!({ "content": "query" }),
            )
            .await
            .expect("execute webhook with query")["id"],
        json!("800")
    );
    assert_eq!(
        client
            .execute_slack_compatible_webhook(
                test_id(700),
                "token",
                &compatible_query,
                &json!({ "text": "deploy" }),
            )
            .await
            .expect("execute slack-compatible webhook")["ok"],
        json!("slack")
    );
    assert_eq!(
        client
            .execute_github_compatible_webhook(
                test_id(700),
                "token",
                &compatible_query,
                &json!({ "zen": "Keep it logically awesome." }),
            )
            .await
            .expect("execute github-compatible webhook")["ok"],
        json!("github")
    );
    client
        .delete_webhook_with_token(test_id(700), "token")
        .await
        .expect("delete webhook with token");

    server.await.expect("server finished");
    {
        let requests = captured.lock().expect("captured requests");
        assert_eq!(requests.len(), 7);
        assert_request_basics(
            &requests[0],
            "POST",
            "/channels/500/webhooks",
            Some("Bot webhook-token"),
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap(),
            json!({ "name": "deployments", "avatar": null })
        );
        assert_request_basics(
            &requests[1],
            "PATCH",
            "/webhooks/700",
            Some("Bot webhook-token"),
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap(),
            json!({ "name": "ops", "avatar": null, "channel_id": "501" })
        );
        assert_request_basics(&requests[2], "PATCH", "/webhooks/700/token", None);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&requests[2].body).unwrap(),
            json!({ "name": "public", "avatar": null })
        );
        assert_request_basics(
            &requests[3],
            "POST",
            "/webhooks/700/token?wait=false&thread_id=42&with_components=true",
            None,
        );
        assert_request_basics(
            &requests[4],
            "POST",
            "/webhooks/700/token/slack?wait=true&thread_id=43",
            None,
        );
        assert_request_basics(
            &requests[5],
            "POST",
            "/webhooks/700/token/github?wait=true&thread_id=43",
            None,
        );
        assert_request_basics(&requests[6], "DELETE", "/webhooks/700/token", None);
    }

    assert!(client
        .modify_webhook_with_token(
            test_id(700),
            "bad/token",
            &ModifyWebhookWithToken::default()
        )
        .await
        .is_err());
    assert!(client
        .delete_webhook_with_token(test_id(700), "bad/token")
        .await
        .is_err());
}

#[tokio::test]
async fn typed_guild_admin_and_automod_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "banned_users": ["1"], "failed_users": ["2"] }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "reason": "spam",
                "user": user_payload("3", "banned")
            }]),
        ),
        PlannedResponse::json(StatusCode::OK, role_payload("9", "admin")),
        PlannedResponse::json(StatusCode::OK, json!([auto_moderation_rule_payload("7")])),
        PlannedResponse::json(StatusCode::OK, auto_moderation_rule_payload("7")),
        PlannedResponse::json(StatusCode::OK, auto_moderation_rule_payload("8")),
        PlannedResponse::json(StatusCode::OK, auto_moderation_rule_payload("8")),
        PlannedResponse::json(StatusCode::OK, guild_preview_payload()),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "application_commands": [],
                "audit_log_entries": [{
                    "id": "900",
                    "user_id": "901",
                    "target_id": "902",
                    "action_type": 192,
                    "changes": [{ "key": "status", "new_value": "live" }],
                    "options": { "channel_id": "903", "status": "live" },
                    "reason": "voice status"
                }],
                "auto_moderation_rules": [],
                "guild_scheduled_events": [],
                "integrations": [],
                "threads": [],
                "users": [{ "id": "901", "username": "moderator" }],
                "webhooks": []
            }),
        ),
        PlannedResponse::json(StatusCode::OK, json!({ "pruned": 3 })),
        PlannedResponse::json(StatusCode::OK, json!({ "pruned": 2 })),
        PlannedResponse::json(StatusCode::OK, json!({ "pruned": 4 })),
        PlannedResponse::json(StatusCode::OK, json!({ "code": "discordrs", "uses": 10 })),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "id": "rotterdam",
                "name": "Rotterdam",
                "optimal": true,
                "deprecated": false,
                "custom": false
            }]),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("typed-admin-token", 555, base_url);

    let ban = client
        .bulk_guild_ban(
            test_id(200),
            &BulkGuildBanRequest {
                user_ids: vec![test_id(1), test_id(2)],
                delete_message_seconds: Some(60),
            },
        )
        .await
        .unwrap();
    assert_eq!(ban.banned_users[0].get(), 1);
    let bans = client
        .get_guild_bans_with_query(
            test_id(200),
            &GuildBansQuery {
                limit: Some(100),
                before: Some(test_id(90)),
                after: Some(test_id(10)),
            },
        )
        .await
        .unwrap();
    assert_eq!(bans[0].reason.as_deref(), Some("spam"));
    assert_eq!(bans[0].user.as_ref().unwrap().id.get(), 3);
    assert_eq!(
        client
            .get_guild_role(test_id(200), test_id(9))
            .await
            .unwrap()
            .name,
        "admin"
    );
    assert_eq!(
        client
            .get_auto_moderation_rules(test_id(200))
            .await
            .unwrap()[0]
            .id
            .get(),
        7
    );
    assert_eq!(
        client
            .get_auto_moderation_rule(test_id(200), test_id(7))
            .await
            .unwrap()
            .name,
        "block bad words"
    );
    assert!(
        client
            .create_auto_moderation_rule(test_id(200), &sample_auto_moderation_rule("new"))
            .await
            .unwrap()
            .enabled
    );
    assert!(
        client
            .modify_auto_moderation_rule(
                test_id(200),
                test_id(8),
                &ModifyAutoModerationRule::new().name("updated"),
            )
            .await
            .unwrap()
            .enabled
    );
    assert_eq!(
        client
            .get_guild_preview(test_id(200))
            .await
            .unwrap()
            .approximate_member_count,
        100
    );
    let audit_log = client
        .get_guild_audit_log(
            test_id(200),
            &AuditLogQuery {
                user_id: Some(test_id(901)),
                action_type: Some(AuditLogActionType::VOICE_CHANNEL_STATUS_CREATE),
                before: Some(test_id(950)),
                after: Some(test_id(850)),
                limit: Some(25),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        audit_log.audit_log_entries[0].id.as_ref().unwrap().get(),
        900
    );
    assert_eq!(
        audit_log.audit_log_entries[0]
            .options
            .as_ref()
            .unwrap()
            .status
            .as_deref(),
        Some("live")
    );
    assert_eq!(audit_log.users[0].username, "moderator");
    assert_eq!(
        client
            .get_guild_prune_count(test_id(200), Some(7), &[test_id(9)])
            .await
            .unwrap()
            .pruned,
        Some(3)
    );
    assert_eq!(
        client
            .begin_guild_prune(
                test_id(200),
                &BeginGuildPruneRequest {
                    days: Some(7),
                    compute_prune_count: Some(false),
                    include_roles: None,
                    reason: None,
                },
            )
            .await
            .unwrap()
            .pruned,
        Some(2)
    );
    assert_eq!(
        client
            .begin_guild_prune(
                test_id(200),
                &BeginGuildPruneRequest {
                    days: Some(7),
                    compute_prune_count: Some(false),
                    include_roles: Some(vec![test_id(9)]),
                    reason: None,
                },
            )
            .await
            .unwrap()
            .pruned,
        Some(4)
    );
    assert_eq!(
        client
            .get_guild_vanity_url(test_id(200))
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("discordrs")
    );
    assert_eq!(client.get_voice_regions().await.unwrap()[0].id, "rotterdam");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 14);
    assert_request_basics(
        &requests[0],
        "POST",
        "/guilds/200/bulk-ban",
        Some("Bot typed-admin-token"),
    );
    assert!(requests[0].body.contains("delete_message_seconds"));
    assert_request_basics(
        &requests[1],
        "GET",
        "/guilds/200/bans?limit=100&before=90&after=10",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/guilds/200/roles/9",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[3],
        "GET",
        "/guilds/200/auto-moderation/rules",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[4],
        "GET",
        "/guilds/200/auto-moderation/rules/7",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[5],
        "POST",
        "/guilds/200/auto-moderation/rules",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[6],
        "PATCH",
        "/guilds/200/auto-moderation/rules/8",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[7],
        "GET",
        "/guilds/200/preview",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[8],
        "GET",
        "/guilds/200/audit-logs?user_id=901&action_type=192&before=950&after=850&limit=25",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[9],
        "GET",
        "/guilds/200/prune?days=7&include_roles=9",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[10],
        "POST",
        "/guilds/200/prune",
        Some("Bot typed-admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[10].body).unwrap(),
        json!({ "days": 7, "compute_prune_count": false })
    );
    assert_request_basics(
        &requests[11],
        "POST",
        "/guilds/200/prune",
        Some("Bot typed-admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[11].body).unwrap(),
        json!({
            "days": 7,
            "compute_prune_count": false,
            "include_roles": ["9"]
        })
    );
    assert_request_basics(
        &requests[12],
        "GET",
        "/guilds/200/vanity-url",
        Some("Bot typed-admin-token"),
    );
    assert_request_basics(
        &requests[13],
        "GET",
        "/voice/regions",
        Some("Bot typed-admin-token"),
    );
}

#[tokio::test]
async fn raw_auto_moderation_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([auto_moderation_rule_payload("7")])),
        PlannedResponse::json(StatusCode::OK, auto_moderation_rule_payload("8")),
        PlannedResponse::json(StatusCode::OK, auto_moderation_rule_payload("9")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("automod-token", 555, base_url);

    assert_eq!(
        client
            .get_auto_moderation_rules(test_id(200))
            .await
            .unwrap()[0]
            .id
            .get(),
        7
    );
    assert_eq!(
        client
            .create_auto_moderation_rule(
                test_id(200),
                &sample_auto_moderation_rule("block bad words")
            )
            .await
            .unwrap()
            .id
            .get(),
        8
    );
    assert_eq!(
        client
            .modify_auto_moderation_rule(
                test_id(200),
                test_id(9),
                &ModifyAutoModerationRule::new().name("block bad words"),
            )
            .await
            .unwrap()
            .id
            .get(),
        9
    );
    client
        .delete_auto_moderation_rule(test_id(200), test_id(9))
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let expected = [
        ("GET", "/guilds/200/auto-moderation/rules"),
        ("POST", "/guilds/200/auto-moderation/rules"),
        ("PATCH", "/guilds/200/auto-moderation/rules/9"),
        ("DELETE", "/guilds/200/auto-moderation/rules/9"),
    ];
    assert_eq!(requests.len(), expected.len());
    for (request, (method, path)) in requests.iter().zip(expected) {
        assert_request_basics(request, method, path, Some("Bot automod-token"));
    }
}

#[tokio::test]
#[allow(
    deprecated,
    reason = "covers the deprecated wrappers until they are removed"
)]
async fn raw_guild_admin_and_emoji_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(
            StatusCode::OK,
            json!([{ "reason": "spam", "user": user_payload("3", "banned") }]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "reason": "abuse", "user": user_payload("4", "banned-one") }),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, member_payload("6", "raw")),
        PlannedResponse::json(StatusCode::OK, member_payload("7", "typed")),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "application_commands": [],
                "audit_log_entries": [],
                "auto_moderation_rules": [],
                "guild_scheduled_events": [],
                "integrations": [],
                "threads": [],
                "users": [],
                "webhooks": []
            }),
        ),
        PlannedResponse::json(StatusCode::OK, json!([role_payload("9", "admin")])),
        PlannedResponse::json(StatusCode::OK, json!([role_payload("10", "mod")])),
        PlannedResponse::json(StatusCode::OK, json!([emoji_payload("1", "guild_raw")])),
        PlannedResponse::json(StatusCode::OK, emoji_payload("1", "guild_raw")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("2", "guild_create")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("2", "guild_edit")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "items": [emoji_payload("3", "app_raw")] }),
        ),
        PlannedResponse::json(StatusCode::OK, emoji_payload("3", "app_raw")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("4", "app_create")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("4", "app_edit")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("raw-admin-token", 555, base_url);

    assert_eq!(
        client
            .get_guild_bans(test_id(200), Some(2), Some(test_id(9)))
            .await
            .unwrap()[0]
            .user
            .as_ref()
            .unwrap()
            .username,
        "banned"
    );
    assert_eq!(
        client
            .get_guild_ban(test_id(200), test_id(4))
            .await
            .unwrap()
            .reason
            .as_deref(),
        Some("abuse")
    );
    client
        .create_guild_ban(
            test_id(200),
            test_id(4),
            &CreateGuildBan {
                delete_message_days: None,
                delete_message_seconds: Some(60),
            },
        )
        .await
        .unwrap();
    client
        .create_guild_ban(
            test_id(200),
            test_id(5),
            &CreateGuildBan {
                delete_message_days: None,
                delete_message_seconds: Some(120),
            },
        )
        .await
        .unwrap();
    client
        .remove_guild_ban(test_id(200), test_id(4))
        .await
        .unwrap();
    let member = client
        .modify_guild_member(
            test_id(200),
            test_id(6),
            &ModifyGuildMember {
                nick: Some(Some("raw".to_string())),
                ..ModifyGuildMember::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(member.user.unwrap().id.get(), 6);
    client
        .modify_guild_member(
            test_id(200),
            test_id(7),
            &ModifyGuildMember {
                nick: Some(Some("typed".to_string())),
                ..ModifyGuildMember::default()
            },
        )
        .await
        .unwrap();
    assert!(client
        .get_guild_audit_log(test_id(200), &AuditLogQuery::default())
        .await
        .unwrap()
        .audit_log_entries
        .is_empty());
    assert_eq!(
        client
            .modify_guild_role_positions(
                test_id(200),
                &[ModifyGuildRolePosition {
                    id: test_id(9),
                    position: Some(Some(1)),
                }],
            )
            .await
            .unwrap()[0]
            .name,
        "admin"
    );
    assert_eq!(
        client
            .modify_guild_role_positions(
                test_id(200),
                &[ModifyGuildRolePosition {
                    id: test_id(10),
                    position: Some(Some(2)),
                }],
            )
            .await
            .unwrap()[0]
            .name,
        "mod"
    );
    assert_eq!(
        client.get_guild_emojis(test_id(200)).await.unwrap()[0]
            .name
            .as_deref(),
        Some("guild_raw")
    );
    assert_eq!(
        client
            .get_guild_emoji(test_id(200), test_id(1))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(1)
    );
    assert_eq!(
        client
            .create_guild_emoji(
                test_id(200),
                &CreateGuildEmoji::new("emoji", "data:image/png;base64,AAAA"),
            )
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("guild_create")
    );
    assert_eq!(
        client
            .modify_guild_emoji(
                test_id(200),
                test_id(2),
                &ModifyGuildEmoji::new().name("emoji")
            )
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("guild_edit")
    );
    client
        .delete_guild_emoji(test_id(200), test_id(2))
        .await
        .unwrap();
    assert_eq!(
        client.get_application_emojis().await.unwrap()[0]
            .name
            .as_deref(),
        Some("app_raw")
    );
    assert_eq!(
        client
            .get_application_emoji(test_id(3))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(3)
    );
    assert_eq!(
        client
            .create_application_emoji(&CreateApplicationEmoji::new(
                "emoji",
                "data:image/png;base64,AAAA"
            ))
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("app_create")
    );
    assert_eq!(
        client
            .modify_application_emoji(test_id(4), &ModifyApplicationEmoji::new("emoji"))
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("app_edit")
    );
    client.delete_application_emoji(test_id(4)).await.unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let expected = [
        ("GET", "/guilds/200/bans?limit=2&before=9"),
        ("GET", "/guilds/200/bans/4"),
        ("PUT", "/guilds/200/bans/4"),
        ("PUT", "/guilds/200/bans/5"),
        ("DELETE", "/guilds/200/bans/4"),
        ("PATCH", "/guilds/200/members/6"),
        ("PATCH", "/guilds/200/members/7"),
        ("GET", "/guilds/200/audit-logs"),
        ("PATCH", "/guilds/200/roles"),
        ("PATCH", "/guilds/200/roles"),
        ("GET", "/guilds/200/emojis"),
        ("GET", "/guilds/200/emojis/1"),
        ("POST", "/guilds/200/emojis"),
        ("PATCH", "/guilds/200/emojis/2"),
        ("DELETE", "/guilds/200/emojis/2"),
        ("GET", "/applications/555/emojis"),
        ("GET", "/applications/555/emojis/3"),
        ("POST", "/applications/555/emojis"),
        ("PATCH", "/applications/555/emojis/4"),
        ("DELETE", "/applications/555/emojis/4"),
    ];
    assert_eq!(requests.len(), expected.len());
    for (request, (method, path)) in requests.iter().zip(expected) {
        assert_request_basics(request, method, path, Some("Bot raw-admin-token"));
    }
}

#[tokio::test]
async fn typed_emoji_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([emoji_payload("1", "guild_one")])),
        PlannedResponse::json(StatusCode::OK, emoji_payload("1", "guild_one")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("2", "guild_two")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("2", "guild_two_edit")),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "items": [emoji_payload("3", "app_one")] }),
        ),
        PlannedResponse::json(StatusCode::OK, emoji_payload("3", "app_one")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("4", "app_two")),
        PlannedResponse::json(StatusCode::OK, emoji_payload("4", "app_two_edit")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("emoji-token", 555, base_url);

    assert_eq!(
        client.get_guild_emojis(test_id(200)).await.unwrap()[0]
            .name
            .as_deref(),
        Some("guild_one")
    );
    assert_eq!(
        client
            .get_guild_emoji(test_id(200), test_id(1))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(1)
    );
    assert_eq!(
        client
            .create_guild_emoji(
                test_id(200),
                &CreateGuildEmoji::new("emoji", "data:image/png;base64,AAAA"),
            )
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("guild_two")
    );
    assert_eq!(
        client
            .modify_guild_emoji(
                test_id(200),
                test_id(2),
                &ModifyGuildEmoji::new().name("emoji")
            )
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("guild_two_edit")
    );
    assert_eq!(
        client.get_application_emojis().await.unwrap()[0]
            .name
            .as_deref(),
        Some("app_one")
    );
    assert_eq!(
        client
            .get_application_emoji(test_id(3))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(3)
    );
    assert_eq!(
        client
            .create_application_emoji(&CreateApplicationEmoji::new(
                "emoji",
                "data:image/png;base64,AAAA",
            ))
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("app_two")
    );
    assert_eq!(
        client
            .modify_application_emoji(test_id(4), &ModifyApplicationEmoji::new("emoji"))
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("app_two_edit")
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 8);
    assert_request_basics(
        &requests[0],
        "GET",
        "/guilds/200/emojis",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/guilds/200/emojis/1",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[2],
        "POST",
        "/guilds/200/emojis",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[3],
        "PATCH",
        "/guilds/200/emojis/2",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[4],
        "GET",
        "/applications/555/emojis",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[5],
        "GET",
        "/applications/555/emojis/3",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[6],
        "POST",
        "/applications/555/emojis",
        Some("Bot emoji-token"),
    );
    assert_request_basics(
        &requests[7],
        "PATCH",
        "/applications/555/emojis/4",
        Some("Bot emoji-token"),
    );
}

#[tokio::test]
async fn application_management_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, application_payload("555", "app")),
        PlannedResponse::json(StatusCode::OK, application_payload("555", "renamed")),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "application_id": "555",
                "instance_id": "i-123",
                "launch_id": "999",
                "location": {
                    "id": "gc-200-300",
                    "kind": "gc",
                    "channel_id": "300",
                    "guild_id": "200"
                },
                "users": ["400"]
            }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([role_connection_metadata_payload("score")]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([role_connection_metadata_payload("score")]),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("app-token", 555, base_url);
    let records = vec![ApplicationRoleConnectionMetadata::new(
        RoleConnectionMetadataType::INTEGER_LESS_THAN_OR_EQUAL,
        "score",
        "Score",
        "User score",
    )];

    assert_eq!(client.get_current_application().await.unwrap().name, "app");
    assert_eq!(
        client
            .edit_current_application(&ModifyCurrentApplication {
                description: Some("updated".to_string()),
                icon: Some(None),
                tags: Some(vec!["utility".to_string()]),
                ..ModifyCurrentApplication::default()
            })
            .await
            .unwrap()
            .name,
        "renamed"
    );
    let activity = client
        .get_application_activity_instance("i-123")
        .await
        .unwrap();
    assert_eq!(activity.location.kind, "gc");
    assert_eq!(activity.users[0].get(), 400);
    assert_eq!(
        client
            .get_application_role_connection_metadata_records()
            .await
            .unwrap()[0]
            .key,
        "score"
    );
    assert_eq!(
        client
            .update_application_role_connection_metadata_records(&records)
            .await
            .unwrap()[0]
            .description,
        "User score"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 5);
    assert_request_basics(
        &requests[0],
        "GET",
        "/applications/@me",
        Some("Bot app-token"),
    );
    assert_request_basics(
        &requests[1],
        "PATCH",
        "/applications/@me",
        Some("Bot app-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap(),
        json!({
            "description": "updated",
            "icon": null,
            "tags": ["utility"]
        })
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/applications/555/activity-instances/i-123",
        Some("Bot app-token"),
    );
    assert_request_basics(
        &requests[3],
        "GET",
        "/applications/555/role-connections/metadata",
        Some("Bot app-token"),
    );
    assert_request_basics(
        &requests[4],
        "PUT",
        "/applications/555/role-connections/metadata",
        Some("Bot app-token"),
    );
}

#[tokio::test]
async fn application_command_permissions_use_expected_auth_and_payloads() {
    let permissions_payload = json!({
        "id": "401",
        "application_id": "555",
        "guild_id": "200",
        "permissions": [{
            "id": "300",
            "type": 1,
            "permission": true
        }]
    });
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([permissions_payload.clone()])),
        PlannedResponse::json(StatusCode::OK, permissions_payload.clone()),
        PlannedResponse::json(StatusCode::OK, json!([permissions_payload.clone()])),
        PlannedResponse::json(StatusCode::OK, permissions_payload),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("command-token", 555, base_url);
    let edit = EditApplicationCommandPermissions::new([ApplicationCommandPermission::role(
        test_id(300),
        true,
    )]);

    let all = client
        .get_guild_application_command_permissions(test_id(200))
        .await
        .unwrap();
    assert_eq!(
        all[0].permissions[0],
        ApplicationCommandPermission::role(test_id(300), true)
    );

    let single = client
        .get_application_command_permissions(test_id(200), test_id(401))
        .await
        .unwrap();
    assert_eq!(single.guild_id.get(), 200);

    let batch_body = GuildApplicationCommandPermissions {
        id: CommandPermissionsId::Command(test_id(401)),
        application_id: test_id(555),
        guild_id: test_id(200),
        permissions: edit.permissions.clone(),
    };
    #[expect(deprecated, reason = "covers the disabled batch route while it exists")]
    let batch = client
        .batch_edit_application_command_permissions(
            "oauth.access-token",
            test_id(200),
            std::slice::from_ref(&batch_body),
        )
        .await
        .unwrap();
    assert_eq!(batch[0].id, CommandPermissionsId::Command(test_id(401)));

    let edited = client
        .edit_application_command_permissions(
            "oauth.access-token",
            test_id(200),
            test_id(401),
            &edit,
        )
        .await
        .unwrap();
    assert_eq!(edited.id, CommandPermissionsId::Command(test_id(401)));

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);
    assert_request_basics(
        &requests[0],
        "GET",
        "/applications/555/guilds/200/commands/permissions",
        Some("Bot command-token"),
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/applications/555/guilds/200/commands/401/permissions",
        Some("Bot command-token"),
    );
    assert_request_basics(
        &requests[2],
        "PUT",
        "/applications/555/guilds/200/commands/permissions",
        Some("Bearer oauth.access-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[2].body).unwrap(),
        json!([{
            "id": "401",
            "application_id": "555",
            "guild_id": "200",
            "permissions": [{
                "id": "300",
                "type": 1,
                "permission": true
            }]
        }])
    );
    assert_request_basics(
        &requests[3],
        "PUT",
        "/applications/555/guilds/200/commands/401/permissions",
        Some("Bearer oauth.access-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[3].body).unwrap(),
        json!({
            "permissions": [{
                "id": "300",
                "type": 1,
                "permission": true
            }]
        })
    );
}

#[tokio::test]
async fn application_command_permissions_reject_unsafe_bearer_tokens_before_network() {
    let (base_url, captured, server) = spawn_test_server(Vec::new()).await;
    let client = RestClient::new_with_base_url("command-token", 555, base_url);
    let edit = EditApplicationCommandPermissions::new([ApplicationCommandPermission::user(
        test_id(301),
        false,
    )]);

    let error = client
        .edit_application_command_permissions("bad\r\ntoken", test_id(200), test_id(401), &edit)
        .await
        .expect_err("control characters should be rejected before sending a request");
    assert!(error.to_string().contains("Authorization header"));

    server.await.expect("server finished");
    assert!(captured.lock().expect("captured requests").is_empty());
}

#[tokio::test]
async fn current_user_oauth_wrappers_use_bearer_auth_and_typed_models() {
    let role_connection = json!({
        "platform_name": "Example",
        "platform_username": "user-42",
        "metadata": {
            "level": "7"
        }
    });
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, user_payload("555", "current")),
        PlannedResponse::json(StatusCode::OK, user_payload("556", "target")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{ "id": "200", "name": "Guild", "icon": null, "banner": null, "owner": false, "permissions": "8", "features": [], "approximate_member_count": 10, "approximate_presence_count": 3 }]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{ "id": "201", "name": "Guild Page", "icon": null, "banner": null, "owner": true, "permissions": "8", "features": [] }]),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "id": "external-1",
                "name": "example-user",
                "type": "github",
                "revoked": false,
                "integrations": [{
                    "id": "33590653072239123",
                    "type": "twitch",
                    "account": { "id": "1234567", "name": "twitchusername" }
                }],
                "verified": true,
                "friend_sync": false,
                "show_activity": true,
                "two_way_link": true,
                "visibility": 1
            }]),
        ),
        PlannedResponse::json(StatusCode::OK, member_payload("701", "oauth-member")),
        PlannedResponse::json(StatusCode::OK, role_connection.clone()),
        PlannedResponse::json(StatusCode::OK, role_connection),
        PlannedResponse::json(StatusCode::OK, user_payload("900", "renamed")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("bot-token", 555, base_url);
    let role_update = UpdateUserApplicationRoleConnection::new()
        .platform_name("Example")
        .platform_username("user-42")
        .metadata([("level", "7")]);

    assert_eq!(client.get_current_user().await.unwrap().username, "current");
    assert_eq!(
        client.get_user(test_id(556)).await.unwrap().username,
        "target"
    );
    assert_eq!(client.get_current_user_guilds().await.unwrap().len(), 1);
    assert_eq!(
        client
            .get_current_user_guilds_with_query(&CurrentUserGuildsQuery {
                before: None,
                after: Some(test_id(200)),
                limit: Some(1),
                with_counts: Some(true),
                shard: None,
            })
            .await
            .unwrap()[0]
            .name,
        "Guild Page"
    );
    client.leave_guild(test_id(200)).await.unwrap();

    let connections = client
        .get_current_user_connections("oauth.access")
        .await
        .unwrap();
    assert_eq!(connections[0].kind, "github");
    let integration = &connections[0].integrations[0];
    assert_eq!(integration.id.get(), 33590653072239123);
    assert_eq!(integration.kind.as_deref(), Some("twitch"));
    assert_eq!(
        integration
            .account
            .as_ref()
            .map(|account| account.name.as_str()),
        Some("twitchusername")
    );

    let member = client
        .get_current_user_guild_member("oauth.access", test_id(200))
        .await
        .unwrap();
    assert_eq!(member.user.unwrap().username, "oauth-member");

    let role_connection = client
        .get_current_user_application_role_connection("oauth.access")
        .await
        .unwrap();
    assert_eq!(
        role_connection.metadata.get("level").map(String::as_str),
        Some("7")
    );

    let updated = client
        .update_current_user_application_role_connection("oauth.access", &role_update)
        .await
        .unwrap();
    assert_eq!(updated.platform_username.as_deref(), Some("user-42"));

    let user = client
        .modify_current_user(&ModifyCurrentUser {
            username: Some("renamed".to_string()),
            ..ModifyCurrentUser::default()
        })
        .await
        .unwrap();
    assert_eq!(user.username, "renamed");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 10);
    assert_request_basics(&requests[0], "GET", "/users/@me", Some("Bot bot-token"));
    assert_request_basics(&requests[1], "GET", "/users/556", Some("Bot bot-token"));
    assert_request_basics(
        &requests[2],
        "GET",
        "/users/@me/guilds",
        Some("Bot bot-token"),
    );
    assert_request_basics(
        &requests[3],
        "GET",
        "/users/@me/guilds?after=200&limit=1&with_counts=true",
        Some("Bot bot-token"),
    );
    assert_request_basics(
        &requests[4],
        "DELETE",
        "/users/@me/guilds/200",
        Some("Bot bot-token"),
    );
    assert_request_basics(
        &requests[5],
        "GET",
        "/users/@me/connections",
        Some("Bearer oauth.access"),
    );
    assert_request_basics(
        &requests[6],
        "GET",
        "/users/@me/guilds/200/member",
        Some("Bearer oauth.access"),
    );
    assert_request_basics(
        &requests[7],
        "GET",
        "/users/@me/applications/555/role-connection",
        Some("Bearer oauth.access"),
    );
    assert_request_basics(
        &requests[8],
        "PUT",
        "/users/@me/applications/555/role-connection",
        Some("Bearer oauth.access"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[8].body).unwrap(),
        json!({
            "platform_name": "Example",
            "platform_username": "user-42",
            "metadata": {
                "level": "7"
            }
        })
    );
    assert_request_basics(&requests[9], "PATCH", "/users/@me", Some("Bot bot-token"));
}

#[tokio::test]
async fn lobby_wrappers_cover_bot_and_bearer_authorized_routes() {
    let member = json!({
        "id": "42",
        "metadata": {
            "role": "host"
        },
        "flags": 1
    });
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, lobby_payload("900")),
        PlannedResponse::json(StatusCode::OK, lobby_payload("900")),
        PlannedResponse::json(StatusCode::OK, lobby_payload("900")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, member.clone()),
        PlannedResponse::json(StatusCode::OK, json!([member])),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, lobby_payload("900")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("lobby-token", 555, base_url);
    let metadata = HashMap::from([("topic".to_string(), "redstone".to_string())]);
    let member_metadata = HashMap::from([("role".to_string(), "host".to_string())]);

    let lobby = client
        .create_lobby(&CreateLobby {
            metadata: Some(metadata.clone()),
            members: Some(vec![LobbyMember {
                id: test_id(42),
                metadata: Some(member_metadata.clone()),
                flags: Some(1),
            }]),
            idle_timeout_seconds: Some(60),
        })
        .await
        .unwrap();
    assert_eq!(lobby.members[0].id.get(), 42);

    assert_eq!(client.get_lobby(test_id(900)).await.unwrap().id.get(), 900);
    assert_eq!(
        client
            .modify_lobby(
                test_id(900),
                &ModifyLobby {
                    metadata: Some(metadata.clone()),
                    members: None,
                    idle_timeout_seconds: Some(120),
                },
            )
            .await
            .unwrap()
            .application_id
            .get(),
        555
    );
    client.delete_lobby(test_id(900)).await.unwrap();

    let added = client
        .add_lobby_member(
            test_id(900),
            test_id(42),
            &AddLobbyMember {
                metadata: Some(member_metadata.clone()),
                flags: Some(1),
            },
        )
        .await
        .unwrap();
    assert_eq!(added.flags, Some(1));

    let updated = client
        .bulk_update_lobby_members(
            test_id(900),
            &[LobbyMemberUpdate {
                id: test_id(42),
                metadata: Some(member_metadata),
                flags: Some(1),
                remove_member: None,
            }],
        )
        .await
        .unwrap();
    assert_eq!(updated[0].id.get(), 42);

    client
        .remove_lobby_member(test_id(900), test_id(42))
        .await
        .unwrap();
    client
        .leave_lobby("oauth.access", test_id(900))
        .await
        .unwrap();
    client
        .link_lobby_channel(
            "oauth.access",
            test_id(900),
            &LinkLobbyChannel {
                channel_id: Some(test_id(700)),
            },
        )
        .await
        .unwrap();
    client
        .update_lobby_message_moderation_metadata(test_id(900), test_id(777), &metadata)
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 10);
    assert_request_basics(&requests[0], "POST", "/lobbies", Some("Bot lobby-token"));
    assert_request_basics(&requests[1], "GET", "/lobbies/900", Some("Bot lobby-token"));
    assert_request_basics(
        &requests[2],
        "PATCH",
        "/lobbies/900",
        Some("Bot lobby-token"),
    );
    assert_request_basics(
        &requests[3],
        "DELETE",
        "/lobbies/900",
        Some("Bot lobby-token"),
    );
    assert_request_basics(
        &requests[4],
        "PUT",
        "/lobbies/900/members/42",
        Some("Bot lobby-token"),
    );
    assert_request_basics(
        &requests[5],
        "POST",
        "/lobbies/900/members/bulk",
        Some("Bot lobby-token"),
    );
    assert_request_basics(
        &requests[6],
        "DELETE",
        "/lobbies/900/members/42",
        Some("Bot lobby-token"),
    );
    assert_request_basics(
        &requests[7],
        "DELETE",
        "/lobbies/900/members/@me",
        Some("Bearer oauth.access"),
    );
    assert_request_basics(
        &requests[8],
        "PATCH",
        "/lobbies/900/channel-linking",
        Some("Bearer oauth.access"),
    );
    assert_request_basics(
        &requests[9],
        "PUT",
        "/lobbies/900/messages/777/moderation-metadata",
        Some("Bot lobby-token"),
    );
}

#[tokio::test]
async fn gateway_and_oauth2_metadata_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!({ "url": "wss://gateway.discord.gg" })),
        PlannedResponse::json(StatusCode::OK, application_payload("555", "discordrs")),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "application": application_payload("555", "discordrs"),
                "scopes": ["identify", "guilds"],
                "expires": "2026-05-02T00:00:00+00:00",
                "user": user_payload("42", "oauth-user")
            }),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("metadata-token", 555, base_url);

    assert_eq!(
        client.get_gateway().await.unwrap().url,
        "wss://gateway.discord.gg"
    );
    assert_eq!(
        client
            .get_current_bot_application_information()
            .await
            .unwrap()
            .name,
        "discordrs"
    );
    let authorization = client
        .get_current_authorization_information("oauth.access")
        .await
        .unwrap();
    assert_eq!(authorization.scopes, ["identify", "guilds"]);
    assert_eq!(authorization.user.unwrap().username, "oauth-user");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 3);
    assert_request_basics(&requests[0], "GET", "/gateway", Some("Bot metadata-token"));
    assert_request_basics(
        &requests[1],
        "GET",
        "/oauth2/applications/@me",
        Some("Bot metadata-token"),
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/oauth2/@me",
        Some("Bearer oauth.access"),
    );
}

#[tokio::test]
async fn legacy_pin_template_and_nick_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, template_payload("tmpl")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("legacy-token", 555, base_url);

    client
        .pin_message_legacy(test_id(100), test_id(5))
        .await
        .unwrap();
    client
        .unpin_message_legacy(test_id(100), test_id(5))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_guild_template_by_code("tmpl")
            .await
            .unwrap()
            .code,
        "tmpl"
    );
    client
        .modify_current_member_nick(test_id(200), Some("nick"))
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);
    assert_request_basics(
        &requests[0],
        "PUT",
        "/channels/100/pins/5",
        Some("Bot legacy-token"),
    );
    assert_request_basics(
        &requests[1],
        "DELETE",
        "/channels/100/pins/5",
        Some("Bot legacy-token"),
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/guilds/templates/tmpl",
        Some("Bot legacy-token"),
    );
    assert_request_basics(
        &requests[3],
        "PATCH",
        "/guilds/200/members/@me/nick",
        Some("Bot legacy-token"),
    );
}

#[tokio::test]
#[allow(
    deprecated,
    reason = "covers the deprecated wrappers until they are removed"
)]
async fn channel_thread_permission_and_archive_wrappers_hit_expected_paths() {
    let thread_list_payload = || {
        json!({
            "threads": [channel_payload("900", 11, Some("thread"))],
            "members": [{
                "id": "900",
                "user_id": "42",
                "join_timestamp": "2024-01-01T00:00:00.000000+00:00",
                "flags": 0
            }],
            "has_more": false
        })
    };
    let thread_member_payload = || {
        json!({
            "id": "900",
            "user_id": "42",
            "join_timestamp": "2024-01-01T00:00:00.000000+00:00",
            "flags": 0
        })
    };
    let responses = vec![
        PlannedResponse::json(
            StatusCode::OK,
            json!([message_payload("5", "100", "pinned")]),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            channel_payload("900", 11, Some("from-message")),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            channel_payload("901", 11, Some("standalone")),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, thread_member_payload()),
        PlannedResponse::json(StatusCode::OK, json!([thread_member_payload()])),
        PlannedResponse::json(StatusCode::OK, json!([thread_member_payload()])),
        PlannedResponse::json(StatusCode::OK, thread_list_payload()),
        PlannedResponse::json(StatusCode::OK, thread_list_payload()),
        PlannedResponse::json(StatusCode::OK, thread_list_payload()),
        PlannedResponse::json(StatusCode::OK, thread_list_payload()),
        PlannedResponse::json(StatusCode::OK, thread_list_payload()),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("thread-token", 555, base_url);

    assert_eq!(
        client.get_pinned_messages(test_id(100)).await.unwrap()[0].content,
        "pinned"
    );
    client
        .set_voice_channel_status(
            test_id(100),
            &SetVoiceChannelStatus {
                status: Some("focus".to_string()),
            },
        )
        .await
        .unwrap();
    client.trigger_typing_indicator(test_id(100)).await.unwrap();
    client
        .edit_channel_permissions(
            test_id(100),
            OverwriteTarget::Role(test_id(200)),
            &EditChannelPermission {
                kind: OverwriteType::ROLE,
                allow: Some(Permissions::from_bits(8)),
                deny: None,
            },
        )
        .await
        .unwrap();
    client
        .edit_channel_permissions(
            test_id(100),
            OverwriteTarget::Member(test_id(201)),
            &EditChannelPermission {
                kind: OverwriteType::MEMBER,
                allow: Some(Permissions::from_bits(8)),
                deny: None,
            },
        )
        .await
        .unwrap();
    client
        .delete_channel_permission(test_id(100), OverwriteTarget::Member(test_id(202)))
        .await
        .unwrap();
    assert_eq!(
        client
            .create_thread_from_message(
                test_id(100),
                test_id(5),
                &StartThreadFromMessage::new("from-message"),
            )
            .await
            .unwrap()
            .id
            .get(),
        900
    );
    assert_eq!(
        client
            .create_thread(test_id(100), &StartThreadWithoutMessage::new("standalone"))
            .await
            .unwrap()
            .id
            .get(),
        901
    );
    client.join_thread(test_id(900)).await.unwrap();
    client
        .add_thread_member(test_id(900), test_id(42))
        .await
        .unwrap();
    client.leave_thread(test_id(900)).await.unwrap();
    client
        .remove_thread_member(test_id(900), test_id(42))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_thread_member(test_id(900), test_id(42), Some(true))
            .await
            .unwrap()
            .user_id
            .unwrap()
            .get(),
        42
    );
    assert_eq!(
        client.get_thread_members(test_id(900)).await.unwrap()[0].flags,
        0
    );
    assert_eq!(
        client
            .list_thread_members(
                test_id(900),
                &ThreadMemberQuery {
                    with_member: Some(false),
                    after: Some(test_id(41)),
                    limit: Some(1),
                },
            )
            .await
            .unwrap()[0]
            .id
            .as_ref()
            .unwrap()
            .get(),
        900
    );
    assert!(
        !client
            .list_public_archived_threads(
                test_id(100),
                &ArchivedThreadsQuery {
                    before: None,
                    limit: Some(2),
                },
            )
            .await
            .unwrap()
            .has_more
    );
    assert!(
        !client
            .list_public_archived_threads(
                test_id(100),
                &ArchivedThreadsQuery {
                    before: Some("abc".to_string()),
                    limit: Some(3),
                },
            )
            .await
            .unwrap()
            .has_more
    );
    assert!(
        !client
            .list_private_archived_threads(
                test_id(100),
                &ArchivedThreadsQuery {
                    before: Some("def".to_string()),
                    limit: Some(4),
                },
            )
            .await
            .unwrap()
            .has_more
    );
    assert!(
        !client
            .list_joined_private_archived_threads(
                test_id(100),
                &JoinedArchivedThreadsQuery {
                    before: Some(test_id(899)),
                    limit: Some(5),
                },
            )
            .await
            .unwrap()
            .has_more
    );
    assert!(
        !client
            .get_active_guild_threads(test_id(300))
            .await
            .unwrap()
            .has_more
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let expected = [
        ("GET", "/channels/100/pins"),
        ("PUT", "/channels/100/voice-status"),
        ("POST", "/channels/100/typing"),
        ("PUT", "/channels/100/permissions/200"),
        ("PUT", "/channels/100/permissions/201"),
        ("DELETE", "/channels/100/permissions/202"),
        ("POST", "/channels/100/messages/5/threads"),
        ("POST", "/channels/100/threads"),
        ("PUT", "/channels/900/thread-members/@me"),
        ("PUT", "/channels/900/thread-members/42"),
        ("DELETE", "/channels/900/thread-members/@me"),
        ("DELETE", "/channels/900/thread-members/42"),
        ("GET", "/channels/900/thread-members/42?with_member=true"),
        ("GET", "/channels/900/thread-members"),
        (
            "GET",
            "/channels/900/thread-members?with_member=false&after=41&limit=1",
        ),
        ("GET", "/channels/100/threads/archived/public?limit=2"),
        (
            "GET",
            "/channels/100/threads/archived/public?before=abc&limit=3",
        ),
        (
            "GET",
            "/channels/100/threads/archived/private?before=def&limit=4",
        ),
        (
            "GET",
            "/channels/100/users/@me/threads/archived/private?before=899&limit=5",
        ),
        ("GET", "/guilds/300/threads/active"),
    ];
    assert_eq!(requests.len(), expected.len());
    for (request, (method, path)) in requests.iter().zip(expected) {
        assert_request_basics(request, method, path, Some("Bot thread-token"));
    }
}

#[tokio::test]
async fn remaining_admin_gap_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, member_payload("700", "bot-member")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([current_user_guild_payload("200", "guild")]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([current_user_guild_payload("201", "guild-with-counts")]),
        ),
        PlannedResponse::json(StatusCode::OK, webhook_payload("300", "hook")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "id": "sydney",
                "name": "Sydney",
                "optimal": false,
                "deprecated": false,
                "custom": true
            }]),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("gap-token", 555, base_url);

    assert_eq!(
        client
            .modify_current_member(
                test_id(200),
                &ModifyCurrentMember {
                    nick: Some(Some("bot".to_string())),
                    ..ModifyCurrentMember::default()
                },
            )
            .await
            .unwrap()
            .user
            .unwrap()
            .username,
        "bot-member"
    );
    assert_eq!(
        client.get_current_user_guilds().await.unwrap()[0].name,
        "guild"
    );
    let guilds_with_counts = client
        .get_current_user_guilds_with_query(&CurrentUserGuildsQuery {
            after: Some(test_id(199)),
            limit: Some(50),
            with_counts: Some(true),
            ..CurrentUserGuildsQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(guilds_with_counts[0].name, "guild-with-counts");
    assert_eq!(guilds_with_counts[0].banner.as_deref(), Some("banner_hash"));
    assert_eq!(guilds_with_counts[0].approximate_member_count, Some(3268));
    assert_eq!(guilds_with_counts[0].approximate_presence_count, Some(784));
    assert_eq!(
        client
            .get_webhook_with_token(test_id(300), "token")
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("hook")
    );
    assert_eq!(
        client.get_guild_voice_regions(test_id(200)).await.unwrap()[0].id,
        "sydney"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 5);
    assert_request_basics(
        &requests[0],
        "PATCH",
        "/guilds/200/members/@me",
        Some("Bot gap-token"),
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/users/@me/guilds",
        Some("Bot gap-token"),
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/users/@me/guilds?after=199&limit=50&with_counts=true",
        Some("Bot gap-token"),
    );
    assert_request_basics(&requests[3], "GET", "/webhooks/300/token", None);
    assert_request_basics(
        &requests[4],
        "GET",
        "/guilds/200/regions",
        Some("Bot gap-token"),
    );
}

#[tokio::test]
async fn voice_state_resource_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, voice_state_payload("555", "300")),
        PlannedResponse::json(StatusCode::OK, voice_state_payload("777", "300")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("voice-state-token", 555, base_url);

    assert_eq!(
        client
            .get_current_user_voice_state(test_id(200))
            .await
            .unwrap()
            .user_id
            .unwrap()
            .get(),
        555
    );
    assert_eq!(
        client
            .get_user_voice_state(test_id(200), test_id(777))
            .await
            .unwrap()
            .user_id
            .unwrap()
            .get(),
        777
    );
    client
        .modify_current_user_voice_state(
            test_id(200),
            &ModifyCurrentUserVoiceState {
                channel_id: Some(test_id(300)),
                suppress: Some(false),
                request_to_speak_timestamp: Some(None),
            },
        )
        .await
        .unwrap();
    client
        .modify_user_voice_state(
            test_id(200),
            test_id(777),
            &ModifyUserVoiceState {
                channel_id: Some(test_id(300)),
                suppress: Some(true),
            },
        )
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);
    assert_request_basics(
        &requests[0],
        "GET",
        "/guilds/200/voice-states/@me",
        Some("Bot voice-state-token"),
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/guilds/200/voice-states/777",
        Some("Bot voice-state-token"),
    );
    assert_request_basics(
        &requests[2],
        "PATCH",
        "/guilds/200/voice-states/@me",
        Some("Bot voice-state-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[2].body).unwrap(),
        json!({
            "channel_id": "300",
            "suppress": false,
            "request_to_speak_timestamp": null
        })
    );
    assert_request_basics(
        &requests[3],
        "PATCH",
        "/guilds/200/voice-states/777",
        Some("Bot voice-state-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[3].body).unwrap(),
        json!({
            "channel_id": "300",
            "suppress": true
        })
    );
}

#[tokio::test]
async fn channel_resource_gap_wrappers_hit_current_routes() {
    let responses = vec![
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "code": "abc",
                "uses": 1,
                "max_uses": 10,
                "max_age": 3600,
                "temporary": false,
                "created_at": "2026-05-01T00:00:00.000000+00:00"
            }]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "code": "created",
                "max_age": 600,
                "max_uses": 2,
                "temporary": true
            }),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("channel-token", 0, base_url);

    assert_eq!(
        client.get_channel_invites(test_id(100)).await.unwrap()[0]
            .code
            .as_deref(),
        Some("abc")
    );
    assert_eq!(
        client
            .create_channel_invite(
                test_id(100),
                &CreateChannelInvite {
                    max_age: Some(600),
                    max_uses: Some(2),
                    temporary: Some(true),
                    unique: Some(true),
                    role_ids: Some(vec![test_id(300)]),
                    ..CreateChannelInvite::default()
                },
            )
            .await
            .unwrap()
            .code
            .as_deref(),
        Some("created")
    );
    client
        .set_voice_channel_status(
            test_id(200),
            &SetVoiceChannelStatus {
                status: Some("Planning sprint".to_string()),
            },
        )
        .await
        .unwrap();
    client
        .edit_channel_permissions(
            test_id(100),
            OverwriteTarget::Role(test_id(300)),
            &EditChannelPermission {
                kind: OverwriteType::ROLE,
                allow: Some(Permissions::from_bits(1024)),
                deny: Some(Permissions::from_bits(2048)),
            },
        )
        .await
        .unwrap();
    client
        .delete_channel_permission(test_id(100), OverwriteTarget::Role(test_id(300)))
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 5);
    assert_request_basics(
        &requests[0],
        "GET",
        "/channels/100/invites",
        Some("Bot channel-token"),
    );
    assert_request_basics(
        &requests[1],
        "POST",
        "/channels/100/invites",
        Some("Bot channel-token"),
    );
    assert!(requests[1].body.contains(r#""role_ids":["300"]"#));
    assert_request_basics(
        &requests[2],
        "PUT",
        "/channels/200/voice-status",
        Some("Bot channel-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[2].body).unwrap()["status"],
        json!("Planning sprint")
    );
    assert_request_basics(
        &requests[3],
        "PUT",
        "/channels/100/permissions/300",
        Some("Bot channel-token"),
    );
    let permission_body = serde_json::from_str::<serde_json::Value>(&requests[3].body).unwrap();
    assert_eq!(permission_body["type"], json!(0));
    assert_eq!(permission_body["allow"], json!("1024"));
    assert_eq!(permission_body["deny"], json!("2048"));
    assert_request_basics(
        &requests[4],
        "DELETE",
        "/channels/100/permissions/300",
        Some("Bot channel-token"),
    );
}

#[tokio::test]
async fn sticker_stage_and_guild_admin_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!({ "items": [{ "id": "1" }] })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "1" })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "2" })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "2", "name": "renamed" })),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, sticker_payload("10")),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "sticker_packs": [{
                    "id": "20",
                    "name": "pack",
                    "stickers": [sticker_payload("10")]
                }]
            }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "id": "20",
                "name": "pack",
                "stickers": [sticker_payload("10")]
            }),
        ),
        PlannedResponse::json(StatusCode::OK, json!([sticker_payload("11")])),
        PlannedResponse::json(StatusCode::OK, sticker_payload("11")),
        PlannedResponse::json(StatusCode::OK, sticker_payload("12")),
        PlannedResponse::json(StatusCode::OK, sticker_payload("12")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, json!({ "pruned": 3 })),
        PlannedResponse::json(StatusCode::OK, json!({ "pruned": 2 })),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "enabled": true, "channel_id": "300" }),
        ),
        PlannedResponse::json(StatusCode::OK, json!({ "enabled": false })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "200", "name": "widget" })),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "id": "200",
                "name": "typed widget",
                "instant_invite": "https://discord.gg/example",
                "channels": [{
                    "id": "300",
                    "name": "voice",
                    "position": 1
                }],
                "members": [{
                    "id": "0",
                    "username": "anon",
                    "discriminator": "0000",
                    "avatar": null,
                    "status": "online",
                    "avatar_url": "https://cdn.discordapp.com/widget-avatars/avatar"
                }],
                "presence_count": 1
            }),
        ),
        PlannedResponse::text(StatusCode::OK, "PNGDATA"),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "channel_id": "100", "webhook_id": "101" }),
        ),
        PlannedResponse::json(StatusCode::OK, stage_payload("400")),
        PlannedResponse::json(StatusCode::OK, stage_payload("400")),
        PlannedResponse::json(StatusCode::OK, stage_payload("400")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, welcome_screen_payload()),
        PlannedResponse::json(StatusCode::OK, welcome_screen_payload()),
        PlannedResponse::json(StatusCode::OK, onboarding_payload()),
        PlannedResponse::json(StatusCode::OK, onboarding_payload()),
        PlannedResponse::json(StatusCode::OK, incidents_payload()),
        PlannedResponse::json(StatusCode::OK, json!([template_payload("tmpl")])),
        PlannedResponse::json(StatusCode::OK, template_payload("tmpl")),
        PlannedResponse::json(StatusCode::OK, template_payload("tmpl")),
        PlannedResponse::json(StatusCode::OK, template_payload("tmpl")),
        PlannedResponse::json(StatusCode::OK, template_payload("tmpl")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("admin-token", 555, base_url);

    assert_eq!(client.get_application_emojis().await.unwrap().len(), 1);
    assert_eq!(
        client
            .get_application_emoji(test_id(1))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(1)
    );
    assert_eq!(
        client
            .create_application_emoji(&CreateApplicationEmoji::new(
                "name",
                "data:image/png;base64,AAAA"
            ))
            .await
            .unwrap()
            .id
            .map(u64::from),
        Some(2)
    );
    assert_eq!(
        client
            .modify_application_emoji(test_id(2), &ModifyApplicationEmoji::new("renamed"))
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("renamed")
    );
    client.delete_application_emoji(test_id(2)).await.unwrap();
    assert_eq!(
        client.get_sticker(test_id(10)).await.unwrap().name,
        "sticker"
    );
    assert_eq!(
        client
            .list_sticker_packs()
            .await
            .unwrap()
            .sticker_packs
            .len(),
        1
    );
    assert_eq!(
        client.get_sticker_pack(test_id(20)).await.unwrap().id.get(),
        20
    );
    assert_eq!(
        client.get_guild_stickers(test_id(200)).await.unwrap().len(),
        1
    );
    assert_eq!(
        client
            .get_guild_sticker(test_id(200), test_id(11))
            .await
            .unwrap()
            .id
            .get(),
        11
    );
    assert_eq!(
        client
            .create_guild_sticker(
                test_id(200),
                &CreateGuildSticker {
                    name: "name".to_string(),
                    description: "desc".to_string(),
                    tags: "tag".to_string(),
                },
                sample_file("sticker.png", "png")
            )
            .await
            .unwrap()
            .id
            .get(),
        12
    );
    assert_eq!(
        client
            .modify_guild_sticker(
                test_id(200),
                test_id(12),
                &ModifyGuildSticker {
                    name: Some("name".to_string()),
                    description: Some(Some("desc".to_string())),
                    tags: Some("tag".to_string()),
                },
            )
            .await
            .unwrap()
            .id
            .get(),
        12
    );
    client
        .delete_guild_sticker(test_id(200), test_id(12))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_guild_prune_count(test_id(200), Some(7), &[test_id(9)])
            .await
            .unwrap()
            .pruned,
        Some(3)
    );
    assert_eq!(
        client
            .begin_guild_prune(
                test_id(200),
                &BeginGuildPruneRequest {
                    days: Some(7),
                    compute_prune_count: Some(false),
                    include_roles: None,
                    reason: None,
                },
            )
            .await
            .unwrap()
            .pruned,
        Some(2)
    );
    assert!(
        client
            .get_guild_widget_settings(test_id(200))
            .await
            .unwrap()
            .enabled
    );
    assert!(
        !client
            .modify_guild_widget_settings(
                test_id(200),
                &ModifyGuildWidgetSettings {
                    enabled: Some(false),
                    ..ModifyGuildWidgetSettings::default()
                },
            )
            .await
            .unwrap()
            .enabled
    );
    assert_eq!(
        client.get_guild_widget(test_id(200)).await.unwrap().name,
        "widget"
    );
    assert_eq!(
        client.get_guild_widget(test_id(200)).await.unwrap().members[0]
            .status
            .as_deref(),
        Some("online")
    );
    assert_eq!(
        client
            .get_guild_widget_image(test_id(200), Some(GuildWidgetImageStyle::Banner2))
            .await
            .unwrap(),
        b"PNGDATA".to_vec()
    );
    assert_eq!(
        client
            .follow_announcement_channel(test_id(100), test_id(101))
            .await
            .unwrap()
            .webhook_id
            .get(),
        101
    );
    assert_eq!(
        client
            .create_stage_instance(&CreateStageInstance {
                channel_id: test_id(400),
                topic: "town hall".to_string(),
                privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
                send_start_notification: Some(true),
                guild_scheduled_event_id: Some(test_id(500)),
            })
            .await
            .unwrap()
            .channel_id
            .get(),
        400
    );
    assert_eq!(
        client.get_stage_instance(test_id(400)).await.unwrap().topic,
        "town hall"
    );
    assert_eq!(
        client
            .modify_stage_instance(
                test_id(400),
                &ModifyStageInstance {
                    privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
                },
            )
            .await
            .unwrap()
            .privacy_level,
        StageInstancePrivacyLevel::GUILD_ONLY
    );
    client.delete_stage_instance(test_id(400)).await.unwrap();
    assert_eq!(
        client
            .get_guild_welcome_screen(test_id(200))
            .await
            .unwrap()
            .welcome_channels
            .len(),
        1
    );
    assert_eq!(
        client
            .modify_guild_welcome_screen(
                test_id(200),
                &ModifyGuildWelcomeScreen {
                    description: Some(Some("welcome".to_string())),
                    ..ModifyGuildWelcomeScreen::default()
                },
            )
            .await
            .unwrap()
            .description
            .as_deref(),
        Some("welcome")
    );
    assert!(
        client
            .get_guild_onboarding(test_id(200))
            .await
            .unwrap()
            .enabled
    );
    assert!(
        client
            .modify_guild_onboarding(
                test_id(200),
                &ModifyGuildOnboarding {
                    enabled: Some(true),
                    ..ModifyGuildOnboarding::default()
                },
            )
            .await
            .unwrap()
            .enabled
    );
    assert_eq!(
        client
            .modify_guild_incident_actions(
                test_id(200),
                &ModifyGuildIncidentActions {
                    invites_disabled_until: Some("2026-05-01T12:00:00.000000+00:00".to_string()),
                    dms_disabled_until: None,
                },
            )
            .await
            .unwrap()
            .raid_detected_at
            .as_deref(),
        Some("2026-05-01T11:00:00.000000+00:00")
    );
    assert_eq!(
        client
            .get_guild_templates(test_id(200))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        client
            .create_guild_template(test_id(200), &CreateGuildTemplate::new("name"))
            .await
            .unwrap()
            .code,
        "tmpl"
    );
    assert_eq!(
        client
            .sync_guild_template(test_id(200), "tmpl")
            .await
            .unwrap()
            .code,
        "tmpl"
    );
    assert_eq!(
        client
            .modify_guild_template(
                test_id(200),
                "tmpl",
                &ModifyGuildTemplate::new().description("desc"),
            )
            .await
            .unwrap()
            .code,
        "tmpl"
    );
    assert_eq!(
        client
            .delete_guild_template(test_id(200), "tmpl")
            .await
            .unwrap()
            .code,
        "tmpl"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 35);
    assert_request_basics(
        &requests[0],
        "GET",
        "/applications/555/emojis",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[4],
        "DELETE",
        "/applications/555/emojis/2",
        Some("Bot admin-token"),
    );
    assert_request_basics(&requests[5], "GET", "/stickers/10", Some("Bot admin-token"));
    assert_request_basics(
        &requests[7],
        "GET",
        "/sticker-packs/20",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[8],
        "GET",
        "/guilds/200/stickers",
        Some("Bot admin-token"),
    );
    assert_eq!(requests[10].method, "POST");
    assert_eq!(requests[10].path, "/guilds/200/stickers");
    assert_eq!(
        requests[10].header("authorization"),
        Some("Bot admin-token")
    );
    assert!(requests[10]
        .header("content-type")
        .is_some_and(|value| value.starts_with("multipart/form-data; boundary=")));
    assert!(requests[10].body.contains(r#"name="file""#));
    assert_request_basics(
        &requests[11],
        "PATCH",
        "/guilds/200/stickers/12",
        Some("Bot admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[11].body).unwrap(),
        json!({
            "name": "name",
            "description": "desc",
            "tags": "tag"
        })
    );
    assert_request_basics(
        &requests[13],
        "GET",
        "/guilds/200/prune?days=7&include_roles=9",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[14],
        "POST",
        "/guilds/200/prune",
        Some("Bot admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[14].body).unwrap(),
        json!({ "days": 7, "compute_prune_count": false })
    );
    assert_request_basics(
        &requests[18],
        "GET",
        "/guilds/200/widget.json",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[19],
        "GET",
        "/guilds/200/widget.png?style=banner2",
        None,
    );
    assert_request_basics(
        &requests[20],
        "POST",
        "/channels/100/followers",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[21],
        "POST",
        "/stage-instances",
        Some("Bot admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[21].body).unwrap(),
        json!({
            "channel_id": "400",
            "topic": "town hall",
            "privacy_level": 2,
            "send_start_notification": true,
            "guild_scheduled_event_id": "500"
        })
    );
    assert_request_basics(
        &requests[23],
        "PATCH",
        "/stage-instances/400",
        Some("Bot admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[23].body).unwrap(),
        json!({ "privacy_level": 2 })
    );
    assert_request_basics(
        &requests[25],
        "GET",
        "/guilds/200/welcome-screen",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[28],
        "PUT",
        "/guilds/200/onboarding",
        Some("Bot admin-token"),
    );
    assert_request_basics(
        &requests[29],
        "PUT",
        "/guilds/200/incident-actions",
        Some("Bot admin-token"),
    );
    assert!(requests[29]
        .body
        .contains(r#""invites_disabled_until":"2026-05-01T12:00:00.000000+00:00""#));
    assert!(requests[29].body.contains(r#""dms_disabled_until":null"#));
    assert_request_basics(
        &requests[32],
        "PUT",
        "/guilds/200/templates/tmpl",
        Some("Bot admin-token"),
    );
}

#[tokio::test]
async fn raw_sticker_wrappers_preserve_multipart_and_json_bodies() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, sticker_payload("13")),
        PlannedResponse::json(StatusCode::OK, sticker_payload("13")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("admin-token", 555, base_url);
    let body = json!({ "name": "name", "description": "desc", "tags": "tag" });
    let sticker = CreateGuildSticker {
        name: "name".to_string(),
        description: "desc".to_string(),
        tags: "tag".to_string(),
    };

    assert_eq!(
        client
            .create_guild_sticker(test_id(200), &sticker, sample_file("sticker.png", "png"))
            .await
            .unwrap()
            .id
            .get(),
        13
    );
    assert_eq!(
        client
            .modify_guild_sticker(
                test_id(200),
                test_id(13),
                &ModifyGuildSticker {
                    name: Some("name".to_string()),
                    description: Some(Some("desc".to_string())),
                    tags: Some("tag".to_string()),
                },
            )
            .await
            .unwrap()
            .id
            .get(),
        13
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/guilds/200/stickers");
    assert_eq!(requests[0].header("authorization"), Some("Bot admin-token"));
    assert_eq!(
        requests[0].header("user-agent"),
        Some(concat!(
            "DiscordBot (discordrs, ",
            env!("CARGO_PKG_VERSION"),
            ")"
        ))
    );
    assert!(requests[0]
        .header("content-type")
        .is_some_and(|value| value.starts_with("multipart/form-data; boundary=")));
    assert!(requests[0].body.contains(r#"name="name""#));
    assert!(requests[0].body.contains(r#"name="description""#));
    assert!(requests[0].body.contains(r#"name="tags""#));
    assert!(requests[0].body.contains(r#"name="file""#));
    assert_request_basics(
        &requests[1],
        "PATCH",
        "/guilds/200/stickers/13",
        Some("Bot admin-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap(),
        body
    );
}

#[tokio::test]
async fn scheduled_event_wrappers_return_typed_models() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([scheduled_event_payload("1")])),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("2")),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("2")),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("2")),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("2")),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("3")),
        PlannedResponse::json(StatusCode::OK, scheduled_event_payload("3")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "guild_scheduled_event_id": "2",
                "user": {
                    "id": "500",
                    "username": "attendee",
                    "discriminator": "0000",
                    "bot": false
                }
            }]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "guild_scheduled_event_id": "2",
                "user": {
                    "id": "501",
                    "username": "standby",
                    "discriminator": "0000",
                    "bot": false
                }
            }]),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("scheduled-token", 0, base_url);
    let body = CreateGuildScheduledEvent::new(
        "community night",
        ScheduledEventEntityType::EXTERNAL,
        ScheduledEventPrivacyLevel::GUILD_ONLY,
        "2026-05-01T18:00:00Z",
    );
    let modify = ModifyGuildScheduledEvent::new().name("community night");

    assert_eq!(
        client
            .get_guild_scheduled_events(test_id(200))
            .await
            .unwrap()[0]
            .name,
        "community night"
    );
    assert_eq!(
        client
            .create_guild_scheduled_event(test_id(200), &body)
            .await
            .unwrap()
            .entity_metadata
            .and_then(|metadata| metadata.location),
        Some("Stage".to_string())
    );
    assert_eq!(
        client
            .get_guild_scheduled_event(test_id(200), test_id(2))
            .await
            .unwrap()
            .recurrence_rule
            .map(|rule| rule.interval),
        Some(1)
    );
    assert_eq!(
        client
            .modify_guild_scheduled_event(test_id(200), test_id(2), &modify)
            .await
            .unwrap()
            .status,
        ScheduledEventStatus::SCHEDULED
    );
    assert_eq!(
        client
            .get_guild_scheduled_event(test_id(200), test_id(2))
            .await
            .unwrap()
            .user_count,
        Some(5)
    );
    assert_eq!(
        client
            .create_guild_scheduled_event(test_id(200), &body)
            .await
            .unwrap()
            .id
            .get(),
        3
    );
    assert_eq!(
        client
            .modify_guild_scheduled_event(test_id(200), test_id(3), &modify)
            .await
            .unwrap()
            .id
            .get(),
        3
    );
    client
        .delete_guild_scheduled_event(test_id(200), test_id(2))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_guild_scheduled_event_users(test_id(200), test_id(2), Some(50))
            .await
            .unwrap()[0]
            .user
            .username,
        "attendee"
    );
    assert_eq!(
        client
            .get_guild_scheduled_event_users(test_id(200), test_id(2), None)
            .await
            .unwrap()[0]
            .user
            .username,
        "standby"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 10);
    assert_request_basics(
        &requests[0],
        "GET",
        "/guilds/200/scheduled-events",
        Some("Bot scheduled-token"),
    );
    assert_request_basics(
        &requests[5],
        "POST",
        "/guilds/200/scheduled-events",
        Some("Bot scheduled-token"),
    );
    assert_request_basics(
        &requests[6],
        "PATCH",
        "/guilds/200/scheduled-events/3",
        Some("Bot scheduled-token"),
    );
    assert_request_basics(
        &requests[7],
        "DELETE",
        "/guilds/200/scheduled-events/2",
        Some("Bot scheduled-token"),
    );
    assert_request_basics(
        &requests[8],
        "GET",
        "/guilds/200/scheduled-events/2/users?limit=50",
        Some("Bot scheduled-token"),
    );
    assert_request_basics(
        &requests[9],
        "GET",
        "/guilds/200/scheduled-events/2/users",
        Some("Bot scheduled-token"),
    );
}

#[test]
fn default_http_client_builds_for_public_constructors() {
    let client = default_http_client();
    drop(client);
}

#[tokio::test]
async fn monetization_and_soundboard_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([sku_payload("900")])),
        PlannedResponse::json(StatusCode::OK, json!([subscription_payload("950")])),
        PlannedResponse::json(StatusCode::OK, subscription_payload("950")),
        PlannedResponse::json(StatusCode::OK, json!([entitlement_payload("901")])),
        PlannedResponse::json(StatusCode::OK, entitlement_payload("901")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, entitlement_payload("902")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, json!([soundboard_payload("1")])),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "items": [soundboard_payload("2")] }),
        ),
        PlannedResponse::json(StatusCode::OK, soundboard_payload("2")),
        PlannedResponse::json(StatusCode::OK, soundboard_payload("3")),
        PlannedResponse::json(StatusCode::OK, soundboard_payload("3")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("premium-token", 555, base_url);
    let query = EntitlementQuery {
        user_id: Some(test_id(777)),
        sku_ids: vec![test_id(900), test_id(901)],
        limit: Some(25),
        guild_id: Some(test_id(200)),
        exclude_ended: Some(true),
        exclude_deleted: Some(false),
        ..EntitlementQuery::default()
    };
    let subscription_query = SubscriptionQuery {
        user_id: Some(test_id(777)),
        limit: Some(10),
        ..SubscriptionQuery::default()
    };
    let send_body = SendSoundboardSound::new(test_id(1)).source_guild_id(test_id(200));
    let sound_body = CreateGuildSoundboardSound::new("quack", "data:audio/ogg;base64,AAAA");

    let sku = client.get_skus().await.unwrap().remove(0);
    assert_eq!(sku.slug, "premium");
    assert_eq!(sku.access_type, Some(SkuAccessType::new(1)));
    assert_eq!(sku.premium, Some(false));
    assert_eq!(sku.show_age_gate, Some(false));
    assert!(sku.features.is_empty());
    assert_eq!(
        client
            .get_sku_subscriptions(test_id(900), &subscription_query)
            .await
            .unwrap()[0]
            .id
            .get(),
        950
    );
    assert_eq!(
        client
            .get_sku_subscription(test_id(900), test_id(950))
            .await
            .unwrap()
            .user_id
            .get(),
        777
    );
    let entitlement = client.get_entitlements(&query).await.unwrap().remove(0);
    assert_eq!(entitlement.sku_id.get(), 900);
    assert_eq!(entitlement.gift_code_flags, Some(0));
    assert_eq!(entitlement.subscription_id.map(u64::from), Some(950));
    assert_eq!(
        client
            .get_entitlement(test_id(901))
            .await
            .unwrap()
            .user_id
            .unwrap()
            .get(),
        777
    );
    client.consume_entitlement(test_id(901)).await.unwrap();
    assert_eq!(
        client
            .create_test_entitlement(&CreateTestEntitlement::new(
                test_id(900),
                EntitlementOwner::Guild(test_id(200)),
            ))
            .await
            .unwrap()
            .id
            .get(),
        902
    );
    client.delete_test_entitlement(test_id(902)).await.unwrap();
    client
        .send_soundboard_sound(test_id(300), &send_body)
        .await
        .unwrap();
    assert_eq!(
        client.list_default_soundboard_sounds().await.unwrap().len(),
        1
    );
    assert_eq!(
        client
            .list_guild_soundboard_sounds(test_id(200))
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        client
            .get_guild_soundboard_sound(test_id(200), test_id(2))
            .await
            .unwrap()
            .name,
        "quack"
    );
    assert_eq!(
        client
            .create_guild_soundboard_sound(test_id(200), &sound_body)
            .await
            .unwrap()
            .sound_id
            .get(),
        3
    );
    assert_eq!(
        client
            .modify_guild_soundboard_sound(
                test_id(200),
                test_id(3),
                &ModifyGuildSoundboardSound::new().name("quack"),
            )
            .await
            .unwrap()
            .sound_id
            .get(),
        3
    );
    client
        .delete_guild_soundboard_sound(test_id(200), test_id(3))
        .await
        .unwrap();

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 15);
    assert_request_basics(
        &requests[0],
        "GET",
        "/applications/555/skus",
        Some("Bot premium-token"),
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/skus/900/subscriptions?limit=10&user_id=777",
        Some("Bot premium-token"),
    );
    assert_request_basics(
        &requests[2],
        "GET",
        "/skus/900/subscriptions/950",
        Some("Bot premium-token"),
    );
    assert_request_basics(
            &requests[3],
            "GET",
        "/applications/555/entitlements?user_id=777&sku_ids=900%2C901&limit=25&guild_id=200&exclude_ended=true&exclude_deleted=false",
            Some("Bot premium-token"),
        );
    assert_request_basics(
        &requests[5],
        "POST",
        "/applications/555/entitlements/901/consume",
        Some("Bot premium-token"),
    );
    assert_request_basics(
        &requests[8],
        "POST",
        "/channels/300/send-soundboard-sound",
        Some("Bot premium-token"),
    );
    assert_request_basics(
        &requests[9],
        "GET",
        "/soundboard-default-sounds",
        Some("Bot premium-token"),
    );
    assert_request_basics(
        &requests[14],
        "DELETE",
        "/guilds/200/soundboard-sounds/3",
        Some("Bot premium-token"),
    );
}

#[tokio::test]
async fn poll_thread_invite_and_integration_wrappers_hit_expected_paths() {
    let thread_list = json!({
        "threads": [{ "id": "700", "type": 11, "name": "thread" }],
        "members": [{ "id": "700", "user_id": "777", "flags": 0 }],
        "has_more": false
    });
    let responses = vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "id": "700", "user_id": "777", "flags": 0 }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{ "id": "700", "user_id": "777", "flags": 0 }]),
        ),
        PlannedResponse::json(StatusCode::OK, thread_list.clone()),
        PlannedResponse::json(StatusCode::OK, thread_list.clone()),
        PlannedResponse::json(StatusCode::OK, thread_list.clone()),
        PlannedResponse::json(StatusCode::OK, thread_list),
        PlannedResponse::json(StatusCode::OK, json!({ "code": "abc", "uses": 2 })),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "type": 0,
                "code": "abc",
                "roles": [{ "id": "701", "name": "invite role" }]
            }),
        ),
        PlannedResponse::text(StatusCode::OK, "user_id\n777\n888\n"),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "status": 1,
                "total_users": 2,
                "processed_users": 1,
                "created_at": "2025-01-08T12:00:00.000000+00:00",
                "completed_at": null,
                "error_message": null
            }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{
                "id": "900",
                "name": "integration",
                "type": "discord",
                "enabled": true,
                "account": { "id": "acc", "name": "account" }
            }]),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "users": [{ "id": "777", "username": "voter" }] }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "id": "800", "channel_id": "100", "content": "poll ended" }),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("coverage-token", 555, base_url);

    client
        .add_thread_member(test_id(700), test_id(777))
        .await
        .unwrap();
    client
        .remove_thread_member(test_id(700), test_id(777))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_thread_member(test_id(700), test_id(777), Some(true))
            .await
            .unwrap()
            .user_id
            .unwrap()
            .get(),
        777
    );
    assert_eq!(
        client
            .list_thread_members(
                test_id(700),
                &ThreadMemberQuery {
                    with_member: Some(true),
                    after: Some(test_id(10)),
                    limit: Some(25),
                },
            )
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        client
            .list_public_archived_threads(
                test_id(100),
                &ArchivedThreadsQuery {
                    before: Some("2026-04-29T00:00:00Z".to_string()),
                    limit: Some(50),
                },
            )
            .await
            .unwrap()
            .threads
            .len(),
        1
    );
    client
        .list_private_archived_threads(test_id(100), &ArchivedThreadsQuery::default())
        .await
        .unwrap();
    client
        .list_joined_private_archived_threads(
            test_id(100),
            &JoinedArchivedThreadsQuery {
                before: Some(test_id(700)),
                limit: Some(10),
            },
        )
        .await
        .unwrap();
    client.get_active_guild_threads(test_id(200)).await.unwrap();
    assert_eq!(
        client
            .get_invite_with_options("abc", Some(true), Some(true), Some(test_id(300)))
            .await
            .unwrap()
            .uses,
        Some(2)
    );
    assert_eq!(
        client
            .create_channel_invite_with_target_users_file(
                test_id(100),
                &CreateChannelInvite {
                    role_ids: Some(vec![test_id(701)]),
                    ..CreateChannelInvite::default()
                },
                &sample_file("target_users.csv", "user_id\n777\n888\n"),
            )
            .await
            .unwrap()
            .roles[0]
            .id
            .get(),
        701
    );
    assert_eq!(
        client.get_invite_target_users_csv("abc").await.unwrap(),
        "user_id\n777\n888\n"
    );
    client
        .update_invite_target_users("abc", &sample_file("target_users.csv", "user_id\n777\n"))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_invite_target_users_job_status("abc")
            .await
            .unwrap()
            .processed_users,
        1
    );
    assert_eq!(
        client.get_guild_integrations(test_id(200)).await.unwrap()[0]
            .id
            .get(),
        900
    );
    client
        .delete_guild_integration(test_id(200), test_id(900))
        .await
        .unwrap();
    assert_eq!(
        client
            .get_poll_answer_voters(
                test_id(100),
                test_id(800),
                PollAnswerId::new(1),
                Some(test_id(777)),
                Some(10),
            )
            .await
            .unwrap()
            .users[0]
            .username,
        "voter"
    );
    assert_eq!(
        client
            .end_poll(test_id(100), test_id(800))
            .await
            .unwrap()
            .content,
        "poll ended"
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 17);
    assert_request_basics(
        &requests[0],
        "PUT",
        "/channels/700/thread-members/777",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[3],
        "GET",
        "/channels/700/thread-members?with_member=true&after=10&limit=25",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[4],
        "GET",
        "/channels/100/threads/archived/public?before=2026-04-29T00%3A00%3A00Z&limit=50",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[6],
        "GET",
        "/channels/100/users/@me/threads/archived/private?before=700&limit=10",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[7],
        "GET",
        "/guilds/200/threads/active",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[8],
        "GET",
        "/invites/abc?with_counts=true&with_expiration=true&guild_scheduled_event_id=300",
        Some("Bot coverage-token"),
    );
    assert_named_file_multipart_request(
        &requests[9],
        "POST",
        "/channels/100/invites",
        Some("Bot coverage-token"),
        "target_users_file",
        true,
    );
    assert_request_basics(
        &requests[10],
        "GET",
        "/invites/abc/target-users",
        Some("Bot coverage-token"),
    );
    assert_named_file_multipart_request(
        &requests[11],
        "PUT",
        "/invites/abc/target-users",
        Some("Bot coverage-token"),
        "target_users_file",
        false,
    );
    assert_request_basics(
        &requests[12],
        "GET",
        "/invites/abc/target-users/job-status",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[14],
        "DELETE",
        "/guilds/200/integrations/900",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[15],
        "GET",
        "/channels/100/polls/800/answers/1?after=777&limit=10",
        Some("Bot coverage-token"),
    );
    assert_request_basics(
        &requests[16],
        "POST",
        "/channels/100/polls/800/expire",
        Some("Bot coverage-token"),
    );
}

#[tokio::test]
async fn client_methods_reject_missing_application_id_before_request() {
    let client = RestClient::new("token", None);
    let command = sample_command();
    let commands = vec![command.clone()];

    assert_model_error_contains(
        client
            .bulk_overwrite_global_commands(&commands)
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client.create_global_command(&command).await.unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client.get_global_commands().await.unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .bulk_overwrite_guild_commands(test_id(456), &commands)
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .create_followup_message("token", &sample_followup())
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .create_followup_message("token", &sample_followup())
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .get_original_interaction_response("token")
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .edit_original_interaction_response("token", &sample_edit())
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .delete_original_interaction_response("token")
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .edit_followup_message("token", "123", &sample_edit())
            .await
            .unwrap_err(),
        "application_id must be set",
    );
    assert_model_error_contains(
        client
            .delete_followup_message("token", "123")
            .await
            .unwrap_err(),
        "application_id must be set",
    );
}

#[tokio::test]
async fn client_methods_reject_unsafe_tokens_before_request() {
    let client = RestClient::new("token", Some(test_id(123)));
    let response = sample_interaction_response();

    assert_model_error_contains(
        client
            .execute_webhook(test_id(456), "bad/token", &json!({}))
            .await
            .unwrap_err(),
        "webhook_token",
    );
    assert_model_error_contains(
        client
            .create_interaction_response_typed(test_id(789), "bad/token", &response)
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .create_interaction_response(
                test_id(789),
                "bad/token",
                &crate::request::InteractionResponse::Pong,
            )
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .create_followup_message_with_application_id(
                test_id(123),
                "bad/token",
                &sample_followup(),
            )
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .create_followup_message_with_application_id(
                test_id(123),
                "bad/token",
                &sample_followup(),
            )
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .get_original_interaction_response_with_application_id(test_id(123), "bad/token")
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .edit_original_interaction_response_with_application_id(
                test_id(123),
                "bad/token",
                &sample_edit(),
            )
            .await
            .unwrap_err(),
        "interaction_token",
    );
    assert_model_error_contains(
        client
            .delete_original_interaction_response_with_application_id(test_id(123), "bad/token")
            .await
            .unwrap_err(),
        "interaction_token",
    );
}

#[tokio::test]
async fn client_followup_methods_validate_application_and_message_segments() {
    let client = RestClient::new("token", Some(test_id(123)));

    assert_model_error_contains(
        client
            .edit_followup_message_with_application_id(
                test_id(123),
                "token",
                "bad/id",
                &sample_edit(),
            )
            .await
            .unwrap_err(),
        "message_id",
    );
    assert_model_error_contains(
        client
            .delete_followup_message_with_application_id(test_id(123), "token", "bad/id")
            .await
            .unwrap_err(),
        "message_id",
    );
}

#[test]
fn header_string_returns_none_for_invalid_header_bytes() {
    let invalid = HeaderValue::from_bytes(&[0xFF]).expect("invalid but allowed header bytes");
    assert_eq!(header_string(Some(&invalid)), None);
}

#[test]
fn rate_limit_state_does_not_block_without_reset_after_header() {
    let state = RateLimitState::default();
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));

    let messages = route(Method::GET, "/channels/123/messages");
    state.observe(&messages, &headers, StatusCode::OK, "");

    assert!(state.wait_duration(&messages).is_none());
}

#[tokio::test]
async fn sleep_for_retry_after_clamps_negative_values() {
    let start = Instant::now();
    sleep_for_retry_after(-1.0).await;
    assert!(start.elapsed() < Duration::from_millis(50));
}

#[tokio::test]
async fn client_message_and_channel_wrappers_hit_local_server() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, message_payload("1", "100", "hello")),
        PlannedResponse::json(StatusCode::OK, message_payload("1", "100", "updated")),
        PlannedResponse::json(StatusCode::OK, message_payload("1", "100", "updated")),
        PlannedResponse::json(StatusCode::OK, channel_payload("100", 0, Some("general"))),
        PlannedResponse::json(StatusCode::OK, channel_payload("100", 0, Some("general"))),
        PlannedResponse::json(StatusCode::OK, channel_payload("100", 0, Some("renamed"))),
        PlannedResponse::json(
            StatusCode::OK,
            json!([message_payload("1", "100", "updated")]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([message_payload("2", "100", "latest")]),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, message_payload("1", "100", "crosspost")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([message_payload("3", "100", "paginated")]),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!([{ "id": "42", "username": "reactor" }]),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, json!({ "ok": true })),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "903", "content": "raw" })),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "id": "903", "content": "edited raw" }),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "doing_deep_historical_index": false,
                "total_results": 1,
                "messages": [[message_payload("4", "100", "searched")]],
                "threads": [channel_payload("700", 11, Some("thread"))],
                "members": [{
                    "id": "700",
                    "user_id": "42",
                    "join_timestamp": "2026-05-01T00:00:00.000000+00:00",
                    "flags": 0
                }]
            }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "items": [{
                    "pinned_at": "2026-05-01T00:00:00.000000+00:00",
                    "message": message_payload("5", "100", "pinned")
                }],
                "has_more": false
            }),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("token", 321, base_url);
    let body = sample_message();

    let created = client
        .create_message(test_id(100), &body)
        .await
        .expect("create message");
    assert_eq!(created.content, "hello");

    let updated = client
        .update_message(test_id(100), test_id(1), &body)
        .await
        .expect("update message");
    assert_eq!(updated.content, "updated");

    let fetched = client
        .get_message(test_id(100), test_id(1))
        .await
        .expect("get message");
    assert_eq!(fetched.content, "updated");

    let channel = client.get_channel(test_id(100)).await.expect("get channel");
    assert_eq!(channel.name.as_deref(), Some("general"));

    let deleted_channel = client
        .delete_channel(test_id(100))
        .await
        .expect("delete channel");
    assert_eq!(deleted_channel.id.get(), 100);

    let renamed = client
        .update_channel(test_id(100), &ModifyChannel::new().name("renamed"))
        .await
        .expect("update channel");
    assert_eq!(renamed.name.as_deref(), Some("renamed"));

    let limited_messages = client
        .get_channel_messages(test_id(100), Some(2))
        .await
        .expect("channel messages with limit");
    assert_eq!(limited_messages.len(), 1);

    let all_messages = client
        .get_channel_messages(test_id(100), None)
        .await
        .expect("channel messages without limit");
    assert_eq!(all_messages[0].content, "latest");

    client
        .bulk_delete_messages(test_id(100), vec![test_id(1), test_id(2)])
        .await
        .expect("bulk delete");
    client
        .add_reaction(test_id(100), test_id(1), "spark")
        .await
        .expect("add reaction");
    client
        .remove_reaction(test_id(100), test_id(1), "spark")
        .await
        .expect("remove reaction");

    assert_eq!(
        client
            .crosspost_message(test_id(100), test_id(1))
            .await
            .expect("crosspost message")
            .content,
        "crosspost"
    );
    assert_eq!(
        client
            .get_channel_messages_paginated(
                test_id(100),
                Some(50),
                Some(test_id(10)),
                Some(test_id(11)),
                Some(test_id(12)),
            )
            .await
            .expect("paginated messages")[0]
            .content,
        "paginated"
    );
    assert_eq!(
        client
            .get_reactions(
                test_id(100),
                test_id(1),
                "spark",
                Some(25),
                Some(test_id(42)),
            )
            .await
            .expect("get reactions")[0]
            .username,
        "reactor"
    );
    client
        .remove_user_reaction(test_id(100), test_id(1), "spark", test_id(42))
        .await
        .expect("remove user reaction");
    client
        .remove_all_reactions(test_id(100), test_id(1))
        .await
        .expect("remove all reactions");
    client
        .remove_all_reactions_for_emoji(test_id(100), test_id(1), "spark")
        .await
        .expect("remove all reactions for emoji");

    let raw: serde_json::Value = client
        .request(RawRequest::get("channels/100/custom"))
        .await
        .expect("request with normalized path");
    assert_eq!(raw["ok"], json!(true));

    let sent = client
        .send_message_json(test_id(100), &json!({ "content": "raw" }))
        .await
        .expect("send raw message");
    assert_eq!(sent["id"], json!("903"));

    let edited = client
        .edit_message_json(
            test_id(100),
            test_id(903),
            &json!({ "content": "edited raw" }),
        )
        .await
        .expect("edit raw message");
    assert_eq!(edited["content"], json!("edited raw"));

    client
        .delete_message(test_id(100), test_id(903))
        .await
        .expect("delete raw message");

    let search = client
        .search_guild_messages(
            test_id(200),
            &SearchGuildMessagesQuery {
                limit: Some(5),
                content: Some("hello world & tea".to_string()),
                channel_ids: vec![test_id(100), test_id(101)],
                author_types: vec!["bot".to_string()],
                mention_everyone: Some(false),
                has: vec!["link".to_string()],
                include_nsfw: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("search guild messages");
    assert_eq!(search.total_results, 1);
    assert_eq!(search.messages[0][0].content, "searched");
    assert_eq!(search.threads[0].id.get(), 700);
    assert_eq!(search.members[0].user_id.as_ref().unwrap().get(), 42);

    let pins = client
        .get_channel_pins(
            test_id(100),
            &ChannelPinsQuery {
                before: Some("2026-05-01T00:00:00.000Z".to_string()),
                limit: Some(10),
            },
        )
        .await
        .expect("get channel pins");
    assert_eq!(pins.items[0].message.content, "pinned");
    assert!(!pins.has_more);

    client
        .pin_message(test_id(100), test_id(5))
        .await
        .expect("pin message");
    client
        .unpin_message(test_id(100), test_id(5))
        .await
        .expect("unpin message");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot token");

    assert_request_basics(&requests[0], "POST", "/channels/100/messages", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap()["content"],
        json!("hello")
    );
    assert_request_basics(&requests[1], "PATCH", "/channels/100/messages/1", auth);
    assert_request_basics(&requests[2], "GET", "/channels/100/messages/1", auth);
    assert_request_basics(&requests[3], "GET", "/channels/100", auth);
    assert_request_basics(&requests[4], "DELETE", "/channels/100", auth);
    assert_request_basics(&requests[5], "PATCH", "/channels/100", auth);
    assert_request_basics(&requests[6], "GET", "/channels/100/messages?limit=2", auth);
    assert_request_basics(&requests[7], "GET", "/channels/100/messages", auth);
    assert_request_basics(
        &requests[8],
        "POST",
        "/channels/100/messages/bulk-delete",
        auth,
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[8].body).unwrap(),
        json!({ "messages": ["1", "2"] })
    );
    assert_request_basics(
        &requests[9],
        "PUT",
        "/channels/100/messages/1/reactions/spark/@me",
        auth,
    );
    assert_request_basics(
        &requests[10],
        "DELETE",
        "/channels/100/messages/1/reactions/spark/@me",
        auth,
    );
    assert_request_basics(
        &requests[11],
        "POST",
        "/channels/100/messages/1/crosspost",
        auth,
    );
    assert_request_basics(
        &requests[12],
        "GET",
        "/channels/100/messages?limit=50&before=10&after=11&around=12",
        auth,
    );
    assert_request_basics(
        &requests[13],
        "GET",
        "/channels/100/messages/1/reactions/spark?limit=25&after=42",
        auth,
    );
    assert_request_basics(
        &requests[14],
        "DELETE",
        "/channels/100/messages/1/reactions/spark/42",
        auth,
    );
    assert_request_basics(
        &requests[15],
        "DELETE",
        "/channels/100/messages/1/reactions",
        auth,
    );
    assert_request_basics(
        &requests[16],
        "DELETE",
        "/channels/100/messages/1/reactions/spark",
        auth,
    );
    assert_request_basics(&requests[17], "GET", "/channels/100/custom", auth);
    assert_request_basics(&requests[18], "POST", "/channels/100/messages", auth);
    assert_request_basics(&requests[19], "PATCH", "/channels/100/messages/903", auth);
    assert_request_basics(&requests[20], "DELETE", "/channels/100/messages/903", auth);
    assert_request_basics(
        &requests[21],
        "GET",
        "/guilds/200/messages/search?limit=5&content=hello+world+%26+tea&channel_id=100&channel_id=101&author_type=bot&mention_everyone=false&has=link&include_nsfw=true",
        auth,
    );
    assert_request_basics(
        &requests[22],
        "GET",
        "/channels/100/messages/pins?before=2026-05-01T00%3A00%3A00.000Z&limit=10",
        auth,
    );
    assert_request_basics(&requests[23], "PUT", "/channels/100/messages/pins/5", auth);
    assert_request_basics(
        &requests[24],
        "DELETE",
        "/channels/100/messages/pins/5",
        auth,
    );
}

#[tokio::test]
async fn client_guild_and_command_wrappers_hit_local_server() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, guild_payload("200", "guild")),
        PlannedResponse::json(StatusCode::OK, guild_payload("200", "guild-counts")),
        PlannedResponse::json(StatusCode::OK, guild_payload("200", "guild-updated")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([channel_payload("201", 0, Some("rules"))]),
        ),
        PlannedResponse::json(StatusCode::OK, channel_payload("202", 0, Some("new"))),
        PlannedResponse::json(StatusCode::OK, json!([{}])),
        PlannedResponse::json(StatusCode::OK, json!([{}])),
        PlannedResponse::json(StatusCode::OK, json!([{}])),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, role_payload("300", "admin")),
        PlannedResponse::json(StatusCode::OK, role_payload("300", "mod")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, json!({})),
        PlannedResponse::json(StatusCode::OK, json!([role_payload("300", "mod")])),
        PlannedResponse::json(StatusCode::OK, json!([role_payload("300", "mod")])),
        PlannedResponse::json(
            StatusCode::OK,
            json!([command_payload("401", "ping", "pong")]),
        ),
        PlannedResponse::json(StatusCode::OK, command_payload("402", "pong", "reply")),
        PlannedResponse::json(
            StatusCode::OK,
            json!([command_payload("401", "ping", "pong")]),
        ),
        PlannedResponse::json(StatusCode::OK, gateway_payload()),
        PlannedResponse::json(
            StatusCode::OK,
            json!([command_payload("403", "guild", "only")]),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("guild-token", 0, base_url);
    client.set_application_id(test_id(555));
    let command = sample_command();

    let guild = client.get_guild(test_id(200)).await.expect("get guild");
    assert_eq!(guild.name, "guild");

    let guild_with_counts = client
        .get_guild_with_query(
            test_id(200),
            &GetGuildQuery {
                with_counts: Some(true),
            },
        )
        .await
        .expect("get guild with counts");
    assert_eq!(guild_with_counts.name, "guild-counts");

    let updated = client
        .modify_guild(
            test_id(200),
            &ModifyGuild {
                name: Some("guild-updated".to_string()),
                ..ModifyGuild::default()
            },
        )
        .await
        .expect("update guild");
    assert_eq!(updated.name, "guild-updated");

    let channels = client
        .get_guild_channels(test_id(200))
        .await
        .expect("get guild channels");
    assert_eq!(channels.len(), 1);

    let created_channel = client
        .create_guild_channel(
            test_id(200),
            &CreateGuildChannel {
                name: "new".to_string(),
                ..CreateGuildChannel::default()
            },
        )
        .await
        .expect("create guild channel");
    assert_eq!(created_channel.id.get(), 202);

    assert_eq!(
        client
            .get_guild_members(test_id(200), Some(3))
            .await
            .expect("members with limit")
            .len(),
        1
    );
    assert_eq!(
        client
            .get_guild_members_with_query(
                test_id(200),
                &GuildMembersQuery {
                    limit: Some(2),
                    after: Some(test_id(201)),
                },
            )
            .await
            .expect("members after cursor")
            .len(),
        1
    );
    assert_eq!(
        client
            .get_guild_members(test_id(200), None)
            .await
            .expect("members without limit")
            .len(),
        1
    );
    client
        .remove_guild_member(test_id(200), test_id(201))
        .await
        .expect("remove guild member");
    client
        .add_guild_member_role(test_id(200), test_id(201), test_id(300))
        .await
        .expect("add guild member role");
    client
        .remove_guild_member_role(test_id(200), test_id(201), test_id(300))
        .await
        .expect("remove guild member role");

    let created_role = client
        .create_guild_role(
            test_id(200),
            &CreateGuildRole {
                name: Some("admin".to_string()),
                ..CreateGuildRole::default()
            },
        )
        .await
        .expect("create role");
    assert_eq!(created_role.name, "admin");

    let updated_role = client
        .modify_guild_role(
            test_id(200),
            test_id(300),
            &ModifyGuildRole {
                name: Some(Some("mod".to_string())),
                ..ModifyGuildRole::default()
            },
        )
        .await
        .expect("update role");
    assert_eq!(updated_role.name, "mod");

    client
        .delete_role(test_id(200), test_id(300))
        .await
        .expect("delete role");

    let member = client
        .get_member(test_id(200), test_id(201))
        .await
        .expect("get member");
    assert!(member.roles.is_empty());

    let roles = client.list_roles(test_id(200)).await.expect("list roles");
    assert_eq!(roles[0].name, "mod");

    let reordered_roles = client
        .modify_guild_role_positions(
            test_id(200),
            &[ModifyGuildRolePosition {
                id: test_id(300),
                position: Some(Some(1)),
            }],
        )
        .await
        .expect("modify guild role positions");
    assert_eq!(reordered_roles[0].name, "mod");

    let overwritten = client
        .bulk_overwrite_global_commands(std::slice::from_ref(&command))
        .await
        .expect("bulk overwrite global commands");
    assert_eq!(overwritten[0].name, "ping");

    let created_command = client
        .create_global_command(&CommandDefinition {
            name: "pong".to_string(),
            description: "reply".to_string(),
            ..command.clone()
        })
        .await
        .expect("create global command");
    assert_eq!(created_command.name, "pong");

    let global_commands = client
        .get_global_commands()
        .await
        .expect("get global commands");
    assert_eq!(global_commands.len(), 1);

    let gateway = client.get_gateway_bot().await.expect("get gateway bot");
    assert_eq!(gateway.shards, 1);

    let guild_commands = client
        .bulk_overwrite_guild_commands(test_id(200), &[command])
        .await
        .expect("bulk overwrite guild commands");
    assert_eq!(guild_commands[0].name, "guild");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot guild-token");

    assert_request_basics(&requests[0], "GET", "/guilds/200", auth);
    assert_request_basics(&requests[1], "GET", "/guilds/200?with_counts=true", auth);
    assert_request_basics(&requests[2], "PATCH", "/guilds/200", auth);
    assert_request_basics(&requests[3], "GET", "/guilds/200/channels", auth);
    assert_request_basics(&requests[4], "POST", "/guilds/200/channels", auth);
    assert_request_basics(&requests[5], "GET", "/guilds/200/members?limit=3", auth);
    assert_request_basics(
        &requests[6],
        "GET",
        "/guilds/200/members?limit=2&after=201",
        auth,
    );
    assert_request_basics(&requests[7], "GET", "/guilds/200/members", auth);
    assert_request_basics(&requests[8], "DELETE", "/guilds/200/members/201", auth);
    assert_request_basics(
        &requests[9],
        "PUT",
        "/guilds/200/members/201/roles/300",
        auth,
    );
    assert_request_basics(
        &requests[10],
        "DELETE",
        "/guilds/200/members/201/roles/300",
        auth,
    );
    assert_request_basics(&requests[11], "POST", "/guilds/200/roles", auth);
    assert_request_basics(&requests[12], "PATCH", "/guilds/200/roles/300", auth);
    assert_request_basics(&requests[13], "DELETE", "/guilds/200/roles/300", auth);
    assert_request_basics(&requests[14], "GET", "/guilds/200/members/201", auth);
    assert_request_basics(&requests[15], "GET", "/guilds/200/roles", auth);
    assert_request_basics(&requests[16], "PATCH", "/guilds/200/roles", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[16].body).unwrap(),
        json!([{ "id": "300", "position": 1 }])
    );
    assert_request_basics(&requests[17], "PUT", "/applications/555/commands", auth);
    assert_request_basics(&requests[18], "POST", "/applications/555/commands", auth);
    assert_request_basics(&requests[19], "GET", "/applications/555/commands", auth);
    assert_request_basics(&requests[20], "GET", "/gateway/bot", auth);
    assert_request_basics(
        &requests[21],
        "PUT",
        "/applications/555/guilds/200/commands",
        auth,
    );
}

#[tokio::test]
async fn single_command_wrappers_hit_expected_paths() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, command_payload("401", "global", "read")),
        PlannedResponse::json(StatusCode::OK, command_payload("401", "global", "edited")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, command_payload("402", "guild", "read")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("command-token", 555, base_url);

    let global = client
        .get_global_command(test_id(401))
        .await
        .expect("get global command");
    assert_eq!(global.name, "global");

    let edited = client
        .edit_global_command(test_id(401), &EditApplicationCommand::new().name("global"))
        .await
        .expect("edit global command");
    assert_eq!(edited.description, "edited");

    client
        .delete_global_command(test_id(401))
        .await
        .expect("delete global command");

    let guild = client
        .get_guild_command(test_id(200), test_id(402))
        .await
        .expect("get guild command");
    assert_eq!(guild.description, "read");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot command-token");
    assert_request_basics(&requests[0], "GET", "/applications/555/commands/401", auth);
    assert_request_basics(
        &requests[1],
        "PATCH",
        "/applications/555/commands/401",
        auth,
    );
    assert_request_basics(
        &requests[2],
        "DELETE",
        "/applications/555/commands/401",
        auth,
    );
    assert_request_basics(
        &requests[3],
        "GET",
        "/applications/555/guilds/200/commands/402",
        auth,
    );
}

#[tokio::test]
async fn client_typed_guild_request_bodies_hit_local_server() {
    let responses = vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, role_payload("300", "gradient")),
        PlannedResponse::json(StatusCode::OK, role_payload("300", "muted")),
        PlannedResponse::json(StatusCode::OK, member_payload("201", "bot")),
        PlannedResponse::json(StatusCode::OK, member_payload("201", "bot")),
        PlannedResponse::json(StatusCode::OK, guild_payload("200", "renamed")),
        PlannedResponse::json(StatusCode::OK, channel_payload("202", 0, Some("rules"))),
        PlannedResponse::json(
            StatusCode::OK,
            json!({ "enabled": true, "channel_id": null }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "description": null,
                "welcome_channels": []
            }),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            json!({
                "guild_id": "200",
                "prompts": [{
                    "id": "1",
                    "type": 0,
                    "options": [],
                    "title": "Pick a topic",
                    "single_select": false,
                    "required": false,
                    "in_onboarding": true
                }],
                "default_channel_ids": ["202"],
                "enabled": true,
                "mode": 0
            }),
        ),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("guild-token", 0, base_url);

    client
        .create_guild_ban(
            test_id(200),
            test_id(201),
            &CreateGuildBan {
                delete_message_days: None,
                delete_message_seconds: Some(60),
            },
        )
        .await
        .expect("create guild ban");

    let created_role = client
        .create_guild_role(
            test_id(200),
            &CreateGuildRole {
                name: Some("gradient".to_string()),
                colors: Some(RoleColors {
                    primary_color: 11127295,
                    secondary_color: Some(16759788),
                    tertiary_color: Some(16761760),
                }),
                icon: Some(None),
                ..Default::default()
            },
        )
        .await
        .expect("create guild role");
    assert_eq!(created_role.name, "gradient");

    let updated_role = client
        .modify_guild_role(
            test_id(200),
            test_id(300),
            &ModifyGuildRole {
                name: Some(Some("muted".to_string())),
                permissions: Some(Some(Permissions::from_bits(8))),
                unicode_emoji: Some(None),
                mentionable: Some(Some(true)),
                ..Default::default()
            },
        )
        .await
        .expect("modify guild role");
    assert_eq!(updated_role.name, "muted");

    client
        .modify_guild_member(
            test_id(200),
            test_id(201),
            &ModifyGuildMember {
                nick: Some(None),
                roles: Some(Some(vec![test_id(300)])),
                mute: Some(Some(false)),
                channel_id: Some(None),
                communication_disabled_until: Some(Some("2026-05-01T00:00:00Z".to_string())),
                ..Default::default()
            },
        )
        .await
        .expect("modify guild member");

    let current_member = client
        .modify_current_member(
            test_id(200),
            &ModifyCurrentMember {
                nick: Some(Some("bot".to_string())),
                avatar: Some(None),
                bio: Some(Some("typed".to_string())),
                ..Default::default()
            },
        )
        .await
        .expect("modify current member");
    assert_eq!(current_member.user.unwrap().username, "bot");

    let guild = client
        .modify_guild(
            test_id(200),
            &ModifyGuild {
                name: Some("renamed".to_string()),
                afk_channel_id: Some(None),
                features: Some(vec!["COMMUNITY".to_string()]),
                description: Some(None),
                premium_progress_bar_enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("modify guild");
    assert_eq!(guild.name, "renamed");

    let channel = client
        .create_guild_channel(
            test_id(200),
            &CreateGuildChannel {
                name: "rules".to_string(),
                kind: Some(ChannelType::GUILD_TEXT),
                topic: Some(Some("read first".to_string())),
                parent_id: Some(None),
                default_reaction_emoji: Some(None),
                ..Default::default()
            },
        )
        .await
        .expect("create guild channel");
    assert_eq!(channel.name.as_deref(), Some("rules"));

    let widget = client
        .modify_guild_widget_settings(
            test_id(200),
            &ModifyGuildWidgetSettings {
                enabled: Some(true),
                channel_id: Some(None),
            },
        )
        .await
        .expect("modify guild widget");
    assert!(widget.enabled);

    let welcome_screen = client
        .modify_guild_welcome_screen(
            test_id(200),
            &ModifyGuildWelcomeScreen {
                enabled: Some(Some(true)),
                welcome_channels: Some(Some(vec![WelcomeScreenChannel {
                    channel_id: test_id(202),
                    description: "Start here".to_string(),
                    emoji_id: None,
                    emoji_name: Some("wave".to_string()),
                }])),
                description: Some(None),
            },
        )
        .await
        .expect("modify guild welcome screen");
    assert!(welcome_screen.description.is_none());

    let onboarding = client
        .modify_guild_onboarding(
            test_id(200),
            &ModifyGuildOnboarding {
                prompts: Some(vec![crate::model::OnboardingPrompt {
                    id: test_id(1),
                    kind: crate::model::OnboardingPromptType::MULTIPLE_CHOICE,
                    options: Vec::new(),
                    title: "Pick a topic".to_string(),
                    single_select: false,
                    required: false,
                    in_onboarding: true,
                }]),
                default_channel_ids: Some(vec![test_id(202)]),
                enabled: Some(true),
                mode: Some(OnboardingMode::ONBOARDING_DEFAULT),
            },
        )
        .await
        .expect("modify guild onboarding");
    assert!(onboarding.enabled);

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot guild-token");
    assert_request_basics(&requests[0], "PUT", "/guilds/200/bans/201", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap(),
        json!({ "delete_message_seconds": 60 })
    );
    assert_request_basics(&requests[1], "POST", "/guilds/200/roles", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap(),
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
    assert_request_basics(&requests[2], "PATCH", "/guilds/200/roles/300", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[2].body).unwrap(),
        json!({
            "name": "muted",
            "permissions": "8",
            "unicode_emoji": null,
            "mentionable": true
        })
    );
    assert_request_basics(&requests[3], "PATCH", "/guilds/200/members/201", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[3].body).unwrap(),
        json!({
            "nick": null,
            "roles": ["300"],
            "mute": false,
            "channel_id": null,
            "communication_disabled_until": "2026-05-01T00:00:00Z"
        })
    );
    assert_request_basics(&requests[4], "PATCH", "/guilds/200/members/@me", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[4].body).unwrap(),
        json!({
            "nick": "bot",
            "avatar": null,
            "bio": "typed"
        })
    );
    assert_request_basics(&requests[5], "PATCH", "/guilds/200", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[5].body).unwrap(),
        json!({
            "name": "renamed",
            "afk_channel_id": null,
            "features": ["COMMUNITY"],
            "description": null,
            "premium_progress_bar_enabled": true
        })
    );
    assert_request_basics(&requests[6], "POST", "/guilds/200/channels", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[6].body).unwrap(),
        json!({
            "name": "rules",
            "type": 0,
            "topic": "read first",
            "parent_id": null,
            "default_reaction_emoji": null
        })
    );
    assert_request_basics(&requests[7], "PATCH", "/guilds/200/widget", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[7].body).unwrap(),
        json!({ "enabled": true, "channel_id": null })
    );
    assert_request_basics(&requests[8], "PATCH", "/guilds/200/welcome-screen", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[8].body).unwrap(),
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
    assert_request_basics(&requests[9], "PUT", "/guilds/200/onboarding", auth);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[9].body).unwrap(),
        json!({
            "prompts": [{
                "id": "1",
                "type": 0,
                "options": [],
                "title": "Pick a topic",
                "single_select": false,
                "required": false,
                "in_onboarding": true
            }],
            "default_channel_ids": ["202"],
            "enabled": true,
            "mode": 0
        })
    );
}

#[tokio::test]
async fn client_search_guild_members_uses_shared_query_encoding() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!([member_payload("201", "alice")])),
        PlannedResponse::json(StatusCode::OK, json!([member_payload("202", "bob")])),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("guild-token", 0, base_url);

    let typed = client
        .search_guild_members_with_query(
            test_id(200),
            &SearchGuildMembersQuery {
                query: "alice & bob".to_string(),
                limit: Some(5),
            },
        )
        .await
        .expect("typed search members");
    assert_eq!(typed[0].user.as_ref().unwrap().username.as_str(), "alice");

    let legacy = client
        .search_guild_members(test_id(200), "bob/smith", None)
        .await
        .expect("legacy search members");
    assert_eq!(legacy[0].user.as_ref().unwrap().id.get(), 202);

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot guild-token");
    assert_request_basics(
        &requests[0],
        "GET",
        "/guilds/200/members/search?query=alice+%26+bob&limit=5",
        auth,
    );
    assert_request_basics(
        &requests[1],
        "GET",
        "/guilds/200/members/search?query=bob%2Fsmith",
        auth,
    );
}

#[tokio::test]
async fn client_guild_member_and_channel_position_wrappers_hit_local_server() {
    let responses = vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::CREATED, member_payload("201", "joined")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("guild-token", 0, base_url);

    client
        .modify_guild_channel_positions(
            test_id(200),
            &[ModifyGuildChannelPosition {
                id: test_id(202),
                position: Some(Some(1)),
                lock_permissions: Some(Some(true)),
                parent_id: Some(None),
            }],
        )
        .await
        .expect("modify guild channel positions");

    let added = client
        .add_guild_member(
            test_id(200),
            test_id(201),
            &AddGuildMember {
                access_token: "oauth-access-token".to_string(),
                nick: Some("joined".to_string()),
                roles: Some(vec![test_id(300)]),
                mute: Some(false),
                deaf: Some(false),
            },
        )
        .await
        .expect("add guild member");
    assert_eq!(
        added
            .and_then(|member| member.user)
            .map(|user| user.id)
            .expect("joined user id")
            .get(),
        201
    );

    let already_member = client
        .add_guild_member(
            test_id(200),
            test_id(201),
            &AddGuildMember {
                access_token: "oauth-access-token".to_string(),
                ..Default::default()
            },
        )
        .await
        .expect("add existing guild member");
    assert!(already_member.is_none());

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let auth = Some("Bot guild-token");

    assert_request_basics(&requests[0], "PATCH", "/guilds/200/channels", auth);
    let positions: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("position body json");
    assert_eq!(
        positions,
        json!([{
            "id": "202",
            "position": 1,
            "lock_permissions": true,
            "parent_id": null
        }])
    );

    assert_request_basics(&requests[1], "PUT", "/guilds/200/members/201", auth);
    let add_body: serde_json::Value =
        serde_json::from_str(&requests[1].body).expect("add guild member body json");
    assert_eq!(add_body["access_token"], "oauth-access-token");
    assert_eq!(add_body["nick"], "joined");
    assert_eq!(add_body["roles"], json!(["300"]));
    assert_eq!(add_body["mute"], false);
    assert_eq!(add_body["deaf"], false);

    assert_request_basics(&requests[2], "PUT", "/guilds/200/members/201", auth);
    let existing_body: serde_json::Value =
        serde_json::from_str(&requests[2].body).expect("existing member body json");
    assert_eq!(
        existing_body,
        json!({ "access_token": "oauth-access-token" })
    );
}

#[tokio::test]
async fn client_guild_role_member_counts_wrapper_hits_local_server() {
    let responses = vec![PlannedResponse::json(
        StatusCode::OK,
        json!({
            "300": 7,
            "301": 2
        }),
    )];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("guild-token", 0, base_url);

    let counts = client
        .get_guild_role_member_counts(test_id(200))
        .await
        .expect("get guild role member counts");
    assert_eq!(counts.get(&test_id(300)), Some(&7));
    assert_eq!(counts.get(&test_id(301)), Some(&2));

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_request_basics(
        &requests[0],
        "GET",
        "/guilds/200/roles/member-counts",
        Some("Bot guild-token"),
    );
}

#[tokio::test]
async fn client_group_dm_recipient_wrappers_hit_local_server() {
    let responses = vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("dm-token", 0, base_url);

    client
        .add_group_dm_recipient(
            test_id(500),
            test_id(42),
            &AddGroupDmRecipient {
                access_token: "oauth-gdm-token".to_string(),
                nick: "friend".to_string(),
            },
        )
        .await
        .expect("add group dm recipient");
    client
        .remove_group_dm_recipient(test_id(500), test_id(42))
        .await
        .expect("remove group dm recipient");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_request_basics(
        &requests[0],
        "PUT",
        "/channels/500/recipients/42",
        Some("Bot dm-token"),
    );
    let body: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("group dm add body");
    assert_eq!(body["access_token"], "oauth-gdm-token");
    assert_eq!(body["nick"], "friend");
    assert_request_basics(
        &requests[1],
        "DELETE",
        "/channels/500/recipients/42",
        Some("Bot dm-token"),
    );
}

#[tokio::test]
async fn public_widget_image_retries_rate_limits_and_surfaces_api_errors() {
    let responses = vec![
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 0.0, "global": false }),
        ),
        PlannedResponse::text(StatusCode::OK, "PNGDATA"),
        PlannedResponse::json(StatusCode::BAD_REQUEST, json!({ "message": "bad widget" })),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("image-token", 0, base_url);

    let image = client
        .get_guild_widget_image(test_id(200), None)
        .await
        .expect("widget image after retry");
    assert_eq!(image, b"PNGDATA".to_vec());

    match client
        .get_guild_widget_image(test_id(200), Some(GuildWidgetImageStyle::Shield))
        .await
        .expect_err("bad widget image should return api error")
    {
        DiscordError::Api {
            status, message, ..
        } => {
            assert_eq!(status, 400);
            assert_eq!(message, "bad widget");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 3);
    assert_request_basics(&requests[0], "GET", "/guilds/200/widget.png", None);
    assert_request_basics(&requests[1], "GET", "/guilds/200/widget.png", None);
    assert_request_basics(
        &requests[2],
        "GET",
        "/guilds/200/widget.png?style=shield",
        None,
    );
}

#[tokio::test]
async fn client_webhook_and_followup_wrappers_hit_local_server() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, webhook_payload("900", "hook")),
        PlannedResponse::json(StatusCode::OK, json!([webhook_payload("900", "hook")])),
        PlannedResponse::json(StatusCode::OK, json!([])),
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 0.0, "global": false }),
        ),
        PlannedResponse::json(StatusCode::OK, json!({ "id": "901" })),
        PlannedResponse::json(StatusCode::OK, channel_payload("500", 1, None)),
        PlannedResponse::json(StatusCode::OK, channel_payload("501", 3, Some("group"))),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, message_payload("902", "500", "json")),
        PlannedResponse::json(StatusCode::OK, message_payload("903", "500", "implicit")),
        PlannedResponse::json(StatusCode::OK, message_payload("904", "500", "followup")),
        PlannedResponse::json(StatusCode::OK, message_payload("905", "500", "followup")),
        PlannedResponse::json(StatusCode::OK, message_payload("906", "500", "original")),
        PlannedResponse::json(StatusCode::OK, message_payload("907", "500", "original")),
        PlannedResponse::json(StatusCode::OK, message_payload("908", "500", "edited")),
        PlannedResponse::json(StatusCode::OK, message_payload("909", "500", "edited")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(
            StatusCode::OK,
            message_payload("910", "500", "followup-edit"),
        ),
        PlannedResponse::json(
            StatusCode::OK,
            message_payload("911", "500", "followup-edit"),
        ),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("hook-token", 123, base_url);

    let webhook = client
        .create_webhook(
            test_id(500),
            &CreateWebhook {
                name: "hook".to_string(),
                avatar: None,
            },
        )
        .await
        .expect("create webhook");
    assert_eq!(webhook.id.map(u64::from), Some(900));

    let webhooks = client
        .get_channel_webhooks(test_id(500))
        .await
        .expect("get channel webhooks array");
    assert_eq!(webhooks.len(), 1);

    let fallback = client
        .get_channel_webhooks(test_id(500))
        .await
        .expect("get channel webhooks, none");
    assert!(fallback.is_empty());

    let executed = client
        .execute_webhook(test_id(777), "token", &json!({ "content": "hook" }))
        .await
        .expect("execute webhook with retry");
    assert_eq!(executed["id"], json!("901"));

    let dm = client
        .create_dm_channel(&crate::model::CreateDmChannel {
            recipient_id: test_id(42),
        })
        .await
        .expect("create dm channel");
    assert_eq!(dm.id.get(), 500);

    let group_dm = client
        .create_group_dm_channel(&crate::model::CreateGroupDmChannel {
            access_tokens: vec!["oauth-gdm-one".to_string(), "oauth-gdm-two".to_string()],
            nicks: HashMap::from([(test_id(43), "friend".to_string())]),
        })
        .await
        .expect("create group dm channel");
    assert_eq!(group_dm.id.get(), 501);

    client
        .create_interaction_response_typed(test_id(777), "token", &sample_interaction_response())
        .await
        .expect("create interaction response typed");
    client
        .create_interaction_response(
            test_id(778),
            "token",
            &crate::request::InteractionResponse::reply("json"),
        )
        .await
        .expect("create interaction response");

    assert_eq!(
        client
            .create_followup_message_with_application_id(
                test_id(123),
                "token",
                &crate::request::CreateFollowup::from("json"),
            )
            .await
            .expect("explicit followup")
            .id,
        test_id(902)
    );
    assert_eq!(
        client
            .create_followup_message("token", &crate::request::CreateFollowup::from("implicit"))
            .await
            .expect("implicit followup")
            .id,
        test_id(903)
    );
    assert_eq!(
        client
            .create_followup_message_with_application_id(test_id(123), "token", &sample_followup())
            .await
            .expect("explicit followup message")
            .content,
        "followup"
    );
    assert_eq!(
        client
            .create_followup_message("token", &sample_followup())
            .await
            .expect("implicit followup message")
            .content,
        "followup"
    );
    assert_eq!(
        client
            .get_original_interaction_response_with_application_id(test_id(123), "token")
            .await
            .expect("explicit original get")
            .content,
        "original"
    );
    assert_eq!(
        client
            .get_original_interaction_response("token")
            .await
            .expect("implicit original get")
            .content,
        "original"
    );
    assert_eq!(
        client
            .edit_original_interaction_response_with_application_id(
                test_id(123),
                "token",
                &sample_edit()
            )
            .await
            .expect("explicit original edit")
            .content,
        "edited"
    );
    assert_eq!(
        client
            .edit_original_interaction_response("token", &sample_edit())
            .await
            .expect("implicit original edit")
            .content,
        "edited"
    );
    client
        .delete_original_interaction_response_with_application_id(test_id(123), "token")
        .await
        .expect("explicit original delete");
    client
        .delete_original_interaction_response("token")
        .await
        .expect("implicit original delete");

    assert_eq!(
        client
            .edit_followup_message_with_application_id(test_id(123), "token", "55", &sample_edit())
            .await
            .expect("explicit followup edit")
            .content,
        "followup-edit"
    );
    assert_eq!(
        client
            .edit_followup_message("token", "55", &sample_edit())
            .await
            .expect("implicit followup edit")
            .content,
        "followup-edit"
    );
    client
        .delete_followup_message_with_application_id(test_id(123), "token", "55")
        .await
        .expect("explicit followup delete");
    client
        .delete_followup_message("token", "55")
        .await
        .expect("implicit followup delete");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    let bot_auth = Some("Bot hook-token");

    assert_request_basics(&requests[0], "POST", "/channels/500/webhooks", bot_auth);
    assert_request_basics(&requests[1], "GET", "/channels/500/webhooks", bot_auth);
    assert_request_basics(&requests[2], "GET", "/channels/500/webhooks", bot_auth);
    assert_request_basics(&requests[3], "POST", "/webhooks/777/token?wait=true", None);
    assert_request_basics(&requests[4], "POST", "/webhooks/777/token?wait=true", None);
    assert_request_basics(&requests[5], "POST", "/users/@me/channels", bot_auth);
    assert_request_basics(&requests[6], "POST", "/users/@me/channels", bot_auth);
    assert!(requests[6].body.contains("oauth-gdm-one"));
    assert!(requests[6].body.contains("\"43\":\"friend\""));
    assert_request_basics(
        &requests[7],
        "POST",
        "/interactions/777/token/callback",
        None,
    );
    assert_request_basics(
        &requests[8],
        "POST",
        "/interactions/778/token/callback",
        None,
    );
    assert_request_basics(&requests[9], "POST", "/webhooks/123/token", None);
    assert_request_basics(&requests[10], "POST", "/webhooks/123/token", None);
    assert_request_basics(&requests[11], "POST", "/webhooks/123/token", None);
    assert_request_basics(&requests[12], "POST", "/webhooks/123/token", None);
    assert_request_basics(
        &requests[13],
        "GET",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[14],
        "GET",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[15],
        "PATCH",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[16],
        "PATCH",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[17],
        "DELETE",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[18],
        "DELETE",
        "/webhooks/123/token/messages/@original",
        None,
    );
    assert_request_basics(
        &requests[19],
        "PATCH",
        "/webhooks/123/token/messages/55",
        None,
    );
    assert_request_basics(
        &requests[20],
        "PATCH",
        "/webhooks/123/token/messages/55",
        None,
    );
    assert_request_basics(
        &requests[21],
        "DELETE",
        "/webhooks/123/token/messages/55",
        None,
    );
    assert_request_basics(
        &requests[22],
        "DELETE",
        "/webhooks/123/token/messages/55",
        None,
    );
}

#[tokio::test]
async fn request_surfaces_api_and_rate_limit_errors_from_local_server() {
    let mut responses = vec![PlannedResponse::text(
        StatusCode::BAD_REQUEST,
        r#"{"code":50035,"message":"bad payload"}"#,
    )];
    responses.extend((0..6).map(|_| {
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 0.0, "global": false }),
        )
    }));
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("err-token", 123, base_url);

    match client
        .request::<serde_json::Value>(
            RawRequest::post("/channels/9/messages").json(&json!({ "content": "boom" })),
        )
        .await
        .unwrap_err()
    {
        DiscordError::Api {
            status,
            code,
            message,
        } => {
            assert_eq!(status, 400);
            assert_eq!(code, Some(50035));
            assert_eq!(message, "bad payload");
        }
        other => panic!("unexpected api error: {other:?}"),
    }

    match client
        .execute_webhook(test_id(9), "token", &json!({ "content": "boom" }))
        .await
        .unwrap_err()
    {
        DiscordError::RateLimit { route, retry_after } => {
            assert!(
                route.starts_with("POST:webhooks/9/:token-") && !route.ends_with("/token"),
                "rate-limit errors must not expose the webhook token: {route}"
            );
            assert_eq!(retry_after, 0.0);
        }
        other => panic!("unexpected rate limit error: {other:?}"),
    }

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 7);
    assert_request_basics(
        &requests[0],
        "POST",
        "/channels/9/messages",
        Some("Bot err-token"),
    );
    for request in requests.iter().skip(1) {
        assert_request_basics(request, "POST", "/webhooks/9/token?wait=true", None);
    }
}

#[tokio::test]
async fn request_retries_repeated_rate_limits_until_success() {
    let responses = vec![
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 0.0, "global": false }),
        ),
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 0.0, "global": false }),
        ),
        PlannedResponse::json(StatusCode::OK, json!({ "ok": true })),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;
    let client = RestClient::new_with_base_url("retry-token", 123, base_url);

    assert_eq!(
        client
            .request::<serde_json::Value>(RawRequest::get("/channels/9"))
            .await
            .unwrap()["ok"],
        json!(true)
    );

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 3);
    for request in requests.iter() {
        assert_request_basics(request, "GET", "/channels/9", Some("Bot retry-token"));
    }
}

#[test]
fn audit_log_reason_encoding_matches_encode_uri_component() {
    assert_eq!(super::encode_audit_log_reason("simple"), "simple");
    assert_eq!(super::encode_audit_log_reason("a b"), "a%20b");
    assert_eq!(super::encode_audit_log_reason("50%"), "50%25");
    assert_eq!(super::encode_audit_log_reason("스팸"), "%EC%8A%A4%ED%8C%B8");
    assert_eq!(super::encode_audit_log_reason("ok!~*'()"), "ok!~*'()");
}

#[test]
fn server_error_backoff_doubles_and_is_bounded() {
    assert!((super::server_error_backoff(1) - 0.5).abs() < f64::EPSILON);
    assert!((super::server_error_backoff(2) - 1.0).abs() < f64::EPSILON);
    assert!((super::server_error_backoff(3) - 2.0).abs() < f64::EPSILON);
    assert!(super::server_error_backoff(100) <= 200.0);
}

#[tokio::test]
async fn with_reason_sends_encoded_audit_log_header_on_mutations_only() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, channel_payload("9", 0, Some("general"))),
    ])
    .await;

    let client = RestClient::new_with_base_url("reason-token", 123, base_url);
    let scoped = client.with_reason("spam 스팸");
    scoped
        .remove_guild_member(test_id(1), test_id(2))
        .await
        .expect("kick with reason");
    scoped.get_channel(test_id(9)).await.expect("get channel");
    task.await.expect("server task");

    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("x-audit-log-reason"),
        Some("spam%20%EC%8A%A4%ED%8C%B8")
    );
    assert_eq!(
        requests[1].header("x-audit-log-reason"),
        None,
        "GET requests must not carry the audit-log header"
    );
}

#[tokio::test]
async fn server_errors_are_retried_and_then_succeed() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::text(StatusCode::BAD_GATEWAY, "bad gateway"),
        PlannedResponse::json(StatusCode::OK, channel_payload("9", 0, Some("general"))),
    ])
    .await;

    let client = RestClient::new_with_base_url("retry-5xx-token", 123, base_url);
    let channel = client
        .get_channel(test_id(9))
        .await
        .expect("retried channel get");
    assert_eq!(channel.id, crate::model::test_id(9));
    task.await.expect("server task");

    assert_eq!(captured.lock().expect("captured").len(), 2);
}

#[tokio::test]
async fn server_errors_are_not_replayed_for_post_requests() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::text(
        StatusCode::BAD_GATEWAY,
        "bad gateway",
    )])
    .await;

    let client = RestClient::new_with_base_url("no-replay-token", 123, base_url);
    let error = client
        .create_message(
            test_id(9),
            &CreateMessage {
                content: Some("hello".to_string()),
                ..CreateMessage::default()
            },
        )
        .await
        .expect_err("a 502 on POST must surface instead of being replayed");
    assert!(
        matches!(error, DiscordError::Api { status: 502, .. }),
        "{error:?}"
    );
    task.await.expect("server task");
    assert_eq!(
        captured.lock().expect("captured").len(),
        1,
        "a POST must not be sent twice after an ambiguous 5xx"
    );
}

#[tokio::test]
async fn server_errors_are_replayed_for_nonce_enforced_message_creates() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::text(StatusCode::BAD_GATEWAY, "bad gateway"),
        PlannedResponse::json(StatusCode::OK, message_payload("1", "9", "hello")),
    ])
    .await;

    let client = RestClient::new_with_base_url("nonce-token", 123, base_url);
    client
        .create_message(
            test_id(9),
            &CreateMessage {
                content: Some("hello".to_string()),
                nonce: Some("dedupe-1".to_string()),
                enforce_nonce: true,
                ..CreateMessage::default()
            },
        )
        .await
        .expect("Discord deduplicates enforced nonces, so the replay is safe");
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 2);
}

#[tokio::test]
async fn server_errors_are_replayed_for_post_when_explicitly_enabled() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::text(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        PlannedResponse::json(StatusCode::OK, message_payload("1", "9", "hello")),
    ])
    .await;

    let client = RestClient::builder("opt-in-token", Some(test_id(123)))
        .api_base(base_url)
        .retry_non_idempotent_requests(true)
        .build()
        .expect("client");
    client
        .create_message(
            test_id(9),
            &CreateMessage {
                content: Some("hello".to_string()),
                ..CreateMessage::default()
            },
        )
        .await
        .expect("opted-in POST retry");
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 2);
}

#[test]
fn replay_policy_follows_method_operation_and_nonce_enforcement() {
    use super::body::RequestBody;
    use super::ReplayPolicy;

    let policy = |method: Method, path: &str, body: Option<&RequestBody>, opt_in: bool| {
        ReplayPolicy::for_request(&method, &route(method.clone(), path), body, opt_in)
    };
    let routed_policy = |id: RouteId, path: &str, body: Option<&RequestBody>, opt_in: bool| {
        ReplayPolicy::for_request(&id.method(), &routed(id, path), body, opt_in)
    };
    for method in [
        Method::GET,
        Method::HEAD,
        Method::OPTIONS,
        Method::PUT,
        Method::DELETE,
    ] {
        assert_eq!(
            policy(method.clone(), "/channels/1", None, false),
            ReplayPolicy::Always,
            "{method} is idempotent"
        );
    }
    let encoded = |enforces_nonce: bool| {
        super::body::message_json_body(
            &json!({ "nonce": "n", "enforce_nonce": true }),
            enforces_nonce,
        )
        .unwrap()
        .unwrap()
    };
    // The flag comes from the typed request, not from the encoded JSON.
    let plain = encoded(false);
    let enforced = encoded(true);
    let message_create = "/channels/1/messages";

    assert_eq!(
        routed_policy(RouteId::CreateMessage, message_create, Some(&plain), false),
        ReplayPolicy::Never
    );
    assert_eq!(
        routed_policy(
            RouteId::CreateMessage,
            message_create,
            Some(&enforced),
            false
        ),
        ReplayPolicy::WithinNonceWindow
    );
    // Only the channel message create route implements nonce deduplication.
    assert_eq!(
        routed_policy(
            RouteId::ExecuteWebhook,
            "/webhooks/1/tok",
            Some(&enforced),
            false
        ),
        ReplayPolicy::Never
    );
    assert_eq!(
        routed_policy(RouteId::EditMessage, message_create, Some(&enforced), false),
        ReplayPolicy::Never
    );
    assert_eq!(
        routed_policy(
            RouteId::StartThreadFromMessage,
            "/channels/1/messages/2/threads",
            Some(&enforced),
            false
        ),
        ReplayPolicy::Never
    );
    // A raw request is opaque to the client: same path and body, no replay.
    assert_eq!(
        policy(Method::POST, message_create, Some(&enforced), false),
        ReplayPolicy::Never
    );
    assert_eq!(
        routed_policy(RouteId::CreateMessage, message_create, Some(&plain), true),
        ReplayPolicy::Always,
        "the explicit opt-in replays every method"
    );

    let multipart = super::body::message_multipart_body(&json!({}), &[], true)
        .unwrap()
        .unwrap();
    assert!(multipart.enforces_nonce());
    assert!(
        !super::body::json_body(&json!({ "nonce": "n", "enforce_nonce": true }))
            .unwrap()
            .unwrap()
            .enforces_nonce()
    );

    // The 2.x adapters read the same rule from their bodies.
    use super::messages::{enforces_nonce, value_enforces_nonce};
    assert!(!value_enforces_nonce(
        &json!({ "nonce": "n", "enforce_nonce": false })
    ));
    assert!(!value_enforces_nonce(
        &json!({ "nonce": null, "enforce_nonce": true })
    ));
    assert!(value_enforces_nonce(
        &json!({ "nonce": 5, "enforce_nonce": true })
    ));
    let mut message = CreateMessage {
        nonce: Some("n".to_string()),
        ..CreateMessage::default()
    };
    assert!(!enforces_nonce(&message));
    message.enforce_nonce = true;
    assert!(enforces_nonce(&message));
    message.nonce = None;
    assert!(!enforces_nonce(&message));
}

#[test]
fn nonce_replays_are_bounded_by_the_deduplication_window() {
    use super::{ReplayPolicy, NONCE_DEDUP_WINDOW};

    let now = Instant::now();
    let policy = ReplayPolicy::WithinNonceWindow;
    assert!(policy.allows_replay(None, Duration::ZERO));
    assert!(policy.allows_replay(Some(now), Duration::from_secs(2)));
    assert!(
        !policy.allows_replay(Some(now), NONCE_DEDUP_WINDOW),
        "a wait that reaches the window must stop the replay"
    );
    if let Some(long_ago) = now.checked_sub(NONCE_DEDUP_WINDOW + Duration::from_secs(1)) {
        assert!(!policy.allows_replay(Some(long_ago), Duration::ZERO));
    }
    assert!(ReplayPolicy::Always.allows_replay(Some(now), Duration::from_secs(86_400)));
    assert!(!ReplayPolicy::Never.allows_replay(None, Duration::ZERO));
}

#[tokio::test]
async fn nonce_bearing_webhook_posts_are_not_replayed() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::text(
        StatusCode::BAD_GATEWAY,
        "bad gateway",
    )])
    .await;

    let client = RestClient::new_with_base_url("webhook-nonce", 123, base_url);
    client
        .request::<serde_json::Value>(
            RawRequest::post("/webhooks/9/webhook-secret")
                .json(&json!({ "content": "hi", "nonce": "n", "enforce_nonce": true })),
        )
        .await
        .expect_err("webhooks do not deduplicate nonces, so the 502 must surface");
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 1);
}

#[tokio::test]
async fn long_rate_limit_after_ambiguous_attempt_stops_nonce_replay() {
    // 502 (maybe applied) → nonce replay after backoff → 429 asking for a
    // wait longer than Discord's dedup window → the original failure is
    // returned instead of sleeping and replaying outside the window.
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::text(StatusCode::BAD_GATEWAY, "bad gateway"),
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "retry_after": 120.0, "global": false }),
        ),
    ])
    .await;

    let client = RestClient::new_with_base_url("nonce-window", 123, base_url);
    let started = Instant::now();
    let error = client
        .create_message(
            test_id(9),
            &CreateMessage {
                content: Some("hello".to_string()),
                nonce: Some("dedupe-2".to_string()),
                enforce_nonce: true,
                ..CreateMessage::default()
            },
        )
        .await
        .expect_err("the replay window was exceeded");
    assert!(
        matches!(error, DiscordError::Api { status: 502, .. }),
        "{error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "must not sleep out the 429"
    );
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 2);
}

#[tokio::test]
async fn managed_client_does_not_follow_redirects() {
    let mut redirect = PlannedResponse::empty(StatusCode::SEE_OTHER);
    redirect
        .headers
        .push(("Location".to_string(), "/elsewhere".to_string()));
    let (base_url, captured, task) = spawn_test_server(vec![redirect]).await;

    let client = RestClient::builder("redirect-token", Some(test_id(123)))
        .api_base(base_url)
        .build()
        .expect("client");
    let error = client
        .request::<serde_json::Value>(
            RawRequest::post("/channels/9/messages").json(&json!({ "content": "x" })),
        )
        .await
        .expect_err("a redirect is surfaced, not followed");
    assert!(
        matches!(error, DiscordError::Api { status: 303, .. }),
        "{error:?}"
    );
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 1);
}

#[tokio::test]
async fn custom_clients_do_not_retry_post_connect_failures() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);

    // The custom client's redirect policy is unknown, so a connect failure
    // does not prove a POST was never delivered.
    let client = RestClient::builder("custom-client", Some(test_id(123)))
        .api_base(format!("http://{address}"))
        .use_client(reqwest::Client::new())
        .build()
        .expect("client");
    let started = Instant::now();
    let error = client
        .request::<serde_json::Value>(
            RawRequest::post("/channels/9/messages").json(&json!({ "content": "x" })),
        )
        .await
        .expect_err("connection refused");
    assert!(error.is_retryable_transport(), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_millis(450),
        "no 0.5s backoff retry may happen, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn api_error_text_echoing_the_route_token_is_scrubbed() {
    let (base_url, _captured, task) = spawn_test_server(vec![PlannedResponse::json(
        StatusCode::NOT_FOUND,
        json!({ "code": 10015, "message": "Unknown Webhook /webhooks/9/echoed-secret" }),
    )])
    .await;

    let client = RestClient::new_with_base_url("echo", 123, base_url);
    let error = client
        .execute_webhook(test_id(9), "echoed-secret", &json!({ "content": "x" }))
        .await
        .expect_err("404");
    let text = format!("{error} {error:?}");
    assert!(!text.contains("echoed-secret"), "token leaked: {text}");
    assert!(text.contains("/webhooks/9/:token"), "{text}");
    task.await.expect("server task");
}

#[tokio::test]
async fn queued_requests_move_to_the_learned_bucket_gate() {
    // A holds the route gate R while the bucket is unknown and B queues on
    // R. A's response teaches bucket K; C then takes K's gate directly. When
    // A releases R, B must notice the key changed and wait for K instead of
    // running beside C.
    let client = RestClient::new("gate-race", Some(test_id(123)));
    let messages = route(Method::POST, "/channels/1/messages");
    let (route_gate_key, a_permit) = client.acquire_route_gate(&messages).await;
    assert_eq!(route_gate_key, messages.key);

    let b_client = client.clone();
    let b_route = messages.clone();
    let (b_done_tx, mut b_done_rx) = tokio::sync::oneshot::channel();
    let b = tokio::spawn(async move {
        let (key, permit) = b_client.acquire_route_gate(&b_route).await;
        let _ = b_done_tx.send(key);
        drop(permit);
    });
    tokio::task::yield_now().await;

    let mut bucket_headers = HeaderMap::new();
    bucket_headers.insert("x-ratelimit-bucket", HeaderValue::from_static("learned"));
    client
        .rate_limits
        .observe(&messages, &bucket_headers, StatusCode::OK, "");
    let (bucket_gate_key, c_permit) = client.acquire_route_gate(&messages).await;
    assert_eq!(bucket_gate_key, "learned@channels/1");

    drop(a_permit);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        b_done_rx.try_recv().is_err(),
        "B must wait for the bucket gate held by C"
    );

    drop(c_permit);
    let b_key = tokio::time::timeout(Duration::from_secs(2), b_done_rx)
        .await
        .expect("B acquires once C releases")
        .expect("B reports its key");
    assert_eq!(b_key, "learned@channels/1");
    b.await.expect("B task");
}

#[tokio::test]
async fn transport_error_messages_redact_route_tokens() {
    // Bind and drop a listener to obtain a port that refuses connections,
    // producing a reqwest connect error whose message embeds the URL.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);

    let error = default_http_client()
        .post(format!(
            "http://{address}/api/v10/webhooks/1/super-secret-token?wait=true"
        ))
        .send()
        .await
        .expect_err("connection must be refused");
    let http_error = crate::error::HttpError::new(&error);
    assert!(
        !http_error.message().contains("super-secret-token"),
        "token leaked: {}",
        http_error.message()
    );
    assert!(
        http_error.message().contains("/webhooks/1/:token"),
        "{}",
        http_error.message()
    );
}

#[tokio::test]
async fn rate_limit_retry_uses_retry_after_header_for_non_json_bodies() {
    let mut limited = PlannedResponse::text(StatusCode::TOO_MANY_REQUESTS, "<html>limited</html>");
    limited
        .headers
        .push(("Retry-After".to_string(), "0".to_string()));

    let (base_url, captured, task) = spawn_test_server(vec![
        limited,
        PlannedResponse::json(StatusCode::OK, channel_payload("9", 0, Some("general"))),
    ])
    .await;

    let client = RestClient::new_with_base_url("retry-header-token", 123, base_url);
    client
        .get_channel(test_id(9))
        .await
        .expect("channel after 429");
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 2);
}

#[tokio::test]
#[allow(
    deprecated,
    reason = "routes Discord removed in 2025 stay deprecated in 3.0"
)]
async fn new_guild_lifecycle_routes_have_expected_shapes() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::json(StatusCode::CREATED, guild_payload("1", "created")),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::CREATED, guild_payload("2", "from-template")),
        PlannedResponse::json(StatusCode::OK, json!({ "level": 1 })),
    ])
    .await;

    let client = RestClient::new_with_base_url("guild-token", 123, base_url);
    client
        .create_guild(&crate::model::CreateGuild {
            name: "created".to_string(),
            ..crate::model::CreateGuild::default()
        })
        .await
        .expect("create guild");
    client.delete_guild(test_id(1)).await.expect("delete guild");
    client
        .create_guild_from_template(
            "abcDEF123",
            &crate::model::CreateGuildFromTemplate {
                name: "from-template".to_string(),
                icon: None,
            },
        )
        .await
        .expect("create guild from template");
    let mfa = client
        .modify_guild_mfa_level(test_id(2), MfaLevel::ELEVATED)
        .await
        .expect("modify mfa");
    assert_eq!(mfa.level, MfaLevel::ELEVATED);
    task.await.expect("server task");

    let requests = captured.lock().expect("captured");
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/guilds");
    assert_eq!(requests[1].method, "DELETE");
    assert_eq!(requests[1].path, "/guilds/1");
    assert_eq!(requests[2].method, "POST");
    assert_eq!(requests[2].path, "/guilds/templates/abcDEF123");
    assert_eq!(requests[3].method, "POST");
    assert_eq!(requests[3].path, "/guilds/2/mfa");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[3].body).expect("mfa body"),
        json!({ "level": 1 })
    );
}

#[tokio::test]
async fn interaction_response_with_result_requests_callback_resource() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::json(
        StatusCode::OK,
        json!({
            "interaction": { "id": "10", "type": 2, "response_message_id": "11" },
            "resource": {
                "type": 4,
                "message": message_payload("11", "12", "hello")
            }
        }),
    )])
    .await;

    let client = RestClient::new_with_base_url("callback-token", 123, base_url);
    let result = client
        .create_interaction_response_with_result(
            test_id(10),
            "interactiontoken",
            &crate::request::InteractionResponse::reply("hello"),
        )
        .await
        .expect("callback result");
    task.await.expect("server task");

    assert_eq!(
        result.interaction.response_message_id,
        Some(crate::model::test_id(11))
    );
    let message = result
        .resource
        .expect("resource present")
        .message
        .expect("message present");
    assert_eq!(message.content, "hello");

    let requests = captured.lock().expect("captured");
    assert_eq!(
        requests[0].path,
        "/interactions/10/interactiontoken/callback?with_response=true"
    );
}

#[tokio::test]
async fn forward_message_sends_forward_reference_type() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::json(
        StatusCode::OK,
        message_payload("31", "30", ""),
    )])
    .await;

    let client = RestClient::new_with_base_url("forward-token", 123, base_url);
    client
        .forward_message(test_id(20), test_id(21), test_id(30))
        .await
        .expect("forward message");
    task.await.expect("server task");

    let requests = captured.lock().expect("captured");
    assert_eq!(requests[0].path, "/channels/30/messages");
    let body: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("forward body json");
    assert_eq!(body["message_reference"]["type"], json!(1));
    assert_eq!(body["message_reference"]["channel_id"], json!("20"));
    assert_eq!(body["message_reference"]["message_id"], json!("21"));
}

#[tokio::test]
async fn rest_client_builder_applies_base_url_user_agent_and_api_version() {
    let responses = vec![PlannedResponse::json(
        StatusCode::OK,
        json!({ "url": "wss://gateway.discord.gg" }),
    )];
    let (base_url, captured, server) = spawn_test_server(responses).await;

    let client = RestClient::builder("builder-token", Some(test_id(123)))
        .api_base(format!("{base_url}/"))
        .user_agent("MyLib/1.2 (+https://example.test)")
        .connect_timeout(Duration::from_secs(2))
        .request_timeout(Duration::from_secs(5))
        .build()
        .expect("build client");
    assert_eq!(client.api_base(), base_url);

    let gateway = client.get_gateway().await.expect("get gateway");
    assert_eq!(gateway.url, "wss://gateway.discord.gg");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/gateway");
    assert_eq!(
        requests[0].header("user-agent"),
        Some("MyLib/1.2 (+https://example.test)")
    );

    // api_version selects the versioned default host; api_base wins when set.
    let versioned = RestClient::builder("t", Some(test_id(1)))
        .api_version(9)
        .build()
        .expect("versioned client");
    assert_eq!(versioned.api_base(), "https://discord.com/api/v9");
    let overridden = RestClient::builder("t", Some(test_id(1)))
        .api_version(9)
        .api_base("http://proxy.example.test/api")
        .build()
        .expect("overridden client");
    assert_eq!(overridden.api_base(), "http://proxy.example.test/api");
    let default_base = RestClient::builder("t", Some(test_id(1)))
        .build()
        .expect("default client");
    assert_eq!(default_base.api_base(), super::API_BASE);
}

#[tokio::test]
async fn rest_client_builder_rate_limit_callback_fires_on_every_429() {
    let mut global_rate_limited = PlannedResponse::json(
        StatusCode::TOO_MANY_REQUESTS,
        json!({ "message": "You are being rate limited.", "retry_after": 0.01 }),
    );
    global_rate_limited
        .headers
        .push(("x-ratelimit-global".to_string(), "true".to_string()));
    let responses = vec![
        PlannedResponse::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({
                "message": "You are being rate limited.",
                "retry_after": 0.01,
                "global": false
            }),
        ),
        global_rate_limited,
        PlannedResponse::json(StatusCode::OK, message_payload("900", "555", "sent")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;

    let observed: Arc<Mutex<Vec<super::RateLimitInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let observed_for_callback = Arc::clone(&observed);
    let client = RestClient::builder("limited-token", Some(test_id(123)))
        .api_base(base_url)
        .rate_limit_callback(Arc::new(move |info| {
            observed_for_callback
                .lock()
                .expect("callback mutex")
                .push(info);
        }))
        .build()
        .expect("build client");

    let message = client
        .create_message(
            test_id(555),
            &CreateMessage {
                content: Some("hello".to_string()),
                ..CreateMessage::default()
            },
        )
        .await
        .expect("create message after retries");
    assert_eq!(message.content, "sent");

    server.await.expect("server finished");
    assert_eq!(captured.lock().expect("captured requests").len(), 3);

    let observed = observed.lock().expect("observed rate limits");
    let expected_route = rate_limit_route_key(&Method::POST, "/channels/555/messages");
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0].route, expected_route);
    assert!((observed[0].retry_after - 0.01).abs() < 1e-9);
    assert!(!observed[0].global);
    assert_eq!(observed[1].route, expected_route);
    assert!(observed[1].global);
}

#[test]
fn rest_client_builder_proxy_and_custom_client_construct() {
    RestClient::builder("t", Some(test_id(1)))
        .proxy("http://127.0.0.1:9999")
        .build()
        .expect("http proxy accepted");
    RestClient::builder("t", Some(test_id(1)))
        .proxy("https://user:pass@127.0.0.1:9999")
        .build()
        .expect("https proxy accepted");
    assert!(RestClient::builder("t", Some(test_id(1)))
        .proxy("::::not a proxy url")
        .build()
        .is_err());
    RestClient::builder("t", Some(test_id(1)))
        .use_client(default_http_client())
        .build()
        .expect("custom reqwest client accepted");
}

#[tokio::test]
async fn default_allowed_mentions_injected_when_absent_and_kept_when_present() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, message_payload("901", "555", "first")),
        PlannedResponse::json(StatusCode::OK, message_payload("902", "555", "second")),
        PlannedResponse::json(StatusCode::OK, message_payload("903", "555", "third")),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;

    let client = RestClient::builder("mentions-token", Some(test_id(123)))
        .api_base(base_url)
        .default_allowed_mentions(crate::model::AllowedMentions {
            users: vec![test_id(42)],
            replied_user: Some(false),
            ..crate::model::AllowedMentions::default()
        })
        .build()
        .expect("build client");

    client
        .create_message(
            test_id(555),
            &CreateMessage {
                content: Some("no explicit mentions".to_string()),
                ..CreateMessage::default()
            },
        )
        .await
        .expect("create message");
    client
        .create_message(
            test_id(555),
            &CreateMessage {
                content: Some("explicit mentions".to_string()),
                allowed_mentions: Some(crate::model::AllowedMentions {
                    parse: vec!["users".to_string()],
                    ..crate::model::AllowedMentions::default()
                }),
                ..CreateMessage::default()
            },
        )
        .await
        .expect("create message with explicit mentions");
    client
        .update_message(
            test_id(555),
            test_id(901),
            &CreateMessage {
                content: Some("edited".to_string()),
                ..CreateMessage::default()
            },
        )
        .await
        .expect("update message");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 3);

    let injected: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("first body json");
    assert_eq!(
        injected["allowed_mentions"],
        json!({ "users": ["42"], "replied_user": false })
    );

    let explicit: serde_json::Value =
        serde_json::from_str(&requests[1].body).expect("second body json");
    assert_eq!(explicit["allowed_mentions"], json!({ "parse": ["users"] }));

    let updated: serde_json::Value =
        serde_json::from_str(&requests[2].body).expect("third body json");
    assert_eq!(
        updated["allowed_mentions"],
        json!({ "users": ["42"], "replied_user": false })
    );
}

#[tokio::test]
async fn default_allowed_mentions_cover_webhook_and_interaction_response_payloads() {
    let responses = vec![
        PlannedResponse::json(StatusCode::OK, json!({ "id": "1000" })),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ];
    let (base_url, captured, server) = spawn_test_server(responses).await;

    let client = RestClient::builder("mentions-token", Some(test_id(123)))
        .api_base(base_url)
        .default_allowed_mentions(crate::model::AllowedMentions {
            replied_user: Some(false),
            ..crate::model::AllowedMentions::default()
        })
        .build()
        .expect("build client");

    client
        .execute_webhook(
            test_id(777),
            "webhooktoken",
            &json!({ "content": "from webhook" }),
        )
        .await
        .expect("execute webhook");
    client
        .create_interaction_response_typed(
            test_id(10),
            "interactiontoken",
            &InteractionCallbackResponse {
                kind: InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
                data: Some(json!({ "content": "reply" })),
            },
        )
        .await
        .expect("interaction response");
    client
        .create_interaction_response_typed(
            test_id(11),
            "interactiontoken",
            &InteractionCallbackResponse {
                kind: InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
                data: Some(json!({
                    "content": "explicit",
                    "allowed_mentions": { "parse": ["roles"] }
                })),
            },
        )
        .await
        .expect("interaction response with explicit mentions");
    client
        .create_interaction_response_typed(
            test_id(12),
            "interactiontoken",
            &InteractionCallbackResponse {
                kind: InteractionResponseType::MODAL,
                data: Some(json!({ "custom_id": "modal", "title": "t", "components": [] })),
            },
        )
        .await
        .expect("modal response");

    server.await.expect("server finished");
    let requests = captured.lock().expect("captured requests");
    assert_eq!(requests.len(), 4);

    let webhook_body: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("webhook body json");
    assert_eq!(
        webhook_body["allowed_mentions"],
        json!({ "replied_user": false })
    );

    let injected: serde_json::Value =
        serde_json::from_str(&requests[1].body).expect("interaction body json");
    assert_eq!(
        injected["data"]["allowed_mentions"],
        json!({ "replied_user": false })
    );

    let explicit: serde_json::Value =
        serde_json::from_str(&requests[2].body).expect("explicit body json");
    assert_eq!(
        explicit["data"]["allowed_mentions"],
        json!({ "parse": ["roles"] })
    );

    let modal: serde_json::Value =
        serde_json::from_str(&requests[3].body).expect("modal body json");
    assert!(modal["data"].get("allowed_mentions").is_none());
}

// --- Phase 1.5: Discord API changes after 2026-07-27 ---

#[tokio::test]
async fn invite_target_user_routes_have_expected_shapes() {
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ])
    .await;

    let client = RestClient::new_with_base_url("invite-token", 123, base_url);
    client
        .add_invite_target_user("abc", test_id(10))
        .await
        .expect("add target user");
    client
        .remove_invite_target_user("abc", test_id(11))
        .await
        .expect("remove target user");
    let ids = [test_id(12), test_id(13)];
    client
        .bulk_add_invite_target_users("abc", &ids)
        .await
        .expect("bulk add");
    client
        .bulk_delete_invite_target_users("abc", &ids)
        .await
        .expect("bulk delete");
    task.await.expect("server task");

    let requests = captured.lock().expect("captured");
    assert_eq!(requests.len(), 4);
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("PUT", "/invites/abc/target-users/10")
    );
    assert_eq!(
        (requests[1].method.as_str(), requests[1].path.as_str()),
        ("DELETE", "/invites/abc/target-users/11")
    );
    assert_eq!(
        (requests[2].method.as_str(), requests[2].path.as_str()),
        ("POST", "/invites/abc/target-users/bulk-add")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[2].body).expect("json"),
        json!({ "user_ids": ["12", "13"] })
    );
    assert_eq!(requests[3].path, "/invites/abc/target-users/bulk-delete");
}

#[tokio::test]
async fn invite_target_user_bulk_routes_reject_more_than_1000_ids_locally() {
    let client = RestClient::new("bulk-limit", Some(test_id(123)));
    let ids = (1..=1001).map(test_id).collect::<Vec<_>>();
    let error = client
        .bulk_add_invite_target_users("abc", &ids)
        .await
        .expect_err("1001 ids exceed Discord's cap");
    assert!(error.to_string().contains("1000"), "{error}");
    assert!(client
        .add_invite_target_user("bad/code", test_id(1))
        .await
        .is_err());
}

#[test]
fn create_channel_invite_serializes_target_user_ids() {
    let body = CreateChannelInvite {
        target_user_ids: Some(vec![test_id(1), test_id(2)]),
        ..CreateChannelInvite::default()
    };
    let value = serde_json::to_value(&body).expect("serialize");
    assert_eq!(value["target_user_ids"], json!(["1", "2"]));
    let empty = serde_json::to_value(CreateChannelInvite::default()).expect("serialize");
    assert!(empty.get("target_user_ids").is_none());
}

#[test]
fn percent_encoding_and_external_segments_block_path_changes() {
    use super::paths::{external_path_segment, percent_encode_component};

    assert_eq!(percent_encode_component("a/b?c#d e"), "a%2Fb%3Fc%23d%20e");
    assert_eq!(percent_encode_component("player-1_x.y"), "player-1_x.y");
    assert_eq!(
        external_path_segment("id", "steam:765/../x").expect("encoded"),
        "steam%3A765%2F..%2Fx"
    );
    for bad in ["", ".", ".."] {
        assert!(external_path_segment("id", bad).is_err(), "{bad:?}");
    }
}

#[tokio::test]
async fn application_identity_profile_routes_have_expected_shapes() {
    let profile = json!({
        "username": "PlayerOne",
        "metadata": { "region": "kr" },
        "data": {
            "primary": { "rank_name": "Gold", "total_wins": 42, "playtime_hours": 69.41 },
            "dynamic": [
                { "type": 1, "name": "Main", "value": "Mage" },
                { "type": 2, "name": "KDA", "value": 3.5 },
                { "type": 3, "name": "Badge", "value": { "url": "https://cdn.example/b.png" } }
            ]
        }
    });
    let identities = json!({
        "identities": [
            { "user_id": "7", "provider_type": "NONE", "provider_issued_user_id": "p/1" }
        ]
    });
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::json(StatusCode::CREATED, profile.clone()),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
        PlannedResponse::json(StatusCode::OK, profile.clone()),
        PlannedResponse::json(StatusCode::OK, identities.clone()),
        PlannedResponse::json(StatusCode::OK, identities),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ])
    .await;

    let client = RestClient::new_with_base_url("identity-token", 555, base_url);
    let update = crate::model::UpdateApplicationIdentityProfile {
        username: Some("PlayerOne".to_string()),
        data: Some(crate::model::ApplicationIdentityProfileData {
            primary: Some(crate::model::PrimaryProfileData {
                rank_name: Some("Gold".to_string()),
                ..Default::default()
            }),
            dynamic: vec![crate::model::DynamicProfileField::Number {
                name: "KDA".to_string(),
                value: 3.5,
            }],
        }),
    };

    let created = client
        .update_application_identity_profile(test_id(7), "p/1", &update)
        .await
        .expect("first write");
    let created = created.expect("201 returns the profile");
    assert_eq!(created.username.as_deref(), Some("PlayerOne"));
    assert_eq!(
        created.data.as_ref().map(|data| data.dynamic.len()),
        Some(3)
    );
    assert!(client
        .update_application_identity_profile(test_id(7), "p/1", &update)
        .await
        .expect("second write")
        .is_none());
    let fetched = client
        .get_application_identity_profile(test_id(7), "p/1")
        .await
        .expect("get profile");
    // The profile has no `PartialEq` (raw `metadata`); compare the JSON.
    assert_eq!(
        serde_json::to_value(&fetched).expect("fetched json"),
        serde_json::to_value(&created).expect("created json")
    );
    assert_eq!(
        created
            .metadata_as::<serde_json::Value>()
            .expect("metadata json"),
        Some(json!({ "region": "kr" }))
    );
    let by_user = client
        .get_user_application_identities(test_id(7))
        .await
        .expect("by user");
    assert_eq!(by_user.identities[0].provider_issued_user_id, "p/1");
    client
        .get_application_identities_by_external_id("NONE", "p/1", Some("eu west"))
        .await
        .expect("by external id");
    client
        .delete_application_identity(test_id(7), "NONE", "p/1", None)
        .await
        .expect("delete");
    task.await.expect("server task");

    let requests = captured.lock().expect("captured");
    assert_eq!(requests[0].method, "PATCH");
    assert_eq!(
        requests[0].path,
        "/applications/555/users/7/identities/p%2F1/profile"
    );
    let sent: serde_json::Value = serde_json::from_str(&requests[0].body).expect("json body");
    assert_eq!(
        sent["data"]["dynamic"][0],
        json!({ "type": 2, "name": "KDA", "value": 3.5 })
    );
    assert_eq!(requests[2].method, "GET");
    assert_eq!(requests[3].path, "/users/7/application-identities/555");
    assert_eq!(
        requests[4].path,
        "/applications/555/application-identities/NONE/p%2F1?provider_id=eu+west"
    );
    assert_eq!(requests[5].method, "POST");
    assert_eq!(
        requests[5].path,
        "/users/7/application-identities/555/NONE/p%2F1/delete"
    );
    assert_eq!(requests[5].body, "{}");
}

#[tokio::test]
async fn application_identity_profile_update_enforces_documented_limits() {
    let client = RestClient::new("identity-limits", Some(test_id(555)));
    let too_many = crate::model::UpdateApplicationIdentityProfile {
        username: None,
        data: Some(crate::model::ApplicationIdentityProfileData {
            primary: None,
            dynamic: (0..31)
                .map(|index| crate::model::DynamicProfileField::Number {
                    name: format!("f{index}"),
                    value: 1.0,
                })
                .collect(),
        }),
    };
    let error = client
        .update_application_identity_profile(test_id(7), "p", &too_many)
        .await
        .expect_err("31 dynamic fields exceed the cap");
    assert!(error.to_string().contains("30"), "{error}");

    let long_rank = crate::model::UpdateApplicationIdentityProfile {
        data: Some(crate::model::ApplicationIdentityProfileData {
            primary: Some(crate::model::PrimaryProfileData {
                rank_name: Some("r".repeat(101)),
                ..Default::default()
            }),
            dynamic: Vec::new(),
        }),
        ..Default::default()
    };
    assert!(long_rank.validate().is_err());

    let oversized = crate::model::UpdateApplicationIdentityProfile {
        data: Some(crate::model::ApplicationIdentityProfileData {
            primary: None,
            dynamic: (0..30)
                .map(|index| {
                    crate::model::DynamicProfileField::Unknown(
                        serde_json::value::to_raw_value(&json!({
                            "type": 99,
                            "name": format!("f{index}"),
                            "value": "x".repeat(400),
                        }))
                        .expect("raw field"),
                    )
                })
                .collect(),
        }),
        ..Default::default()
    };
    let error = oversized.validate().expect_err("over 10 KB serialized");
    assert!(error.to_string().contains("10240"), "{error}");

    assert!(RestClient::new("no-app", None)
        .get_user_application_identities(test_id(7))
        .await
        .is_err());
}

#[tokio::test]
async fn social_sdk_lobby_routes_added_since_the_last_audit_have_expected_shapes() {
    let lobby = json!({ "id": "100", "application_id": "555", "members": [] });
    let message = json!({
        "id": "900", "type": 0, "content": "gg", "lobby_id": "100", "channel_id": "100",
        "author": { "id": "7", "username": "player" },
        "lobby_member": { "additional_name": "Captain" },
        "flags": 0, "application_id": "555"
    });
    let (base_url, captured, task) = spawn_test_server(vec![
        PlannedResponse::json(StatusCode::OK, lobby),
        PlannedResponse::json(StatusCode::OK, message.clone()),
        PlannedResponse::json(StatusCode::OK, json!([message])),
        PlannedResponse::json(StatusCode::OK, json!({ "code": "self-code" })),
        PlannedResponse::json(StatusCode::OK, json!({ "code": "user-code" })),
        PlannedResponse::empty(StatusCode::NO_CONTENT),
    ])
    .await;

    let client = RestClient::new_with_base_url("bot-token", 555, base_url);
    let joined = client
        .get_or_create_lobby(
            "bearer-1",
            &crate::model::GetOrCreateLobby {
                secret: "match-42".to_string(),
                idle_timeout_seconds: Some(300),
                ..Default::default()
            },
        )
        .await
        .expect("get or create lobby");
    assert_eq!(joined.id, test_id(100));
    let sent = client
        .send_lobby_message(
            "bearer-1",
            test_id(100),
            &crate::model::CreateLobbyMessage {
                content: "gg".to_string(),
                ..Default::default()
            },
        )
        .await
        .expect("send lobby message");
    assert_eq!(
        sent.lobby_member
            .and_then(|member| member.additional_name)
            .as_deref(),
        Some("Captain")
    );
    let history = client
        .get_lobby_messages("bearer-1", test_id(100), Some(20))
        .await
        .expect("lobby messages");
    assert_eq!(history.len(), 1);
    let own = client
        .create_lobby_channel_invite_for_self("bearer-1", test_id(100))
        .await
        .expect("self invite");
    assert_eq!(own.code, "self-code");
    let theirs = client
        .create_lobby_channel_invite_for_user(test_id(100), test_id(8))
        .await
        .expect("user invite");
    assert_eq!(theirs.code, "user-code");
    client
        .delete_current_user_application_role_connection("bearer-2")
        .await
        .expect("delete role connection");
    task.await.expect("server task");

    let requests = captured.lock().expect("captured");
    let shapes = requests
        .iter()
        .map(|request| {
            (
                request.method.as_str(),
                request.path.as_str(),
                request.header("authorization"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        shapes,
        vec![
            ("PUT", "/lobbies", Some("Bearer bearer-1")),
            ("POST", "/lobbies/100/messages", Some("Bearer bearer-1")),
            (
                "GET",
                "/lobbies/100/messages?limit=20",
                Some("Bearer bearer-1")
            ),
            (
                "POST",
                "/lobbies/100/members/@me/invites",
                Some("Bearer bearer-1")
            ),
            (
                "POST",
                "/lobbies/100/members/8/invites",
                Some("Bot bot-token")
            ),
            (
                "DELETE",
                "/users/@me/applications/555/role-connection",
                Some("Bearer bearer-2")
            ),
        ]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[0].body).expect("json"),
        json!({ "secret": "match-42", "idle_timeout_seconds": 300 })
    );
}

#[tokio::test]
async fn lobby_message_limit_is_checked_locally() {
    let client = RestClient::new("bot-token", Some(test_id(555)));
    for limit in [0_u8, 201] {
        assert!(client
            .get_lobby_messages("bearer", test_id(100), Some(limit))
            .await
            .is_err());
    }
}

#[tokio::test]
async fn overwrite_edits_reject_a_body_type_that_contradicts_the_target() {
    // Port 9 (discard) is never contacted: the mismatch is caught first.
    let client = RestClient::new_with_base_url("overwrite-token", 123, "http://127.0.0.1:9");
    let role_body = EditChannelPermission::for_target(OverwriteTarget::Role(test_id(300)));
    let error = client
        .edit_channel_permissions(
            test_id(100),
            OverwriteTarget::Member(test_id(300)),
            &role_body,
        )
        .await
        .expect_err("a role body for a member target is rejected");
    assert!(
        error
            .to_string()
            .contains("does not match the overwrite target type 1"),
        "{error}"
    );
}

#[tokio::test]
async fn rest_client_application_id_is_optional_and_shared_by_clones() {
    let client = RestClient::new("app-token", None);
    assert_eq!(client.application_id(), None);
    let clone = client.clone();
    client.set_application_id(test_id(77));
    assert_eq!(clone.application_id(), Some(test_id(77)));
    assert_eq!(
        RestClient::builder("app-token", Some(test_id(5)))
            .build()
            .expect("default client")
            .application_id(),
        Some(test_id(5))
    );
}

#[tokio::test]
async fn raw_requests_choose_authorization_from_the_path_or_the_caller() {
    let (base_url, captured, server) = spawn_test_server(vec![
        PlannedResponse::json(StatusCode::OK, json!({})),
        PlannedResponse::json(StatusCode::OK, json!({})),
        PlannedResponse::json(StatusCode::OK, json!({})),
        PlannedResponse::json(StatusCode::OK, json!({})),
    ])
    .await;
    let client = RestClient::new_with_base_url("raw-token", 123, base_url);
    let _: serde_json::Value = client
        .request(RawRequest::get("/channels/1"))
        .await
        .unwrap();
    let _: serde_json::Value = client
        .request(RawRequest::get("/webhooks/1/hook-secret"))
        .await
        .unwrap();
    let _: serde_json::Value = client
        .request(RawRequest::get("/users/@me").bearer("user-access"))
        .await
        .unwrap();
    let _: serde_json::Value = client
        .request(RawRequest::get("/guilds/1/widget.png").without_authorization())
        .await
        .unwrap();
    server.await.unwrap();
    let auth = captured
        .lock()
        .unwrap()
        .iter()
        .map(|request| request.header("authorization").map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(
        auth.iter().map(Option::as_deref).collect::<Vec<_>>(),
        [
            Some("Bot raw-token"),
            None,
            Some("Bearer user-access"),
            None
        ]
    );

    // A bad bearer token fails before anything is sent.
    let offline = RestClient::new_with_base_url("raw-token", 123, "http://127.0.0.1:9");
    let error = offline
        .request::<serde_json::Value>(RawRequest::get("/users/@me").bearer(" "))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("bearer_token"), "{error}");
}

#[tokio::test]
async fn raw_posts_are_not_replayed_even_with_an_enforced_nonce() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::text(
        StatusCode::BAD_GATEWAY,
        "bad gateway",
    )])
    .await;
    let client = RestClient::new_with_base_url("raw-nonce", 123, base_url);
    let error = client
        .request::<serde_json::Value>(
            RawRequest::post("/channels/9/messages")
                .json(&json!({ "content": "hi", "nonce": "n", "enforce_nonce": true })),
        )
        .await
        .expect_err("the client cannot know the raw body enforces a nonce");
    assert!(
        matches!(error, DiscordError::Api { status: 502, .. }),
        "{error:?}"
    );
    task.await.expect("server task");
    assert_eq!(captured.lock().expect("captured").len(), 1);
}

#[tokio::test]
async fn raw_requests_scrub_the_route_token_and_hide_it_from_debug() {
    let (base_url, captured, task) = spawn_test_server(vec![PlannedResponse::json(
        StatusCode::NOT_FOUND,
        json!({ "code": 10015, "message": "Unknown Webhook /webhooks/9/raw-secret" }),
    )])
    .await;
    let client = RestClient::new_with_base_url("raw", 123, base_url);
    let request = RawRequest::post("/webhooks/9/raw-secret?wait=true").json(&json!({}));
    let debug = format!("{request:?}");
    assert!(!debug.contains("raw-secret"), "{debug}");
    assert_eq!(request.method(), Method::POST);
    assert_eq!(request.path(), "/webhooks/9/raw-secret?wait=true");
    let error = client
        .request::<serde_json::Value>(request)
        .await
        .expect_err("404");
    let text = format!("{error} {error:?}");
    assert!(!text.contains("raw-secret"), "token leaked: {text}");
    task.await.expect("server task");
    let requests = captured.lock().unwrap();
    assert_eq!(requests[0].path, "/webhooks/9/raw-secret?wait=true");
    assert_eq!(requests[0].header("authorization"), None);
    assert_eq!(requests[0].body, "{}");
}

#[tokio::test]
async fn raw_json_bytes_are_sent_as_given() {
    let (base_url, captured, task) =
        spawn_test_server(vec![PlannedResponse::empty(StatusCode::NO_CONTENT)]).await;
    let client = RestClient::new_with_base_url("raw", 123, base_url);
    client
        .request::<()>(RawRequest::patch("/channels/1").json_bytes(br#"{"b":1,"a":2}"#.to_vec()))
        .await
        .unwrap();
    task.await.expect("server task");
    let requests = captured.lock().unwrap();
    assert_eq!(requests[0].method, "PATCH");
    assert_eq!(requests[0].body, r#"{"b":1,"a":2}"#);
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
}

#[tokio::test]
async fn forum_threads_send_the_first_message_with_default_mentions() {
    use crate::request::{ForumThreadMessage, StartForumThread};

    let (base_url, captured, server) = spawn_test_server(vec![
        PlannedResponse::json(
            StatusCode::CREATED,
            channel_payload("910", 11, Some("post")),
        ),
        PlannedResponse::json(
            StatusCode::CREATED,
            channel_payload("911", 11, Some("post")),
        ),
        PlannedResponse::json(
            StatusCode::CREATED,
            channel_payload("912", 11, Some("post")),
        ),
    ])
    .await;
    let client = RestClient::new_with_base_url("forum-token", 1, base_url)
        .with_default_allowed_mentions(AllowedMentions {
            parse: vec!["users".to_string()],
            ..AllowedMentions::default()
        });
    let body = StartForumThread::new("post", ForumThreadMessage::new().content("hello"))
        .applied_tags(vec![test_id(5)]);
    let explicit = StartForumThread::new(
        "post",
        ForumThreadMessage::new()
            .content("quiet")
            .allowed_mentions(AllowedMentions::default()),
    );

    client
        .create_forum_thread(test_id(700), &body)
        .await
        .unwrap();
    client
        .create_forum_thread(test_id(700), &explicit)
        .await
        .unwrap();
    client
        .create_forum_thread_with_files(test_id(700), &body, &[sample_file("a.txt", "a")])
        .await
        .unwrap();
    server.await.unwrap();

    let requests = captured.lock().unwrap();
    assert_request_basics(
        &requests[0],
        "POST",
        "/channels/700/threads",
        Some("Bot forum-token"),
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[0].body).unwrap(),
        json!({
            "name": "post",
            "message": { "content": "hello", "allowed_mentions": { "parse": ["users"] } },
            "applied_tags": ["5"]
        })
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap()["message"]
            ["allowed_mentions"],
        json!({})
    );
    assert_multipart_request(
        &requests[2],
        "POST",
        "/channels/700/threads",
        Some("Bot forum-token"),
    );
    assert!(requests[2]
        .body
        .contains(r#""allowed_mentions":{"parse":["users"]}"#));
}
