//! Message polls and poll results.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{PollAnswerId, PollLayoutType, User};

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
    /// Set by Discord on received polls; leave `None` when creating one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_id: Option<PollAnswerId>,
    pub poll_media: PollMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `PollAnswerCount`.
pub struct PollAnswerCount {
    /// The [`PollAnswer::answer_id`] this count belongs to.
    pub id: PollAnswerId,
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

/// A poll on a received message. No `Default`: `question` and
/// `layout_type` are required (2.x filled a missing `layout_type` with `0`,
/// which is not a layout).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Poll {
    pub question: PollMedia,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(default)]
    pub allow_multiselect: bool,
    pub layout_type: PollLayoutType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<PollResults>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerVoters`.
pub struct PollAnswerVoters {
    #[serde(default)]
    pub users: Vec<User>,
}
