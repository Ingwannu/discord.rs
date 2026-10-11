//! Guild audit log endpoints.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{AuditLog, AuditLogEntryId, AuditLogQuery, GuildId, UserId};

use super::paths::audit_log_query;
use super::RestClient;

impl RestClient {
    /// Fetches a guild audit log as raw JSON.
    #[deprecated(
        since = "2.0.1",
        note = "Use get_guild_audit_log_typed for a typed AuditLog"
    )]
    pub async fn get_guild_audit_log(
        &self,
        guild_id: GuildId,
        user_id: Option<UserId>,
        action_type: Option<u64>,
        before: Option<AuditLogEntryId>,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, DiscordError> {
        let query = audit_log_query(&AuditLogQuery {
            user_id,
            action_type,
            before,
            after: None,
            limit,
        });
        self.request(
            Method::GET,
            &format!("/guilds/{}/audit-logs{}", guild_id, query),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_audit_log_typed(
        &self,
        guild_id: GuildId,
        query: &AuditLogQuery,
    ) -> Result<AuditLog, DiscordError> {
        let query = audit_log_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/audit-logs{}", guild_id, query),
            Option::<&Value>::None,
        )
        .await
    }
}
