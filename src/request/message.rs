//! Message-shaped request bodies (3.0 phase 5, design §4).
//!
//! One type per endpoint family, each holding only the fields that endpoint
//! accepts, so a field Discord would reject (or silently drop) on an
//! endpoint does not exist on its type:
//!
//! | Type | Endpoint | Semantics |
//! |---|---|---|
//! | [`CreateMessage`] | `POST /channels/{id}/messages` | create; reply/forward, stickers, nonce |
//! | [`EditMessage`] | `PATCH /channels/{id}/messages/{id}` | patch; no nonce, stickers, reply, tts, poll |
//! | [`InteractionMessage`] | callback types 4 and 7 (`data`) | create; ephemeral flag, poll |
//! | [`CreateFollowup`] | `POST /webhooks/{app}/{token}` | create; ephemeral flag, poll |
//! | [`ExecuteWebhook`] | `POST /webhooks/{id}/{token}` | create; username, avatar, thread |
//! | [`EditWebhookMessage`] / [`EditInteractionResponse`] | `PATCH .../messages/{id or @original}` | patch; attachments listed explicitly, poll |
//!
//! Query parameters (`wait`, `thread_id`, `with_components`) are not body
//! fields; they live in [`WebhookExecuteQuery`](crate::model::WebhookExecuteQuery)
//! and [`WebhookMessageQuery`](crate::model::WebhookMessageQuery).
//!
//! Create bodies use `Option` (`None` omits the key; `Some(vec![])` writes
//! `[]`, which an `UPDATE_MESSAGE` callback reads as "remove all"). Patch
//! bodies use [`NullablePatch`]: omitted keeps the current value, `null`
//! clears it, a value replaces it.
//!
//! Every type serializes straight to JSON with a hand-written `Serialize`
//! (no `serde_json::Value`), and offers [`allowed_mentions_or`]-style views
//! ([`WithDefaultMentions`]) so the REST client can apply its default
//! `allowed_mentions` while encoding, without cloning the body or reading it
//! back as JSON. [`CreateMessage::nonce_replay`] tells the transport whether
//! a retry is deduplicated by Discord.
//!
//! [`allowed_mentions_or`]: CreateMessage::allowed_mentions_or
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 메시지 계열 요청 본문을 엔드포인트별 타입으로 고정해 받지 않는 필드(EditMessage의
//!   nonce·sticker·reply, 콜백의 message_reference 등)를 표현할 수 없게 하고, Value 없이 바로 바이트로
//!   인코딩한다.
//! - 기존 구현 및 제약 조건: 2.x는 model::CreateMessage 하나를 생성·수정·웹훅에 함께 쓰고(components와
//!   embeds는 Vec<Value>), InteractionReplyData가 별도로 있었다. 전송 계층은 nonce 재전송 판정과 기본
//!   allowed_mentions 주입을 위해 Value를 읽거나 본문 전체를 복제했다. model/* 요청 타입은 4단계 C가
//!   끝날 때까지 옮기지 않는다.
//! - 검토한 주요 대안: (1) 공통 MessageBody 하나 + 엔드포인트별 검증, (2) 공통 내부 구조체를 필드로
//!   품는 래퍼(msg.body.content), (3) 엔드포인트별 구조체 + 공유 직렬화·검증 헬퍼 + setter 매크로,
//!   (4) 기본 allowed_mentions를 주입하려고 본문을 복제(현행).
//! - 선택한 방식: (3). 생성 계열은 Option 필드, 수정 계열은 NullablePatch 필드. 기본 allowed_mentions는
//!   WithDefaultMentions 뷰가 직렬화 중에 끼워 넣고, nonce 재전송 판정은 nonce_replay()가 답한다.
//!   EditInteractionResponse는 규격상 Edit Webhook Message와 같아 타입 별칭이다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 2.x와 같은 문제(받지 않는 필드가 컴파일됨)를 남긴다.
//!   (2)는 필드 접근이 한 단계 깊어지고 문서가 흩어진다. (4)는 components 트리 전체를 복제한다.
//! - 장점, 단점 및 영향: 잘못된 필드는 컴파일되지 않고(tests/ui), 변환은 손실 시 실패한다(TryFrom).
//!   구조체가 여러 개라 setter가 매크로로 생성되고, 필드 추가 시 각 타입의 Serialize와 변환을 함께
//!   고쳐야 한다(변환은 구조 분해로 누락을 컴파일러가 잡는다).
//! ```

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use crate::bitfield::{message_flags, MessageFlags};
use crate::builders::limits;
use crate::error::DiscordError;
use crate::model::{
    AllowedMentions, ForumTagId, MessageId, MessageReference, Nullable, NullablePatch, Patch,
    SharedClientTheme, StickerId,
};

use super::attachment::MessageAttachment;
use super::component::{
    check_count, check_max_len, check_message_components, child_path, entry_patch, entry_some,
    Component, ComponentsMode,
};
use super::embed::Embed;
use super::poll::CreatePoll;

// ---------------------------------------------------------------------------
// Flags each endpoint accepts
// ---------------------------------------------------------------------------

const SUPPRESS_EMBEDS: u64 = message_flags::SUPPRESS_EMBEDS.bits();
const SUPPRESS_NOTIFICATIONS: u64 = message_flags::SUPPRESS_NOTIFICATIONS.bits();
const IS_VOICE_MESSAGE: u64 = message_flags::IS_VOICE_MESSAGE.bits();
const IS_COMPONENTS_V2: u64 = message_flags::IS_COMPONENTS_V2.bits();
const EPHEMERAL: u64 = message_flags::EPHEMERAL.bits();

/// Create Message `flags`: "only `SUPPRESS_EMBEDS`, `SUPPRESS_NOTIFICATIONS`,
/// `IS_VOICE_MESSAGE`, and `IS_COMPONENTS_V2` can be set".
const CREATE_MESSAGE_FLAGS: u64 =
    SUPPRESS_EMBEDS | SUPPRESS_NOTIFICATIONS | IS_VOICE_MESSAGE | IS_COMPONENTS_V2;
/// Edit Message and Edit Webhook Message `flags`: "`SUPPRESS_EMBEDS` and
/// `IS_COMPONENTS_V2` only".
const EDIT_MESSAGE_FLAGS: u64 = SUPPRESS_EMBEDS | IS_COMPONENTS_V2;
/// Interaction callback message `flags`: "only `SUPPRESS_EMBEDS`,
/// `EPHEMERAL`, `IS_COMPONENTS_V2`, `IS_VOICE_MESSAGE`, and
/// `SUPPRESS_NOTIFICATIONS` can be set".
const INTERACTION_MESSAGE_FLAGS: u64 =
    SUPPRESS_EMBEDS | EPHEMERAL | IS_COMPONENTS_V2 | IS_VOICE_MESSAGE | SUPPRESS_NOTIFICATIONS;
/// Execute Webhook `flags`: "only `SUPPRESS_EMBEDS`, `SUPPRESS_NOTIFICATIONS`
/// and `IS_COMPONENTS_V2` can be set".
const EXECUTE_WEBHOOK_FLAGS: u64 = SUPPRESS_EMBEDS | SUPPRESS_NOTIFICATIONS | IS_COMPONENTS_V2;
/// Create Followup Message: Execute Webhook's flags plus `EPHEMERAL` ("You
/// can use the `EPHEMERAL` message flag").
const FOLLOWUP_FLAGS: u64 = EXECUTE_WEBHOOK_FLAGS | EPHEMERAL;

// ---------------------------------------------------------------------------
// Default allowed_mentions view
// ---------------------------------------------------------------------------

/// A message body that writes `default` as its `allowed_mentions` when it
/// sets none itself. Returned by `allowed_mentions_or` on every message type.
///
/// The REST client's configured default mentions are applied this way while
/// the body is encoded: nothing is cloned and the body is not read back as
/// JSON. An explicit value (or, on a patch body, an explicit `null`) wins
/// over the default.
#[derive(Clone, Copy, Debug)]
pub struct WithDefaultMentions<'a, T> {
    body: &'a T,
    default: Option<&'a AllowedMentions>,
}

