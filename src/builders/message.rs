//! [`MessageBuilder`], the fluent builder for channel and interaction
//! message bodies. Moved from `response.rs` unchanged; `response::MessageBuilder`
//! re-exports it.

use serde_json::Value;

use crate::builders::ComponentsV2Message;
use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
use crate::model::CreateMessage;
use crate::response::EPHEMERAL_FLAG;

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `MessageBuilder`.
pub struct MessageBuilder {
    inner: CreateMessage,
}

impl MessageBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.inner.content = Some(content.into());
        self
    }

    pub fn components(mut self, components: Vec<Value>) -> Self {
        self.inner.components = Some(components);
        self
    }

    pub fn components_v2(mut self, message: ComponentsV2Message) -> Self {
        self.inner.components = Some(message.build());
        self.inner.flags = Some(self.inner.flags.unwrap_or(0) | MESSAGE_FLAG_IS_COMPONENTS_V2);
        self
    }

    pub fn flags(mut self, flags: u64) -> Self {
        self.inner.flags = Some(flags);
        self
    }

    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        if ephemeral {
            self.inner.flags = Some(self.inner.flags.unwrap_or(0) | EPHEMERAL_FLAG);
        }
        self
    }

    pub fn build(self) -> CreateMessage {
        self.inner
    }
}
