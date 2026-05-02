use reqwest::Method;
use url::form_urlencoded;

use crate::error::DiscordError;
use crate::model::{
    ArchivedThreadsQuery, AuditLogQuery, CurrentUserGuildsQuery, EntitlementQuery, GetGuildQuery,
    GuildBansQuery, GuildMembersQuery, JoinedArchivedThreadsQuery, SearchGuildMembersQuery,
    Snowflake, SubscriptionQuery, ThreadMemberQuery, WebhookExecuteQuery, WebhookMessageQuery,
};
use crate::types::invalid_data_error;

pub(crate) fn configured_application_id(application_id: u64) -> Result<String, DiscordError> {
    if application_id == 0 {
        return Err(invalid_data_error(
            "application_id must be set before follow-up webhook calls; use set_application_id() or create_followup_message_with_application_id()",
        ));
    }

    Ok(application_id.to_string())
}

pub(crate) fn validate_token_path_segment(
    name: &str,
    value: &str,
    allow_original_marker: bool,
) -> Result<(), DiscordError> {
    if allow_original_marker && value == "@original" {
        return Ok(());
    }
    if value.trim().is_empty() {
        return Err(invalid_data_error(format!("{name} must not be empty")));
    }
    if !value.bytes().all(|byte| {
        matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'~'
        )
    }) {
        return Err(invalid_data_error(format!(
            "{name} contains characters that are unsafe in a Discord path segment"
        )));
    }
    Ok(())
}

pub(crate) fn validate_snowflake_path_segment(
    name: &str,
    value: &Snowflake,
) -> Result<(), DiscordError> {
    if value.is_valid() {
        Ok(())
    } else {
        Err(invalid_data_error(format!(
            "{name} must be a valid Discord snowflake containing only ASCII digits"
        )))
    }
}

pub(crate) fn global_commands_path(application_id: u64) -> Result<String, DiscordError> {
    let application_id = configured_application_id(application_id)?;
    Ok(format!("/applications/{application_id}/commands"))
}

pub(crate) fn interaction_callback_path(
    interaction_id: Snowflake,
    interaction_token: &str,
) -> Result<String, DiscordError> {
    validate_snowflake_path_segment("interaction_id", &interaction_id)?;
    let interaction_token = interaction_token.trim();
    validate_token_path_segment("interaction_token", interaction_token, false)?;
    Ok(format!(
        "/interactions/{interaction_id}/{interaction_token}/callback"
    ))
}

pub(crate) fn execute_webhook_path(
    webhook_id: Snowflake,
    token: &str,
) -> Result<String, DiscordError> {
    execute_webhook_path_with_query(
        webhook_id,
        token,
        &WebhookExecuteQuery {
            wait: Some(true),
            ..WebhookExecuteQuery::default()
        },
        None,
    )
}

pub(crate) fn execute_webhook_path_with_query(
    webhook_id: Snowflake,
    token: &str,
    query: &WebhookExecuteQuery,
    suffix: Option<&str>,
) -> Result<String, DiscordError> {
    validate_snowflake_path_segment("webhook_id", &webhook_id)?;
    validate_token_path_segment("webhook_token", token, false)?;
    let suffix = suffix.unwrap_or_default();
    Ok(format!(
        "/webhooks/{webhook_id}/{token}{suffix}{}",
        webhook_execute_query(query, suffix.is_empty())
    ))
}

pub(crate) fn webhook_message_path(
    webhook_id: Snowflake,
    token: &str,
    message_id: &str,
) -> Result<String, DiscordError> {
    webhook_message_path_with_query(
        webhook_id,
        token,
        message_id,
        &WebhookMessageQuery::default(),
        false,
    )
}

pub(crate) fn webhook_message_path_with_query(
    webhook_id: Snowflake,
    token: &str,
    message_id: &str,
    query: &WebhookMessageQuery,
    include_components: bool,
) -> Result<String, DiscordError> {
    validate_snowflake_path_segment("webhook_id", &webhook_id)?;
    validate_token_path_segment("webhook_token", token, false)?;
    validate_token_path_segment("message_id", message_id, true)?;
    Ok(format!(
        "/webhooks/{webhook_id}/{token}/messages/{message_id}{}",
        webhook_message_query(query, include_components)
    ))
}

