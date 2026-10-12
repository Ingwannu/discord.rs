//! Guild audit log endpoints.

use crate::error::DiscordError;
use crate::model::{AuditLog, AuditLogActionType, AuditLogEntryId, AuditLogQuery, GuildId, UserId};

use super::paths::audit_log_query;
use super::{RestClient, RouteId, NO_BODY};

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
        action_type: Option<AuditLogActionType>,
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
        self.send(
            RouteId::GetGuildAuditLog,
            &format!("/guilds/{}/audit-logs{}", guild_id, query),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_audit_log_typed(
        &self,
        guild_id: GuildId,
        query: &AuditLogQuery,
    ) -> Result<AuditLog, DiscordError> {
        let query = audit_log_query(query);
        self.send(
            RouteId::GetGuildAuditLog,
            &format!("/guilds/{}/audit-logs{}", guild_id, query),
            NO_BODY,
        )
        .await
    }
}
