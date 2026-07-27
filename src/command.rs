use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::DiscordError;
use crate::model::{
    ApplicationCommand, ApplicationCommandHandlerType, ApplicationCommandOption,
    ApplicationCommandOptionChoice, ApplicationIntegrationType, InteractionContextType,
    PermissionsBitField,
};

/// Crate-internal helpers shared by the builder validation methods.
///
/// Discord counts limits in Unicode codepoints, so all length checks use
/// `chars().count()` rather than byte length.
pub(crate) mod validation {
    use crate::error::DiscordError;

    /// Number of Unicode codepoints in `value` (Discord's counting unit).
    pub(crate) fn char_count(value: &str) -> usize {
        value.chars().count()
    }

    /// Ensures `value` is `min..=max` characters long.
    pub(crate) fn ensure_len(
        field: &str,
        value: &str,
        min: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        let count = char_count(value);
        if count < min || count > max {
            return Err(DiscordError::model(format!(
                "{field} must be {min}-{max} characters, got {count}"
            )));
        }
        Ok(())
    }

    /// Ensures `value` is at most `max` characters long.
    pub(crate) fn ensure_max_len(field: &str, value: &str, max: usize) -> Result<(), DiscordError> {
        let count = char_count(value);
        if count > max {
            return Err(DiscordError::model(format!(
                "{field} must be at most {max} characters, got {count}"
            )));
        }
        Ok(())
    }

    /// Ensures a collection holds `min..=max` items.
    pub(crate) fn ensure_count(
        field: &str,
        actual: usize,
        min: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        if actual < min || actual > max {
            return Err(DiscordError::model(format!(
                "{field} must contain {min}-{max} items, got {actual}"
            )));
        }
        Ok(())
    }

    /// Ensures a collection holds at most `max` items.
    pub(crate) fn ensure_max_count(
        field: &str,
        actual: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        if actual > max {
            return Err(DiscordError::model(format!(
                "{field} must contain at most {max} items, got {actual}"
            )));
        }
        Ok(())
    }

    /// Ensures a chat-input command/option name is valid: 1-32 characters,
    /// no spaces, no uppercase ASCII.
    pub(crate) fn ensure_command_name(field: &str, value: &str) -> Result<(), DiscordError> {
        ensure_len(field, value, 1, 32)?;
        if value.contains(' ') {
            return Err(DiscordError::model(format!(
                "{field} must not contain spaces, got \"{value}\""
            )));
        }
        if value.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(DiscordError::model(format!(
                "{field} must be lowercase, got \"{value}\""
            )));
        }
        Ok(())
    }
}

/// Recursively validates a chat-input command option against Discord limits.
fn validate_option(option: &ApplicationCommandOption) -> Result<(), DiscordError> {
    let name = &option.name;
    validation::ensure_command_name(&format!("option \"{name}\" name"), name)?;
    validation::ensure_len(
        &format!("option \"{name}\" description"),
        &option.description,
        1,
        100,
    )?;
    validation::ensure_max_count(
        &format!("option \"{name}\" choices"),
        option.choices.len(),
        25,
    )?;
    for choice in &option.choices {
        validation::ensure_len(
            &format!("option \"{name}\" choice name"),
            &choice.name,
            1,
            100,
        )?;
    }
    validation::ensure_max_count(
        &format!("option \"{name}\" options"),
        option.options.len(),
        25,
    )?;
    match option.kind {
        option_type::SUB_COMMAND_GROUP => {
            for nested in &option.options {
                if nested.kind != option_type::SUB_COMMAND {
                    return Err(DiscordError::model(format!(
                        "subcommand group \"{name}\" may only contain subcommands, \
                         got option \"{}\" of type {}",
                        nested.name, nested.kind
                    )));
                }
                validate_option(nested)?;
            }
        }
        option_type::SUB_COMMAND => {
            for nested in &option.options {
                if nested.kind == option_type::SUB_COMMAND
                    || nested.kind == option_type::SUB_COMMAND_GROUP
                {
                    return Err(DiscordError::model(format!(
                        "subcommand \"{name}\" cannot contain nested subcommand \
                         or subcommand group \"{}\"",
                        nested.name
                    )));
                }
                validate_option(nested)?;
            }
        }
        _ => {
            if !option.options.is_empty() {
                return Err(DiscordError::model(format!(
                    "option \"{name}\" of type {} cannot have nested options",
                    option.kind
                )));
            }
        }
    }
    Ok(())
}

