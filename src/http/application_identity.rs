//! Application Identity Profile routes (Game Stats Widgets).
//!
//! Every route requires the bot token of the application in the path and a
//! target user who authorized the app with the `application_identities.write`
//! OAuth2 scope.

use url::form_urlencoded;

use crate::error::DiscordError;
use crate::model::{
    ApplicationIdentities, ApplicationIdentityProfile, ApplicationIdentityProviderId,
    UpdateApplicationIdentityProfile, UserId,
};

use super::paths::external_path_segment;
use super::{configured_application_id, json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    /// Writes game stats to the user's Application Identity
    /// (`PATCH /applications/{app}/users/{user}/identities/{external_id}/profile`).
    ///
    /// The first write for a user creates a profile-only identity (provider
    /// type `NONE`) and Discord returns the profile; later writes return
    /// `204 No Content`, mapped to `Ok(None)`. `body.data` replaces the
    /// stored stats entirely. The body is checked against Discord's
    /// documented limits before sending.
    pub async fn update_application_identity_profile(
        &self,
        user_id: UserId,
        provider_issued_user_id: &str,
        body: &UpdateApplicationIdentityProfile,
    ) -> Result<Option<ApplicationIdentityProfile>, DiscordError> {
        body.validate()?;
        let path = self.identity_profile_path(user_id, provider_issued_user_id)?;
        self.send_optional(
            RouteId::UpdateApplicationIdentityProfile,
            &path,
            json_body(body),
        )
        .await
    }

    /// Reads the profile stored on the user's matching Application Identity.
    pub async fn get_application_identity_profile(
        &self,
        user_id: UserId,
        provider_issued_user_id: &str,
    ) -> Result<ApplicationIdentityProfile, DiscordError> {
        let path = self.identity_profile_path(user_id, provider_issued_user_id)?;
        self.send(RouteId::GetApplicationIdentityProfile, &path, NO_BODY)
            .await
    }

    /// Lists the user's Application Identities for this application, to
    /// discover the exact external ID values (no profile data).
    pub async fn get_user_application_identities(
        &self,
        user_id: UserId,
    ) -> Result<ApplicationIdentities, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetApplicationIdentitiesByUserId,
            &format!("/users/{}/application-identities/{application_id}", user_id),
            NO_BODY,
        )
        .await
    }

    /// Resolves the Discord user behind an external account
    /// (`provider_type` + `provider_issued_user_id`, optionally
    /// `provider_id`). Returns an empty list when nothing matches.
    pub async fn get_application_identities_by_external_id(
        &self,
        provider_type: &str,
        provider_issued_user_id: &str,
        provider_id: Option<&str>,
    ) -> Result<ApplicationIdentities, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let provider_type = external_path_segment("provider_type", provider_type)?;
        let external_id =
            external_path_segment("provider_issued_user_id", provider_issued_user_id)?;
        let query = provider_id
            .map(|provider_id| {
                let mut serializer = form_urlencoded::Serializer::new(String::new());
                serializer.append_pair("provider_id", provider_id);
                format!("?{}", serializer.finish())
            })
            .unwrap_or_default();
        self.send(RouteId::GetApplicationIdentitiesByExternalId, &format!(
                "/applications/{application_id}/application-identities/{provider_type}/{external_id}{query}"
            ), NO_BODY)
        .await
    }

    /// Deletes one Application Identity and its profile data. Discord
    /// blocks deleting the user's last account-linking identity; profile-only
    /// `NONE` identities can always be deleted.
    pub async fn delete_application_identity(
        &self,
        user_id: UserId,
        provider_type: &str,
        provider_issued_user_id: &str,
        provider_id: Option<&str>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let provider_type = external_path_segment("provider_type", provider_type)?;
        let external_id =
            external_path_segment("provider_issued_user_id", provider_issued_user_id)?;
        let body = ApplicationIdentityProviderId {
            provider_id: provider_id.map(str::to_string),
        };
        self.send_unit(RouteId::DeleteApplicationIdentity, &format!(
                "/users/{}/application-identities/{application_id}/{provider_type}/{external_id}/delete",
                user_id
            ), json_body(&body))
        .await
    }

    fn identity_profile_path(
        &self,
        user_id: UserId,
        provider_issued_user_id: &str,
    ) -> Result<String, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let external_id =
            external_path_segment("provider_issued_user_id", provider_issued_user_id)?;
        Ok(format!(
            "/applications/{application_id}/users/{user_id}/identities/{external_id}/profile"
        ))
    }
}
