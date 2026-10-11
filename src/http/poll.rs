//! Poll endpoints: answer voters and ending a poll.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{Message, PollAnswerVoters, Snowflake};

use super::paths::poll_answer_voters_query;
use super::RestClient;

impl RestClient {
    pub async fn get_poll_answer_voters(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        answer_id: u64,
        after: Option<Snowflake>,
        limit: Option<u64>,
    ) -> Result<PollAnswerVoters, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/polls/{}/answers/{answer_id}{}",
                channel_id.into(),
                message_id.into(),
                poll_answer_voters_query(after, limit)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn end_poll(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!(
                "/channels/{}/polls/{}/expire",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }
}
