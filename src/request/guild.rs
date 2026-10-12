//! Guild template request bodies (3.0 phase 5 B).
//!
//! The other guild bodies (`ModifyGuild`, `ModifyGuildWelcomeScreen`, ...)
//! are still in [`crate::model`]; they move here in checkpoint 5C.

use serde::Serialize;

use super::fields::request_setters;
use crate::model::{NullablePatch, Patch};

/// Body of Create Guild Template (`POST /guilds/{id}/templates`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateGuildTemplate {
    /// 1-100 characters.
    pub name: String,
    /// 0-120 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl CreateGuildTemplate {
    /// A template named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
        }
    }

    request_setters! {
        opt description: String;
    }
}

/// Body of Modify Guild Template (`PATCH /guilds/{id}/templates/{code}`).
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyGuildTemplate {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub description: NullablePatch<String>,
}

impl ModifyGuildTemplate {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        nullable description / clear_description: String;
    }
}