impl<'a, T> WithDefaultMentions<'a, T> {
    /// The wrapped body.
    pub fn body(&self) -> &'a T {
        self.body
    }

    /// The default applied when the body sets no `allowed_mentions`.
    pub fn default_mentions(&self) -> Option<&'a AllowedMentions> {
        self.default
    }
}

/// `Serialize` for a body type and its [`WithDefaultMentions`] view, plus
/// `allowed_mentions_or`. Each type implements
/// `serialize_with(serializer, default_mentions)`.
macro_rules! message_body {
    ($($ty:ident),* $(,)?) => {$(
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.serialize_with(serializer, None)
            }
        }

        impl Serialize for WithDefaultMentions<'_, $ty> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.body.serialize_with(serializer, self.default)
            }
        }

        impl $ty {
            /// A view of this body that writes `default` as
            /// `allowed_mentions` when the body sets none. With `None` it
            /// serializes exactly like the body.
            pub fn allowed_mentions_or<'a>(
                &'a self,
                default: Option<&'a AllowedMentions>,
            ) -> WithDefaultMentions<'a, Self> {
                WithDefaultMentions { body: self, default }
            }
        }
    )*};
}

message_body!(
    CreateMessage,
    EditMessage,
    InteractionMessage,
    CreateFollowup,
    ExecuteWebhook,
    EditWebhookMessage,
);

/// The `allowed_mentions` a create body writes.
fn effective_mentions<'a>(
    own: &'a Option<AllowedMentions>,
    default: Option<&'a AllowedMentions>,
) -> Option<&'a AllowedMentions> {
    own.as_ref().or(default)
}

/// Writes a patch body's `allowed_mentions`: its own value or `null`, else
/// the default, else nothing.
fn entry_patch_mentions<M: SerializeMap>(
    map: &mut M,
    own: &NullablePatch<AllowedMentions>,
    default: Option<&AllowedMentions>,
) -> Result<(), M::Error> {
    match (own, default) {
        (Patch::Value(value), _) => map.serialize_entry("allowed_mentions", value),
        (Patch::Missing, Some(default)) => map.serialize_entry("allowed_mentions", default),
        (Patch::Missing, None) => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Shared setters
// ---------------------------------------------------------------------------

/// Setters for the fields every create body shares.
macro_rules! create_setters {
    () => {
        /// Sets the text content (up to 2000 characters).
        pub fn content(mut self, content: impl Into<String>) -> Self {
            self.content = Some(content.into());
            self
        }

        /// Sends the message as text-to-speech.
        pub fn tts(mut self, tts: bool) -> Self {
            self.tts = Some(tts);
            self
        }

        /// Replaces the embeds (up to 10).
        pub fn embeds(mut self, embeds: Vec<Embed>) -> Self {
            self.embeds = Some(embeds);
            self
        }

        /// Appends an embed.
        pub fn add_embed(mut self, embed: impl Into<Embed>) -> Self {
            self.embeds.get_or_insert_with(Vec::new).push(embed.into());
            self
        }

        /// Sets which mentions in the content notify.
        pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
            self.allowed_mentions = Some(allowed_mentions);
            self
        }

        /// Replaces the components.
        pub fn components(mut self, components: Vec<Component>) -> Self {
            self.components = Some(components);
            self
        }

        /// Appends a component.
        pub fn add_component(mut self, component: impl Into<Component>) -> Self {
            self.components
                .get_or_insert_with(Vec::new)
                .push(component.into());
            self
        }

        /// Replaces the components and sets the `IS_COMPONENTS_V2` flag, which
        /// enables layout components and disables content, embeds, polls, and
        /// stickers.
        pub fn components_v2(mut self, components: Vec<Component>) -> Self {
            self.components = Some(components);
            self.flags = Some(self.flags.unwrap_or_default() | IS_COMPONENTS_V2);
            self
        }

        /// Replaces the attachment metadata.
        pub fn attachments(mut self, attachments: Vec<MessageAttachment>) -> Self {
            self.attachments = Some(attachments);
            self
        }

        /// Appends attachment metadata.
        pub fn add_attachment(mut self, attachment: impl Into<MessageAttachment>) -> Self {
            self.attachments
                .get_or_insert_with(Vec::new)
                .push(attachment.into());
            self
        }

        /// Replaces the message flags.
        pub fn flags(mut self, flags: impl Into<MessageFlags>) -> Self {
            self.flags = Some(flags.into());
            self
        }

        /// Attaches a poll.
        pub fn poll(mut self, poll: CreatePoll) -> Self {
            self.poll = Some(poll);
            self
        }

        /// Whether the `IS_COMPONENTS_V2` flag is set.
        pub fn is_components_v2(&self) -> bool {
            self.flags
                .is_some_and(|flags| flags.contains(IS_COMPONENTS_V2))
        }
    };
}

/// `ephemeral` / `is_ephemeral` for bodies that accept the flag.
macro_rules! ephemeral_setters {
    () => {
        /// Sets or clears the `EPHEMERAL` flag (only the invoking user sees
        /// the message).
        pub fn ephemeral(mut self, ephemeral: bool) -> Self {
            let flags = self.flags.unwrap_or_default();
            self.flags = Some(if ephemeral {
                flags | EPHEMERAL
            } else {
                flags - EPHEMERAL
            });
            self
        }

        /// Whether the `EPHEMERAL` flag is set.
        pub fn is_ephemeral(&self) -> bool {
            self.flags.is_some_and(|flags| flags.contains(EPHEMERAL))
        }
    };
}

/// Setters for the fields every patch body shares.
macro_rules! patch_setters {
    () => {
        /// Replaces the text content.
        pub fn content(mut self, content: impl Into<String>) -> Self {
            self.content = set(content.into());
            self
        }

        /// Removes the text content (`"content": null`).
        pub fn clear_content(mut self) -> Self {
            self.content = Patch::Value(Nullable::Null);
            self
        }

        /// Replaces the embeds.
        pub fn embeds(mut self, embeds: Vec<Embed>) -> Self {
            self.embeds = set(embeds);
            self
        }

        /// Appends an embed to the embeds this edit sets.
        pub fn add_embed(mut self, embed: impl Into<Embed>) -> Self {
            push(&mut self.embeds, embed.into());
            self
        }

        /// Removes every embed (`"embeds": []`).
        pub fn clear_embeds(mut self) -> Self {
            self.embeds = set(Vec::new());
            self
        }

        /// Replaces the message flags.
        pub fn flags(mut self, flags: impl Into<MessageFlags>) -> Self {
            self.flags = set(flags.into());
            self
        }

        /// Sets which mentions in the new content notify. Without it Discord
        /// parses the edited content with default allowances.
        pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
            self.allowed_mentions = set(allowed_mentions);
            self
        }

        /// Replaces the components.
        pub fn components(mut self, components: Vec<Component>) -> Self {
            self.components = set(components);
            self
        }

        /// Appends a component to the components this edit sets.
        pub fn add_component(mut self, component: impl Into<Component>) -> Self {
            push(&mut self.components, component.into());
            self
        }

        /// Removes every component (`"components": []`).
        pub fn clear_components(mut self) -> Self {
            self.components = set(Vec::new());
            self
        }

        /// Replaces the components and adds `IS_COMPONENTS_V2` to the flags
        /// this edit sets. The flag cannot be removed again, and Discord
        /// rejects the edit unless the content, embeds, stickers, and poll
        /// the message has are cleared in the same request
        /// ([`Self::clear_content`], [`Self::clear_embeds`], and
        /// `clear_stickers` / `clear_poll` where the type has them).
        pub fn components_v2(mut self, components: Vec<Component>) -> Self {
            self.components = set(components);
            let flags = match self.flags {
                Patch::Value(Nullable::Value(flags)) => flags,
                Patch::Value(Nullable::Null) | Patch::Missing => MessageFlags::default(),
            };
            self.flags = set(flags | IS_COMPONENTS_V2);
            self
        }

        /// Sets the attachments the message keeps after the edit: existing
        /// ones to retain and new uploads. Existing attachments not listed
        /// are removed.
        pub fn attachments(mut self, attachments: Vec<MessageAttachment>) -> Self {
            self.attachments = set(attachments);
            self
        }

        /// Appends to the attachments this edit keeps.
        pub fn add_attachment(mut self, attachment: impl Into<MessageAttachment>) -> Self {
            push(&mut self.attachments, attachment.into());
            self
        }

        /// Removes every attachment (`"attachments": []`).
        pub fn clear_attachments(mut self) -> Self {
            self.attachments = set(Vec::new());
            self
        }
    };
}