/// Public module for `command_type` APIs.
pub mod command_type {
    /// Public constant for `CHAT_INPUT`.
    pub const CHAT_INPUT: u8 = 1;
    /// Public constant for `USER`.
    pub const USER: u8 = 2;
    /// Public constant for `MESSAGE`.
    pub const MESSAGE: u8 = 3;
    /// Public constant for `PRIMARY_ENTRY_POINT`.
    pub const PRIMARY_ENTRY_POINT: u8 = 4;
}

/// Public module for `option_type` APIs.
pub mod option_type {
    /// Public constant for `SUB_COMMAND`.
    pub const SUB_COMMAND: u8 = 1;
    /// Public constant for `SUB_COMMAND_GROUP`.
    pub const SUB_COMMAND_GROUP: u8 = 2;
    /// Public constant for `STRING`.
    pub const STRING: u8 = 3;
    /// Public constant for `INTEGER`.
    pub const INTEGER: u8 = 4;
    /// Public constant for `BOOLEAN`.
    pub const BOOLEAN: u8 = 5;
    /// Public constant for `USER`.
    pub const USER: u8 = 6;
    /// Public constant for `CHANNEL`.
    pub const CHANNEL: u8 = 7;
    /// Public constant for `ROLE`.
    pub const ROLE: u8 = 8;
    /// Public constant for `MENTIONABLE`.
    pub const MENTIONABLE: u8 = 9;
    /// Public constant for `NUMBER`.
    pub const NUMBER: u8 = 10;
    /// Public constant for `ATTACHMENT`.
    pub const ATTACHMENT: u8 = 11;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandDefinition`.
pub struct CommandDefinition {
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
    pub nsfw: Option<bool>,
}

impl From<CommandDefinition> for ApplicationCommand {
    fn from(value: CommandDefinition) -> Self {
        ApplicationCommand {
            id: None,
            application_id: None,
            guild_id: None,
            kind: value.kind,
            name: value.name,
            name_localizations: value.name_localizations,
            description: value.description,
            description_localizations: value.description_localizations,
            options: value.options,
            default_member_permissions: value.default_member_permissions,
            dm_permission: value.dm_permission,
            integration_types: value.integration_types,
            contexts: value.contexts,
            handler: value.handler,
            version: None,
            nsfw: value.nsfw,
        }
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CommandOptionBuilder`.
pub struct CommandOptionBuilder {
    inner: ApplicationCommandOption,
}

impl CommandOptionBuilder {
    /// Creates a `new` value.
    pub fn new(kind: u8, name: &str, description: &str) -> Self {
        Self {
            inner: ApplicationCommandOption {
                kind,
                name: name.to_string(),
                description: description.to_string(),
                ..ApplicationCommandOption::default()
            },
        }
    }

    pub fn required(mut self, required: bool) -> Self {
        self.inner.required = Some(required);
        self
    }

    pub fn autocomplete(mut self, enabled: bool) -> Self {
        self.inner.autocomplete = Some(enabled);
        self
    }

    pub fn choice(mut self, name: &str, value: impl Serialize) -> Self {
        self.inner
            .choices
            .push(ApplicationCommandOptionChoice::new(name, value));
        self
    }

    pub fn try_choice(
        mut self,
        name: &str,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        self.inner
            .choices
            .push(ApplicationCommandOptionChoice::try_new(name, value)?);
        Ok(self)
    }

    pub fn min_value(mut self, value: f64) -> Self {
        self.inner.min_value = Some(value);
        self
    }

    pub fn max_value(mut self, value: f64) -> Self {
        self.inner.max_value = Some(value);
        self
    }

    pub fn min_length(mut self, value: u16) -> Self {
        self.inner.min_length = Some(value);
        self
    }

    pub fn max_length(mut self, value: u16) -> Self {
        self.inner.max_length = Some(value);
        self
    }

    pub fn option(mut self, option: CommandOptionBuilder) -> Self {
        self.inner.options.push(option.build());
        self
    }

    /// Creates a `subcommand` value.
    pub fn subcommand(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND, name, description)
    }

