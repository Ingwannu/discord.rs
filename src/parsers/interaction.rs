use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ApplicationId, AutocompleteInteraction, ChannelId, ChatInputCommandInteraction, CommandId,
    CommandInteractionData, ComponentInteraction, ComponentInteractionData, GuildId, Interaction,
    InteractionContextData, InteractionId, InteractionResponseState, MessageContextMenuInteraction,
    ModalSubmitInteraction, PingInteraction, UserContextMenuInteraction, UserId,
};
use crate::types::invalid_data_error;

use super::modal::{parse_modal_submission, V2ModalSubmission};
use super::{
    optional_id_field, optional_string_field, optional_string_values_field, required_id_field,
    required_object_field, required_string_field, required_u8_field, value_to_u8,
};

#[derive(Clone, Debug)]
/// Typed Discord API enum for `RawInteraction`.
pub enum RawInteraction {
    /// Discord API enum variant `Ping`.
    Ping,
    /// Discord API enum variant `Command`.
    Command {
        /// Command ID (`data.id`) from the raw command payload.
        id: Option<CommandId>,
        /// Command name from the raw command payload.
        name: Option<String>,
        /// Discord command type value.
        command_type: Option<u8>,
        /// Raw interaction data object.
        data: Value,
    },
    /// Discord API enum variant `Autocomplete`.
    Autocomplete {
        /// Command ID (`data.id`) from the raw autocomplete payload.
        id: Option<CommandId>,
        /// Command name from the raw autocomplete payload.
        name: Option<String>,
        /// Discord command type value.
        command_type: Option<u8>,
        /// Raw interaction data object.
        data: Value,
    },
    /// Discord API enum variant `Component`.
    Component {
        /// Component custom ID.
        custom_id: Option<String>,
        /// Discord component type value.
        component_type: Option<u8>,
        /// Submitted select values.
        values: Vec<String>,
        /// Raw interaction data object.
        data: Value,
    },
    /// Discord API enum variant `ModalSubmit`.
    ModalSubmit(V2ModalSubmission),
}

#[derive(Clone)]
/// Typed Discord API object for `InteractionContext`.
///
/// The raw-handler context carries no shared response state: helpers that
/// take its id and token are stateless (see [`crate::helpers`]).
pub struct InteractionContext {
    pub id: InteractionId,
    /// Interaction token. Redacted from `Debug` output, including inside
    /// [`Self::raw`].
    pub token: String,
    pub application_id: ApplicationId,
    pub guild_id: Option<GuildId>,
    pub channel_id: Option<ChannelId>,
    pub user_id: Option<UserId>,
    /// Full interaction payload as received, including the token.
    pub raw: Value,
}

/// Prints every field with the token replaced by `[redacted]`, both the
/// `token` field and every `token` key inside `raw`.
///
/// Raw handlers commonly log the context with `{:?}`; the token would let
/// anyone who reads those logs answer or edit the interaction for 15 minutes.
impl std::fmt::Debug for InteractionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            id,
            token: _,
            application_id,
            guild_id,
            channel_id,
            user_id,
            raw,
        } = self;
        f.debug_struct("InteractionContext")
            .field("id", id)
            .field("token", &format_args!("[redacted]"))
            .field("application_id", application_id)
            .field("guild_id", guild_id)
            .field("channel_id", channel_id)
            .field("user_id", user_id)
            .field("raw", &RedactTokens(raw))
            .finish()
    }
}

/// `Debug` view of a JSON value that prints `[redacted]` for the value of
/// every object key named `token`, at any depth.
///
/// Walks the value instead of cloning and editing it, so formatting a large
/// payload allocates nothing extra. Only the key name is matched: Discord
/// interaction payloads carry the interaction token at the top level, and a
/// nested `token` key is never useful in a log either.
struct RedactTokens<'a>(&'a Value);

impl std::fmt::Debug for RedactTokens<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Value::Object(object) => {
                let mut map = f.debug_map();
                for (key, value) in object {
                    if key == "token" {
                        map.entry(key, &format_args!("[redacted]"));
                    } else {
                        map.entry(key, &RedactTokens(value));
                    }
                }
                map.finish()
            }
            Value::Array(values) => f
                .debug_list()
                .entries(values.iter().map(RedactTokens))
                .finish(),
            scalar => scalar.fmt(f),
        }
    }
}

