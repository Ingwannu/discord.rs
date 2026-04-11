use serde::{Deserialize, Serialize};

use crate::model::{
    ApplicationCommand, ApplicationCommandOption, ApplicationCommandOptionChoice,
    PermissionsBitField,
};

pub mod command_type {
    pub const CHAT_INPUT: u8 = 1;
    pub const USER: u8 = 2;
    pub const MESSAGE: u8 = 3;
}

pub mod option_type {
    pub const SUB_COMMAND: u8 = 1;
    pub const SUB_COMMAND_GROUP: u8 = 2;
    pub const STRING: u8 = 3;
    pub const INTEGER: u8 = 4;
    pub const BOOLEAN: u8 = 5;
    pub const USER: u8 = 6;
    pub const CHANNEL: u8 = 7;
    pub const ROLE: u8 = 8;
    pub const MENTIONABLE: u8 = 9;
    pub const NUMBER: u8 = 10;
    pub const ATTACHMENT: u8 = 11;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CommandDefinition {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
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
            description: value.description,
            options: value.options,
            default_member_permissions: value.default_member_permissions,
            dm_permission: value.dm_permission,
            nsfw: value.nsfw,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandOptionBuilder {
    inner: ApplicationCommandOption,
}

impl CommandOptionBuilder {
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
        self.inner.choices.push(ApplicationCommandOptionChoice {
            name: name.to_string(),
            value: serde_json::to_value(value).expect("failed to serialize command option choice"),
        });
        self
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

    pub fn subcommand(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND, name, description)
    }

    pub fn subcommand_group(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND_GROUP, name, description)
    }

    pub fn string(name: &str, description: &str) -> Self {
        Self::new(option_type::STRING, name, description)
    }

    pub fn integer(name: &str, description: &str) -> Self {
        Self::new(option_type::INTEGER, name, description)
    }

    pub fn boolean(name: &str, description: &str) -> Self {
        Self::new(option_type::BOOLEAN, name, description)
    }

    pub fn user(name: &str, description: &str) -> Self {
        Self::new(option_type::USER, name, description)
    }

    pub fn channel(name: &str, description: &str) -> Self {
        Self::new(option_type::CHANNEL, name, description)
    }

    pub fn role(name: &str, description: &str) -> Self {
        Self::new(option_type::ROLE, name, description)
    }

    pub fn mentionable(name: &str, description: &str) -> Self {
        Self::new(option_type::MENTIONABLE, name, description)
    }

    pub fn number(name: &str, description: &str) -> Self {
        Self::new(option_type::NUMBER, name, description)
    }

    pub fn attachment(name: &str, description: &str) -> Self {
        Self::new(option_type::ATTACHMENT, name, description)
    }

    pub fn build(self) -> ApplicationCommandOption {
        self.inner
    }
}

#[derive(Clone, Debug, Default)]
pub struct SlashCommandBuilder {
    inner: CommandDefinition,
}

impl SlashCommandBuilder {
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

    pub fn build(self) -> CommandDefinition {
        self.inner
    }
}

#[derive(Clone, Debug, Default)]
pub struct UserCommandBuilder {
    inner: CommandDefinition,
}

impl UserCommandBuilder {
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
}

#[derive(Clone, Debug, Default)]
pub struct MessageCommandBuilder {
    inner: CommandDefinition,
}

impl MessageCommandBuilder {
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
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{command_type, option_type, CommandOptionBuilder, SlashCommandBuilder};

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
}
