use serde::{Deserialize, Serialize};

use super::{Channel, Member, Snowflake};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadMember`.
pub struct ThreadMember {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_timestamp: Option<String>,
    #[serde(default)]
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadListResponse`.
pub struct ThreadListResponse {
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub members: Vec<ThreadMember>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ThreadMemberQuery`.
pub struct ThreadMemberQuery {
    pub with_member: Option<bool>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ArchivedThreadsQuery`.
pub struct ArchivedThreadsQuery {
    pub before: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `JoinedArchivedThreadsQuery`.
pub struct JoinedArchivedThreadsQuery {
    pub before: Option<Snowflake>,
    pub limit: Option<u64>,
}