/// A patch that sets `value`.
fn set<T>(value: T) -> NullablePatch<T> {
    Patch::Value(Nullable::Value(value))
}

/// Appends to a patched list, starting a new list when the patch omits or
/// nulls it.
fn push<T>(patch: &mut NullablePatch<Vec<T>>, item: T) {
    match patch {
        Patch::Value(Nullable::Value(items)) => items.push(item),
        Patch::Value(Nullable::Null) | Patch::Missing => *patch = set(vec![item]),
    }
}

/// The value a patch sets, if any.
fn patch_value<T>(patch: &NullablePatch<T>) -> Option<&T> {
    match patch {
        Patch::Value(Nullable::Value(value)) => Some(value),
        Patch::Value(Nullable::Null) | Patch::Missing => None,
    }
}

/// A create body's value as the equivalent patch: `None` omits.
fn into_patch<T>(value: Option<T>) -> NullablePatch<T> {
    value.map_or(Patch::Missing, set)
}

// ---------------------------------------------------------------------------
// CreateMessage
// ---------------------------------------------------------------------------

/// Body of `POST /channels/{channel.id}/messages`.
///
/// At least one of `content`, `embeds`, `sticker_ids`, `components`, an
/// uploaded file, `poll`, or `shared_client_theme` must be set; a forward
/// needs only `message_reference`.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct CreateMessage {
    /// Text content, up to 2000 characters.
    pub content: Option<String>,
    /// Value echoed in the Message Create event, up to 25 characters.
    pub nonce: Option<String>,
    /// With `nonce`: Discord returns the earlier message instead of creating
    /// a duplicate when the same nonce was used in the past few minutes.
    pub enforce_nonce: bool,
    /// Text-to-speech message.
    pub tts: Option<bool>,
    /// Up to 10 embeds.
    pub embeds: Option<Vec<Embed>>,
    /// Which mentions in the content notify.
    pub allowed_mentions: Option<AllowedMentions>,
    /// Makes the message a reply or a forward.
    pub message_reference: Option<MessageReference>,
    /// Components.
    pub components: Option<Vec<Component>>,
    /// Up to 3 guild stickers.
    pub sticker_ids: Option<Vec<StickerId>>,
    /// Metadata for files uploaded with the message.
    pub attachments: Option<Vec<MessageAttachment>>,
    /// `SUPPRESS_EMBEDS`, `SUPPRESS_NOTIFICATIONS`, `IS_VOICE_MESSAGE`,
    /// `IS_COMPONENTS_V2`.
    pub flags: Option<MessageFlags>,
    /// A poll.
    pub poll: Option<CreatePoll>,
    /// A client theme to share.
    pub shared_client_theme: Option<SharedClientTheme>,
}

impl CreateMessage {
    /// An empty message.
    pub fn new() -> Self {
        Self::default()
    }

    create_setters!();

