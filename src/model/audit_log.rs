//! Guild audit log entries and queries.

use serde::{Deserialize, Serialize};

use super::{
    ApplicationCommand, AuditLogEntryId, AutoModerationRule, Channel, GenericId,
    GuildScheduledEvent, Integration, User, UserId, Webhook,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLogEntry`.
pub struct AuditLogEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<AuditLogEntryId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// The affected entity: a user, role, channel, webhook, emoji, ...
    /// depending on `action_type` (and absent for some actions), so it stays
    /// a [`GenericId`]. Use [`Id::cast`](super::Id::cast) once the action type
    /// tells you the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLog`.
pub struct AuditLog {
    #[serde(default)]
    pub application_commands: Vec<ApplicationCommand>,
    #[serde(default)]
    pub audit_log_entries: Vec<AuditLogEntry>,
    #[serde(default)]
    pub auto_moderation_rules: Vec<AutoModerationRule>,
    #[serde(default)]
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
    #[serde(default)]
    pub integrations: Vec<Integration>,
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub webhooks: Vec<Webhook>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `AuditLogQuery`.
pub struct AuditLogQuery {
    pub user_id: Option<UserId>,
    pub action_type: Option<u64>,
    pub before: Option<AuditLogEntryId>,
    pub after: Option<AuditLogEntryId>,
    pub limit: Option<u64>,
}
