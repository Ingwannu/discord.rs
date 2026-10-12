//! Interaction callbacks: [`InteractionResponse`], the body of `POST
//! /interactions/{id}/{token}/callback` and of an inline HTTP answer, and
//! [`EndpointReply`], how the HTTP interactions endpoint answers Discord.
//!
//! [Decision Log]
//! - 목적과 의도: REST 콜백, HTTP 엔드포인트, 상태를 관리하는 응답기가 같은 콜백 타입 하나를
//!   쓰고, 그 타입을 `Value` 없이 한 번에 바이트로 직렬화하게 한다.
//! - 기존 구현 및 제약 조건: 2.x에는 경로가 셋이었다. REST는 `InteractionCallbackResponse {
//!   kind, data: Option<Value> }`, 엔드포인트는 자체 `InteractionResponse` enum(10 variant, 메시지는
//!   `Value`, `Accepted` 포함)을 `json!`으로 다시 만들어 `Json<Value>`로 보냈고, 응답기는 빌더 출력을
//!   `to_value`한 뒤 REST로 보냈다. 엔드포인트의 202 무본문 `Accepted`가 콜백 enum 안에 있어 REST로
//!   보낼 수도 있는 것처럼 보였다.
//! - 검토한 주요 대안: (1) `kind` + 타입 없는 `data` 구조체 유지, (2) serde 내부 태그 enum
//!   (`#[serde(tag = "type")]`), (3) 알려진 콜백 variant + 손으로 쓴 `Serialize`(숫자 `type` 먼저) +
//!   눈에 띄는 `Raw`, 202는 별도 `EndpointReply`.
//! - 선택한 방식: (3). `Raw`는 `type`을 따로 들고(`AckKind`를 판단해야 하므로) `data`는 검증된 JSON
//!   텍스트(`RawValue`)다. 직렬화는 비제네릭 `encode_interaction_response` 하나가 맡아 REST와
//!   엔드포인트가 같은 `to_vec` 인스턴스를 쓴다. 기본 `allowed_mentions`는 메시지 DTO의 빌린 뷰
//!   (`allowed_mentions_or`)로 직렬화 중에 넣는다(복제·변경 없음).
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 콜백 종류와 데이터가 어긋나는 값을 표현할 수 있다.
//!   (2)는 `type`을 문자열 태그나 정수 태그로 만들 수 없고(serde 내부 태그는 문자열만 쓴다)
//!   `Content` 버퍼를 거친다. `EndpointReply`를 따로 두면 `Accepted`는 `Serialize`가 없어 콜백
//!   본문으로 보낼 수 없다(컴파일 오류).
//! - 장점, 단점 및 영향: 핸들러와 프레임워크 라우트는 `EndpointReply`를 돌려준다
//!   (`InteractionResponse`에서 `From`/`.into()`). `PREMIUM_REQUIRED`(10)는 Discord가 폐기했지만
//!   보낼 수는 있도록 deprecated variant로 남겼다. `Raw` 본문에는 클라이언트 기본
//!   `allowed_mentions`를 넣지 않는다(본문을 해석하지 않으므로).

use std::fmt;

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::value::RawValue;

use crate::command::{limits, validation};
use crate::error::DiscordError;
use crate::model::{AllowedMentions, ApplicationCommandOptionChoice, InteractionResponseType};
use crate::response::EPHEMERAL_FLAG;

use super::{InteractionMessage, Modal};

