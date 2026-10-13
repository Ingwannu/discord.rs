//! Current-application endpoints: application, activity instances, and role
//! connection metadata.

use crate::error::DiscordError;
use crate::model::{ActivityInstance, Application, ApplicationRoleConnectionMetadata};
use crate::request::ModifyCurrentApplication;

use super::paths::{configured_application_id, validate_token_path_segment};
use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_current_application(&self) -> Result<Application, DiscordError> {
        self.send(RouteId::GetCurrentApplication, "/applications/@me", NO_BODY)
            .await
    }

    pub async fn edit_current_application(
        &self,
        body: &ModifyCurrentApplication,
    ) -> Result<Application, DiscordError> {
        self.send(
            RouteId::EditCurrentApplication,
            "/applications/@me",
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `edit_current_application`")]
    pub async fn edit_current_application_from_request(
        &self,
        body: &ModifyCurrentApplication,
    ) -> Result<Application, DiscordError> {
        self.edit_current_application(body).await
    }

    pub async fn get_application_activity_instance(
        &self,
        instance_id: &str,
    ) -> Result<ActivityInstance, DiscordError> {
        validate_token_path_segment("instance_id", instance_id, false)?;
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetApplicationActivityInstance,
            &format!("/applications/{application_id}/activity-instances/{instance_id}"),
            NO_BODY,
        )
        .await
    }

    pub async fn get_application_role_connection_metadata_records(
        &self,
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetApplicationRoleConnectionMetadataRecords,
            &format!("/applications/{application_id}/role-connections/metadata"),
            NO_BODY,
        )
        .await
    }

    pub async fn update_application_role_connection_metadata_records(
        &self,
        records: &[ApplicationRoleConnectionMetadata],
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::UpdateApplicationRoleConnectionMetadataRecords,
            &format!("/applications/{application_id}/role-connections/metadata"),
            json_body(records),
        )
        .await
    }
}
