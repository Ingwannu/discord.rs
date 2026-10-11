//! Interaction payloads, interaction context, and callback responses.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::parsers::V2ModalSubmission;

use super::{
    Attachment, Channel, Entitlement, GenericId, GuildInstallOwner, InteractionContextType,
    InteractionResponseState, Member, Message, PermissionsBitField, Role, User, UserId,
};

/// `authorizing_integration_owners` of an interaction (or of a message's
/// interaction metadata): which installations of the app authorized it.
///
/// Discord keys the object by application integration type. Key `"0"`
/// (`GUILD_INSTALL`) holds the guild ID, or `"0"` when the interaction came
/// from a DM with the bot, so it is a [`GuildInstallOwner`] rather than an
/// ID. Key `"1"` (`USER_INSTALL`) holds the installing user's ID. Keys added
/// by future integration types are kept in [`Self::other`] and written back
/// unchanged.
///
/// ```
/// use discordrs::{AuthorizingIntegrationOwners, GuildInstallOwner};
///
/// let owners: AuthorizingIntegrationOwners =
///     serde_json::from_str(r#"{"0": "0", "1": "80351110224678912"}"#).unwrap();
/// assert_eq!(owners.guild_install, Some(GuildInstallOwner::BotDm));
/// assert_eq!(owners.user_install.map(|user| user.get()), Some(80351110224678912));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorizingIntegrationOwners {
    /// `GUILD_INSTALL` (`"0"`): the guild, or [`GuildInstallOwner::BotDm`].
    #[serde(rename = "0", default, skip_serializing_if = "Option::is_none")]
    pub guild_install: Option<GuildInstallOwner>,
    /// `USER_INSTALL` (`"1"`): the user who installed the app.
    #[serde(rename = "1", default, skip_serializing_if = "Option::is_none")]
    pub user_install: Option<UserId>,
    /// Entries for integration types this version does not know, as sent.
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

impl AuthorizingIntegrationOwners {
    /// Whether no installation context is listed at all.
    pub fn is_empty(&self) -> bool {
        self.guild_install.is_none() && self.user_install.is_none() && self.other.is_empty()
    }
}

/// Resolved objects referenced by interaction options, components, or a
/// forwarded message, keyed by snowflake ID. Every map defaults to empty
/// when Discord omits it.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ResolvedData {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub users: HashMap<GenericId, User>,
    /// Partial members (no `user`, `deaf`, or `mute` fields).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub members: HashMap<GenericId, Member>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<GenericId, Role>,
    /// Partial channels (id, name, type, permissions, and thread fields).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub channels: HashMap<GenericId, Channel>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub messages: HashMap<GenericId, Message>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attachments: HashMap<GenericId, Attachment>,
}

impl ResolvedData {
    /// Returns true when every resolved map is empty.
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
            && self.members.is_empty()
            && self.roles.is_empty()
            && self.channels.is_empty()
            && self.messages.is_empty()
            && self.attachments.is_empty()
    }

    /// Looks up a resolved user by ID.
    pub fn user(&self, id: GenericId) -> Option<&User> {
        self.users.get(&id)
    }

    /// Looks up a resolved partial member by user ID.
    pub fn member(&self, id: GenericId) -> Option<&Member> {
        self.members.get(&id)
    }

    /// Looks up a resolved role by ID.
    pub fn role(&self, id: GenericId) -> Option<&Role> {
        self.roles.get(&id)
    }

    /// Looks up a resolved partial channel by ID.
    pub fn channel(&self, id: GenericId) -> Option<&Channel> {
        self.channels.get(&id)
    }

    /// Looks up a resolved message by ID.
    pub fn message(&self, id: GenericId) -> Option<&Message> {
        self.messages.get(&id)
    }

    /// Looks up a resolved attachment by ID.
    pub fn attachment(&self, id: GenericId) -> Option<&Attachment> {
        self.attachments.get(&id)
    }
}

// `Debug` is implemented by hand below so the token never reaches logs.
#[derive(Clone, Serialize, Deserialize)]
/// Typed Discord API object for `InteractionContextData`.
pub struct InteractionContextData {
    pub id: GenericId,
    pub application_id: GenericId,
    /// Interaction token: authorizes responses and follow-ups for 15
    /// minutes. Redacted from `Debug` output.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entitlements: Option<Vec<Entitlement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<InteractionContextType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
    /// Shared response-state handle. Created once during parsing so every
    /// clone of the same interaction observes the same acknowledgement state.
    #[serde(skip)]
    pub response_state: InteractionResponseState,
}

/// Prints every field except the token, which is replaced by `[redacted]`.
///
/// Interaction contexts end up in `{:?}` logs and panic messages through the
/// interaction variants, `AppContext`, and collectors; the token would let
/// anyone who reads those logs answer or edit the interaction for 15 minutes.
impl std::fmt::Debug for InteractionContextData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Exhaustive so a new field cannot be left out of (or leak through)
        // this impl unnoticed.
        let Self {
            id,
            application_id,
            token: _,
            guild_id,
            channel_id,
            user,
            member,
            app_permissions,
            locale,
            guild_locale,
            entitlements,
            context,
            authorizing_integration_owners,
            response_state,
        } = self;
        f.debug_struct("InteractionContextData")
            .field("id", id)
            .field("application_id", application_id)
            .field("token", &format_args!("[redacted]"))
            .field("guild_id", guild_id)
            .field("channel_id", channel_id)
            .field("user", user)
            .field("member", member)
            .field("app_permissions", app_permissions)
            .field("locale", locale)
            .field("guild_locale", guild_locale)
            .field("entitlements", entitlements)
            .field("context", context)
            .field(
                "authorizing_integration_owners",
                authorizing_integration_owners,
            )
            .field("response_state", response_state)
            .finish()
    }
}

