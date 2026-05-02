use std::sync::Arc;

use async_trait::async_trait;

use crate::error::DiscordError;
use crate::model::{Member, Message, Presence, Snowflake};

use super::CacheHandle;

/// Async extension point for external cache stores.
///
/// The trait focuses on the TTL-backed hot path first: members, messages, and
/// presences. Implementors can back these calls with Redis, DragonflyDB,
/// Valkey, or another shared store while preserving the `Arc`-returning read
/// shape used by the in-memory cache.
#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn clear_cache(&self) -> Result<(), DiscordError>;

    async fn purge_expired_entries(&self) -> Result<(), DiscordError>;

    async fn put_member(
        &self,
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

    async fn delete_member(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError>;

    async fn put_message(&self, message: Message) -> Result<(), DiscordError>;

    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, DiscordError>;

    async fn list_messages(
        &self,
        channel_id: &Snowflake,
    ) -> Result<Vec<Arc<Message>>, DiscordError>;

    async fn delete_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<(), DiscordError>;

    async fn put_presence(
        &self,
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

    async fn delete_presence(
        &self,
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

    async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
        self.purge_expired().await;
        Ok(())
    }

    async fn put_member(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        member: Member,
    ) -> Result<(), DiscordError> {
        self.upsert_member(guild_id, user_id, member).await;
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
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.remove_member(guild_id, user_id).await;
        Ok(())
    }

    async fn put_message(&self, message: Message) -> Result<(), DiscordError> {
        self.upsert_message(message).await;
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
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.remove_message(channel_id, message_id).await;
        Ok(())
    }

    async fn put_presence(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        presence: Presence,
    ) -> Result<(), DiscordError> {
        self.upsert_presence(guild_id, user_id, presence).await;
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
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), DiscordError> {
        self.remove_presence(guild_id, user_id).await;
        Ok(())
    }
}
