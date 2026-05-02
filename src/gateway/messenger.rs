use tokio::sync::mpsc;

use super::client::{
    request_soundboard_sounds_payload, voice_state_update_payload, GatewayCommand,
};
use crate::error::DiscordError;
use crate::types::invalid_data_error;

#[derive(Clone)]
/// Typed Discord API object for `ShardMessenger`.
pub struct ShardMessenger {
    pub(super) shard_id: u32,
    pub(super) command_tx: mpsc::Sender<GatewayCommand>,
}

impl ShardMessenger {
    pub fn shard_id(&self) -> u32 {
        self.shard_id
    }

    pub fn update_presence(&self, status: impl Into<String>) -> Result<(), DiscordError> {
        self.send(GatewayCommand::UpdatePresence(status.into()))
    }

    pub fn update_presence_typed(
        &self,
        presence: crate::model::UpdatePresence,
    ) -> Result<(), DiscordError> {
        self.send(GatewayCommand::UpdatePresenceData(presence))
    }

    pub fn request_guild_members(
        &self,
        request: crate::model::RequestGuildMembers,
    ) -> Result<(), DiscordError> {
        self.send(GatewayCommand::RequestGuildMembers(request))
    }

    /// Sends a Gateway channel-info request through this shard.
    pub fn request_channel_info(
        &self,
        request: crate::model::RequestChannelInfo,
    ) -> Result<(), DiscordError> {
        self.send(GatewayCommand::RequestChannelInfo(request))
    }

    pub fn update_voice_state(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: Option<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.send(GatewayCommand::SendPayload(voice_state_update_payload(
            guild_id.into(),
            channel_id,
            self_mute,
            self_deaf,
        )))
    }

    pub fn request_soundboard_sounds(
        &self,
        guild_ids: impl IntoIterator<Item = crate::model::Snowflake>,
    ) -> Result<(), DiscordError> {
        self.send(GatewayCommand::SendPayload(
            request_soundboard_sounds_payload(guild_ids.into_iter().collect(), None),
        ))
    }

    pub fn join_voice(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(guild_id, Some(channel_id.into()), self_mute, self_deaf)
    }

    pub fn leave_voice(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(guild_id, None, self_mute, self_deaf)
    }

    pub fn reconnect(&self) -> Result<(), DiscordError> {
        self.send(GatewayCommand::Reconnect)
    }

    pub fn shutdown(&self) -> Result<(), DiscordError> {
        self.send(GatewayCommand::Shutdown)
    }

    fn send(&self, command: GatewayCommand) -> Result<(), DiscordError> {
        self.command_tx
            .try_send(command)
            .map_err(|error| invalid_data_error(format!("failed to send gateway command: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::super::client::{GatewayCommand, GATEWAY_COMMAND_QUEUE_CAPACITY};
    use super::ShardMessenger;
    use crate::model::{RequestChannelInfo, RequestGuildMembers, Snowflake, UpdatePresence};

    #[test]
    fn shard_messenger_sends_all_public_command_shapes() {
        let (tx, mut rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let messenger = ShardMessenger {
            shard_id: 7,
            command_tx: tx,
        };

        assert_eq!(messenger.shard_id(), 7);
        messenger.update_presence("online").unwrap();
        messenger
            .update_presence_typed(UpdatePresence::online_with_activity("typed"))
            .unwrap();
        messenger
            .request_guild_members(RequestGuildMembers {
                guild_id: Snowflake::from(1),
                query: Some("a".to_string()),
                limit: Some(1),
                presences: Some(true),
                user_ids: None,
                nonce: Some("nonce".to_string()),
            })
            .unwrap();
        messenger
            .request_channel_info(RequestChannelInfo::voice_metadata("1"))
            .unwrap();
        messenger
            .update_voice_state(Snowflake::from(1), Some(Snowflake::from(2)), true, false)
            .unwrap();
        messenger
            .request_soundboard_sounds([Snowflake::from(1), Snowflake::from(2)])
            .unwrap();
        messenger
            .join_voice(Snowflake::from(3), Snowflake::from(4), false, true)
            .unwrap();
        messenger
            .leave_voice(Snowflake::from(3), false, false)
            .unwrap();
        messenger.reconnect().unwrap();
        messenger.shutdown().unwrap();

        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::UpdatePresence(status)) if status == "online"
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::UpdatePresenceData(presence)) if presence.status == "online"
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::RequestGuildMembers(request))
                if request.guild_id == Snowflake::from(1)
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::RequestChannelInfo(request))
                if request.guild_id == Snowflake::from(1)
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::SendPayload(payload))
                if payload["op"] == serde_json::json!(4)
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::SendPayload(payload))
                if payload["op"] == serde_json::json!(31)
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::SendPayload(payload))
                if payload["op"] == serde_json::json!(4)
                    && payload["d"]["channel_id"] == serde_json::json!("4")
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::SendPayload(payload))
                if payload["op"] == serde_json::json!(4)
                    && payload["d"]["channel_id"].is_null()
        ));
        assert!(matches!(
            rx.blocking_recv(),
            Some(GatewayCommand::Reconnect)
        ));
        assert!(matches!(rx.blocking_recv(), Some(GatewayCommand::Shutdown)));
    }

    #[test]
    fn shard_messenger_reports_closed_command_channel() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        let messenger = ShardMessenger {
            shard_id: 1,
            command_tx: tx,
        };

        let error = messenger.update_presence("offline").unwrap_err();
        assert!(error.to_string().contains("failed to send gateway command"));
    }
}
