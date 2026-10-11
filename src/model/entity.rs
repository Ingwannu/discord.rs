//! The `DiscordModel` trait and its impls for the core ID-bearing models.

use super::{marker, Attachment, Channel, Guild, Id, IdMarker, Message, Role, User};

// --- DiscordModel trait ---

/// Trait for all Discord data models that have a snowflake ID.
///
/// Parallels discord.js's `Base` class, providing a common interface
/// for ID access and creation timestamp extraction. IDs are `Copy`, so they
/// are returned by value, and typed by the model's [`Marker`](Self::Marker):
/// `User::id` is a [`UserId`](super::UserId), `Guild::id` a
/// [`GuildId`](super::GuildId), and so on. Generic code such as
/// [`CachedManager`](crate::manager::CachedManager) uses the marker to take
/// the right kind of ID.
pub trait DiscordModel: Send + Sync + 'static {
    /// The resource kind of this model's ID.
    type Marker: IdMarker;

    /// Returns the ID of this model.
    fn id(&self) -> Id<Self::Marker>;

    /// Returns the ID when the model has one.
    ///
    /// Most Discord models always carry an ID, so the default implementation
    /// simply delegates to [`DiscordModel::id`]. Models that can exist before
    /// Discord assigns an ID override this.
    fn id_opt(&self) -> Option<Id<Self::Marker>> {
        Some(self.id())
    }

    /// Returns the creation timestamp as Unix milliseconds, extracted from
    /// the ID (`None` only when [`DiscordModel::id_opt`] is `None`).
    fn created_at(&self) -> Option<u64> {
        self.id_opt().map(Id::timestamp)
    }
}

impl DiscordModel for User {
    type Marker = marker::User;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}

impl DiscordModel for Guild {
    type Marker = marker::Guild;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}

impl DiscordModel for Channel {
    type Marker = marker::Channel;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}

impl DiscordModel for Message {
    type Marker = marker::Message;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}

impl DiscordModel for Role {
    type Marker = marker::Role;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}

impl DiscordModel for Attachment {
    type Marker = marker::Attachment;

    fn id(&self) -> Id<Self::Marker> {
        self.id
    }
}
