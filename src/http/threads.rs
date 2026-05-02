use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ArchivedThreadsQuery, Channel, JoinedArchivedThreadsQuery, Snowflake, ThreadListResponse,
    ThreadMember, ThreadMemberQuery,
};

use super::{
    archived_threads_query, bool_query, joined_archived_threads_query, thread_member_query,
    RestClient,
};

impl RestClient {
    pub async fn create_thread_from_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!(
                "/channels/{}/messages/{}/threads",
                channel_id.into(),
                message_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn create_thread(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/threads", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn join_thread(&self, channel_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/thread-members/@me", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn add_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/thread-members/{}",
                channel_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn leave_thread(&self, channel_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/thread-members/@me", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn remove_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/thread-members/{}",
                channel_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        with_member: Option<bool>,
    ) -> Result<ThreadMember, DiscordError> {
        let query = bool_query("with_member", with_member);
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members/{}{}",
                channel_id.into(),
                user_id.into(),
                query
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_thread_members(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.list_thread_members(channel_id, &ThreadMemberQuery::default())
            .await
    }

    pub async fn list_thread_members(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ThreadMemberQuery,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members{}",
                channel_id.into(),
                thread_member_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Fetches public archived threads as raw JSON.
    #[deprecated(
        since = "2.0.1",
        note = "Use list_public_archived_threads for a typed ThreadListResponse"
    )]
    pub async fn get_public_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, DiscordError> {
        let path = match limit {
            Some(l) => format!(
                "/channels/{}/threads/archived/public?limit={}",
                channel_id.into(),
                l
            ),
            None => format!("/channels/{}/threads/archived/public", channel_id.into()),
        };
        self.request(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn list_public_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/public{}",
                channel_id.into(),
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_private_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/private{}",
                channel_id.into(),
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_joined_private_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &JoinedArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/users/@me/threads/archived/private{}",
                channel_id.into(),
                joined_archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_active_guild_threads(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/threads/active", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }
}
