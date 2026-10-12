//! Poll create request.
//!
//! Discord's Poll Create Request Object, shaped so that what it cannot hold
//! is not representable: the question is text only ("Only `text` is
//! supported"), and answers have no `answer_id` (Discord assigns those). The
//! 2.x [`crate::model::CreatePoll`] converts with `TryFrom`, which fails
//! instead of dropping a question emoji or an answer id.

use serde::ser::{SerializeMap, SerializeStruct, Serializer};
use serde::Serialize;

use crate::builders::limits;
use crate::error::DiscordError;
use crate::model::{PollAnswer, PollLayoutType, PollMedia};
use crate::types::Emoji;

use super::component::{at, check_count, check_len, check_max_len, child_path, entry_some};

/// A poll to attach to a new message.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CreatePoll {
    /// The question, 1-300 characters.
    pub question: String,
    /// The answers, 1-10.
    pub answers: Vec<CreatePollAnswer>,
    /// Hours the poll stays open, up to 768 (32 days). Discord's default: 24.
    pub duration: Option<u32>,
    /// Whether voters may pick more than one answer.
    pub allow_multiselect: Option<bool>,
    /// Layout; only [`PollLayoutType::DEFAULT`] exists.
    pub layout_type: Option<PollLayoutType>,
}

impl CreatePoll {
    /// A poll asking `question`; add 1-10 answers.
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            answers: Vec::new(),
            duration: None,
            allow_multiselect: None,
            layout_type: None,
        }
    }

    /// Appends an answer.
    pub fn add_answer(mut self, answer: impl Into<CreatePollAnswer>) -> Self {
        self.answers.push(answer.into());
        self
    }

    /// Sets how many hours the poll stays open.
    pub fn duration(mut self, hours: u32) -> Self {
        self.duration = Some(hours);
        self
    }

    /// Allows picking more than one answer.
    pub fn allow_multiselect(mut self, allow: bool) -> Self {
        self.allow_multiselect = Some(allow);
        self
    }

    /// Checks the question, answers, and duration against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.check("")
    }

    pub(crate) fn check(&self, path: &str) -> Result<(), DiscordError> {
        check_len(
            path,
            "poll question",
            &self.question,
            1,
            limits::MAX_POLL_QUESTION_LENGTH,
        )?;
        check_count(
            path,
            "poll answers",
            self.answers.len(),
            1,
            limits::MAX_POLL_ANSWERS,
        )?;
        for (index, answer) in self.answers.iter().enumerate() {
            let answer_path = child_path(path, "answers", index);
            match &answer.text {
                Some(text) => check_max_len(
                    &answer_path,
                    "poll answer text",
                    text,
                    limits::MAX_POLL_ANSWER_LENGTH,
                )?,
                None if answer.emoji.is_none() => {
                    return Err(at(
                        &answer_path,
                        "poll answer must have text or an emoji".to_owned(),
                    ))
                }
                None => {}
            }
        }
        if let Some(hours) = self.duration {
            if !(1..=limits::MAX_POLL_DURATION_HOURS).contains(&hours) {
                return Err(at(
                    path,
                    format!(
                        "poll duration must be 1-{} hours, got {hours}",
                        limits::MAX_POLL_DURATION_HOURS
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl Serialize for CreatePoll {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("question", &QuestionMedia(&self.question))?;
        map.serialize_entry("answers", &self.answers)?;
        entry_some(&mut map, "duration", &self.duration)?;
        entry_some(&mut map, "allow_multiselect", &self.allow_multiselect)?;
        entry_some(&mut map, "layout_type", &self.layout_type)?;
        map.end()
    }
}

/// `{"text": question}`.
struct QuestionMedia<'a>(&'a str);

impl Serialize for QuestionMedia<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut media = serializer.serialize_struct("PollMedia", 1)?;
        media.serialize_field("text", self.0)?;
        media.end()
    }
}

/// One poll answer: text (up to 55 characters), an emoji, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CreatePollAnswer {
    /// Answer text, up to 55 characters.
    pub text: Option<String>,
    /// Answer emoji (a custom emoji by `id`, or a Unicode emoji by `name`).
    pub emoji: Option<Emoji>,
}

impl CreatePollAnswer {
    /// An answer with this text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            emoji: None,
        }
    }

    /// Sets the answer emoji.
    pub fn emoji(mut self, emoji: Emoji) -> Self {
        self.emoji = Some(emoji);
        self
    }
}

impl From<&str> for CreatePollAnswer {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for CreatePollAnswer {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

/// Written as `{"poll_media": {"text": ..., "emoji": ...}}`.
impl Serialize for CreatePollAnswer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Media<'a>(&'a CreatePollAnswer);
        impl Serialize for Media<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                entry_some(&mut map, "text", &self.0.text)?;
                entry_some(&mut map, "emoji", &self.0.emoji)?;
                map.end()
            }
        }
        let mut answer = serializer.serialize_struct("PollAnswer", 1)?;
        answer.serialize_field("poll_media", &Media(self))?;
        answer.end()
    }
}

/// Converts the 2.x request type. Fails when the question has an emoji or
/// no text (the question is text only), when an answer carries an
/// `answer_id` (assigned by Discord, never sent), or when `duration` does
/// not fit `u32`.
impl TryFrom<crate::model::CreatePoll> for CreatePoll {
    type Error = DiscordError;

