use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

use super::client::voice_state_update_payload;
use crate::error::DiscordError;
use crate::sharding::{ShardIpcMessage, ShardRuntimeStatus, ShardSupervisorEvent, ShardingManager};
use crate::types::invalid_data_error;

pub(super) fn lock_sharding_manager(
    manager: &StdMutex<ShardingManager>,
) -> StdMutexGuard<'_, ShardingManager> {
    manager
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Typed Discord API object for `ShardSupervisor`.
pub struct ShardSupervisor {
    pub(super) manager: Arc<StdMutex<ShardingManager>>,
    pub(super) tasks: Vec<(u32, JoinHandle<Result<(), DiscordError>>)>,
}

impl ShardSupervisor {
    const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(15_000);

    pub fn manager(&self) -> Arc<StdMutex<ShardingManager>> {
        Arc::clone(&self.manager)
    }

    pub fn statuses(&self) -> Vec<ShardRuntimeStatus> {
        lock_sharding_manager(&self.manager).statuses()
    }

    pub fn drain_events(&self) -> Result<Vec<ShardSupervisorEvent>, DiscordError> {
        lock_sharding_manager(&self.manager).drain_events()
    }

    pub fn send(&self, shard_id: u32, message: ShardIpcMessage) -> Result<(), DiscordError> {
        lock_sharding_manager(&self.manager).send(shard_id, message)
    }

    pub fn reconnect(&self, shard_id: u32) -> Result<(), DiscordError> {
        self.send(shard_id, ShardIpcMessage::Reconnect)
    }

    pub fn update_presence(
        &self,
        shard_id: u32,
        status: impl Into<String>,
    ) -> Result<(), DiscordError> {
        self.send(shard_id, ShardIpcMessage::UpdatePresence(status.into()))
    }

    pub fn update_voice_state(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: Option<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.send(
            shard_id,
            ShardIpcMessage::SendPayload(voice_state_update_payload(
                guild_id.into(),
                channel_id,
                self_mute,
                self_deaf,
            )),
        )
    }

    pub fn join_voice(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(
            shard_id,
            guild_id,
            Some(channel_id.into()),
            self_mute,
            self_deaf,
        )
    }

    pub fn leave_voice(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(shard_id, guild_id, None, self_mute, self_deaf)
    }

    pub fn broadcast(&self, message: ShardIpcMessage) -> Result<(), DiscordError> {
        lock_sharding_manager(&self.manager).broadcast(message)
    }

    pub fn shutdown(&self) -> Result<(), DiscordError> {
        self.broadcast(ShardIpcMessage::Shutdown)
    }

    pub async fn shutdown_and_wait(self) -> Result<(), DiscordError> {
        self.shutdown()?;
        self.wait_for_shutdown(Self::SHUTDOWN_TIMEOUT).await
    }

    pub async fn wait_for_shutdown(self, timeout_duration: Duration) -> Result<(), DiscordError> {
        self.wait_with_timeout(Some(timeout_duration)).await
    }

    pub async fn wait(self) -> Result<(), DiscordError> {
        self.wait_with_timeout(None).await
    }

    async fn wait_with_timeout(
        self,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DiscordError> {
        for (shard_id, task) in self.tasks {
            let mut task = task;
            let result = if let Some(timeout_duration) = timeout_duration {
                match timeout(timeout_duration, &mut task).await {
                    Ok(result) => result,
                    Err(_) => {
                        task.abort();
                        return Err(invalid_data_error(format!(
                            "timed out waiting for shard {shard_id} shutdown after {timeout_duration:?}"
                        )));
                    }
                }
            } else {
                task.await
            };

            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return Err(error),
                Err(error) => return Err(format!("shard task failed: {error}").into()),
            }
        }

        Ok(())
    }
}