impl InteractionContextData {
    /// Returns true when Discord accepted a non-deferred initial response
    /// (reply, message update, modal, ...) or a deferred reply was edited.
    pub fn is_replied(&self) -> bool {
        self.response_state.is_replied()
    }

    /// Returns true when the interaction was deferred and not yet replied to.
    pub fn is_deferred(&self) -> bool {
        self.response_state.is_deferred()
    }

    /// Returns true when Discord confirmed an initial response of any kind
    /// (deferred or replied). False while a response is in flight or its
    /// outcome is uncertain; see [`Self::response_phase`].
    pub fn is_acknowledged(&self) -> bool {
        self.response_state.is_acknowledged()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionOption`.
pub struct CommandInteractionOption {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub options: Vec<CommandInteractionOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
}

impl CommandInteractionOption {
    pub fn is_focused(&self) -> bool {
        self.focused.unwrap_or(false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionData`.
pub struct CommandInteractionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(default)]
    pub options: Vec<CommandInteractionOption>,
    #[serde(default)]
    pub resolved: Option<ResolvedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<GenericId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ComponentInteractionData`.
pub struct ComponentInteractionData {
    pub custom_id: String,
    pub component_type: u8,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ChatInputCommandInteraction`.
pub struct ChatInputCommandInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UserContextMenuInteraction`.
pub struct UserContextMenuInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MessageContextMenuInteraction`.
pub struct MessageContextMenuInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `AutocompleteInteraction`.
pub struct AutocompleteInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ComponentInteraction`.
pub struct ComponentInteraction {
    pub context: InteractionContextData,
    pub data: ComponentInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ModalSubmitInteraction`.
pub struct ModalSubmitInteraction {
    pub context: InteractionContextData,
    pub submission: V2ModalSubmission,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `PingInteraction`.
pub struct PingInteraction {
    pub context: InteractionContextData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// Typed Discord API enum for `Interaction`.
pub enum Interaction {
    /// Discord API enum variant `Ping`.
    Ping(PingInteraction),
    /// Discord API enum variant `ChatInputCommand`.
    ChatInputCommand(ChatInputCommandInteraction),
    /// Discord API enum variant `UserContextMenu`.
    UserContextMenu(UserContextMenuInteraction),
    /// Discord API enum variant `MessageContextMenu`.
    MessageContextMenu(MessageContextMenuInteraction),
    /// Discord API enum variant `Autocomplete`.
    Autocomplete(AutocompleteInteraction),
    /// Discord API enum variant `Component`.
    Component(ComponentInteraction),
    /// Discord API enum variant `ModalSubmit`.
    ModalSubmit(ModalSubmitInteraction),
    /// Discord API enum variant `Unknown`.
    Unknown {
        /// Parsed interaction context shared with known variants.
        context: InteractionContextData,
        /// Raw Discord interaction type value.
        kind: u8,
        /// Raw interaction data object for unsupported variants.
        raw_data: serde_json::Value,
    },
}

impl Interaction {
    pub fn context(&self) -> &InteractionContextData {
        match self {
            Interaction::Ping(interaction) => &interaction.context,
            Interaction::ChatInputCommand(interaction) => &interaction.context,
            Interaction::UserContextMenu(interaction) => &interaction.context,
            Interaction::MessageContextMenu(interaction) => &interaction.context,
            Interaction::Autocomplete(interaction) => &interaction.context,
            Interaction::Component(interaction) => &interaction.context,
            Interaction::ModalSubmit(interaction) => &interaction.context,
            Interaction::Unknown { context, .. } => context,
        }
    }

    pub fn id(&self) -> GenericId {
        self.context().id
    }

    pub fn token(&self) -> &str {
        &self.context().token
    }

    pub fn application_id(&self) -> GenericId {
        self.context().application_id
    }

    /// Returns true when an initial response has been sent for this
    /// interaction.
    pub fn is_replied(&self) -> bool {
        self.context().is_replied()
    }

    /// Returns true when the interaction was deferred and not yet replied to.
    pub fn is_deferred(&self) -> bool {
        self.context().is_deferred()
    }

    /// Returns true when the interaction was acknowledged in any way
    /// (deferred or replied).
    pub fn is_acknowledged(&self) -> bool {
        self.context().is_acknowledged()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `InteractionCallbackResponse`.
pub struct InteractionCallbackResponse {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Interaction object returned inside an interaction callback result
/// (`POST /interactions/{id}/{token}/callback?with_response=true`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackInteraction {
    pub id: GenericId,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_ephemeral: Option<bool>,
}

/// Resource created by an interaction response, returned when
/// `with_response=true` is passed to the callback endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct InteractionCallbackResource {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_instance: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
}

/// Full interaction callback result (`with_response=true`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackResult {
    pub interaction: InteractionCallbackInteraction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<InteractionCallbackResource>,
}