pub(crate) fn webhook_message_query(
    query: &WebhookMessageQuery,
    include_components: bool,
) -> String {
    let mut params = Vec::new();
    if let Some(thread_id) = &query.thread_id {
        params.push(format!("thread_id={}", thread_id.as_str()));
    }
    if include_components {
        if let Some(with_components) = query.with_components {
            params.push(format!("with_components={with_components}"));
        }
    }
    query_string(params)
}

pub(crate) fn webhook_execute_query(
    query: &WebhookExecuteQuery,
    include_components: bool,
) -> String {
    let mut params = Vec::new();
    if let Some(wait) = query.wait {
        params.push(format!("wait={wait}"));
    }
    if let Some(thread_id) = &query.thread_id {
        params.push(format!("thread_id={}", thread_id.as_str()));
    }
    if include_components {
        if let Some(with_components) = query.with_components {
            params.push(format!("with_components={with_components}"));
        }
    }
    query_string(params)
}

pub(crate) fn guild_prune_query(
    days: Option<u64>,
    compute_prune_count: Option<bool>,
    include_roles: &[Snowflake],
) -> String {
    let mut params = Vec::new();
    if let Some(days) = days {
        params.push(format!("days={days}"));
    }
    if let Some(compute_prune_count) = compute_prune_count {
        params.push(format!("compute_prune_count={compute_prune_count}"));
    }
    if !include_roles.is_empty() {
        let roles = include_roles
            .iter()
            .map(Snowflake::as_str)
            .collect::<Vec<_>>()
            .join(",");
        params.push(format!("include_roles={roles}"));
    }

    query_string(params)
}

pub(crate) fn query_string(params: Vec<String>) -> String {
    if params.is_empty() {
        String::new()
    } else {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for param in params {
            let (name, value) = param.split_once('=').unwrap_or((param.as_str(), ""));
            serializer.append_pair(name, value);
        }
        format!("?{}", serializer.finish())
    }
}

pub(crate) fn bool_query(name: &str, value: Option<bool>) -> String {
    query_string(
        value
            .map(|value| vec![format!("{name}={value}")])
            .unwrap_or_default(),
    )
}

