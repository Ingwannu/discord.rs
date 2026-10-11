//! Invite endpoints, including channel invites and invite target users.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateChannelInvite, Invite, InviteTargetUserIds, InviteTargetUsersJobStatus, Snowflake,
};
use crate::types::invalid_data_error;

use super::body::{named_file_multipart_body, parse_body_value, payload_named_file_multipart_body};
use super::paths::{invite_query, validate_token_path_segment};
use super::{FileAttachment, RequestAuthorization, RestClient};

/// Discord's per-request cap for invite target-user bulk add/delete.
const MAX_INVITE_TARGET_USERS: usize = 1000;

impl RestClient {
    pub async fn get_guild_invites(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Invite>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/invites", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite(&self, code: &str) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!("/invites/{code}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite_with_options(
        &self,
        code: &str,
        with_counts: Option<bool>,
        with_expiration: Option<bool>,
        guild_scheduled_event_id: Option<Snowflake>,
    ) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!(
                "/invites/{code}{}",
                invite_query(with_counts, with_expiration, guild_scheduled_event_id)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn delete_invite(&self, code: &str) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::DELETE,
            &format!("/invites/{code}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite_target_users(&self, code: &str) -> Result<Vec<u8>, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_bytes_with_authorization(
            RequestAuthorization::Auto,
            Method::GET,
            &format!("/invites/{code}/target-users"),
        )
        .await
    }

    pub async fn get_invite_target_users_csv(&self, code: &str) -> Result<String, DiscordError> {
        String::from_utf8(self.get_invite_target_users(code).await?)
            .map_err(|_| invalid_data_error("invite target users response was not valid UTF-8"))
    }

    pub async fn update_invite_target_users(
        &self,
        code: &str,
        target_users_file: &FileAttachment,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_with_headers(
            Method::PUT,
            &format!("/invites/{code}/target-users"),
            Some(named_file_multipart_body(
                "target_users_file",
                target_users_file,
            )),
        )
        .await?;
        Ok(())
    }

    /// Adds one user to an invite's target users (`PUT
    /// /invites/{code}/target-users/{user.id}`). Requires being the inviter
    /// or `MANAGE_GUILD`.
    pub async fn add_invite_target_user(
        &self,
        code: &str,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_no_content(
            Method::PUT,
            &format!("/invites/{code}/target-users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Removes one user from an invite's target users.
    pub async fn remove_invite_target_user(
        &self,
        code: &str,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_no_content(
            Method::DELETE,
            &format!("/invites/{code}/target-users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Adds up to 1000 users to an invite's target users in one request,
    /// without rebuilding the whole list from a CSV.
    pub async fn bulk_add_invite_target_users(
        &self,
        code: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        self.bulk_invite_target_users(code, "bulk-add", user_ids)
            .await
    }

    /// Removes up to 1000 users from an invite's target users in one request.
    pub async fn bulk_delete_invite_target_users(
        &self,
        code: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        self.bulk_invite_target_users(code, "bulk-delete", user_ids)
            .await
    }

    async fn bulk_invite_target_users(
        &self,
        code: &str,
        action: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        if user_ids.len() > MAX_INVITE_TARGET_USERS {
            return Err(invalid_data_error(format!(
                "invite target-user {action} accepts at most {MAX_INVITE_TARGET_USERS} users, got {}",
                user_ids.len()
            )));
        }
        let body = InviteTargetUserIds {
            user_ids: user_ids.to_vec(),
        };
        self.request_no_content(
            Method::POST,
            &format!("/invites/{code}/target-users/{action}"),
            Some(&body),
        )
        .await
    }

    pub async fn get_invite_target_users_job_status(
        &self,
        code: &str,
    ) -> Result<InviteTargetUsersJobStatus, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!("/invites/{code}/target-users/job-status"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_channel_invite(
        &self,
        channel_id: impl Into<Snowflake>,
        body: Option<&Value>,
    ) -> Result<Invite, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/invites", channel_id.into()),
            body,
        )
        .await
    }

    pub async fn get_channel_invites(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Invite>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/invites", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_channel_invite_typed(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateChannelInvite,
    ) -> Result<Invite, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/invites", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_channel_invite_with_target_users_file(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateChannelInvite,
        target_users_file: &FileAttachment,
    ) -> Result<Invite, DiscordError> {
        let response = self
            .request_with_headers(
                Method::POST,
                &format!("/channels/{}/invites", channel_id.into()),
                Some(payload_named_file_multipart_body(
                    body,
                    "target_users_file",
                    target_users_file,
                )?),
            )
            .await?;
        let value = parse_body_value(response.body);
        serde_json::from_value(value).map_err(Into::into)
    }
}