    /// Sets a nonce without enforcement.
    pub fn nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    /// Sets a nonce and asks Discord to deduplicate on it, which also lets
    /// the REST client retry the request safely.
    pub fn enforce_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self.enforce_nonce = true;
        self
    }

    /// Makes the message a reply to `message_id` in the same channel.
    pub fn reply_to(mut self, message_id: MessageId) -> Self {
        self.message_reference = Some(MessageReference::reply(message_id));
        self
    }

    /// Sets the message reference (reply or forward).
    pub fn message_reference(mut self, reference: MessageReference) -> Self {
        self.message_reference = Some(reference);
        self
    }

    /// Sets the stickers (up to 3).
    pub fn sticker_ids(mut self, sticker_ids: Vec<StickerId>) -> Self {
        self.sticker_ids = Some(sticker_ids);
        self
    }

    /// Shares a client theme.
    pub fn shared_client_theme(mut self, theme: SharedClientTheme) -> Self {
        self.shared_client_theme = Some(theme);
        self
    }

    /// The nonce Discord deduplicates on, when `enforce_nonce` is set with a
    /// nonce. The REST client uses it to decide that resending the request
    /// after an uncertain failure cannot post a second message.
    pub fn nonce_replay(&self) -> Option<&str> {
        if self.enforce_nonce {
            self.nonce.as_deref()
        } else {
            None
        }
    }

    /// Checks the body against Discord's limits: content length, embeds,
    /// components (recursively), stickers, nonce, poll, attachments, the
    /// flags this endpoint accepts, and the Components V2 exclusions.
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(nonce) = &self.nonce {
            check_max_len("", "message nonce", nonce, limits::MAX_NONCE_LENGTH)?;
        }
        check_body(&BodyRef {
            allowed_flags: CREATE_MESSAGE_FLAGS,
            endpoint: "Create Message",
            patch: false,
            content: self.content.as_deref(),
            embeds: self.embeds.as_deref(),
            components: self.components.as_deref(),
            flags: self.flags,
            poll: self.poll.as_ref(),
            stickers: self.sticker_ids.as_ref().map_or(0, Vec::len),
            attachments: self.attachments.as_deref(),
            shared_client_theme: self.shared_client_theme.is_some(),
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_some(&mut map, "content", &self.content)?;
        entry_some(&mut map, "nonce", &self.nonce)?;
        if self.enforce_nonce {
            map.serialize_entry("enforce_nonce", &true)?;
        }
        entry_some(&mut map, "tts", &self.tts)?;
        entry_some(&mut map, "embeds", &self.embeds)?;
        entry_some(
            &mut map,
            "allowed_mentions",
            &effective_mentions(&self.allowed_mentions, default_mentions),
        )?;
        entry_some(&mut map, "message_reference", &self.message_reference)?;
        entry_some(&mut map, "components", &self.components)?;
        entry_some(&mut map, "sticker_ids", &self.sticker_ids)?;
        entry_some(&mut map, "attachments", &self.attachments)?;
        entry_some(&mut map, "flags", &self.flags)?;
        entry_some(&mut map, "poll", &self.poll)?;
        entry_some(&mut map, "shared_client_theme", &self.shared_client_theme)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// EditMessage
// ---------------------------------------------------------------------------

/// Body of `PATCH /channels/{channel.id}/messages/{message.id}`.
///
/// Every field is a patch: omitted keeps the current value, `null` clears
/// it, a value replaces it. There is no nonce, sticker, reply reference, tts,
/// or poll: Discord does not accept them on an edit.
///
/// [`validate`](Self::validate) applies the Components V2 rules only when
/// this edit sets `IS_COMPONENTS_V2`. An edit that omits `flags`, or sets
/// them without that bit (which cannot be removed once set), leaves the
/// message in whatever mode it has, so only the rules both modes share are
/// checked.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct EditMessage {
    /// New text content.
    pub content: NullablePatch<String>,
    /// New embeds.
    pub embeds: NullablePatch<Vec<Embed>>,
    /// New flags (`SUPPRESS_EMBEDS` can be set and unset; `IS_COMPONENTS_V2`
    /// only set).
    pub flags: NullablePatch<MessageFlags>,
    /// Mentions allowed in the new content.
    pub allowed_mentions: NullablePatch<AllowedMentions>,
    /// New components.
    pub components: NullablePatch<Vec<Component>>,
    /// Every attachment the message keeps (existing and newly uploaded).
    pub attachments: NullablePatch<Vec<MessageAttachment>>,
    /// Writes `"sticker_ids": []`, removing the message's stickers. Discord
    /// requires it when an edit sets `IS_COMPONENTS_V2` on a message that
    /// has stickers; stickers cannot be added or replaced by an edit.
    pub clear_sticker_ids: bool,
    /// Writes `"poll": null`, removing the message's poll. Discord requires
    /// it when an edit sets `IS_COMPONENTS_V2` on a message that has a poll;
    /// a poll cannot be added or replaced by an edit.
    pub clear_poll: bool,
}

impl EditMessage {
    /// An edit that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    patch_setters!();

    /// Removes the message's stickers (`"sticker_ids": []`), as Discord
    /// requires when this edit turns a message with stickers into a
    /// Components V2 message.
    pub fn clear_stickers(mut self) -> Self {
        self.clear_sticker_ids = true;
        self
    }

    /// Removes the message's poll (`"poll": null`), as Discord requires
    /// when this edit turns a message with a poll into a Components V2
    /// message.
    pub fn clear_poll(mut self) -> Self {
        self.clear_poll = true;
        self
    }

    /// Checks the values this edit sets against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_body(&BodyRef {
            allowed_flags: EDIT_MESSAGE_FLAGS,
            endpoint: "Edit Message",
            patch: true,
            content: patch_value(&self.content).map(String::as_str),
            embeds: patch_value(&self.embeds).map(Vec::as_slice),
            components: patch_value(&self.components).map(Vec::as_slice),
            flags: patch_value(&self.flags).copied(),
            poll: None,
            stickers: 0,
            attachments: patch_value(&self.attachments).map(Vec::as_slice),
            shared_client_theme: false,
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_patch(&mut map, "content", &self.content)?;
        entry_patch(&mut map, "embeds", &self.embeds)?;
        entry_patch(&mut map, "flags", &self.flags)?;
        entry_patch_mentions(&mut map, &self.allowed_mentions, default_mentions)?;
        entry_patch(&mut map, "components", &self.components)?;
        entry_patch(&mut map, "attachments", &self.attachments)?;
        if self.clear_sticker_ids {
            map.serialize_entry("sticker_ids", &[(); 0])?;
        }
        if self.clear_poll {
            map.serialize_entry("poll", &None::<()>)?;
        }
        map.end()
    }
}

// ---------------------------------------------------------------------------
// InteractionMessage
// ---------------------------------------------------------------------------

/// The `data` of a `CHANNEL_MESSAGE_WITH_SOURCE` (4) or `UPDATE_MESSAGE` (7)
/// interaction callback.
///
/// For `UPDATE_MESSAGE` an omitted field keeps the message's current value
/// and `Some(vec![])` removes every embed, component, or attachment.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct InteractionMessage {
    /// Text-to-speech response.
    pub tts: Option<bool>,
    /// Text content, up to 2000 characters.
    pub content: Option<String>,
    /// Up to 10 embeds.
    pub embeds: Option<Vec<Embed>>,
    /// Which mentions in the content notify.
    pub allowed_mentions: Option<AllowedMentions>,
    /// `SUPPRESS_EMBEDS`, `EPHEMERAL`, `IS_COMPONENTS_V2`,
    /// `IS_VOICE_MESSAGE`, `SUPPRESS_NOTIFICATIONS`.
    pub flags: Option<MessageFlags>,
    /// Components.
    pub components: Option<Vec<Component>>,
    /// Metadata for files uploaded with the response.
    pub attachments: Option<Vec<MessageAttachment>>,
    /// A poll.
    pub poll: Option<CreatePoll>,
}

