use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ArchivedThreadsQuery, Channel, ChannelId, GuildId, JoinedArchivedThreadsQuery, MessageId,
    ThreadListResponse, ThreadMember, ThreadMemberQuery, UserId,
};

use super::{
    archived_threads_query, bool_query, joined_archived_threads_query, json_body,
    thread_member_query, RestClient, RouteId, NO_BODY,
};

impl RestClient {
    pub async fn create_thread_from_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::StartThreadFromMessage,
            &format!("/channels/{}/messages/{}/threads", channel_id, message_id),
            json_body(body),
        )
        .await
    }

    pub async fn create_thread(
        &self,
        channel_id: ChannelId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::StartThreadWithoutMessage,
            &format!("/channels/{}/threads", channel_id),
            json_body(body),
        )
        .await
    }

    pub async fn join_thread(&self, channel_id: ChannelId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::JoinThread,
            &format!("/channels/{}/thread-members/@me", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn add_thread_member(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::AddThreadMember,
            &format!("/channels/{}/thread-members/{}", channel_id, user_id),
            NO_BODY,
        )
        .await
    }

    pub async fn leave_thread(&self, channel_id: ChannelId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::LeaveThread,
            &format!("/channels/{}/thread-members/@me", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn remove_thread_member(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::RemoveThreadMember,
            &format!("/channels/{}/thread-members/{}", channel_id, user_id),
            NO_BODY,
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
        self.send(
            RouteId::GetThreadMember,
            &format!(
                "/channels/{}/thread-members/{}{}",
                channel_id, user_id, query
            ),
            NO_BODY,
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
        self.send(
            RouteId::ListThreadMembers,
            &format!(
                "/channels/{}/thread-members{}",
                channel_id,
                thread_member_query(query)
            ),
            NO_BODY,
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
        self.send(RouteId::ListPublicArchivedThreads, &path, NO_BODY)
            .await
    }

    pub async fn list_public_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.send(
            RouteId::ListPublicArchivedThreads,
            &format!(
                "/channels/{}/threads/archived/public{}",
                channel_id,
                archived_threads_query(query)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn list_private_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.send(
            RouteId::ListPrivateArchivedThreads,
            &format!(
                "/channels/{}/threads/archived/private{}",
                channel_id,
                archived_threads_query(query)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn list_joined_private_archived_threads(
        &self,
        channel_id: ChannelId,
        query: &JoinedArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.send(
            RouteId::ListJoinedPrivateArchivedThreads,
            &format!(
                "/channels/{}/users/@me/threads/archived/private{}",
                channel_id,
                joined_archived_threads_query(query)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn get_active_guild_threads(
        &self,
        guild_id: GuildId,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.send(
            RouteId::ListActiveGuildThreads,
            &format!("/guilds/{}/threads/active", guild_id),
            NO_BODY,
        )
        .await
    }
}
