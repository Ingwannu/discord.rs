use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ArchivedThreadsQuery, Channel, ChannelId, GuildId, JoinedArchivedThreadsQuery, MessageId,
    ThreadListResponse, ThreadMember, ThreadMemberQuery, UserId,
};

use super::{
    archived_threads_query, bool_query, joined_archived_threads_query, thread_member_query,
    RestClient,
};

impl RestClient {
    pub async fn create_thread_from_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/messages/{}/threads", channel_id, message_id),
            Some(body),
        )
        .await
    }

    pub async fn create_thread(
        &self,
        channel_id: ChannelId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/threads", channel_id),
            Some(body),
        )
        .await
    }

    pub async fn join_thread(&self, channel_id: ChannelId) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/thread-members/@me", channel_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn add_thread_member(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/thread-members/{}", channel_id, user_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn leave_thread(&self, channel_id: ChannelId) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/thread-members/@me", channel_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn remove_thread_member(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/thread-members/{}", channel_id, user_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_thread_member(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
        with_member: Option<bool>,
    ) -> Result<ThreadMember, DiscordError> {
        let query = bool_query("with_member", with_member);
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members/{}{}",
                channel_id, user_id, query
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_thread_members(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.list_thread_members(channel_id, &ThreadMemberQuery::default())
            .await
    }

    pub async fn list_thread_members(
        &self,
        channel_id: ChannelId,
        query: &ThreadMemberQuery,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members{}",
                channel_id,
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
        channel_id: ChannelId,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, DiscordError> {
        let path = match limit {
            Some(l) => format!(
                "/channels/{}/threads/archived/public?limit={}",
                channel_id, l
            ),
            None => format!("/channels/{}/threads/archived/public", channel_id),
        };
        self.request(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn list_public_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/public{}",
                channel_id,
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_private_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/private{}",
                channel_id,
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_joined_private_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &JoinedArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/users/@me/threads/archived/private{}",
                channel_id,
                joined_archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_active_guild_threads(
        &self,
        guild_id: GuildId,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/threads/active", guild_id),
            Option::<&Value>::None,
        )
        .await
    }
}
