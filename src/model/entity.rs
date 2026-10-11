//! The `DiscordModel` trait and its impls for the core ID-bearing models.

use super::{Attachment, Channel, GenericId, Guild, Message, Role, User};

// --- DiscordModel trait ---

/// Trait for all Discord data models that have a snowflake ID.
///
/// Parallels discord.js's `Base` class, providing a common interface
/// for ID access and creation timestamp extraction. IDs are `Copy`, so they
/// are returned by value.
pub trait DiscordModel: Send + Sync + 'static {
    /// Returns the ID of this model.
    fn id(&self) -> GenericId;

    /// Returns the ID when the model has one.
    ///
    /// Most Discord models always carry an ID, so the default implementation
    /// simply delegates to [`DiscordModel::id`]. Models that can exist before
    /// Discord assigns an ID, such as `ApplicationCommand`, override this.
    fn id_opt(&self) -> Option<GenericId> {
        Some(self.id())
    }

    /// Returns the creation timestamp as Unix milliseconds, extracted from
    /// the ID (`None` only when [`DiscordModel::id_opt`] is `None`).
    fn created_at(&self) -> Option<u64> {
        self.id_opt().map(GenericId::timestamp)
    }
}

impl DiscordModel for User {
    fn id(&self) -> GenericId {
        self.id
    }
}

impl DiscordModel for Guild {
    fn id(&self) -> GenericId {
        self.id
    }
}

impl DiscordModel for Channel {
    fn id(&self) -> GenericId {
        self.id
    }
}

impl DiscordModel for Message {
    fn id(&self) -> GenericId {
        self.id
    }
}

impl DiscordModel for Role {
    fn id(&self) -> GenericId {
        self.id
    }
}

impl DiscordModel for Attachment {
    fn id(&self) -> GenericId {
        self.id
    }
}
