//! Application request bodies: Modify Current Application and Update Application Identity Profile.
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use crate::error::DiscordError;
use crate::model::{
    ApplicationEventWebhookStatus, ApplicationIdentityProfileData, ApplicationInstallParams,
    ApplicationIntegrationTypeConfig, DynamicProfileField, MAX_PROFILE_DATA_BYTES,
    MAX_PROFILE_DYNAMIC_FIELDS, MAX_PROFILE_STRING_CHARS, MAX_PROFILE_USERNAME_CHARS,
};
use serde::Serialize;
use std::collections::HashMap;

/// Request body for editing the current application.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<ApplicationInstallParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, ApplicationIntegrationTypeConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<ApplicationEventWebhookStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
}

/// JSON body for Update Application Identity Profile.
///
/// `data` is **fully replaced** on every update: fields left out of a sent
/// `data` are deleted. Leave `data` as `None` to keep the stored stats.
#[derive(Clone, Debug, Serialize, Default, PartialEq)]
pub struct UpdateApplicationIdentityProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ApplicationIdentityProfileData>,
}

impl UpdateApplicationIdentityProfile {
    /// Checks the documented limits: 10 KB serialized `data`, 30 dynamic
    /// fields, 100-character primary string values and dynamic field names,
    /// and a 1024-character username.
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(username) = &self.username {
            ensure_chars("profile username", username, MAX_PROFILE_USERNAME_CHARS)?;
        }
        let Some(data) = &self.data else {
            return Ok(());
        };
        if data.dynamic.len() > MAX_PROFILE_DYNAMIC_FIELDS {
            return Err(DiscordError::model(format!(
                "profile data may have at most {MAX_PROFILE_DYNAMIC_FIELDS} dynamic fields, got {}",
                data.dynamic.len()
            )));
        }
        if let Some(primary) = &data.primary {
            for (field, value) in primary.string_fields() {
                if let Some(value) = value {
                    ensure_chars(
                        &format!("profile primary {field}"),
                        value,
                        MAX_PROFILE_STRING_CHARS,
                    )?;
                }
            }
        }
        for field in &data.dynamic {
            if let Some(name) = field.name() {
                ensure_chars(
                    "profile dynamic field name",
                    &name,
                    MAX_PROFILE_STRING_CHARS,
                )?;
            }
            if let DynamicProfileField::String { value, .. } = field {
                ensure_chars(
                    "profile dynamic string value",
                    value,
                    MAX_PROFILE_STRING_CHARS,
                )?;
            }
        }
        let size = serde_json::to_vec(data)?.len();
        if size > MAX_PROFILE_DATA_BYTES {
            return Err(DiscordError::model(format!(
                "serialized profile data must be at most {MAX_PROFILE_DATA_BYTES} bytes, got {size}"
            )));
        }
        Ok(())
    }
}

fn ensure_chars(field: &str, value: &str, max: usize) -> Result<(), DiscordError> {
    let count = value.chars().count();
    if count > max {
        return Err(DiscordError::model(format!(
            "{field} must be at most {max} characters, got {count}"
        )));
    }
    Ok(())
}
