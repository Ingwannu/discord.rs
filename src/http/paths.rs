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

/// Validates a guild template code before it is placed in a URL path.
pub(crate) fn validate_template_code(code: &str) -> Result<(), DiscordError> {
    validate_token_path_segment("template_code", code, false)
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

/// Percent-encodes one URI component the way JavaScript's
/// `encodeURIComponent` does: unreserved characters and `!~*'()` pass
/// through, every other byte (including `/`, `?`, `#`, `%`, spaces, and any
/// non-ASCII UTF-8 byte) becomes `%XX`. Used for header values and for path
/// segments that carry caller-supplied strings.
pub(crate) fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}

/// Encodes a caller-supplied string (for example an external account ID)
/// as a single path segment.
///
/// Percent-encoding keeps `/`, `?`, and `#` from changing the route, but
/// `.` is unreserved and stays literal, so the dot segments `.` and `..`
/// would still be normalized away by servers and proxies; those and the
/// empty string are rejected instead.
pub(crate) fn external_path_segment(name: &str, value: &str) -> Result<String, DiscordError> {
    if value.is_empty() || value == "." || value == ".." {
        return Err(invalid_data_error(format!(
            "{name} must not be empty, \".\", or \"..\""
        )));
    }
    Ok(percent_encode_component(value))
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
    if let Some(shard) = query.shard {
        params.push(format!("shard={shard}"));
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

/// Returns true for the secret token segment of a tokenized route:
/// `webhooks/{id}/{token}/...` (webhook and interaction follow-up routes) and
/// `interactions/{id}/{token}/callback`.
pub(crate) fn is_token_segment(segments: &[&str], index: usize) -> bool {
    index == 2 && matches!(segments.first().copied(), Some("webhooks" | "interactions"))
}

/// Replaces a route token with a non-reversible digest.
///
/// Route keys appear in logs, [`RateLimitInfo`](super::RateLimitInfo) callbacks,
/// and `DiscordError::RateLimit`, so the raw token must never be part of them.
/// The token cannot simply become a fixed placeholder, though: interaction
/// follow-ups share the application id as their "webhook id", and Discord
/// limits them per token, so a placeholder would let one interaction's 429
/// block every other interaction. A 64-bit SipHash digest keeps per-token
/// identity for the lifetime of the process without revealing the token.
/// `DefaultHasher` is only stable within one build, which is all an
/// in-memory key needs.
fn token_digest(token: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    format!(":token-{:016x}", hasher.finish())
}

/// Returns the secret token segment of a request path relative to the API
/// base (`/webhooks/{id}/{token}/...` or `/interactions/{id}/{token}/...`).
///
/// The transport knows this exact string for every request it sends, so
/// diagnostics can be scrubbed by value instead of guessing where a token
/// sits inside an arbitrary URL.
pub(crate) fn route_token(path: &str) -> Option<&str> {
    let segments = path
        .split('?')
        .next()
        .unwrap_or(path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    segments
        .get(2)
        .copied()
        .filter(|_| is_token_segment(&segments, 2))
}

/// Replaces every occurrence of `token` in diagnostic text with `:token`.
///
/// Used for transport error strings (reqwest embeds the request URL) and for
/// API or proxy error bodies, which may echo the request URI back.
pub(crate) fn scrub_token(text: &str, token: Option<&str>) -> String {
    match token {
        Some(token) if !token.is_empty() => text.replace(token, ":token"),
        _ => text.to_string(),
    }
}

/// Returns `path` with the route token replaced by `:token`, for messages
/// that only need to identify the route (for example reqwest error strings,
/// which embed the full request URL).
///
/// This is the fallback when the token value is not known (see
/// [`scrub_token`] for the transport path). It accepts paths that still carry
/// an API base path (`/api/v10/...`, or any custom base) by locating the
/// token relative to the **last** `webhooks`/`interactions` segment that is
/// followed by at least two more segments, so a base such as
/// `/proxy/webhooks/` cannot shift the mask onto the wrong segment.
pub(crate) fn redact_path_tokens(path: &str) -> String {
    let (path_part, has_query) = match path.split_once('?') {
        Some((path_part, _)) => (path_part, true),
        None => (path, false),
    };
    let leading_slash = path_part.starts_with('/');
    let segments = path_part
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let token_index = segments
        .iter()
        .enumerate()
        .rev()
        .find(|(index, segment)| {
            matches!(**segment, "webhooks" | "interactions") && index + 2 < segments.len()
        })
        .map(|(resource_index, _)| resource_index + 2);
    let mut redacted = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            if Some(index) == token_index {
                ":token"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    if leading_slash {
        redacted.insert(0, '/');
    }
    if has_query {
        // Query strings can carry user-supplied values; they are never
        // needed to identify a route in diagnostics.
        redacted.push_str("?…");
    }
    redacted
}

/// What a request does, for policies that depend on the operation rather
/// than on the HTTP method alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RouteOperation {
    /// `POST /channels/{channel.id}/messages`: the one route whose
    /// `nonce` + `enforce_nonce` contract makes a replay safe.
    CreateChannelMessage,
    Other,
}

/// Rate-limit and retry identity of one REST request, derived once from the
/// method and path.
///
/// Execution decisions (bucket scoping, replay safety) read these fields
/// directly instead of re-parsing a display string. Phase 8's API drift
/// report is meant to consume the same metadata. The route token itself is
/// deliberately not stored; see [`route_token`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequestRoute {
    /// `METHOD:path` with minor ids replaced by `:id`, major parameters kept
    /// verbatim, and route tokens replaced by a digest. Safe to log.
    pub(crate) key: String,
    /// Major-parameter scope (for example `channels/123`,
    /// `webhooks/1/:token-…`) that Discord's bucket hash excludes.
    pub(crate) major_scope: String,
    pub(crate) operation: RouteOperation,
}

impl RequestRoute {
    pub(crate) fn new(method: &Method, path: &str) -> Self {
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
                if is_token_segment(&segments, index) {
                    token_digest(segment)
                } else if segment.chars().all(|ch| ch.is_ascii_digit())
                    && !is_major_parameter_segment(&segments, index)
                {
                    ":id".to_string()
                } else {
                    (*segment).to_string()
                }
            })
            .collect::<Vec<_>>();

        let mut keep = vec![false; segments.len()];
        for index in 1..segments.len() {
            if is_token_segment(&segments, index) {
                // A token identifies one webhook or one interaction; scoping
                // by it errs toward independent buckets, so an unexpected
                // shared limit costs a retried 429 instead of blocking other
                // tokens.
                keep[..=index].iter_mut().for_each(|keep| *keep = true);
            } else if is_major_parameter_segment(&segments, index) {
                keep[index - 1] = true;
                keep[index] = true;
            }
        }
        let major_scope = normalized
            .iter()
            .zip(&keep)
            .filter_map(|(segment, keep)| keep.then_some(segment.as_str()))
            .collect::<Vec<_>>()
            .join("/");

        let operation = match (method, segments.as_slice()) {
            (&Method::POST, ["channels", channel_id, "messages"])
                if channel_id.chars().all(|ch| ch.is_ascii_digit()) =>
            {
                RouteOperation::CreateChannelMessage
            }
            _ => RouteOperation::Other,
        };

        Self {
            key: format!("{method}:{}", normalized.join("/")),
            major_scope,
            operation,
        }
    }
}

/// Builds the rate-limit route key; see [`RequestRoute::key`].
#[cfg(test)]
pub(crate) fn rate_limit_route_key(method: &Method, path: &str) -> String {
    RequestRoute::new(method, path).key
}