    /// Creates a `subcommand_group` value.
    pub fn subcommand_group(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND_GROUP, name, description)
    }

    /// Creates a `string` value.
    pub fn string(name: &str, description: &str) -> Self {
        Self::new(option_type::STRING, name, description)
    }

    /// Creates a `integer` value.
    pub fn integer(name: &str, description: &str) -> Self {
        Self::new(option_type::INTEGER, name, description)
    }

    /// Creates a `boolean` value.
    pub fn boolean(name: &str, description: &str) -> Self {
        Self::new(option_type::BOOLEAN, name, description)
    }

    /// Creates a `user` value.
    pub fn user(name: &str, description: &str) -> Self {
        Self::new(option_type::USER, name, description)
    }

    /// Creates a `channel` value.
    pub fn channel(name: &str, description: &str) -> Self {
        Self::new(option_type::CHANNEL, name, description)
    }

    /// Creates a `role` value.
    pub fn role(name: &str, description: &str) -> Self {
        Self::new(option_type::ROLE, name, description)
    }

    /// Creates a `mentionable` value.
    pub fn mentionable(name: &str, description: &str) -> Self {
        Self::new(option_type::MENTIONABLE, name, description)
    }

    /// Creates a `number` value.
    pub fn number(name: &str, description: &str) -> Self {
        Self::new(option_type::NUMBER, name, description)
    }

    /// Creates a `attachment` value.
    pub fn attachment(name: &str, description: &str) -> Self {
        Self::new(option_type::ATTACHMENT, name, description)
    }

    pub fn build(self) -> ApplicationCommandOption {
        self.inner
    }

    /// Validates this option (recursively) against Discord's limits without
    /// consuming the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validate_option(&self.inner)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<ApplicationCommandOption, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SlashCommandBuilder`.
pub struct SlashCommandBuilder {
    inner: CommandDefinition,
}