/// An interaction callback: what Discord shows in response to an
/// interaction.
///
/// The same value is sent over REST (`RestClient::create_interaction_response`,
/// the [`InteractionResponder`](crate::response::InteractionResponder)
/// methods) and returned inline from the HTTP interactions endpoint
/// (through [`EndpointReply::Inline`]). It serializes as `{"type": N,
/// "data": ...}` with the numeric callback type first.
///
/// ```
/// use discordrs::request::InteractionResponse;
///
/// let reply = InteractionResponse::DeferredChannelMessage { ephemeral: true };
/// assert_eq!(
///     serde_json::to_string(&reply).unwrap(),
///     r#"{"type":5,"data":{"flags":64}}"#
/// );
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InteractionResponse {
    /// `PONG` (1): acknowledges Discord's endpoint-verification ping. Never
    /// an initial response to a real interaction.
    Pong,
    /// `CHANNEL_MESSAGE_WITH_SOURCE` (4): responds with a message.
    ChannelMessage(InteractionMessage),
    /// `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE` (5): acknowledges now and
    /// edits the original response later; the user sees a loading state.
    /// `EPHEMERAL` is the only flag Discord accepts here.
    DeferredChannelMessage {
        /// Whether the loading state (and the message that replaces it) is
        /// visible only to the invoking user.
        ephemeral: bool,
    },
    /// `DEFERRED_UPDATE_MESSAGE` (6): components and modal submits only;
    /// acknowledges without a loading state and edits the message later.
    DeferredUpdateMessage,
    /// `UPDATE_MESSAGE` (7): components and modal submits only; edits the
    /// message the component is on.
    UpdateMessage(InteractionMessage),
    /// `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT` (8): up to 25 suggested
    /// choices.
    AutocompleteResult(Vec<ApplicationCommandOptionChoice>),
    /// `MODAL` (9): opens a popup form. Not available for modal submits.
    Modal(Modal),
    /// `PREMIUM_REQUIRED` (10): Discord deprecated it in favour of premium
    /// buttons. Kept so it can still be sent; it has no data.
    #[deprecated(
        since = "3.0.0",
        note = "Discord deprecated PREMIUM_REQUIRED; send a premium button (ButtonStyle::PREMIUM) \
                instead"
    )]
    PremiumRequired,
    /// `LAUNCH_ACTIVITY` (12): launches the app's Activity.
    LaunchActivity,
    /// Any callback type with data written as JSON text, for callback types
    /// or fields this crate does not model yet. Sent exactly as written.
    Raw(RawInteractionResponse),
}

impl InteractionResponse {
    /// `CHANNEL_MESSAGE_WITH_SOURCE` with this message.
    pub fn reply(message: impl Into<InteractionMessage>) -> Self {
        Self::ChannelMessage(message.into())
    }

    /// `UPDATE_MESSAGE` with this message.
    pub fn update(message: impl Into<InteractionMessage>) -> Self {
        Self::UpdateMessage(message.into())
    }

    /// `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE`, optionally ephemeral.
    pub fn defer(ephemeral: bool) -> Self {
        Self::DeferredChannelMessage { ephemeral }
    }

    /// `MODAL` with this modal (a [`Modal`] or a
    /// [`ModalBuilder`](crate::builders::ModalBuilder)).
    pub fn modal(modal: impl Into<Modal>) -> Self {
        Self::Modal(modal.into())
    }

    /// `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT` with these choices.
    pub fn autocomplete(choices: impl IntoIterator<Item = ApplicationCommandOptionChoice>) -> Self {
        Self::AutocompleteResult(choices.into_iter().collect())
    }

    /// The callback `type` this response sends.
    pub fn kind(&self) -> InteractionResponseType {
        match self {
            Self::Pong => InteractionResponseType::PONG,
            Self::ChannelMessage(_) => InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
            Self::DeferredChannelMessage { .. } => {
                InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
            }
            Self::DeferredUpdateMessage => InteractionResponseType::DEFERRED_UPDATE_MESSAGE,
            Self::UpdateMessage(_) => InteractionResponseType::UPDATE_MESSAGE,
            Self::AutocompleteResult(_) => {
                InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT
            }
            Self::Modal(_) => InteractionResponseType::MODAL,
            #[expect(deprecated, reason = "the deprecated variant still has its wire type")]
            Self::PremiumRequired => InteractionResponseType::PREMIUM_REQUIRED,
            Self::LaunchActivity => InteractionResponseType::LAUNCH_ACTIVITY,
            Self::Raw(raw) => raw.kind,
        }
    }

