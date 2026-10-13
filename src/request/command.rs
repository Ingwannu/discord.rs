//! Application command request bodies (3.0 phase 5 B).
//!
//! Creating and overwriting commands takes
//! [`CommandDefinition`](crate::command::CommandDefinition), which the
//! command builders produce; this module holds the partial edit and
//! [`EditApplicationCommandPermissions`] (moved from `model` in 5C).

use std::collections::HashMap;

use serde::Serialize;

use super::fields::request_setters;
use crate::bitfield::Permissions;
use crate::model::{
    ApplicationCommandHandlerType, ApplicationCommandOption, ApplicationCommandPermission,
    ApplicationIntegrationType, InteractionContextType, NullablePatch, Patch,
};

/// Body of Edit Global / Guild Application Command
/// (`PATCH /applications/{id}[/guilds/{id}]/commands/{id}`). Every field is
/// optional; an omitted field keeps the command's current value. The command
/// type cannot change.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct EditApplicationCommand {
    /// 1-32 characters.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name_localizations: NullablePatch<HashMap<String, String>>,
    /// 1-100 characters (chat input commands).
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub description: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub description_localizations: NullablePatch<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub options: Patch<Vec<ApplicationCommandOption>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_member_permissions: NullablePatch<Permissions>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub integration_types: Patch<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub contexts: Patch<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub nsfw: Patch<bool>,
    /// Primary entry point commands only.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub handler: Patch<ApplicationCommandHandlerType>,
}

impl EditApplicationCommand {
    /// An edit that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        nullable name_localizations / clear_name_localizations: HashMap<String, String>;
        patch description: String;
        nullable description_localizations / clear_description_localizations: HashMap<String, String>;
        patch options: Vec<ApplicationCommandOption>;
        nullable default_member_permissions / clear_default_member_permissions: Permissions;
        patch integration_types: Vec<ApplicationIntegrationType>;
        patch contexts: Vec<InteractionContextType>;
        patch nsfw: bool;
        patch handler: ApplicationCommandHandlerType;
    }
}

/// Request body for editing a guild application command's permissions.
#[derive(Clone, Debug, Serialize, Default, PartialEq, Eq)]
pub struct EditApplicationCommandPermissions {
    /// Permission overwrites to replace on the command.
    pub permissions: Vec<ApplicationCommandPermission>,
}

impl EditApplicationCommandPermissions {
    /// Creates a permission edit body from an iterator of overwrites.
    pub fn new(permissions: impl IntoIterator<Item = ApplicationCommandPermission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }
}
