//! Voice endpoints: voice channel status, stage instances, voice regions, and
//! voice states.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateStageInstance, ModifyCurrentUserVoiceState, ModifyStageInstance, ModifyUserVoiceState,
    SetVoiceChannelStatus, Snowflake, StageInstance, VoiceRegion, VoiceState,
};

use super::RestClient;

impl RestClient {
    pub async fn set_voice_channel_status(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &SetVoiceChannelStatus,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/voice-status", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_stage_instance(&self, body: &Value) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    pub async fn create_stage_instance_typed<B>(
        &self,
        body: &B,
    ) -> Result<StageInstance, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(Method::POST, "/stage-instances", Some(body))
            .await
    }

    pub async fn create_stage_instance_from_request(
        &self,
        body: &CreateStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    pub async fn get_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<StageInstance, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/stage-instances/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    pub async fn modify_stage_instance_typed<B>(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<StageInstance, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/stage-instances/{}", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_stage_instance_from_request(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &ModifyStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    pub async fn delete_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/stage-instances/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_voice_regions(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    pub async fn get_voice_regions_typed(&self) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    pub async fn get_guild_voice_regions(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/regions", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_current_user_voice_state(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<VoiceState, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/voice-states/@me", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_user_voice_state(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<VoiceState, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/voice-states/{}",
                guild_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_current_user_voice_state<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/voice-states/@me", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_current_user_voice_state_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyCurrentUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_current_user_voice_state(guild_id, body).await
    }

    pub async fn modify_user_voice_state<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PATCH,
            &format!(
                "/guilds/{}/voice-states/{}",
                guild_id.into(),
                user_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn modify_user_voice_state_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &ModifyUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_user_voice_state(guild_id, user_id, body).await
    }
}
