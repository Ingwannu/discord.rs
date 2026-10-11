//! Current-application endpoints: application, activity instances, and role
//! connection metadata.

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ActivityInstance, Application, ApplicationRoleConnectionMetadata, ModifyCurrentApplication,
};

use super::paths::{configured_application_id, validate_token_path_segment};
use super::RestClient;

impl RestClient {
    pub async fn get_current_application(&self) -> Result<Application, DiscordError> {
        self.request_typed(Method::GET, "/applications/@me", Option::<&Value>::None)
            .await
    }

    pub async fn edit_current_application<B>(&self, body: &B) -> Result<Application, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(Method::PATCH, "/applications/@me", Some(body))
            .await
    }

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
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/activity-instances/{instance_id}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_application_role_connection_metadata_records(
        &self,
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/role-connections/metadata"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_application_role_connection_metadata_records(
        &self,
        records: &[ApplicationRoleConnectionMetadata],
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PUT,
            &format!("/applications/{application_id}/role-connections/metadata"),
            Some(records),
        )
        .await
    }
}
