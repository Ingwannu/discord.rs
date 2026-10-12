//! Thread endpoints: starting threads, thread members, and archived and
//! active thread lists.

use std::borrow::Cow;

use crate::error::DiscordError;
use crate::model::{
    ArchivedThreadsQuery, Channel, ChannelId, GuildId, JoinedArchivedThreadsQuery, MessageId,
    ThreadListResponse, ThreadMember, ThreadMemberQuery, UserId,
};

use crate::request::{StartForumThread, StartThreadFromMessage, StartThreadWithoutMessage};

use super::{
    archived_threads_query, bool_query, joined_archived_threads_query, json_body, multipart_body,
    thread_member_query, FileAttachment, RestClient, RouteId, NO_BODY,
};

impl RestClient {
    /// Starts a thread from an existing message.
    pub async fn create_thread_from_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        body: &StartThreadFromMessage,
    ) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::StartThreadFromMessage,
            &format!("/channels/{}/messages/{}/threads", channel_id, message_id),
            json_body(body),
        )
        .await
    }

    /// Starts a thread that is not attached to a message (text and
    /// announcement channels). Forum and media channels use
    /// [`Self::create_forum_thread`].
    pub async fn create_thread(
        &self,
        channel_id: ChannelId,
        body: &StartThreadWithoutMessage,
    ) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::StartThreadWithoutMessage,
            &format!("/channels/{}/threads", channel_id),
            json_body(body),
        )
        .await
    }

    /// Starts a thread in a forum or media channel with its first message.
    /// The client's default `allowed_mentions` applies to the message when it
    /// sets none, as for every message the client sends.
    pub async fn create_forum_thread(
        &self,
        channel_id: ChannelId,
        body: &StartForumThread,
    ) -> Result<Channel, DiscordError> {
        let body = self.forum_thread_with_default_allowed_mentions(body);
        self.send(
            RouteId::StartThreadInForumOrMediaChannel,
            &format!("/channels/{}/threads", channel_id),
            json_body(body.as_ref()),
        )
        .await
    }

    /// [`Self::create_forum_thread`] with file uploads (`files[n]`); describe
    /// them in `body.message.attachments`.
    pub async fn create_forum_thread_with_files(
        &self,
        channel_id: ChannelId,
        body: &StartForumThread,
        files: &[FileAttachment],
    ) -> Result<Channel, DiscordError> {
        let body = self.forum_thread_with_default_allowed_mentions(body);
        self.send(
            RouteId::StartThreadInForumOrMediaChannel,
            &format!("/channels/{}/threads", channel_id),
            multipart_body(body.as_ref(), files),
        )
        .await
    }

    /// Fills the first message's `allowed_mentions` from the client default
    /// when it sets none. A forum thread is created rarely, so this clones
    /// the body instead of adding a serialization view like the message
    /// request types have.
    fn forum_thread_with_default_allowed_mentions<'a>(
        &self,
        body: &'a StartForumThread,
    ) -> Cow<'a, StartForumThread> {
        match (
            self.default_allowed_mentions(),
            &body.message.allowed_mentions,
        ) {
            (Some(default), None) => {
                let mut body = body.clone();
                body.message.allowed_mentions = Some(default.clone());
                Cow::Owned(body)
            }
            _ => Cow::Borrowed(body),
        }
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
