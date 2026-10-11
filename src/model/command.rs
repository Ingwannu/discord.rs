//! Application commands, command options, and command permissions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{GenericId, PermissionsBitField};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOptionChoice`.
pub struct ApplicationCommandOptionChoice {
    pub name: String,
    /// Localized `name` per locale key, 1-100 characters each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub value: serde_json::Value,
}

impl ApplicationCommandOptionChoice {
    /// Creates a `new` value.
    pub fn new(name: impl Into<String>, value: impl Serialize) -> Self {
        Self {
            name: name.into(),
            name_localizations: None,
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Creates a `try_new` value.
    pub fn try_new(
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            name_localizations: None,
            value: serde_json::to_value(value)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOption`.
pub struct ApplicationCommandOption {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    /// Localized `name` per locale key; same rules as `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    /// Localized `description` per locale key, 1-100 characters each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<bool>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(default)]
    pub choices: Vec<ApplicationCommandOptionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u16>,
    /// File types an `ATTACHMENT` option accepts: preset groups such as
    /// `image`, `video`, `audio`, or dot-prefixed extensions such as `.pdf`
    /// (max 10). Discord checks only the file name, not the contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,
    /// Channel types a `CHANNEL` option offers (raw channel type values).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_types: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationIntegrationType`.
pub struct ApplicationIntegrationType(pub u8);

impl ApplicationIntegrationType {
    /// Public API item `GUILD_INSTALL`.
    pub const GUILD_INSTALL: Self = Self(0);
    /// Public API item `USER_INSTALL`.
    pub const USER_INSTALL: Self = Self(1);
}

impl Default for ApplicationIntegrationType {
    fn default() -> Self {
        Self::GUILD_INSTALL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `InteractionContextType`.
pub struct InteractionContextType(pub u8);

impl InteractionContextType {
    /// Public API item `GUILD`.
    pub const GUILD: Self = Self(0);
    /// Public API item `BOT_DM`.
    pub const BOT_DM: Self = Self(1);
    /// Public API item `PRIVATE_CHANNEL`.
    pub const PRIVATE_CHANNEL: Self = Self(2);
}

impl Default for InteractionContextType {
    fn default() -> Self {
        Self::GUILD
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationCommandHandlerType`.
pub struct ApplicationCommandHandlerType(pub u8);

impl ApplicationCommandHandlerType {
    /// Public API item `APP_HANDLER`.
    pub const APP_HANDLER: Self = Self(1);
    /// Public API item `DISCORD_LAUNCH_ACTIVITY`.
    pub const DISCORD_LAUNCH_ACTIVITY: Self = Self(2);
}

impl Default for ApplicationCommandHandlerType {
    fn default() -> Self {
        Self::APP_HANDLER
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommand`.
pub struct ApplicationCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types: Option<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<ApplicationCommandHandlerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
}

/// A single application command permission overwrite.
///
/// Discord accepts role, user, and channel overwrites for a guild command.
/// The `id` can also be one of Discord's documented permission constants,
/// such as the guild ID for `@everyone`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationCommandPermission {
    /// Role, user, or channel ID targeted by the overwrite.
    pub id: GenericId,
    /// Discord permission target type.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Whether this target is allowed to use the command.
    pub permission: bool,
}

impl ApplicationCommandPermission {
    /// Permission overwrite target type for roles.
    pub const ROLE: u8 = 1;
    /// Permission overwrite target type for users.
    pub const USER: u8 = 2;
    /// Permission overwrite target type for channels.
    pub const CHANNEL: u8 = 3;

    /// Creates a role permission overwrite.
    pub fn role(role_id: GenericId, permission: bool) -> Self {
        Self {
            id: role_id,
            kind: Self::ROLE,
            permission,
        }
    }

    /// Creates a user permission overwrite.
    pub fn user(user_id: GenericId, permission: bool) -> Self {
        Self {
            id: user_id,
            kind: Self::USER,
            permission,
        }
    }

    /// Creates a channel permission overwrite.
    pub fn channel(channel_id: GenericId, permission: bool) -> Self {
        Self {
            id: channel_id,
            kind: Self::CHANNEL,
            permission,
        }
    }
}

/// Permissions returned for one application command in one guild.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildApplicationCommandPermissions {
    /// Command ID.
    pub id: GenericId,
    /// Application ID that owns the command.
    pub application_id: GenericId,
    /// Guild ID where the permissions apply.
    pub guild_id: GenericId,
    /// Permission overwrites attached to the command.
    #[serde(default)]
    pub permissions: Vec<ApplicationCommandPermission>,
}

/// Request body for editing a guild application command's permissions.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
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

impl ApplicationCommand {
    /// Returns the command ID when Discord has assigned one.
    pub fn id_opt(&self) -> Option<GenericId> {
        self.id
    }

    /// Returns the creation timestamp once Discord has assigned an ID.
    pub fn created_at(&self) -> Option<u64> {
        self.id_opt().map(GenericId::timestamp)
    }
}