impl InteractionMessage {
    /// An empty response message.
    pub fn new() -> Self {
        Self::default()
    }

    create_setters!();
    ephemeral_setters!();

    /// Checks the body against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_body(&BodyRef {
            allowed_flags: INTERACTION_MESSAGE_FLAGS,
            endpoint: "interaction callback",
            patch: false,
            content: self.content.as_deref(),
            embeds: self.embeds.as_deref(),
            components: self.components.as_deref(),
            flags: self.flags,
            poll: self.poll.as_ref(),
            stickers: 0,
            attachments: self.attachments.as_deref(),
            shared_client_theme: false,
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_some(&mut map, "tts", &self.tts)?;
        entry_some(&mut map, "content", &self.content)?;
        entry_some(&mut map, "embeds", &self.embeds)?;
        entry_some(
            &mut map,
            "allowed_mentions",
            &effective_mentions(&self.allowed_mentions, default_mentions),
        )?;
        entry_some(&mut map, "flags", &self.flags)?;
        entry_some(&mut map, "components", &self.components)?;
        entry_some(&mut map, "attachments", &self.attachments)?;
        entry_some(&mut map, "poll", &self.poll)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// CreateFollowup
// ---------------------------------------------------------------------------

/// Body of `POST /webhooks/{application.id}/{interaction.token}` (Create
/// Followup Message): Execute Webhook without `username`, `avatar_url`, or
/// a thread, plus the `EPHEMERAL` flag.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct CreateFollowup {
    /// Text content, up to 2000 characters.
    pub content: Option<String>,
    /// Text-to-speech message.
    pub tts: Option<bool>,
    /// Up to 10 embeds.
    pub embeds: Option<Vec<Embed>>,
    /// Which mentions in the content notify.
    pub allowed_mentions: Option<AllowedMentions>,
    /// Components.
    pub components: Option<Vec<Component>>,
    /// Metadata for files uploaded with the message.
    pub attachments: Option<Vec<MessageAttachment>>,
    /// `SUPPRESS_EMBEDS`, `SUPPRESS_NOTIFICATIONS`, `IS_COMPONENTS_V2`,
    /// `EPHEMERAL`.
    pub flags: Option<MessageFlags>,
    /// A poll.
    pub poll: Option<CreatePoll>,
}

impl CreateFollowup {
    /// An empty follow-up message.
    pub fn new() -> Self {
        Self::default()
    }

    create_setters!();
    ephemeral_setters!();

    /// Checks the body against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_body(&BodyRef {
            allowed_flags: FOLLOWUP_FLAGS,
            endpoint: "Create Followup Message",
            patch: false,
            content: self.content.as_deref(),
            embeds: self.embeds.as_deref(),
            components: self.components.as_deref(),
            flags: self.flags,
            poll: self.poll.as_ref(),
            stickers: 0,
            attachments: self.attachments.as_deref(),
            shared_client_theme: false,
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_some(&mut map, "content", &self.content)?;
        entry_some(&mut map, "tts", &self.tts)?;
        entry_some(&mut map, "embeds", &self.embeds)?;
        entry_some(
            &mut map,
            "allowed_mentions",
            &effective_mentions(&self.allowed_mentions, default_mentions),
        )?;
        entry_some(&mut map, "components", &self.components)?;
        entry_some(&mut map, "attachments", &self.attachments)?;
        entry_some(&mut map, "flags", &self.flags)?;
        entry_some(&mut map, "poll", &self.poll)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// ExecuteWebhook
// ---------------------------------------------------------------------------

/// Body of `POST /webhooks/{webhook.id}/{webhook.token}`. The `wait`,
/// `thread_id`, and `with_components` query parameters are in
/// [`WebhookExecuteQuery`](crate::model::WebhookExecuteQuery).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ExecuteWebhook {
    /// Text content, up to 2000 characters.
    pub content: Option<String>,
    /// Overrides the webhook's username.
    pub username: Option<String>,
    /// Overrides the webhook's avatar.
    pub avatar_url: Option<String>,
    /// Text-to-speech message.
    pub tts: Option<bool>,
    /// Up to 10 embeds.
    pub embeds: Option<Vec<Embed>>,
    /// Which mentions in the content notify.
    pub allowed_mentions: Option<AllowedMentions>,
    /// Components (interactive ones only on application-owned webhooks).
    pub components: Option<Vec<Component>>,
    /// Metadata for files uploaded with the message.
    pub attachments: Option<Vec<MessageAttachment>>,
    /// `SUPPRESS_EMBEDS`, `SUPPRESS_NOTIFICATIONS`, `IS_COMPONENTS_V2`.
    pub flags: Option<MessageFlags>,
    /// Name of the thread to create (forum and media channels).
    pub thread_name: Option<String>,
    /// Tags for the created thread (forum and media channels).
    pub applied_tags: Option<Vec<ForumTagId>>,
    /// A poll.
    pub poll: Option<CreatePoll>,
}

impl ExecuteWebhook {
    /// An empty webhook message.
    pub fn new() -> Self {
        Self::default()
    }

    create_setters!();

    /// Overrides the webhook's username for this message.
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Overrides the webhook's avatar for this message.
    pub fn avatar_url(mut self, avatar_url: impl Into<String>) -> Self {
        self.avatar_url = Some(avatar_url.into());
        self
    }

    /// Creates a thread with this name (forum and media channels).
    pub fn thread_name(mut self, thread_name: impl Into<String>) -> Self {
        self.thread_name = Some(thread_name.into());
        self
    }

    /// Applies these tags to the created thread.
    pub fn applied_tags(mut self, tags: Vec<ForumTagId>) -> Self {
        self.applied_tags = Some(tags);
        self
    }

