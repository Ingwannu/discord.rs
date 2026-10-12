//! Channel endpoints: channel CRUD, guild channel lists and positions, DM and
//! group DM channels, typing, permission overwrites, and announcement follows.

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    AddGroupDmRecipient, Channel, ChannelId, CreateDmChannel, CreateGroupDmChannel,
    CreateGuildChannel, EditChannelPermission, FollowedChannel, GuildId,
    ModifyGuildChannelPosition, OverwriteTarget, UserId,
};

use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_channel(&self, channel_id: ChannelId) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::GetChannel,
            &format!("/channels/{}", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn delete_channel(&self, channel_id: ChannelId) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::DeleteChannel,
            &format!("/channels/{}", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn update_channel(
        &self,
        channel_id: ChannelId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.update_channel_typed(channel_id, body).await
    }

    pub async fn update_channel_typed<B>(
        &self,
        channel_id: ChannelId,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyChannel,
            &format!("/channels/{}", channel_id),
            json_body(body),
        )
        .await
    }

    pub async fn get_guild_channels(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Channel>, DiscordError> {
        self.send(
            RouteId::GetGuildChannels,
            &format!("/guilds/{}/channels", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn create_guild_channel(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    pub async fn create_guild_channel_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::CreateGuildChannel,
            &format!("/guilds/{}/channels", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn create_guild_channel_from_request(
        &self,
        guild_id: GuildId,
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
        guild_id: GuildId,
        positions: &[ModifyGuildChannelPosition],
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::ModifyGuildChannelPositions,
            &format!("/guilds/{}/channels", guild_id),
            json_body(positions),
        )
        .await
    }

    pub async fn create_dm_channel_typed(
        &self,
        body: &CreateDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.send(RouteId::CreateDm, "/users/@me/channels", json_body(body))
            .await
    }

    pub async fn create_group_dm_channel_typed(
        &self,
        body: &CreateGroupDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.send(
            RouteId::CreateGroupDm,
            "/users/@me/channels",
            json_body(body),
        )
        .await
    }

    pub async fn add_group_dm_recipient(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
        body: &AddGroupDmRecipient,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::GroupDmAddRecipient,
            &format!("/channels/{}/recipients/{}", channel_id, user_id),
            json_body(body),
        )
        .await
    }

    pub async fn remove_group_dm_recipient(
        &self,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::GroupDmRemoveRecipient,
            &format!("/channels/{}/recipients/{}", channel_id, user_id),
            NO_BODY,
        )
        .await
    }

    pub async fn trigger_typing_indicator(
        &self,
        channel_id: ChannelId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::TriggerTypingIndicator,
            &format!("/channels/{}/typing", channel_id),
            NO_BODY,
        )
        .await
    }

    /// Creates or replaces the permission overwrite for `target` (the role or
    /// member whose ID goes in the path). `body` must carry the matching
    /// `type`.
    pub async fn edit_channel_permissions(
        &self,
        channel_id: ChannelId,
        target: OverwriteTarget,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::EditChannelPermissions,
            &format!("/channels/{}/permissions/{}", channel_id, target.id()),
            json_body(body),
        )
        .await
    }

    /// Creates or replaces the permission overwrite for `target`.
    ///
    /// Fails before sending anything when `body.kind` is not `target`'s
    /// overwrite type (Discord would otherwise apply the overwrite to the
    /// wrong kind of target or reject it); build the body with
    /// [`EditChannelPermission::for_target`] to keep them in step.
    pub async fn edit_channel_permissions_typed(
        &self,
        channel_id: ChannelId,
        target: OverwriteTarget,
        body: &EditChannelPermission,
    ) -> Result<(), DiscordError> {
        if body.kind != target.kind() {
            return Err(DiscordError::model(format!(
                "EditChannelPermission.kind {} does not match the overwrite target type {}",
                body.kind.get(),
                target.kind().get()
            )));
        }
        self.send_unit(
            RouteId::EditChannelPermissions,
            &format!("/channels/{}/permissions/{}", channel_id, target.id()),
            json_body(body),
        )
        .await
    }

    /// Deletes the permission overwrite for `target`.
    pub async fn delete_channel_permission(
        &self,
        channel_id: ChannelId,
        target: OverwriteTarget,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteChannelPermission,
            &format!("/channels/{}/permissions/{}", channel_id, target.id()),
            NO_BODY,
        )
        .await
    }

    pub async fn follow_announcement_channel(
        &self,
        channel_id: ChannelId,
        webhook_channel_id: ChannelId,
    ) -> Result<FollowedChannel, DiscordError> {
        self.send(
            RouteId::FollowAnnouncementChannel,
            &format!("/channels/{}/followers", channel_id),
            json_body(&serde_json::json!({ "webhook_channel_id": webhook_channel_id })),
        )
        .await
    }
}
