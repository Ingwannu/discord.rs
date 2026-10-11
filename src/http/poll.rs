//! Poll endpoints: answer voters and ending a poll.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{GenericId, Message, PollAnswerId, PollAnswerVoters};

use super::paths::poll_answer_voters_query;
use super::RestClient;

impl RestClient {
    pub async fn get_poll_answer_voters(
        &self,
        channel_id: GenericId,
        message_id: GenericId,
        answer_id: PollAnswerId,
        after: Option<GenericId>,
        limit: Option<u64>,
    ) -> Result<PollAnswerVoters, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/polls/{}/answers/{answer_id}{}",
                channel_id,
                message_id,
                poll_answer_voters_query(after, limit)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn end_poll(
        &self,
        channel_id: GenericId,
        message_id: GenericId,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/polls/{}/expire", channel_id, message_id),
            Option::<&Value>::None,
        )
        .await
    }
}
