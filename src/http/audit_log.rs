//! Guild audit log endpoints.

use crate::error::DiscordError;
use crate::model::{AuditLog, AuditLogQuery, GuildId};

use super::paths::audit_log_query;
use super::{RestClient, RouteId, NO_BODY};

impl RestClient {
    /// Fetches a guild's audit log, filtered and paginated by `query`.
    pub async fn get_guild_audit_log(
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

    #[deprecated(since = "3.0.0", note = "renamed to `get_guild_audit_log`")]
    pub async fn get_guild_audit_log_typed(
        &self,
        guild_id: GuildId,
        query: &AuditLogQuery,
    ) -> Result<AuditLog, DiscordError> {
        self.get_guild_audit_log(guild_id, query).await
    }
}