    /// Checks the body against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_body(&BodyRef {
            allowed_flags: EXECUTE_WEBHOOK_FLAGS,
            endpoint: "Execute Webhook",
            patch: false,
            content: self.content.as_deref(),
            embeds: self.embeds.as_deref(),
            components: self.components.as_deref(),
            flags: self.flags,
            poll: self.poll.as_ref(),
            stickers: 0,
            attachments: self.attachments.as_deref(),
            shared_client_theme: false,
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_some(&mut map, "content", &self.content)?;
        entry_some(&mut map, "username", &self.username)?;
        entry_some(&mut map, "avatar_url", &self.avatar_url)?;
        entry_some(&mut map, "tts", &self.tts)?;
        entry_some(&mut map, "embeds", &self.embeds)?;
        entry_some(
            &mut map,
            "allowed_mentions",
            &effective_mentions(&self.allowed_mentions, default_mentions),
        )?;
        entry_some(&mut map, "components", &self.components)?;
        entry_some(&mut map, "attachments", &self.attachments)?;
        entry_some(&mut map, "flags", &self.flags)?;
        entry_some(&mut map, "thread_name", &self.thread_name)?;
        entry_some(&mut map, "applied_tags", &self.applied_tags)?;
        entry_some(&mut map, "poll", &self.poll)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// EditWebhookMessage / EditInteractionResponse
// ---------------------------------------------------------------------------

/// Body of `PATCH /webhooks/{id}/{token}/messages/{message.id}` (Edit
/// Webhook Message), also used to edit an interaction's original response
/// and follow-ups ([`EditInteractionResponse`]). `thread_id` and
/// `with_components` are in
/// [`WebhookMessageQuery`](crate::model::WebhookMessageQuery).
///
/// Every field is a patch. `attachments` must list every attachment the
/// message keeps, existing and new: files uploaded with the edit are
/// appended, and existing attachments missing from a present list are
/// removed. A poll can be added only when the edit completes a deferred
/// interaction response.
///
/// As with [`EditMessage`], [`validate`](Self::validate) applies the
/// Components V2 rules only when this edit sets `IS_COMPONENTS_V2`.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct EditWebhookMessage {
    /// New text content.
    pub content: NullablePatch<String>,
    /// New embeds.
    pub embeds: NullablePatch<Vec<Embed>>,
    /// New flags (`SUPPRESS_EMBEDS` and `IS_COMPONENTS_V2` only).
    pub flags: NullablePatch<MessageFlags>,
    /// Mentions allowed in the new content.
    pub allowed_mentions: NullablePatch<AllowedMentions>,
    /// New components.
    pub components: NullablePatch<Vec<Component>>,
    /// Every attachment the message keeps (existing and newly uploaded).
    pub attachments: NullablePatch<Vec<MessageAttachment>>,
    /// A poll, only when completing a deferred interaction response.
    pub poll: NullablePatch<CreatePoll>,
}

/// Body of `PATCH /webhooks/{application.id}/{interaction.token}/messages/@original`
/// (and of follow-up edits). Discord documents it as functioning "the same
/// as Edit Webhook Message", so it is that type.
pub type EditInteractionResponse = EditWebhookMessage;

impl EditWebhookMessage {
    /// An edit that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    patch_setters!();

    /// Adds a poll (only when completing a deferred interaction response).
    pub fn poll(mut self, poll: CreatePoll) -> Self {
        self.poll = set(poll);
        self
    }

    /// Removes the message's poll (`"poll": null`), as Discord requires
    /// when this edit turns a message with a poll into a Components V2
    /// message.
    pub fn clear_poll(mut self) -> Self {
        self.poll = Patch::Value(Nullable::Null);
        self
    }

    /// Checks the values this edit sets against Discord's limits.
    pub fn validate(&self) -> Result<(), DiscordError> {
        check_body(&BodyRef {
            allowed_flags: EDIT_MESSAGE_FLAGS,
            endpoint: "Edit Webhook Message",
            patch: true,
            content: patch_value(&self.content).map(String::as_str),
            embeds: patch_value(&self.embeds).map(Vec::as_slice),
            components: patch_value(&self.components).map(Vec::as_slice),
            flags: patch_value(&self.flags).copied(),
            poll: patch_value(&self.poll),
            stickers: 0,
            attachments: patch_value(&self.attachments).map(Vec::as_slice),
            shared_client_theme: false,
        })
    }

    fn serialize_with<S: Serializer>(
        &self,
        serializer: S,
        default_mentions: Option<&AllowedMentions>,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        entry_patch(&mut map, "content", &self.content)?;
        entry_patch(&mut map, "embeds", &self.embeds)?;
        entry_patch(&mut map, "flags", &self.flags)?;
        entry_patch_mentions(&mut map, &self.allowed_mentions, default_mentions)?;
        entry_patch(&mut map, "components", &self.components)?;
        entry_patch(&mut map, "attachments", &self.attachments)?;
        entry_patch(&mut map, "poll", &self.poll)?;
        map.end()
    }
}

// ---------------------------------------------------------------------------
// Shared validation
// ---------------------------------------------------------------------------

/// The fields of any message body, borrowed for [`check_body`]. Patch
/// bodies pass the values they set (`null` and omitted are `None`).
struct BodyRef<'a> {
    allowed_flags: u64,
    endpoint: &'static str,
    /// Whether the body replaces an existing message's fields. An edit that
    /// omits `flags` (or sets them without `IS_COMPONENTS_V2`, which cannot
    /// be removed once set) leaves the message's mode as it is, so the
    /// Components V2 and legacy rules are applied only when the body itself
    /// sets `IS_COMPONENTS_V2`.
    patch: bool,
    content: Option<&'a str>,
    embeds: Option<&'a [Embed]>,
    components: Option<&'a [Component]>,
    flags: Option<MessageFlags>,
    poll: Option<&'a CreatePoll>,
    stickers: usize,
    attachments: Option<&'a [MessageAttachment]>,
    shared_client_theme: bool,
}

