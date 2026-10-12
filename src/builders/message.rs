//! [`MessageBuilder`], the fluent builder for message bodies.
//! `response::MessageBuilder` re-exports it.
//!
//! It holds a typed [`CreateMessage`] and has setters only for the fields
//! every message endpoint shares (content, embeds, components, flags,
//! allowed mentions, attachments, poll, tts), so it converts losslessly into
//! an [`InteractionMessage`] or [`CreateFollowup`] as well. Channel-only
//! fields (reply reference, stickers, nonce) are set on the built
//! [`CreateMessage`].

use crate::bitfield::{message_flags, MessageFlags};
use crate::error::DiscordError;
use crate::model::AllowedMentions;
use crate::request::{
    Component, CreateFollowup, CreateMessage, CreatePoll, Embed, InteractionMessage,
    MessageAttachment,
};

use super::components::ComponentsV2Message;

/// Builds a message body: a [`CreateMessage`], or (through `From`) an
/// [`InteractionMessage`] or [`CreateFollowup`].
///
/// ```
/// use discordrs::builders::{ComponentsV2Message, MessageBuilder, TextDisplayBuilder};
///
/// let message = MessageBuilder::new()
///     .components_v2(
///         ComponentsV2Message::new().add_text_display(TextDisplayBuilder::new("Hello")),
///     )
///     .build();
/// assert!(message.is_components_v2());
/// ```
#[derive(Clone, Debug, Default)]
pub struct MessageBuilder {
    inner: CreateMessage,
}

impl MessageBuilder {
    /// An empty message.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text content (up to 2000 characters).
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.inner.content = Some(content.into());
        self
    }

    /// Replaces the components. For a Components V2 layout use
    /// [`Self::components_v2`], which also sets the flag.
    pub fn components(mut self, components: Vec<Component>) -> Self {
        self.inner.components = Some(components);
        self
    }

    /// Appends a component: a builder such as
    /// [`ActionRowBuilder`](super::ActionRowBuilder), a typed component, or
    /// a [`RawComponent`](crate::request::RawComponent).
    pub fn add_component(mut self, component: impl Into<Component>) -> Self {
        self.inner
            .components
            .get_or_insert_with(Vec::new)
            .push(component.into());
        self
    }

    /// Replaces the components with a Components V2 layout and sets the
    /// `IS_COMPONENTS_V2` flag.
    pub fn components_v2(mut self, message: ComponentsV2Message) -> Self {
        self.inner.components = Some(message.build());
        self.inner.flags =
            Some(self.inner.flags.unwrap_or_default() | message_flags::IS_COMPONENTS_V2);
        self
    }

    /// Replaces the message flags.
    pub fn flags(mut self, flags: impl Into<MessageFlags>) -> Self {
        self.inner.flags = Some(flags.into());
        self
    }

    /// Adds the `EPHEMERAL` flag when `ephemeral` is true (interaction
    /// responses and follow-ups only).
    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        if ephemeral {
            self.inner.flags =
                Some(self.inner.flags.unwrap_or_default() | message_flags::EPHEMERAL);
        }
        self
    }

    /// Replaces the embeds.
    pub fn embeds(mut self, embeds: Vec<Embed>) -> Self {
        self.inner.embeds = Some(embeds);
        self
    }

    /// Appends an embed: an [`EmbedBuilder`](super::EmbedBuilder) or an
    /// [`Embed`].
    pub fn add_embed(mut self, embed: impl Into<Embed>) -> Self {
        self.inner
            .embeds
            .get_or_insert_with(Vec::new)
            .push(embed.into());
        self
    }

    /// Sets which mentions in the content notify.
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
        self.inner.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Appends attachment metadata for a file sent with the message.
    pub fn add_attachment(mut self, attachment: impl Into<MessageAttachment>) -> Self {
        self.inner
            .attachments
            .get_or_insert_with(Vec::new)
            .push(attachment.into());
        self
    }

    /// Attaches a poll.
    pub fn poll(mut self, poll: CreatePoll) -> Self {
        self.inner.poll = Some(poll);
        self
    }

    /// Sends the message as text-to-speech.
    pub fn tts(mut self, tts: bool) -> Self {
        self.inner.tts = Some(tts);
        self
    }

    /// The message body, without checking limits.
    pub fn build(self) -> CreateMessage {
        self.inner
    }

    /// Checks the message against Discord's limits: content, embeds,
    /// components (recursively), poll, attachments, and the Components V2
    /// exclusions. Flags are checked against the union of what channel
    /// messages and interaction responses accept, since this builder serves
    /// both; the endpoint body's own `validate()` is stricter.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.check_message_builder()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<CreateMessage, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

impl From<MessageBuilder> for CreateMessage {
    fn from(builder: MessageBuilder) -> Self {
        builder.build()
    }
}

/// Lossless: the builder only sets fields interaction responses accept.
impl From<MessageBuilder> for InteractionMessage {
    fn from(builder: MessageBuilder) -> Self {
        let CreateMessage {
            content,
            tts,
            embeds,
            allowed_mentions,
            components,
            attachments,
            flags,
            poll,
            // Never set by `MessageBuilder` (no setters).
            nonce: _,
            enforce_nonce: _,
            message_reference: _,
            sticker_ids: _,
            shared_client_theme: _,
        } = builder.inner;
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

/// Lossless, like the [`InteractionMessage`] conversion.
impl From<MessageBuilder> for CreateFollowup {
    fn from(builder: MessageBuilder) -> Self {
        InteractionMessage::from(builder).into()
    }
}

#[cfg(test)]
mod tests {
    use super::MessageBuilder;
    use crate::builders::{
        ActionRowBuilder, ComponentsV2Message, CustomButtonBuilder, EmbedBuilder,
        TextDisplayBuilder,
    };
    use crate::request::{CreateFollowup, InteractionMessage};
    use serde_json::json;

    #[test]
    fn builds_a_typed_body_and_converts_losslessly() {
        let builder = MessageBuilder::new()
            .content("hi")
            .add_embed(EmbedBuilder::new().title("t"))
            .add_component(
                ActionRowBuilder::new().add_button(CustomButtonBuilder::new("go").label("Go")),
            )
            .ephemeral(true);
        let expected = json!({
            "content": "hi",
            "embeds": [{"title": "t"}],
            "components": [{"type": 1, "components": [
                {"type": 2, "style": 1, "label": "Go", "custom_id": "go"}
            ]}],
            "flags": 64
        });
        assert_eq!(
            serde_json::to_value(builder.clone().build()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(InteractionMessage::from(builder.clone())).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(CreateFollowup::from(builder.clone())).unwrap(),
            expected
        );
        builder.validate().expect("ephemeral is fine for a builder");
    }

    #[test]
    fn components_v2_sets_the_flag_and_rejects_content() {
        let message = MessageBuilder::new().flags(1 << 2).components_v2(
            ComponentsV2Message::new().add_text_display(TextDisplayBuilder::new("x")),
        );
        assert_eq!(
            serde_json::to_value(message.clone().build()).unwrap(),
            json!({"components": [{"type": 10, "content": "x"}], "flags": 32772})
        );
        assert!(message
            .content("no")
            .try_build()
            .unwrap_err()
            .to_string()
            .contains("IS_COMPONENTS_V2 flag cannot set content"));
    }
}
