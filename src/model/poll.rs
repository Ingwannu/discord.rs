//! Message polls and poll results.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::User;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollMedia`.
pub struct PollMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswer`.
pub struct PollAnswer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_id: Option<u64>,
    pub poll_media: PollMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerCount`.
pub struct PollAnswerCount {
    pub id: u64,
    pub count: u64,
    pub me_voted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollResults`.
pub struct PollResults {
    pub is_finalized: bool,
    #[serde(default)]
    pub answer_counts: Vec<PollAnswerCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Poll`.
pub struct Poll {
    pub question: PollMedia,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(default)]
    pub allow_multiselect: bool,
    #[serde(default)]
    pub layout_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<PollResults>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerVoters`.
pub struct PollAnswerVoters {
    #[serde(default)]
    pub users: Vec<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreatePoll`.
pub struct CreatePoll {
    pub question: PollMedia,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_multiselect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout_type: Option<u8>,
}