pub(crate) fn audit_log_query(query: &AuditLogQuery) -> String {
    let mut params = Vec::new();
    if let Some(user_id) = &query.user_id {
        params.push(format!("user_id={user_id}"));
    }
    if let Some(action_type) = query.action_type {
        params.push(format!("action_type={action_type}"));
    }
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn current_user_guilds_query(query: &CurrentUserGuildsQuery) -> String {
    let mut params = Vec::new();
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(with_counts) = query.with_counts {
        params.push(format!("with_counts={with_counts}"));
    }
    query_string(params)
}

pub(crate) fn get_guild_query(query: &GetGuildQuery) -> String {
    bool_query("with_counts", query.with_counts)
}

pub(crate) fn guild_bans_query(query: &GuildBansQuery) -> String {
    let mut params = Vec::new();
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    query_string(params)
}

pub(crate) fn guild_members_query(query: &GuildMembersQuery) -> String {
    let mut params = Vec::new();
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    query_string(params)
}

pub(crate) fn search_guild_members_query(query: &SearchGuildMembersQuery) -> String {
    let mut params = vec![format!("query={}", query.query)];
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn thread_member_query(query: &ThreadMemberQuery) -> String {
    let mut params = Vec::new();
    if let Some(with_member) = query.with_member {
        params.push(format!("with_member={with_member}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn archived_threads_query(query: &ArchivedThreadsQuery) -> String {
    let mut params = Vec::new();
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn joined_archived_threads_query(query: &JoinedArchivedThreadsQuery) -> String {
    let mut params = Vec::new();
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn entitlement_query(query: &EntitlementQuery) -> String {
    let mut params = Vec::new();
    if let Some(user_id) = &query.user_id {
        params.push(format!("user_id={user_id}"));
    }
    if !query.sku_ids.is_empty() {
        let sku_ids = query
            .sku_ids
            .iter()
            .map(Snowflake::as_str)
            .collect::<Vec<_>>()
            .join(",");
        params.push(format!("sku_ids={sku_ids}"));
    }
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(guild_id) = &query.guild_id {
        params.push(format!("guild_id={guild_id}"));
    }
    if let Some(exclude_ended) = query.exclude_ended {
        params.push(format!("exclude_ended={exclude_ended}"));
    }
    if let Some(exclude_deleted) = query.exclude_deleted {
        params.push(format!("exclude_deleted={exclude_deleted}"));
    }
    query_string(params)
}

pub(crate) fn subscription_query(query: &SubscriptionQuery) -> String {
    let mut params = Vec::new();
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(after) = &query.after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(user_id) = &query.user_id {
        params.push(format!("user_id={user_id}"));
    }
    query_string(params)
}

pub(crate) fn invite_query(
    with_counts: Option<bool>,
    with_expiration: Option<bool>,
    guild_scheduled_event_id: Option<Snowflake>,
) -> String {
    let mut params = Vec::new();
    if let Some(with_counts) = with_counts {
        params.push(format!("with_counts={with_counts}"));
    }
    if let Some(with_expiration) = with_expiration {
        params.push(format!("with_expiration={with_expiration}"));
    }
    if let Some(guild_scheduled_event_id) = guild_scheduled_event_id {
        params.push(format!(
            "guild_scheduled_event_id={guild_scheduled_event_id}"
        ));
    }
    query_string(params)
}

pub(crate) fn poll_answer_voters_query(after: Option<Snowflake>, limit: Option<u64>) -> String {
    let mut params = Vec::new();
    if let Some(after) = after {
        params.push(format!("after={after}"));
    }
    if let Some(limit) = limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

pub(crate) fn followup_webhook_path(
    application_id: &str,
    interaction_token: &str,
    message_id: Option<&str>,
) -> Result<String, DiscordError> {
    let application_id = application_id.trim();
    if application_id.is_empty() || application_id == "0" {
        return Err(invalid_data_error(
            "application_id must be set before follow-up webhook calls",
        ));
    }
    validate_token_path_segment("application_id", application_id, false)?;

    validate_token_path_segment("interaction_token", interaction_token, false)?;

    match message_id {
        Some(message_id) => {
            validate_token_path_segment("message_id", message_id, true)?;
            Ok(format!(
                "/webhooks/{application_id}/{interaction_token}/messages/{message_id}"
            ))
        }
        None => Ok(format!("/webhooks/{application_id}/{interaction_token}")),
    }
}

pub(crate) fn request_uses_bot_authorization(path: &str) -> bool {
    let normalized_path = path
        .split('?')
        .next()
        .unwrap_or(path)
        .trim_start_matches('/');
    if normalized_path.starts_with("interactions/") {
        return false;
    }
    if normalized_path.starts_with("webhooks/") {
        let segments = normalized_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        return segments.len() < 3;
    }
    true
}

pub(crate) fn is_major_parameter_segment(segments: &[&str], index: usize) -> bool {
    matches!(
        segments.get(index.saturating_sub(1)).copied(),
        Some("applications" | "channels" | "guilds" | "webhooks")
    ) || (index >= 2 && matches!(segments.get(index - 2).copied(), Some("webhooks")))
}

pub(crate) fn rate_limit_route_key(method: &Method, path: &str) -> String {
    let segments = path
        .split('?')
        .next()
        .unwrap_or(path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let normalized = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            if segment.chars().all(|ch| ch.is_ascii_digit())
                && !is_major_parameter_segment(&segments, index)
            {
                ":id".to_string()
            } else {
                (*segment).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{method}:{normalized}")
}