    /// Checks what can be checked locally: a modal's limits
    /// ([`Modal::validate`]) and autocomplete choices (at most 25, names
    /// 1-100 characters, string values at most 100, values a string or a
    /// finite number in Discord's range). Messages are checked by their own
    /// type; raw responses are not inspected.
    pub fn validate(&self) -> Result<(), DiscordError> {
        match self {
            Self::Modal(modal) => modal.validate(),
            Self::AutocompleteResult(choices) => validate_choices(choices),
            _ => Ok(()),
        }
    }

    /// The serialization of this response with `default` filling an unset
    /// `allowed_mentions` of a message callback (types 4 and 7).
    fn wire<'a>(&'a self, default: Option<&'a AllowedMentions>) -> Wire<'a> {
        Wire {
            response: self,
            default,
        }
    }
}

impl Serialize for InteractionResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire(None).serialize(serializer)
    }
}

impl From<Modal> for InteractionResponse {
    fn from(modal: Modal) -> Self {
        Self::Modal(modal)
    }
}

impl From<RawInteractionResponse> for InteractionResponse {
    fn from(raw: RawInteractionResponse) -> Self {
        Self::Raw(raw)
    }
}

/// [`InteractionResponse`] as written on the wire, with an optional default
/// for message `allowed_mentions`.
struct Wire<'a> {
    response: &'a InteractionResponse,
    default: Option<&'a AllowedMentions>,
}

impl Serialize for Wire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        /// `data` of a deferred channel message: only `flags`.
        struct DeferredData(u64);

        impl Serialize for DeferredData {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("flags", &self.0)?;
                map.end()
            }
        }

        /// `data` of an autocomplete result.
        struct ChoicesData<'a>(&'a [ApplicationCommandOptionChoice]);

        impl Serialize for ChoicesData<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("choices", self.0)?;
                map.end()
            }
        }

        let response = self.response;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &response.kind())?;
        match response {
            InteractionResponse::ChannelMessage(message)
            | InteractionResponse::UpdateMessage(message) => {
                map.serialize_entry("data", &message.allowed_mentions_or(self.default))?;
            }
            InteractionResponse::DeferredChannelMessage { ephemeral } => {
                // `flags` is always written (0 when not ephemeral), as the
                // 2.x REST defer did; the HTTP endpoint used to omit `data`.
                let flags = if *ephemeral { EPHEMERAL_FLAG } else { 0 };
                map.serialize_entry("data", &DeferredData(flags))?;
            }
            InteractionResponse::AutocompleteResult(choices) => {
                map.serialize_entry("data", &ChoicesData(choices))?;
            }
            InteractionResponse::Modal(modal) => map.serialize_entry("data", modal)?,
            InteractionResponse::Raw(raw) => {
                if let Some(data) = &raw.data {
                    map.serialize_entry("data", data)?;
                }
            }
            InteractionResponse::Pong
            | InteractionResponse::DeferredUpdateMessage
            | InteractionResponse::LaunchActivity => {}
            #[expect(
                deprecated,
                reason = "the deprecated variant is still sent without data"
            )]
            InteractionResponse::PremiumRequired => {}
        }
        map.end()
    }
}

/// Encodes a callback body once, as the REST callback and the HTTP endpoint
/// both send it.
///
/// Not generic on purpose: every path that sends an [`InteractionResponse`]
/// goes through this one `serde_json::to_vec` instance instead of each
/// writer compiling its own copy of the `Serialize` code.
pub(crate) fn encode_interaction_response(
    response: &InteractionResponse,
    default_allowed_mentions: Option<&AllowedMentions>,
) -> Result<Vec<u8>, DiscordError> {
    Ok(serde_json::to_vec(
        &response.wire(default_allowed_mentions),
    )?)
}

