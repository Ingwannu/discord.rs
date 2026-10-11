// The public async traits require `Send` futures because the runtime polls
// them on spawned Tokio tasks. Holding an `Rc` across an `.await` in a plain
// `async fn` impl must be rejected at the impl, not at a distant spawn.
use std::rc::Rc;
use std::sync::Arc;

use discordrs::cache::{CacheBackend, CacheClear, CacheClearId};
use discordrs::model::Presence;
use discordrs::{DiscordError, GenericId, Member, Message};

struct NotSend;

impl CacheBackend for NotSend {
    async fn clear_cache(
        &self,
        expected_epoch: u64,
        _clear_id: CacheClearId,
    ) -> Result<CacheClear, DiscordError> {
        let held = Rc::new(expected_epoch);
        std::future::ready(()).await;
        Ok(CacheClear::Applied(*held))
    }

    async fn clear_shard_cache(
        &self,
        expected_epoch: u64,
        _clear_id: CacheClearId,
        _shard_id: u32,
        _shard_count: u32,
    ) -> Result<CacheClear, DiscordError> {
        Ok(CacheClear::Applied(expected_epoch))
    }

    async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn put_member(
        &self,
        _epoch: u64,
        _guild_id: GenericId,
        _user_id: GenericId,
        _member: Member,
    ) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn get_member(
        &self,
        _guild_id: GenericId,
        _user_id: GenericId,
    ) -> Result<Option<Arc<Member>>, DiscordError> {
        Ok(None)
    }

    async fn list_members(&self, _guild_id: GenericId) -> Result<Vec<Arc<Member>>, DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_member(
        &self,
        _epoch: u64,
        _guild_id: GenericId,
        _user_id: GenericId,
    ) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn put_message(&self, _epoch: u64, _message: Message) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn get_message(
        &self,
        _channel_id: GenericId,
        _message_id: GenericId,
    ) -> Result<Option<Arc<Message>>, DiscordError> {
        Ok(None)
    }

    async fn list_messages(
        &self,
        _channel_id: GenericId,
    ) -> Result<Vec<Arc<Message>>, DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_message(
        &self,
        _epoch: u64,
        _channel_id: GenericId,
        _message_id: GenericId,
    ) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn put_presence(
        &self,
        _epoch: u64,
        _guild_id: GenericId,
        _user_id: GenericId,
        _presence: Presence,
    ) -> Result<(), DiscordError> {
        Ok(())
    }

    async fn get_presence(
        &self,
        _guild_id: GenericId,
        _user_id: GenericId,
    ) -> Result<Option<Arc<Presence>>, DiscordError> {
        Ok(None)
    }

    async fn list_presences(
        &self,
        _guild_id: GenericId,
    ) -> Result<Vec<Arc<Presence>>, DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_presence(
        &self,
        _epoch: u64,
        _guild_id: GenericId,
        _user_id: GenericId,
    ) -> Result<(), DiscordError> {
        Ok(())
    }
}

fn main() {}
