use std::sync::Arc;

use async_trait::async_trait;

use crate::error::DiscordError;
use crate::model::{Member, Message, Presence, Snowflake};

use super::CacheHandle;

/// The shard generation a [`CacheBackend`] write belongs to.
///
/// Every successful [`CacheBackend::clear_shard_cache`] starts a new
/// generation for that `(shard_id, shard_count)` and returns its epoch. The
/// gateway runtime stamps every later write for the shard with that epoch,
/// and a backend must not apply a write whose epoch is older than the
/// shard's latest clear.
///
/// Why: stopping a client cannot retract writes it already submitted to a
/// remote store. Without a fence, a delayed write from a stopped client can
/// land after a replacement client cleared the shard and resurrect stale
/// data. With it, the store itself rejects the late write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheFence {
    /// Shard the written entry belongs to.
    pub shard_id: u32,
    /// Shard count the shard id refers to.
    pub shard_count: u32,
    /// Epoch returned by the clear that started the current generation.
    pub epoch: u64,
}

impl CacheFence {
    /// Creates a fence for `shard_id` of `shard_count` at `epoch`.
    pub fn new(shard_id: u32, shard_count: u32, epoch: u64) -> Self {
        Self {
            shard_id,
            shard_count,
            epoch,
        }
    }
}