    fn try_from(poll: crate::model::CreatePoll) -> Result<Self, Self::Error> {
        let crate::model::CreatePoll {
            question,
            answers,
            duration,
            allow_multiselect,
            layout_type,
        } = poll;
        let PollMedia {
            text: question,
            emoji: question_emoji,
        } = question;
        if question_emoji.is_some() {
            return Err(DiscordError::model(
                "poll question cannot have an emoji (only text is supported)",
            ));
        }
        let question =
            question.ok_or_else(|| DiscordError::model("poll question must have text"))?;
        let answers = answers
            .into_iter()
            .enumerate()
            .map(|(index, answer)| {
                let PollAnswer {
                    answer_id,
                    poll_media,
                } = answer;
                if answer_id.is_some() {
                    return Err(DiscordError::model(format!(
                        "answers[{index}]: answer_id is assigned by Discord and cannot be sent"
                    )));
                }
                Ok(CreatePollAnswer {
                    text: poll_media.text,
                    emoji: poll_media.emoji,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let duration = duration
            .map(u32::try_from)
            .transpose()
            .map_err(|_| DiscordError::model("poll duration does not fit in u32 hours"))?;
        Ok(Self {
            question,
            answers,
            duration,
            allow_multiselect,
            layout_type,
        })
    }
}

/// Lossless: every field has a place in the 2.x type.
impl From<CreatePoll> for crate::model::CreatePoll {
    fn from(poll: CreatePoll) -> Self {
        Self {
            question: PollMedia {
                text: Some(poll.question),
                emoji: None,
            },
            answers: poll
                .answers
                .into_iter()
                .map(|answer| PollAnswer {
                    answer_id: None,
                    poll_media: PollMedia {
                        text: answer.text,
                        emoji: answer.emoji,
                    },
                })
                .collect(),
            duration: poll.duration.map(u64::from),
            allow_multiselect: poll.allow_multiselect,
            layout_type: poll.layout_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CreatePoll, CreatePollAnswer};
    use crate::model::{PollAnswer, PollAnswerId, PollMedia};
    use crate::types::Emoji;

    fn message(poll: &CreatePoll) -> String {
        let error = poll.validate().expect_err("invalid poll").to_string();
        error
            .strip_prefix("Model error: ")
            .unwrap_or(&error)
            .to_owned()
    }

    fn valid() -> CreatePoll {
        CreatePoll::new("Q?")
            .add_answer("Yes")
            .add_answer(String::from("No"))
    }

    #[test]
    fn poll_limits() {
        valid()
            .duration(768)
            .allow_multiselect(true)
            .validate()
            .expect("valid");
        assert_eq!(
            message(&CreatePoll::new("").add_answer("a")),
            "poll question must be 1-300 characters, got 0"
        );
        assert_eq!(
            message(&CreatePoll::new("q".repeat(301)).add_answer("a")),
            "poll question must be 1-300 characters, got 301"
        );
        assert_eq!(
            message(&CreatePoll::new("Q")),
            "poll answers must contain 1-10 items, got 0"
        );
        let crowded = (0..11).fold(CreatePoll::new("Q"), |poll, index| {
            poll.add_answer(index.to_string())
        });
        assert_eq!(
            message(&crowded),
            "poll answers must contain 1-10 items, got 11"
        );
        assert_eq!(
            message(&CreatePoll::new("Q").add_answer("a".repeat(56))),
            "answers[0]: poll answer text must be at most 55 characters, got 56"
        );
        assert_eq!(
            message(&CreatePoll::new("Q").add_answer(CreatePollAnswer::default())),
            "answers[0]: poll answer must have text or an emoji"
        );
        CreatePoll::new("Q")
            .add_answer(CreatePollAnswer {
                text: None,
                emoji: Some(Emoji::unicode("👍")),
            })
            .validate()
            .expect("emoji-only answer");
        assert_eq!(
            message(&valid().duration(0)),
            "poll duration must be 1-768 hours, got 0"
        );
        assert_eq!(
            message(&valid().duration(769)),
            "poll duration must be 1-768 hours, got 769"
        );
    }

    #[test]
    fn converts_from_and_to_the_2x_type() {
        let poll = valid().duration(24);
        let legacy = crate::model::CreatePoll::from(poll.clone());
        assert_eq!(legacy.question.text.as_deref(), Some("Q?"));
        assert_eq!(legacy.duration, Some(24));
        let back = CreatePoll::try_from(legacy).expect("round trip");
        assert_eq!(back, poll);

        let with_emoji = crate::model::CreatePoll {
            question: PollMedia {
                text: Some("Q".to_owned()),
                emoji: Some(Emoji::unicode("❓")),
            },
            ..crate::model::CreatePoll::default()
        };
        assert!(CreatePoll::try_from(with_emoji)
            .unwrap_err()
            .to_string()
            .contains("cannot have an emoji"));
        assert!(CreatePoll::try_from(crate::model::CreatePoll::default())
            .unwrap_err()
            .to_string()
            .contains("poll question must have text"));
        let with_id = crate::model::CreatePoll {
            question: PollMedia {
                text: Some("Q".to_owned()),
                emoji: None,
            },
            answers: vec![PollAnswer {
                answer_id: Some(PollAnswerId::new(1)),
                poll_media: PollMedia::default(),
            }],
            ..crate::model::CreatePoll::default()
        };
        assert!(CreatePoll::try_from(with_id)
            .unwrap_err()
            .to_string()
            .contains("answers[0]: answer_id is assigned by Discord"));
        let huge = crate::model::CreatePoll {
            question: PollMedia {
                text: Some("Q".to_owned()),
                emoji: None,
            },
            duration: Some(u64::MAX),
            ..crate::model::CreatePoll::default()
        };
        assert!(CreatePoll::try_from(huge)
            .unwrap_err()
            .to_string()
            .contains("does not fit"));
    }
}