/// The rules every message body shares.
fn check_body(body: &BodyRef<'_>) -> Result<(), DiscordError> {
    let flags = body.flags.unwrap_or_default();
    // Only flags this version knows and the endpoint does not accept are
    // rejected; an unknown bit may be a flag Discord added, and local
    // validation must not be stricter than Discord.
    let known = MessageFlags::all().bits();
    let rejected = flags.bits() & known & !body.allowed_flags;
    if rejected != 0 {
        let names = MessageFlags::from_bits(rejected).flag_names().join(", ");
        return Err(DiscordError::model(format!(
            "message flags {names} cannot be set on {}",
            body.endpoint
        )));
    }
    let mode = if flags.contains(IS_COMPONENTS_V2) {
        ComponentsMode::V2
    } else if body.patch {
        ComponentsMode::Unknown
    } else {
        ComponentsMode::Legacy
    };
    if mode == ComponentsMode::V2 {
        // "When the flag IS_COMPONENTS_V2 is set, the message can only
        // contain components" (Create Message, Execute Webhook). An empty
        // embeds list is the documented way to clear them on an edit.
        let mut conflicting = Vec::new();
        if body.content.is_some() {
            conflicting.push("content");
        }
        if body.embeds.is_some_and(|embeds| !embeds.is_empty()) {
            conflicting.push("embeds");
        }
        if body.poll.is_some() {
            conflicting.push("poll");
        }
        if body.stickers > 0 {
            conflicting.push("sticker_ids");
        }
        if body.shared_client_theme {
            conflicting.push("shared_client_theme");
        }
        if !conflicting.is_empty() {
            return Err(DiscordError::model(format!(
                "a message with the IS_COMPONENTS_V2 flag cannot set {}",
                conflicting.join(", ")
            )));
        }
    }
    if let Some(content) = body.content {
        check_max_len(
            "",
            "message content",
            content,
            limits::MAX_MESSAGE_CONTENT_LENGTH,
        )?;
    }
    if let Some(embeds) = body.embeds {
        check_count(
            "",
            "message embeds",
            embeds.len(),
            0,
            limits::MAX_EMBEDS_PER_MESSAGE,
        )?;
        let mut total = 0_usize;
        for (index, embed) in embeds.iter().enumerate() {
            embed.check(&child_path("", "embeds", index))?;
            total = total.saturating_add(embed.text_length());
        }
        // "the combined sum of characters ... across all embeds attached to
        // a message must not exceed 6000 characters" (Embed Limits).
        if total > limits::MAX_EMBED_TOTAL_LENGTH {
            return Err(DiscordError::model(format!(
                "embed total length across the message must be at most {} characters, got {total}",
                limits::MAX_EMBED_TOTAL_LENGTH
            )));
        }
    }
    if let Some(components) = body.components {
        check_message_components(components, mode)?;
    }
    check_count(
        "",
        "message sticker_ids",
        body.stickers,
        0,
        limits::MAX_STICKERS_PER_MESSAGE,
    )?;
    if let Some(poll) = body.poll {
        poll.check("poll")?;
    }
    if let Some(attachments) = body.attachments {
        for (index, attachment) in attachments.iter().enumerate() {
            attachment.check(&child_path("", "attachments", index))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

/// Content-only bodies from text.
macro_rules! from_text {
    ($($ty:ident),*) => {$(
        impl From<&str> for $ty {
            fn from(content: &str) -> Self {
                Self::new().content(content)
            }
        }

        impl From<String> for $ty {
            fn from(content: String) -> Self {
                Self::new().content(content)
            }
        }
    )*};
}

from_text!(
    CreateMessage,
    EditMessage,
    InteractionMessage,
    CreateFollowup,
    ExecuteWebhook,
    EditWebhookMessage
);

/// Lists the set fields among `fields`; empty when all are unset.
fn unsupported(fields: &[(&'static str, bool)]) -> Vec<&'static str> {
    fields
        .iter()
        .filter_map(|(field, set)| set.then_some(*field))
        .collect()
}

/// Fails with the fields a target type does not accept.
fn reject(target: &str, fields: Vec<&'static str>) -> Result<(), DiscordError> {
    if fields.is_empty() {
        Ok(())
    } else {
        Err(DiscordError::model(format!(
            "{target} does not accept CreateMessage fields: {}",
            fields.join(", ")
        )))
    }
}

/// The fields a channel message has and no webhook or interaction body
/// does.
fn channel_only(message: &CreateMessage) -> [(&'static str, bool); 5] {
    [
        ("message_reference", message.message_reference.is_some()),
        ("sticker_ids", message.sticker_ids.is_some()),
        ("nonce", message.nonce.is_some()),
        ("enforce_nonce", message.enforce_nonce),
        ("shared_client_theme", message.shared_client_theme.is_some()),
    ]
}

/// Fails on `message_reference`, `sticker_ids`, `nonce`, `enforce_nonce`,
/// and `shared_client_theme`, which interaction responses do not accept,
/// instead of dropping them (a reply would arrive as a plain message,
/// stickers would vanish).
impl TryFrom<CreateMessage> for InteractionMessage {
    type Error = DiscordError;

    fn try_from(message: CreateMessage) -> Result<Self, Self::Error> {
        reject(
            "an interaction response",
            unsupported(&channel_only(&message)),
        )?;
        // Exhaustive: a new CreateMessage field must be carried or rejected.
        let CreateMessage {
            content,
            nonce: _,
            enforce_nonce: _,
            tts,
            embeds,
            allowed_mentions,
            message_reference: _,
            components,
            sticker_ids: _,
            attachments,
            flags,
            poll,
            shared_client_theme: _,
        } = message;
        Ok(Self {
            tts,
            content,
            embeds,
            allowed_mentions,
            flags,
            components,
            attachments,
            poll,
        })
    }
}

/// Fails on the channel-only fields, like the [`InteractionMessage`]
/// conversion.
impl TryFrom<CreateMessage> for CreateFollowup {
    type Error = DiscordError;

    fn try_from(message: CreateMessage) -> Result<Self, Self::Error> {
        InteractionMessage::try_from(message).map(Self::from)
    }
}

/// Fails on the channel-only fields, like the [`InteractionMessage`]
/// conversion.
impl TryFrom<CreateMessage> for ExecuteWebhook {
    type Error = DiscordError;

    fn try_from(message: CreateMessage) -> Result<Self, Self::Error> {
        let InteractionMessage {
            tts,
            content,
            embeds,
            allowed_mentions,
            flags,
            components,
            attachments,
            poll,
        } = InteractionMessage::try_from(message)?;
        Ok(Self {
            content,
            tts,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            poll,
            ..Self::default()
        })
    }
}

/// An edit that sets every field the message sets. Fails on the
/// channel-only fields and on `tts` and `poll`, which an edit cannot change.
impl TryFrom<CreateMessage> for EditMessage {
    type Error = DiscordError;

    fn try_from(message: CreateMessage) -> Result<Self, Self::Error> {
        let mut fields = unsupported(&channel_only(&message));
        fields.extend(unsupported(&[
            ("tts", message.tts.is_some()),
            ("poll", message.poll.is_some()),
        ]));
        reject("EditMessage", fields)?;
        let CreateMessage {
            content,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            ..
        } = message;
        Ok(Self {
            content: into_patch(content),
            embeds: into_patch(embeds),
            flags: into_patch(flags),
            allowed_mentions: into_patch(allowed_mentions),
            components: into_patch(components),
            attachments: into_patch(attachments),
            clear_sticker_ids: false,
            clear_poll: false,
        })
    }
}

/// An edit that sets every field the message sets. Fails on the
/// channel-only fields and on `tts`.
impl TryFrom<CreateMessage> for EditWebhookMessage {
    type Error = DiscordError;

    fn try_from(message: CreateMessage) -> Result<Self, Self::Error> {
        let mut fields = unsupported(&channel_only(&message));
        fields.extend(unsupported(&[("tts", message.tts.is_some())]));
        reject("EditWebhookMessage", fields)?;
        let CreateMessage {
            content,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            poll,
            ..
        } = message;
        Ok(Self {
            content: into_patch(content),
            embeds: into_patch(embeds),
            flags: into_patch(flags),
            allowed_mentions: into_patch(allowed_mentions),
            components: into_patch(components),
            attachments: into_patch(attachments),
            poll: into_patch(poll),
        })
    }
}

/// Lossless: a follow-up accepts every field of an interaction response.
/// The flags are carried as they are; `IS_VOICE_MESSAGE` is not accepted by
/// follow-ups and fails `validate()`.
impl From<InteractionMessage> for CreateFollowup {
    fn from(message: InteractionMessage) -> Self {
        let InteractionMessage {
            tts,
            content,
            embeds,
            allowed_mentions,
            flags,
            components,
            attachments,
            poll,
        } = message;
        Self {
            content,
            tts,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            poll,
        }
    }
}

/// Lossless: the two bodies have the same fields.
impl From<CreateFollowup> for InteractionMessage {
    fn from(message: CreateFollowup) -> Self {
        let CreateFollowup {
            content,
            tts,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            poll,
        } = message;
        Self {
            tts,
            content,
            embeds,
            allowed_mentions,
            flags,
            components,
            attachments,
            poll,
        }
    }
}

/// An `@original` edit that sets every field the response sets. Fails on
/// `tts`, which an edit cannot change.
impl TryFrom<InteractionMessage> for EditWebhookMessage {
    type Error = DiscordError;

    fn try_from(message: InteractionMessage) -> Result<Self, Self::Error> {
        let InteractionMessage {
            tts,
            content,
            embeds,
            allowed_mentions,
            flags,
            components,
            attachments,
            poll,
        } = message;
        if tts.is_some() {
            return Err(DiscordError::model(
                "EditWebhookMessage does not accept InteractionMessage fields: tts",
            ));
        }
        Ok(Self {
            content: into_patch(content),
            embeds: into_patch(embeds),
            flags: into_patch(flags),
            allowed_mentions: into_patch(allowed_mentions),
            components: into_patch(components),
            attachments: into_patch(attachments),
            poll: into_patch(poll),
        })
    }
}

/// An [`EditMessage`] as a webhook message edit; `clear_poll` becomes
/// `"poll": null`. Fails when it clears stickers, which webhook messages
/// cannot have and the webhook edit cannot express.
impl TryFrom<EditMessage> for EditWebhookMessage {
    type Error = DiscordError;

    fn try_from(edit: EditMessage) -> Result<Self, Self::Error> {
        let EditMessage {
            content,
            embeds,
            flags,
            allowed_mentions,
            components,
            attachments,
            clear_sticker_ids,
            clear_poll,
        } = edit;
        if clear_sticker_ids {
            return Err(DiscordError::model(
                "EditWebhookMessage does not accept EditMessage fields: sticker_ids",
            ));
        }
        Ok(Self {
            content,
            embeds,
            flags,
            allowed_mentions,
            components,
            attachments,
            poll: if clear_poll {
                Patch::Value(Nullable::Null)
            } else {
                Patch::Missing
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_replay_needs_enforcement_and_a_nonce() {
        assert_eq!(CreateMessage::new().nonce("n").nonce_replay(), None);
        assert_eq!(
            CreateMessage::new().enforce_nonce("n").nonce_replay(),
            Some("n")
        );
        let mut message = CreateMessage::new();
        message.enforce_nonce = true;
        assert_eq!(message.nonce_replay(), None);
    }

    #[test]
    fn edits_without_flags_keep_the_message_mode() {
        use crate::request::TextDisplay;

        // A create body without flags is a legacy message: no text display.
        let create = CreateMessage::new().add_component(TextDisplay::new("x"));
        assert!(create
            .validate()
            .unwrap_err()
            .to_string()
            .contains("need the IS_COMPONENTS_V2 message flag"));

        // An edit that omits `flags` (or sets them without the V2 bit, which
        // cannot be removed) may target a V2 message: allowed.
        let edit = EditMessage::new().add_component(TextDisplay::new("x"));
        edit.validate()
            .expect("mode unknown: only shared rules apply");
        edit.clone()
            .flags(SUPPRESS_EMBEDS)
            .validate()
            .expect("flags without IS_COMPONENTS_V2 do not make it legacy");
        let webhook_edit = EditWebhookMessage::new().add_component(TextDisplay::new("x"));
        webhook_edit.validate().expect("same for webhook edits");

        // Once the edit sets the V2 bit, the V2 exclusions apply.
        assert!(edit
            .flags(IS_COMPONENTS_V2)
            .content("text")
            .validate()
            .unwrap_err()
            .to_string()
            .contains("IS_COMPONENTS_V2 flag cannot set content"));
    }

    #[test]
    fn components_v2_edits_can_clear_stickers_and_poll() {
        use crate::request::TextDisplay;

        let edit = EditMessage::new()
            .components_v2(vec![TextDisplay::new("now v2").into()])
            .clear_content()
            .clear_embeds()
            .clear_stickers()
            .clear_poll();
        edit.validate().expect("the documented transition body");
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            r#"{"content":null,"embeds":[],"flags":32768,"components":[{"type":10,"content":"now v2"}],"sticker_ids":[],"poll":null}"#
        );
        assert!(EditWebhookMessage::try_from(edit.clone()).is_err());
        let webhook = EditWebhookMessage::try_from(EditMessage {
            clear_sticker_ids: false,
            ..edit
        })
        .unwrap();
        assert_eq!(webhook.poll, Patch::Value(Nullable::Null));
        assert_eq!(
            serde_json::to_string(&EditWebhookMessage::new().clear_poll()).unwrap(),
            r#"{"poll":null}"#
        );
    }

    #[test]
    fn default_mentions_apply_only_when_unset() {
        let default = AllowedMentions {
            parse: vec!["users".to_owned()],
            ..AllowedMentions::default()
        };
        let own = AllowedMentions::default();
        let bare = CreateMessage::new().content("hi");
        assert_eq!(
            serde_json::to_string(&bare.allowed_mentions_or(Some(&default))).unwrap(),
            r#"{"content":"hi","allowed_mentions":{"parse":["users"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&bare.allowed_mentions_or(None)).unwrap(),
            serde_json::to_string(&bare).unwrap()
        );
        let set = bare.allowed_mentions(own);
        assert_eq!(
            serde_json::to_string(&set.allowed_mentions_or(Some(&default))).unwrap(),
            r#"{"content":"hi","allowed_mentions":{}}"#
        );

        let mut edit = EditMessage::new();
        assert_eq!(
            serde_json::to_string(&edit.allowed_mentions_or(Some(&default))).unwrap(),
            r#"{"allowed_mentions":{"parse":["users"]}}"#
        );
        edit.allowed_mentions = Patch::Value(Nullable::Null);
        assert_eq!(
            serde_json::to_string(&edit.allowed_mentions_or(Some(&default))).unwrap(),
            r#"{"allowed_mentions":null}"#
        );
    }
}
