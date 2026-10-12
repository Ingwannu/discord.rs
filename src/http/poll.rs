//! Poll endpoints: answer voters and ending a poll.

use crate::error::DiscordError;
use crate::model::{ChannelId, Message, MessageId, PollAnswerId, PollAnswerVoters, UserId};

use super::paths::poll_answer_voters_query;
use super::{RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_poll_answer_voters(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        answer_id: PollAnswerId,
        after: Option<UserId>,
        limit: Option<u64>,
    ) -> Result<PollAnswerVoters, DiscordError> {
        self.send(
            RouteId::GetAnswerVoters,
            &format!(
                "/channels/{}/polls/{}/answers/{answer_id}{}",
                channel_id,
                message_id,
                poll_answer_voters_query(after, limit)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn end_poll(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Message, DiscordError> {
        self.send(
            RouteId::EndPoll,
            &format!("/channels/{}/polls/{}/expire", channel_id, message_id),
            NO_BODY,
        )
        .await
    }
}