fn validate_choices(choices: &[ApplicationCommandOptionChoice]) -> Result<(), DiscordError> {
    validation::ensure_max_count("autocomplete choices", choices.len(), limits::MAX_CHOICES)?;
    for choice in choices {
        let name = &choice.name;
        validation::ensure_len(
            "autocomplete choice name",
            name,
            1,
            limits::MAX_CHOICE_NAME_LENGTH,
        )?;
        choice.value.check_choice().map_err(|reason| {
            DiscordError::model(format!("autocomplete choice {name:?} value: {reason}"))
        })?;
        if let Some(value) = choice.value.as_str() {
            validation::ensure_max_len(
                &format!("autocomplete choice {name:?} value"),
                value,
                limits::MAX_CHOICE_STRING_VALUE_LENGTH,
            )?;
        }
    }
    Ok(())
}

/// A callback written by hand: a callback `type` and optional `data` as
/// JSON text.
///
/// The type is kept apart from the data so the response state can still
/// tell a deferral from a reply. The data is checked to be JSON when it is
/// set and sent unchanged; no `serde_json::Value` tree is built.
///
/// ```
/// use discordrs::request::{InteractionResponse, RawInteractionResponse};
/// use discordrs::InteractionResponseType;
///
/// let raw = RawInteractionResponse::new(InteractionResponseType::new(13))
///     .data_json(r#"{"future":true}"#)
///     .unwrap();
/// assert_eq!(
///     serde_json::to_string(&InteractionResponse::Raw(raw)).unwrap(),
///     r#"{"type":13,"data":{"future":true}}"#
/// );
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct RawInteractionResponse {
    /// The callback `type`.
    pub kind: InteractionResponseType,
    /// The callback `data`, omitted when `None`.
    pub data: Option<Box<RawValue>>,
}

impl RawInteractionResponse {
    /// A callback of this type with no data.
    pub fn new(kind: InteractionResponseType) -> Self {
        Self { kind, data: None }
    }

    /// Sets `data` from JSON text, checking that it is one JSON value.
    pub fn data_json(mut self, json: impl Into<String>) -> Result<Self, DiscordError> {
        self.data = Some(RawValue::from_string(json.into())?);
        Ok(self)
    }

    /// Sets `data` by serializing `data` straight to JSON text.
    pub fn data<T: Serialize + ?Sized>(mut self, data: &T) -> Result<Self, DiscordError> {
        self.data = Some(serde_json::value::to_raw_value(data)?);
        Ok(self)
    }
}

impl fmt::Debug for RawInteractionResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawInteractionResponse")
            .field("kind", &self.kind)
            .field("data", &self.data.as_deref().map(RawValue::get))
            .finish()
    }
}

/// How the HTTP interactions endpoint answers Discord's request.
///
/// `Inline` answers `200 OK` with the callback as the body. `Accepted`
/// answers `202 Accepted` with no body: the initial response is (or was)
/// sent through the REST callback endpoint instead, which is what Discord
/// requires when that endpoint is used for an interaction received over
/// HTTP. If nothing is sent over REST within 3 seconds, the interaction
/// fails.
///
/// `EndpointReply` is not a callback and does not implement `Serialize`, so
/// `Accepted` can never be sent as a callback body. Handlers usually write
/// `InteractionResponse::...into()`.
///
/// The inline response is boxed: an [`InteractionResponse`] holding a
/// message is a few hundred bytes, and handlers, guards
/// (`Result<(), EndpointReply>`), and the framework move the reply around by
/// value. One small allocation per inline answer is noise next to encoding
/// the body.
#[derive(Clone, Debug)]
pub enum EndpointReply {
    /// `200 OK` with this callback as the body.
    Inline(Box<InteractionResponse>),
    /// `202 Accepted` with no body; REST owns the initial response.
    Accepted,
}

impl From<InteractionResponse> for EndpointReply {
    fn from(response: InteractionResponse) -> Self {
        Self::Inline(Box::new(response))
    }
}

impl From<Modal> for EndpointReply {
    fn from(modal: Modal) -> Self {
        Self::from(InteractionResponse::Modal(modal))
    }
}