impl SlashCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::CHAT_INPUT,
                name: name.to_string(),
                description: description.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn option(mut self, option: CommandOptionBuilder) -> Self {
        self.inner.options.push(option.build());
        self
    }

    pub fn string_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::string(name, description).required(required))
    }

    pub fn integer_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::integer(name, description).required(required))
    }

    pub fn boolean_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::boolean(name, description).required(required))
    }

    pub fn user_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::user(name, description).required(required))
    }

    pub fn subcommand(self, option: CommandOptionBuilder) -> Self {
        self.option(option)
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn nsfw(mut self, enabled: bool) -> Self {
        self.inner.nsfw = Some(enabled);
        self
    }

    pub fn integration_types<I>(mut self, integration_types: I) -> Self
    where
        I: IntoIterator<Item = ApplicationIntegrationType>,
    {
        self.inner.integration_types = Some(integration_types.into_iter().collect());
        self
    }

    pub fn contexts<I>(mut self, contexts: I) -> Self
    where
        I: IntoIterator<Item = InteractionContextType>,
    {
        self.inner.contexts = Some(contexts.into_iter().collect());
        self
    }

    pub fn name_localization(mut self, locale: &str, name: &str) -> Self {
        self.inner
            .name_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), name.to_string());
        self
    }

    pub fn description_localization(mut self, locale: &str, description: &str) -> Self {
        self.inner
            .description_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), description.to_string());
        self
    }

    pub fn handler(mut self, handler: ApplicationCommandHandlerType) -> Self {
        self.inner.handler = Some(handler);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_command_name("command name", &self.inner.name)?;
        validation::ensure_len("command description", &self.inner.description, 1, 100)?;
        validation::ensure_max_count("command options", self.inner.options.len(), 25)?;
        for option in &self.inner.options {
            validate_option(option)?;
        }
        Ok(())
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `PrimaryEntryPointCommandBuilder`.
pub struct PrimaryEntryPointCommandBuilder {
    inner: CommandDefinition,
}

impl PrimaryEntryPointCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::PRIMARY_ENTRY_POINT,
                name: name.to_string(),
                description: description.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn integration_types<I>(mut self, integration_types: I) -> Self
    where
        I: IntoIterator<Item = ApplicationIntegrationType>,
    {
        self.inner.integration_types = Some(integration_types.into_iter().collect());
        self
    }

    pub fn contexts<I>(mut self, contexts: I) -> Self
    where
        I: IntoIterator<Item = InteractionContextType>,
    {
        self.inner.contexts = Some(contexts.into_iter().collect());
        self
    }

    pub fn name_localization(mut self, locale: &str, name: &str) -> Self {
        self.inner
            .name_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), name.to_string());
        self
    }

    pub fn description_localization(mut self, locale: &str, description: &str) -> Self {
        self.inner
            .description_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), description.to_string());
        self
    }

    pub fn handler(mut self, handler: ApplicationCommandHandlerType) -> Self {
        self.inner.handler = Some(handler);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len("command name", &self.inner.name, 1, 32)?;
        validation::ensure_len("command description", &self.inner.description, 1, 100)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `UserCommandBuilder`.
pub struct UserCommandBuilder {
    inner: CommandDefinition,
}

impl UserCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::USER,
                name: name.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder. Context-menu names may contain spaces and
    /// mixed case, so only the length is checked.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len("command name", &self.inner.name, 1, 32)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `MessageCommandBuilder`.
pub struct MessageCommandBuilder {
    inner: CommandDefinition,
}

impl MessageCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::MESSAGE,
                name: name.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder. Context-menu names may contain spaces and
    /// mixed case, so only the length is checked.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validation::ensure_len("command name", &self.inner.name, 1, 32)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        command_type, option_type, CommandOptionBuilder, MessageCommandBuilder,
        PrimaryEntryPointCommandBuilder, SlashCommandBuilder, UserCommandBuilder,
    };
    use crate::model::{
        ApplicationCommand, ApplicationCommandHandlerType, ApplicationIntegrationType,
        InteractionContextType, PermissionsBitField,
    };

    #[test]
    fn slash_command_builder_serializes_nested_options() {
        let command = SlashCommandBuilder::new("hello", "Say hello")
            .option(
                CommandOptionBuilder::new(option_type::STRING, "target", "Target user")
                    .required(true)
                    .choice("World", "world"),
            )
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], json!(command_type::CHAT_INPUT));
        assert_eq!(value["options"][0]["name"], json!("target"));
        assert_eq!(value["options"][0]["choices"][0]["value"], json!("world"));
    }

    #[test]
    fn slash_command_builder_exposes_common_option_shortcuts() {
        let command = SlashCommandBuilder::new("moderate", "Moderation command")
            .string_option("reason", "Reason", true)
            .boolean_option("silent", "Whether the reply should be hidden", false)
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["options"][0]["type"], json!(option_type::STRING));
        assert_eq!(value["options"][1]["type"], json!(option_type::BOOLEAN));
    }

    #[test]
    fn command_option_builder_shortcuts_cover_supported_types() {
        let cases = [
            (
                CommandOptionBuilder::subcommand("sub", "desc").build(),
                option_type::SUB_COMMAND,
            ),
            (
                CommandOptionBuilder::subcommand_group("group", "desc").build(),
                option_type::SUB_COMMAND_GROUP,
            ),
            (
                CommandOptionBuilder::string("string", "desc").build(),
                option_type::STRING,
            ),
            (
                CommandOptionBuilder::integer("integer", "desc").build(),
                option_type::INTEGER,
            ),
            (
                CommandOptionBuilder::boolean("boolean", "desc").build(),
                option_type::BOOLEAN,
            ),
            (
                CommandOptionBuilder::user("user", "desc").build(),
                option_type::USER,
            ),
            (
                CommandOptionBuilder::channel("channel", "desc").build(),
                option_type::CHANNEL,
            ),
            (
                CommandOptionBuilder::role("role", "desc").build(),
                option_type::ROLE,
            ),
            (
                CommandOptionBuilder::mentionable("mentionable", "desc").build(),
                option_type::MENTIONABLE,
            ),
            (
                CommandOptionBuilder::number("number", "desc").build(),
                option_type::NUMBER,
            ),
            (
                CommandOptionBuilder::attachment("attachment", "desc").build(),
                option_type::ATTACHMENT,
            ),
        ];

        for (option, expected_kind) in cases {
            assert_eq!(option.kind, expected_kind);
        }
    }

    #[test]
    fn command_option_builder_serializes_nested_constraints_and_choices() {
        let option = CommandOptionBuilder::subcommand_group("admin", "Admin tools")
            .option(
                CommandOptionBuilder::subcommand("ban", "Ban a member")
                    .option(CommandOptionBuilder::user("target", "Member").required(true))
                    .option(
                        CommandOptionBuilder::integer("days", "Delete days")
                            .autocomplete(true)
                            .min_value(1.0)
                            .max_value(7.0),
                    )
                    .option(
                        CommandOptionBuilder::string("reason", "Reason")
                            .min_length(3)
                            .max_length(120),
                    )
                    .option(CommandOptionBuilder::number("ratio", "Ratio").choice("Half", 0.5_f64))
                    .option(CommandOptionBuilder::attachment("proof", "Proof")),
            )
            .build();

        let value = serde_json::to_value(option).unwrap();
        let nested = &value["options"][0]["options"];

        assert_eq!(value["type"], json!(option_type::SUB_COMMAND_GROUP));
        assert_eq!(value["options"][0]["type"], json!(option_type::SUB_COMMAND));
        assert_eq!(nested[0]["required"], json!(true));
        assert_eq!(nested[1]["autocomplete"], json!(true));
        assert_eq!(nested[1]["min_value"], json!(1.0));
        assert_eq!(nested[1]["max_value"], json!(7.0));
        assert_eq!(nested[2]["min_length"], json!(3));
        assert_eq!(nested[2]["max_length"], json!(120));
        assert_eq!(nested[3]["choices"][0]["name"], json!("Half"));
        assert_eq!(nested[3]["choices"][0]["value"], json!(0.5));
        assert_eq!(nested[4]["type"], json!(option_type::ATTACHMENT));
    }

    #[test]
    fn command_definition_converts_into_application_command() {
        let permissions = PermissionsBitField(8);
        let command = SlashCommandBuilder::new("ban", "Ban a member")
            .integer_option("days", "Delete days", false)
            .default_member_permissions(permissions)
            .dm_permission(false)
            .nsfw(true)
            .integration_types([
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL,
            ])
            .contexts([
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM,
            ])
            .name_localization("ko", "李⑤떒")
            .description_localization("ko", "硫ㅻ쾭 李⑤떒")
            .handler(ApplicationCommandHandlerType::APP_HANDLER)
            .build();

        let application_command: ApplicationCommand = command.clone().into();

        assert_eq!(application_command.kind, command_type::CHAT_INPUT);
        assert_eq!(application_command.name, "ban");
        assert_eq!(application_command.description, "Ban a member");
        assert_eq!(application_command.options.len(), 1);
        assert_eq!(
            application_command
                .default_member_permissions
                .map(PermissionsBitField::bits),
            Some(8)
        );
        assert_eq!(application_command.dm_permission, Some(false));
        assert_eq!(application_command.nsfw, Some(true));
        assert_eq!(
            application_command.integration_types,
            Some(vec![
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL
            ])
        );
        assert_eq!(
            application_command.contexts,
            Some(vec![
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM
            ])
        );
        assert_eq!(
            application_command
                .name_localizations
                .as_ref()
                .and_then(|localizations| localizations.get("ko"))
                .map(String::as_str),
            Some("李⑤떒")
        );
        assert_eq!(
            application_command.handler,
            Some(ApplicationCommandHandlerType::APP_HANDLER)
        );
    }

    #[test]
    fn primary_entry_point_builder_serializes_activity_fields() {
        let command = PrimaryEntryPointCommandBuilder::new("launch", "Launch activity")
            .integration_types([
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL,
            ])
            .contexts([
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM,
            ])
            .handler(ApplicationCommandHandlerType::DISCORD_LAUNCH_ACTIVITY)
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], json!(command_type::PRIMARY_ENTRY_POINT));
        assert_eq!(value["integration_types"], json!([0, 1]));
        assert_eq!(value["contexts"], json!([0, 1]));
        assert_eq!(value["handler"], json!(2));
    }

    fn expect_model_error<T: std::fmt::Debug>(
        result: Result<T, crate::error::DiscordError>,
        needle: &str,
    ) {
        let err = result.expect_err("expected validation failure").to_string();
        assert!(
            err.contains(needle),
            "error message {err:?} should contain {needle:?}"
        );
    }

    #[test]
    fn slash_command_try_build_matches_build_for_valid_command() {
        let make = || {
            SlashCommandBuilder::new("hello", "Say hello").option(
                CommandOptionBuilder::string("target", "Target user")
                    .required(true)
                    .choice("World", "world"),
            )
        };

        let built = serde_json::to_value(make().build()).unwrap();
        let validated = serde_json::to_value(make().try_build().expect("valid command")).unwrap();
        assert_eq!(built, validated);
    }

    #[test]
    fn slash_command_validate_accepts_multibyte_names_by_codepoint_count() {
        // 32 codepoints but 64 UTF-8 bytes: must pass because Discord counts codepoints.
        let name = "é".repeat(32);
        SlashCommandBuilder::new(&name, "desc")
            .validate()
            .expect("codepoint-length name should be valid");
    }

    #[test]
    fn slash_command_try_build_rejects_invalid_names() {
        expect_model_error(
            SlashCommandBuilder::new(&"a".repeat(45), "desc").try_build(),
            "command name must be 1-32 characters, got 45",
        );
        expect_model_error(
            SlashCommandBuilder::new("Hello", "desc").try_build(),
            "command name must be lowercase",
        );
        expect_model_error(
            SlashCommandBuilder::new("hi there", "desc").try_build(),
            "command name must not contain spaces",
        );
    }

    #[test]
    fn slash_command_try_build_rejects_bad_description_and_option_overflow() {
        expect_model_error(
            SlashCommandBuilder::new("hello", "").try_build(),
            "command description must be 1-100 characters, got 0",
        );
        expect_model_error(
            SlashCommandBuilder::new("hello", &"d".repeat(101)).try_build(),
            "command description must be 1-100 characters, got 101",
        );

        let mut command = SlashCommandBuilder::new("hello", "desc");
        for index in 0..26 {
            command = command.string_option(&format!("opt{index}"), "desc", false);
        }
        expect_model_error(
            command.try_build(),
            "command options must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn command_option_try_build_rejects_invalid_fields() {
        expect_model_error(
            CommandOptionBuilder::string("name", &"d".repeat(101)).try_build(),
            "option \"name\" description must be 1-100 characters, got 101",
        );
        expect_model_error(
            CommandOptionBuilder::string("Name", "desc").try_build(),
            "option \"Name\" name must be lowercase",
        );
        expect_model_error(
            CommandOptionBuilder::string("choices", "desc")
                .choice(&"c".repeat(101), 1)
                .try_build(),
            "option \"choices\" choice name must be 1-100 characters, got 101",
        );

        let mut option = CommandOptionBuilder::string("choices", "desc");
        for index in 0..26 {
            option = option.choice(&format!("choice{index}"), index);
        }
        expect_model_error(
            option.try_build(),
            "option \"choices\" choices must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn command_option_try_build_enforces_subcommand_nesting_rules() {
        expect_model_error(
            CommandOptionBuilder::subcommand("outer", "desc")
                .option(CommandOptionBuilder::subcommand("inner", "desc"))
                .try_build(),
            "subcommand \"outer\" cannot contain nested subcommand",
        );
        expect_model_error(
            CommandOptionBuilder::subcommand_group("group", "desc")
                .option(CommandOptionBuilder::string("plain", "desc"))
                .try_build(),
            "subcommand group \"group\" may only contain subcommands",
        );
        expect_model_error(
            CommandOptionBuilder::string("plain", "desc")
                .option(CommandOptionBuilder::string("nested", "desc"))
                .try_build(),
            "option \"plain\" of type 3 cannot have nested options",
        );

        // Group -> subcommand -> basic option is the valid shape.
        CommandOptionBuilder::subcommand_group("group", "desc")
            .option(
                CommandOptionBuilder::subcommand("sub", "desc")
                    .option(CommandOptionBuilder::string("plain", "desc")),
            )
            .try_build()
            .expect("valid nesting should build");
    }

    #[test]
    fn context_menu_and_entry_point_builders_validate_names() {
        let valid = UserCommandBuilder::new("Inspect Member").try_build().unwrap();
        assert_eq!(valid.name, "Inspect Member");

        expect_model_error(
            UserCommandBuilder::new(&"n".repeat(33)).try_build(),
            "command name must be 1-32 characters, got 33",
        );
        expect_model_error(
            MessageCommandBuilder::new("").try_build(),
            "command name must be 1-32 characters, got 0",
        );
        expect_model_error(
            PrimaryEntryPointCommandBuilder::new("launch", "").try_build(),
            "command description must be 1-100 characters, got 0",
        );
        PrimaryEntryPointCommandBuilder::new("launch", "Launch activity")
            .try_build()
            .expect("valid entry point command");
    }

    #[test]
    fn user_and_message_command_builders_apply_command_kinds_and_permissions() {
        let user_command = UserCommandBuilder::new("Inspect")
            .default_member_permissions(PermissionsBitField(16))
            .dm_permission(true)
            .build();
        let message_command = MessageCommandBuilder::new("Quote")
            .default_member_permissions(PermissionsBitField(32))
            .dm_permission(false)
            .build();

        let user_value = serde_json::to_value(user_command).unwrap();
        let message_value = serde_json::to_value(message_command).unwrap();

        assert_eq!(user_value["type"], json!(command_type::USER));
        assert_eq!(user_value["default_member_permissions"], json!("16"));
        assert_eq!(user_value["dm_permission"], json!(true));
        assert_eq!(message_value["type"], json!(command_type::MESSAGE));
        assert_eq!(message_value["default_member_permissions"], json!("32"));
        assert_eq!(message_value["dm_permission"], json!(false));
    }
}
