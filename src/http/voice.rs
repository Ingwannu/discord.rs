//! Voice endpoints: voice channel status, stage instances, voice regions, and
//! voice states.

use crate::error::DiscordError;
use crate::model::{
    ChannelId, CreateStageInstance, GuildId, ModifyCurrentUserVoiceState, ModifyStageInstance,
    ModifyUserVoiceState, SetVoiceChannelStatus, StageInstance, UserId, VoiceRegion, VoiceState,
};

use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn set_voice_channel_status(
        &self,
        channel_id: ChannelId,
        body: &SetVoiceChannelStatus,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::SetVoiceChannelStatus,
            &format!("/channels/{}/voice-status", channel_id),
            json_body(body),
        )
        .await
    }

    pub async fn create_stage_instance(
        &self,
        body: &CreateStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.send(
            RouteId::CreateStageInstance,
            "/stage-instances",
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `create_stage_instance`")]
    pub async fn create_stage_instance_from_request(
        &self,
        body: &CreateStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance(body).await
    }

    pub async fn get_stage_instance(
        &self,
        channel_id: ChannelId,
    ) -> Result<StageInstance, DiscordError> {
        self.send(
            RouteId::GetStageInstance,
            &format!("/stage-instances/{}", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_stage_instance(
        &self,
        channel_id: ChannelId,
        body: &ModifyStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.send(
            RouteId::ModifyStageInstance,
            &format!("/stage-instances/{}", channel_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_stage_instance`")]
    pub async fn modify_stage_instance_from_request(
        &self,
        channel_id: ChannelId,
        body: &ModifyStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance(channel_id, body).await
    }

    pub async fn delete_stage_instance(&self, channel_id: ChannelId) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteStageInstance,
            &format!("/stage-instances/{}", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_voice_regions(&self) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.send(RouteId::ListVoiceRegions, "/voice/regions", NO_BODY)
            .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `get_voice_regions`")]
    pub async fn get_voice_regions_typed(&self) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.get_voice_regions().await
    }

    pub async fn get_guild_voice_regions(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.send(
            RouteId::GetGuildVoiceRegions,
            &format!("/guilds/{}/regions", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_current_user_voice_state(
        &self,
        guild_id: GuildId,
    ) -> Result<VoiceState, DiscordError> {
        self.send(
            RouteId::GetCurrentUserVoiceState,
            &format!("/guilds/{}/voice-states/@me", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_user_voice_state(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<VoiceState, DiscordError> {
        self.send(
            RouteId::GetUserVoiceState,
            &format!("/guilds/{}/voice-states/{}", guild_id, user_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_current_user_voice_state(
        &self,
        guild_id: GuildId,
        body: &ModifyCurrentUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::ModifyCurrentUserVoiceState,
            &format!("/guilds/{}/voice-states/@me", guild_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_current_user_voice_state`")]
    pub async fn modify_current_user_voice_state_from_request(
        &self,
        guild_id: GuildId,
        body: &ModifyCurrentUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_current_user_voice_state(guild_id, body).await
    }

    pub async fn modify_user_voice_state(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &ModifyUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::ModifyUserVoiceState,
            &format!("/guilds/{}/voice-states/{}", guild_id, user_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_user_voice_state`")]
    pub async fn modify_user_voice_state_from_request(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &ModifyUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_user_voice_state(guild_id, user_id, body).await
    }
}