/// Provides the `parse_interaction_context` helper.
pub fn parse_interaction_context(raw: &Value) -> Result<InteractionContext, DiscordError> {
    let user_id = match raw.get("member").and_then(|member| member.get("user")) {
        Some(user) => optional_id_field(user, "id", "interaction.member.user")?,
        None => match raw.get("user") {
            Some(user) => optional_id_field(user, "id", "interaction.user")?,
            None => None,
        },
    };

    Ok(InteractionContext {
        id: required_id_field(raw, "id", "interaction")?,
        token: required_string_field(raw, "token", "interaction")?,
        application_id: required_id_field(raw, "application_id", "interaction")?,
        guild_id: optional_id_field(raw, "guild_id", "interaction")?,
        channel_id: optional_id_field(raw, "channel_id", "interaction")?,
        user_id,
        raw: raw.clone(),
    })
}

/// Provides the `parse_raw_interaction` helper.
pub fn parse_raw_interaction(raw: &Value) -> Result<RawInteraction, DiscordError> {
    let interaction_type = required_u8_field(raw, "type", "interaction")?;

    match interaction_type {
        1 => Ok(RawInteraction::Ping),
        2 => {
            let data = required_object_field(raw, "data", "interaction")?.clone();
            Ok(RawInteraction::Command {
                id: optional_id_field(&data, "id", "interaction.data")?,
                name: optional_string_field(&data, "name"),
                command_type: data.get("type").and_then(value_to_u8),
                data,
            })
        }
        4 => {
            let data = required_object_field(raw, "data", "interaction")?.clone();
            Ok(RawInteraction::Autocomplete {
                id: optional_id_field(&data, "id", "interaction.data")?,
                name: optional_string_field(&data, "name"),
                command_type: data.get("type").and_then(value_to_u8),
                data,
            })
        }
        3 => {
            let data = required_object_field(raw, "data", "interaction")?.clone();
            let values = optional_string_values_field(&data, "values", "component_data")?
                .unwrap_or_default();

            Ok(RawInteraction::Component {
                custom_id: optional_string_field(&data, "custom_id"),
                component_type: data.get("component_type").and_then(value_to_u8),
                values,
                data,
            })
        }
        5 => Ok(RawInteraction::ModalSubmit(parse_modal_submission(raw)?)),
        _ => Err(invalid_data_error(format!(
            "unsupported interaction type: {interaction_type}"
        ))),
    }
}

/// Provides the `parse_interaction` helper.
pub fn parse_interaction(raw: &Value) -> Result<Interaction, DiscordError> {
    let interaction_type = required_u8_field(raw, "type", "interaction")?;
    let context = parse_typed_interaction_context(raw)?;

    let interaction = match interaction_type {
        1 => Interaction::Ping(PingInteraction { context }),
        2 => {
            let data = parse_command_interaction_data(raw)?;
            match data.kind.unwrap_or(1) {
                1 => Interaction::ChatInputCommand(ChatInputCommandInteraction { context, data }),
                2 => Interaction::UserContextMenu(UserContextMenuInteraction { context, data }),
                3 => {
                    Interaction::MessageContextMenu(MessageContextMenuInteraction { context, data })
                }
                kind => Interaction::Unknown {
                    context,
                    kind,
                    raw_data: raw.get("data").cloned().unwrap_or(Value::Null),
                },
            }
        }
        3 => Interaction::Component(ComponentInteraction {
            context,
            data: parse_component_interaction_data(raw)?,
        }),
        4 => Interaction::Autocomplete(AutocompleteInteraction {
            context,
            data: parse_command_interaction_data(raw)?,
        }),
        5 => Interaction::ModalSubmit(ModalSubmitInteraction {
            context,
            submission: parse_modal_submission(raw)?,
        }),
        kind => Interaction::Unknown {
            context,
            kind,
            raw_data: raw.get("data").cloned().unwrap_or(Value::Null),
        },
    };

    Ok(interaction)
}

