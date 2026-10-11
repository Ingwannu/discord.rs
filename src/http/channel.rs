//! Channel endpoints: channel CRUD, guild channel lists and positions, DM and
//! group DM channels, typing, permission overwrites, and announcement follows.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    AddGroupDmRecipient, Channel, CreateDmChannel, CreateGroupDmChannel, CreateGuildChannel,
    EditChannelPermission, FollowedChannel, GenericId, ModifyGuildChannelPosition,
};

use super::RestClient;

impl RestClient {
    pub async fn get_channel(&self, channel_id: GenericId) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}", channel_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn delete_channel(&self, channel_id: GenericId) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::DELETE,
            &format!("/channels/{}", channel_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_channel(
        &self,
        channel_id: GenericId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.update_channel_typed(channel_id, body).await
    }

    pub async fn update_channel_typed<B>(
        &self,
        channel_id: GenericId,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/channels/{}", channel_id),
            Some(body),
        )
        .await
    }

    pub async fn get_guild_channels(
        &self,
        guild_id: GenericId,
    ) -> Result<Vec<Channel>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/channels", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_channel(
        &self,
        guild_id: GenericId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    pub async fn create_guild_channel_typed<B>(
        &self,
        guild_id: GenericId,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/channels", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn create_guild_channel_from_request(
        &self,
        guild_id: GenericId,
        body: &CreateGuildChannel,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    /// Reorder or reparent guild channels.
    ///
    /// Discord returns no body on success. Only entries for channels being
    /// modified need to be included.
    pub async fn modify_guild_channel_positions(
        &self,
        guild_id: GenericId,
        positions: &[ModifyGuildChannelPosition],
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/channels", guild_id),
            Some(positions),
        )
        .await
    }

    pub async fn create_dm_channel_typed(
        &self,
        body: &CreateDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    pub async fn create_group_dm_channel_typed(
        &self,
        body: &CreateGroupDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    pub async fn add_group_dm_recipient(
        &self,
        channel_id: GenericId,
        user_id: GenericId,
        body: &AddGroupDmRecipient,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/recipients/{}", channel_id, user_id),
            Some(body),
        )
        .await
    }

    pub async fn remove_group_dm_recipient(
        &self,
        channel_id: GenericId,
        user_id: GenericId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/recipients/{}", channel_id, user_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn trigger_typing_indicator(
        &self,
        channel_id: GenericId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/typing", channel_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_channel_permissions(
        &self,
        channel_id: GenericId,
        overwrite_id: GenericId,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/permissions/{}", channel_id, overwrite_id),
            Some(body),
        )
        .await
    }

    pub async fn edit_channel_permissions_typed(
        &self,
        channel_id: GenericId,
        overwrite_id: GenericId,
        body: &EditChannelPermission,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/permissions/{}", channel_id, overwrite_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_channel_permission(
        &self,
        channel_id: GenericId,
        overwrite_id: GenericId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/permissions/{}", channel_id, overwrite_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn follow_announcement_channel(
        &self,
        channel_id: GenericId,
        webhook_channel_id: GenericId,
    ) -> Result<FollowedChannel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/followers", channel_id),
            Some(&serde_json::json!({ "webhook_channel_id": webhook_channel_id })),
        )
        .await
    }
}
