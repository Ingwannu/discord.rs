//! Current-user request bodies.
//!
//! Moved from `model` in 3.0 checkpoint 5C; `model::X` remains as a
//! deprecated alias until 4.0.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Default)]
/// Request body for modifying the current bot user.
pub struct ModifyCurrentUser {
    /// New username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// New avatar image data, or `None` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// New banner image data, or `None` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
/// Request body for updating the current user's application role connection.
pub struct UpdateUserApplicationRoleConnection {
    /// Vanity platform name shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_name: Option<String>,
    /// Platform username shown in linked-role UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_username: Option<String>,
    /// Metadata values keyed by application role connection metadata key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl UpdateUserApplicationRoleConnection {
    /// Creates an empty update body.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the platform name.
    pub fn platform_name(mut self, platform_name: impl Into<String>) -> Self {
        self.platform_name = Some(platform_name.into());
        self
    }

    /// Sets the platform username.
    pub fn platform_username(mut self, platform_username: impl Into<String>) -> Self {
        self.platform_username = Some(platform_username.into());
        self
    }

    /// Sets the metadata map.
    pub fn metadata<I, K, V>(mut self, metadata: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.metadata = Some(
            metadata
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        );
        self
    }
}