fn parse_typed_interaction_context(raw: &Value) -> Result<InteractionContextData, DiscordError> {
    let member = raw
        .get("member")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let user = raw
        .get("user")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .or_else(|| {
            member
                .as_ref()
                .and_then(|member: &crate::model::Member| member.user.clone())
        });

    Ok(InteractionContextData {
        id: required_id_field(raw, "id", "interaction")?,
        application_id: required_id_field(raw, "application_id", "interaction")?,
        token: required_string_field(raw, "token", "interaction")?,
        guild_id: optional_id_field(raw, "guild_id", "interaction")?,
        channel_id: optional_id_field(raw, "channel_id", "interaction")?,
        user,
        member,
        app_permissions: raw
            .get("app_permissions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        locale: optional_string_field(raw, "locale"),
        guild_locale: optional_string_field(raw, "guild_locale"),
        entitlements: raw
            .get("entitlements")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        context: raw
            .get("context")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        authorizing_integration_owners: raw
            .get("authorizing_integration_owners")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        response_state: InteractionResponseState::new(),
    })
}

fn parse_command_interaction_data(raw: &Value) -> Result<CommandInteractionData, DiscordError> {
    let data = required_object_field(raw, "data", "interaction")?.clone();
    let options = data
        .get("options")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    Ok(CommandInteractionData {
        id: optional_id_field(&data, "id", "interaction.data")?,
        name: optional_string_field(&data, "name"),
        kind: data.get("type").and_then(value_to_u8),
        options,
        resolved: data
            .get("resolved")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        target_id: optional_id_field(&data, "target_id", "interaction.data")?,
    })
}

fn parse_component_interaction_data(raw: &Value) -> Result<ComponentInteractionData, DiscordError> {
    let data = required_object_field(raw, "data", "interaction")?.clone();
    Ok(ComponentInteractionData {
        custom_id: required_string_field(&data, "custom_id", "component_data")?,
        component_type: required_u8_field(&data, "component_type", "component_data")?,
        values: optional_string_values_field(&data, "values", "component_data")?
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use crate::model::test_id;
    use serde_json::{json, Value};

    use super::{
        parse_interaction, parse_interaction_context, parse_raw_interaction, RawInteraction,
    };
    use crate::model::{Interaction, InteractionContextType};
    use crate::response::AckKind;

    #[test]
    fn parse_raw_interaction_supports_autocomplete() {
        let interaction = parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 4,
            "data": {
                "id": "3",
                "name": "ticket",
                "type": 1,
                "options": []
            }
        }))
        .unwrap();

        match interaction {
            RawInteraction::Autocomplete {
                id,
                name,
                command_type,
                ..
            } => {
                assert_eq!(id, Some(test_id(3)));
                assert_eq!(name.as_deref(), Some("ticket"));
                assert_eq!(command_type, Some(1));
            }
            other => panic!("unexpected raw interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_interaction_preserves_command_option_values_and_focus() {
        let interaction = parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 4,
            "data": {
                "id": "3",
                "name": "ticket",
                "type": 1,
                "options": [{
                    "type": 3,
                    "name": "topic",
                    "value": "billing",
                    "focused": true
                }, {
                    "type": 1,
                    "name": "nested",
                    "options": [{
                        "type": 4,
                        "name": "priority",
                        "value": 2
                    }]
                }]
            }
        }))
        .unwrap();

        match interaction {
            Interaction::Autocomplete(interaction) => {
                assert_eq!(interaction.data.options.len(), 2);
                assert_eq!(interaction.data.options[0].name, "topic");
                assert_eq!(interaction.data.options[0].value, Some(json!("billing")));
                assert!(interaction.data.options[0].is_focused());
                assert_eq!(interaction.data.options[1].name, "nested");
                assert_eq!(interaction.data.options[1].options.len(), 1);
                assert_eq!(interaction.data.options[1].options[0].value, Some(json!(2)));
            }
            other => panic!("unexpected typed interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_interaction_context_prefers_member_user_and_keeps_raw_payload() {
        let raw = json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "guild_id": "3",
            "channel_id": "4",
            "member": {
                "user": {
                    "id": "5",
                    "username": "discordrs"
                }
            }
        });

        let context = parse_interaction_context(&raw).unwrap();
        assert_eq!(context.id, test_id(1));
        assert_eq!(context.application_id, test_id(2));
        assert_eq!(context.guild_id, Some(test_id(3)));
        assert_eq!(context.channel_id, Some(test_id(4)));
        assert_eq!(context.user_id, Some(test_id(5)));
        assert_eq!(context.raw, raw);
    }

    #[test]
    fn parse_raw_interaction_covers_ping_component_modal_and_unknown_types() {
        assert!(matches!(
            parse_raw_interaction(&json!({
                "id": "1",
                "application_id": "2",
                "token": "token",
                "type": 1
            }))
            .unwrap(),
            RawInteraction::Ping
        ));

        match parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": "button",
                "component_type": 2,
                "values": ["first", "second"]
            }
        }))
        .unwrap()
        {
            RawInteraction::Component {
                custom_id,
                component_type,
                values,
                ..
            } => {
                assert_eq!(custom_id.as_deref(), Some("button"));
                assert_eq!(component_type, Some(2));
                assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
            }
            other => panic!("unexpected component interaction: {other:?}"),
        }

        match parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 5,
            "data": {
                "custom_id": "modal",
                "components": []
            }
        }))
        .unwrap()
        {
            RawInteraction::ModalSubmit(submission) => {
                assert_eq!(submission.custom_id, "modal");
                assert!(submission.components.is_empty());
            }
            other => panic!("unexpected modal submission: {other:?}"),
        }

        let error = parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 99
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unsupported interaction type"));
    }

    #[test]
    fn parse_interaction_covers_component_and_unknown_command_variants() {
        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": "button",
                "component_type": 2,
                "values": ["accepted"]
            }
        }))
        .unwrap()
        {
            Interaction::Component(component) => {
                assert_eq!(component.data.custom_id, "button");
                assert_eq!(component.data.component_type, 2);
                assert_eq!(component.data.values, vec!["accepted".to_string()]);
            }
            other => panic!("unexpected typed component: {other:?}"),
        }

        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 2,
            "data": {
                "type": 99,
                "name": "unknown-command"
            }
        }))
        .unwrap()
        {
            Interaction::Unknown { kind, raw_data, .. } => {
                assert_eq!(kind, 99);
                assert_eq!(raw_data["name"], json!("unknown-command"));
            }
            other => panic!("unexpected unknown interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_interaction_context_falls_back_to_top_level_user() {
        let raw = json!({
            "id": "10",
            "application_id": "20",
            "token": "token",
            "user": {
                "id": "30",
                "username": "fallback-user"
            }
        });

        let context = parse_interaction_context(&raw).unwrap();
        assert_eq!(context.id, test_id(10));
        assert_eq!(context.application_id, test_id(20));
        assert_eq!(context.guild_id, None);
        assert_eq!(context.channel_id, None);
        assert_eq!(context.user_id, Some(test_id(30)));
        assert_eq!(context.raw, raw);
    }

    #[test]
    fn parse_interaction_preserves_install_context_entitlements_and_target_id() {
        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 2,
            "context": 1,
            "authorizing_integration_owners": {
                "0": "44",
                "1": "55"
            },
            "entitlements": [{
                "id": "10",
                "sku_id": "11",
                "application_id": "2",
                "type": 8,
                "deleted": false
            }],
            "data": {
                "id": "3",
                "name": "Inspect",
                "type": 2,
                "target_id": "99"
            }
        }))
        .unwrap()
        {
            Interaction::UserContextMenu(interaction) => {
                assert_eq!(interaction.data.target_id.unwrap().get(), 99);
                assert_eq!(
                    interaction.context.context,
                    Some(InteractionContextType::BOT_DM)
                );
                assert_eq!(interaction.context.entitlements.unwrap().len(), 1);
                assert_eq!(
                    interaction
                        .context
                        .authorizing_integration_owners
                        .as_ref()
                        .and_then(|owners| owners.user_install)
                        .map(|user| user.get()),
                    Some(55)
                );
                assert_eq!(
                    interaction
                        .context
                        .authorizing_integration_owners
                        .and_then(|owners| owners.guild_install)
                        .and_then(|owner| owner.guild_id())
                        .map(|guild| guild.get()),
                    Some(44)
                );
            }
            other => panic!("unexpected interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_raw_interaction_covers_command_defaults_and_component_value_fallback() {
        match parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 2,
            "data": {}
        }))
        .unwrap()
        {
            RawInteraction::Command {
                id,
                name,
                command_type,
                data,
            } => {
                assert_eq!(id, None);
                assert_eq!(name, None);
                assert_eq!(command_type, None);
                assert_eq!(data, json!({}));
            }
            other => panic!("unexpected command interaction: {other:?}"),
        }

        match parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": "menu",
                "component_type": 3
            }
        }))
        .unwrap()
        {
            RawInteraction::Component { values, data, .. } => {
                assert!(values.is_empty());
                assert_eq!(data["custom_id"], json!("menu"));
            }
            other => panic!("unexpected component interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_interaction_covers_ping_command_variants_modal_and_unknown_type() {
        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 1,
            "member": {
                "user": {
                    "id": "9",
                    "username": "member-user"
                }
            },
            "app_permissions": "2048",
            "locale": "en-US",
            "guild_locale": "ko"
        }))
        .unwrap()
        {
            Interaction::Ping(ping) => {
                assert_eq!(ping.context.id.get(), 1);
                assert_eq!(ping.context.application_id.get(), 2);
                assert_eq!(ping.context.user.unwrap().id.get(), 9);
                assert_eq!(ping.context.app_permissions.unwrap().bits(), 2048);
                assert_eq!(ping.context.locale.as_deref(), Some("en-US"));
                assert_eq!(ping.context.guild_locale.as_deref(), Some("ko"));
            }
            other => panic!("unexpected ping interaction: {other:?}"),
        }

        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 2,
            "data": {
                "id": "5",
                "name": "deploy",
                "type": 1,
                "resolved": {
                    "users": {
                        "42": {
                            "id": "42",
                            "username": "resolved-user"
                        }
                    }
                }
            }
        }))
        .unwrap()
        {
            Interaction::ChatInputCommand(command) => {
                assert_eq!(command.data.id.unwrap().get(), 5);
                assert_eq!(command.data.name.as_deref(), Some("deploy"));
                assert_eq!(command.data.kind, Some(1));
                assert!(command.data.options.is_empty());
                assert_eq!(
                    command
                        .data
                        .resolved
                        .unwrap()
                        .user(test_id(42))
                        .map(|user| user.username.as_str()),
                    Some("resolved-user")
                );
            }
            other => panic!("unexpected chat input interaction: {other:?}"),
        }

        assert!(matches!(
            parse_interaction(&json!({
                "id": "1",
                "application_id": "2",
                "token": "token",
                "type": 2,
                "data": {
                    "type": 2
                }
            }))
            .unwrap(),
            Interaction::UserContextMenu(_)
        ));

        assert!(matches!(
            parse_interaction(&json!({
                "id": "1",
                "application_id": "2",
                "token": "token",
                "type": 2,
                "data": {
                    "type": 3
                }
            }))
            .unwrap(),
            Interaction::MessageContextMenu(_)
        ));

        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 5,
            "data": {
                "custom_id": "feedback",
                "components": [{
                    "type": 1,
                    "components": [{
                        "type": 4,
                        "custom_id": "summary",
                        "value": "works"
                    }]
                }]
            }
        }))
        .unwrap()
        {
            Interaction::ModalSubmit(modal) => {
                assert_eq!(modal.submission.custom_id, "feedback");
                assert_eq!(modal.submission.get_text("summary"), Some("works"));
            }
            other => panic!("unexpected modal interaction: {other:?}"),
        }

        match parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 99
        }))
        .unwrap()
        {
            Interaction::Unknown {
                kind,
                raw_data,
                context,
            } => {
                assert_eq!(kind, 99);
                assert_eq!(raw_data, json!(null));
                assert_eq!(context.id.get(), 1);
            }
            other => panic!("unexpected unknown interaction: {other:?}"),
        }
    }

    #[test]
    fn parse_interaction_creates_a_shared_unacknowledged_response_state() {
        let interaction = parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 2,
            "data": {
                "id": "3",
                "name": "deploy",
                "type": 1
            }
        }))
        .unwrap();

        assert!(!interaction.is_acknowledged());
        assert!(!interaction.is_deferred());
        assert!(!interaction.is_replied());

        // Clones (e.g. one handed to a handler, one kept by a collector)
        // must observe the same acknowledgement state.
        let clone = interaction.clone();
        assert!(interaction
            .context()
            .response_state
            .commit_inline(AckKind::Deferred));
        assert!(clone.is_acknowledged());
        assert!(clone.is_deferred());
        assert!(clone
            .context()
            .response_state
            .begin(AckKind::Replied)
            .is_err());
    }

    const SECRET_TOKEN: &str = "aW50ZXJhY3Rpb246c2VjcmV0LXRva2Vu";

    fn payload_with_secret_token(kind: u8, data: Value) -> Value {
        json!({
            "id": "1",
            "application_id": "2",
            "token": SECRET_TOKEN,
            "type": kind,
            "guild_id": "3",
            "member": { "user": { "id": "5", "username": "debug-user" } },
            "data": data
        })
    }

    fn assert_redacted(debug: String) {
        assert!(
            !debug.contains(SECRET_TOKEN),
            "token leaked into Debug output: {debug}"
        );
        assert!(debug.contains("[redacted]"), "{debug}");
    }

    #[test]
    fn typed_interaction_debug_output_never_contains_the_token() {
        let payloads = [
            payload_with_secret_token(2, json!({ "id": "3", "name": "deploy", "type": 1 })),
            payload_with_secret_token(3, json!({ "custom_id": "ok", "component_type": 2 })),
            payload_with_secret_token(4, json!({ "id": "3", "name": "deploy", "type": 1 })),
            payload_with_secret_token(5, json!({ "custom_id": "modal", "components": [] })),
            payload_with_secret_token(42, json!({ "opaque": true })),
        ];
        for payload in payloads {
            let interaction = parse_interaction(&payload).unwrap();
            assert_redacted(format!("{interaction:?}"));
            assert_redacted(format!("{interaction:#?}"));
            let context = interaction.context();
            assert_redacted(format!("{context:?}"));
            // The rest of the context stays useful for debugging.
            let debug = format!("{context:?}");
            assert!(debug.contains("debug-user"), "{debug}");
            assert!(debug.contains("Unacknowledged"), "{debug}");
            assert_eq!(context.token, SECRET_TOKEN);
        }
    }

    #[test]
    fn raw_interaction_context_debug_redacts_the_token_and_raw_payload() {
        let mut payload =
            payload_with_secret_token(3, json!({ "custom_id": "ok", "component_type": 2 }));
        payload["message"] = json!({ "id": "9", "nested": [{ "token": SECRET_TOKEN }] });
        let context = parse_interaction_context(&payload).unwrap();

        let debug = format!("{context:?}");
        assert_redacted(debug.clone());
        assert_redacted(format!("{context:#?}"));
        assert!(debug.contains("custom_id"), "{debug}");
        assert!(debug.contains("debug-user"), "{debug}");
        assert_eq!(context.token, SECRET_TOKEN);
        assert_eq!(context.raw["token"], json!(SECRET_TOKEN));

        let raw = parse_raw_interaction(&payload).unwrap();
        assert!(!format!("{raw:?}").contains(SECRET_TOKEN));
    }

    #[test]
    fn parse_interaction_reports_component_value_errors() {
        let error = parse_raw_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": "menu",
                "component_type": 3,
                "values": [{}]
            }
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("component_data.values must contain strings"));

        let error = parse_interaction(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "component_type": 2
            }
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing or invalid component_data.custom_id"));
    }
}