/// Async extension point for external cache stores.
///
/// The trait focuses on the TTL-backed hot path first: members, messages, and
/// presences. Implementors can back these calls with Redis, DragonflyDB,
/// Valkey, or another shared store while preserving the `Arc`-returning read
/// shape used by the in-memory cache.
///
/// # Epoch fencing contract
///
/// - The store keeps one epoch per `(shard_id, shard_count)`, starting at 0.
/// - [`clear_shard_cache`](Self::clear_shard_cache) atomically deletes the
///   shard's entries, increments its epoch, and returns the new value. The
///   epoch must be strictly greater than every epoch previously returned for
///   that shard, by any client sharing the store.
/// - Every write (`put_*`, `delete_*`) carries a [`CacheFence`]. A write
///   whose `fence.epoch` is lower than the shard's current epoch must be
///   ignored and reported as `Ok(())` (an error would make the stale client
///   schedule another clear and wipe the newer generation). The check and the
///   write must be atomic with respect to a concurrent clear (for example a
///   Lua script or a transaction in Redis).
/// - Changing the shard count needs a full [`clear_cache`](Self::clear_cache):
///   fences of different shard counts do not order against each other.
///
/// [`CacheHandle`] implements this contract for the in-memory store.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn clear_cache(&self) -> Result<(), DiscordError>;

    /// Drops the entries owned by gateway shard `shard_id` of `shard_count`
    /// and starts a new generation for it, returning the new epoch (see the
    /// trait docs).
    ///
    /// The gateway runtime calls this instead of
    /// [`CacheBackend::clear_cache`] when that shard starts a new session
    /// (READY), when the shard stops after losing events, and to resync the
    /// shard after a write was dropped or failed — so the mirror is never
    /// left stale. One shard's call must not erase data other shards (or
    /// other processes sharing the store) are still serving.
    ///
    /// Ownership follows [`CacheHandle::clear_shard`]: a guild belongs to
    /// shard `(guild_id >> 22) % shard_count`, DMs belong to shard 0, and
    /// users are global. With `shard_count <= 1` every entry belongs to the
    /// shard. Implementations must delete the members, presences, and
    /// messages of the shard's guilds (and DM messages on shard 0); there is
    /// deliberately no default, because silently keeping entries would make
    /// the mirror stale.
    async fn clear_shard_cache(&self, shard_id: u32, shard_count: u32)
        -> Result<u64, DiscordError>;

    async fn purge_expired_entries(&self) -> Result<(), DiscordError>;

    /// Stores a member unless `fence` is older than the shard's last clear.
    async fn put_member(
        &self,
        fence: CacheFence,
        guild_id: Snowflake,
        user_id: Snowflake,
        member: Member,
    ) -> Result<(), DiscordError>;

    async fn get_member(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<Option<Arc<Member>>, DiscordError>;

    async fn list_members(&self, guild_id: &Snowflake) -> Result<Vec<Arc<Member>>, DiscordError>;

    /// Deletes a member unless `fence` is older than the shard's last clear.
    async fn delete_member(
        &self,
        fence: CacheFence,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError>;

    /// Stores a message unless `fence` is older than the shard's last clear.
    async fn put_message(&self, fence: CacheFence, message: Message) -> Result<(), DiscordError>;

    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, DiscordError>;

    async fn list_messages(
        &self,
        channel_id: &Snowflake,
    ) -> Result<Vec<Arc<Message>>, DiscordError>;

    /// Deletes a message unless `fence` is older than the shard's last clear.
    async fn delete_message(
        &self,
        fence: CacheFence,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<(), DiscordError>;

    /// Stores a presence unless `fence` is older than the shard's last clear.
    async fn put_presence(
        &self,
        fence: CacheFence,
        guild_id: Snowflake,
        user_id: Snowflake,
        presence: Presence,
    ) -> Result<(), DiscordError>;

    async fn get_presence(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<Option<Arc<Presence>>, DiscordError>;

    async fn list_presences(
        &self,
        guild_id: &Snowflake,
    ) -> Result<Vec<Arc<Presence>>, DiscordError>;

    /// Deletes a presence unless `fence` is older than the shard's last
    /// clear.
    async fn delete_presence(
        &self,
        fence: CacheFence,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError>;
}

#[async_trait]
impl CacheBackend for CacheHandle {
    async fn clear_cache(&self) -> Result<(), DiscordError> {
        self.clear().await;
        Ok(())
    }

    async fn clear_shard_cache(
        &self,
        shard_id: u32,
        shard_count: u32,
    ) -> Result<u64, DiscordError> {
        Ok(self.clear_shard_generation(shard_id, shard_count).await)
    }

    async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
        self.purge_expired().await;
        Ok(())
    }

    async fn put_member(
        &self,
        fence: CacheFence,
        guild_id: Snowflake,
        user_id: Snowflake,
        member: Member,
    ) -> Result<(), DiscordError> {
        self.fenced_upsert_member(fence, guild_id, user_id, member)
            .await;
        Ok(())
    }

    async fn get_member(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<Option<Arc<Member>>, DiscordError> {
        Ok(self.member_arc(guild_id, user_id).await)
    }

    async fn list_members(&self, guild_id: &Snowflake) -> Result<Vec<Arc<Member>>, DiscordError> {
        Ok(self.members_arc(guild_id).await)
    }

    async fn delete_member(
        &self,
        fence: CacheFence,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.fenced_remove_member(fence, guild_id, user_id).await;
        Ok(())
    }

    async fn put_message(&self, fence: CacheFence, message: Message) -> Result<(), DiscordError> {
        self.fenced_upsert_message(fence, message).await;
        Ok(())
    }

    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, DiscordError> {
        Ok(self.message_arc(channel_id, message_id).await)
    }

    async fn list_messages(
        &self,
        channel_id: &Snowflake,
    ) -> Result<Vec<Arc<Message>>, DiscordError> {
        Ok(self.messages_arc(channel_id).await)
    }

    async fn delete_message(
        &self,
        fence: CacheFence,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.fenced_remove_message(fence, channel_id, message_id)
            .await;
        Ok(())
    }

    async fn put_presence(
        &self,
        fence: CacheFence,
        guild_id: Snowflake,
        user_id: Snowflake,
        presence: Presence,
    ) -> Result<(), DiscordError> {
        self.fenced_upsert_presence(fence, guild_id, user_id, presence)
            .await;
        Ok(())
    }

    async fn get_presence(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<Option<Arc<Presence>>, DiscordError> {
        Ok(self.presence_arc(guild_id, user_id).await)
    }

    async fn list_presences(
        &self,
        guild_id: &Snowflake,
    ) -> Result<Vec<Arc<Presence>>, DiscordError> {
        Ok(self.presences_arc(guild_id).await)
    }

    async fn delete_presence(
        &self,
        fence: CacheFence,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.fenced_remove_presence(fence, guild_id, user_id).await;
        Ok(())
    }
}
