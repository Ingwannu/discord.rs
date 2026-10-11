//! Interaction payloads, interaction context, and callback responses.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::parsers::{V2ModalComponent, V2ModalSubmission};

use super::{
    ApplicationId, Attachment, AttachmentId, Channel, ChannelId, CommandId, CommandOptionType,
    CommandOptionValue, CommandType, ComponentType, Entitlement, GenericId, GuildId,
    GuildInstallOwner, Id, IdMarker, InteractionContextType, InteractionId,
    InteractionResponseState, InteractionResponseType, InteractionType, Member, MentionableId,
    Message, MessageId, Permissions, Role, RoleId, User, UserId,
};

/// `authorizing_integration_owners` of an interaction (or of a message's
/// interaction metadata): which installations of the app authorized it.
///
/// Discord keys the object by application integration type. Key `"0"`
/// (`GUILD_INSTALL`) holds the guild ID, or `"0"` when the interaction came
/// from a DM with the bot, so it is a [`GuildInstallOwner`] rather than an
/// ID. Key `"1"` (`USER_INSTALL`) holds the installing user's ID. Keys added
/// by future integration types are kept in [`Self::other`] as JSON text and
/// written back unchanged.
///
/// `Serialize`/`Deserialize` are written by hand (in `parsers/interaction`)
/// instead of a `#[serde(flatten)]` map: flatten buffers the whole object
/// in serde's `Content` tree on every interaction, and `RawValue` cannot be
/// read back out of that buffer.
///
/// ```
/// use discordrs::{AuthorizingIntegrationOwners, GuildInstallOwner};
///
/// let owners: AuthorizingIntegrationOwners =
///     serde_json::from_str(r#"{"0": "0", "1": "80351110224678912"}"#).unwrap();
/// assert_eq!(owners.guild_install, Some(GuildInstallOwner::BotDm));
/// assert_eq!(owners.user_install.map(|user| user.get()), Some(80351110224678912));
/// ```
#[derive(Clone, Debug, Default)]
pub struct AuthorizingIntegrationOwners {
    /// `GUILD_INSTALL` (`"0"`): the guild, or [`GuildInstallOwner::BotDm`].
    pub guild_install: Option<GuildInstallOwner>,
    /// `USER_INSTALL` (`"1"`): the user who installed the app.
    pub user_install: Option<UserId>,
    /// Entries for integration types this version does not know, keyed as
    /// sent, each value as its JSON text (Discord has not documented their
    /// shape yet).
    pub other: BTreeMap<String, Box<RawValue>>,
}

/// Compares the known entries by value and unknown ones by JSON text.
impl PartialEq for AuthorizingIntegrationOwners {
    fn eq(&self, other: &Self) -> bool {
        self.guild_install == other.guild_install
            && self.user_install == other.user_install
            && self.other.len() == other.other.len()
            && self.other.iter().zip(&other.other).all(
                |((key, value), (other_key, other_value))| {
                    key == other_key && value.get() == other_value.get()
                },
            )
    }
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
    pub users: HashMap<UserId, User>,
    /// Partial members (no `user`, `deaf`, or `mute` fields).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub members: HashMap<UserId, Member>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub roles: HashMap<RoleId, Role>,
    /// Partial channels (id, name, type, permissions, and thread fields).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub channels: HashMap<ChannelId, Channel>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub messages: HashMap<MessageId, Message>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attachments: HashMap<AttachmentId, Attachment>,
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
    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    /// Looks up a resolved partial member by user ID.
    pub fn member(&self, id: UserId) -> Option<&Member> {
        self.members.get(&id)
    }

    /// Looks up a resolved role by ID.
    pub fn role(&self, id: RoleId) -> Option<&Role> {
        self.roles.get(&id)
    }

    /// Looks up a resolved partial channel by ID.
    pub fn channel(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.get(&id)
    }

    /// Looks up a resolved message by ID.
    pub fn message(&self, id: MessageId) -> Option<&Message> {
        self.messages.get(&id)
    }

    /// Looks up a resolved attachment by ID.
    pub fn attachment(&self, id: AttachmentId) -> Option<&Attachment> {
        self.attachments.get(&id)
    }

    /// Tells whether a `MENTIONABLE` option value (or mentionable select
    /// value) names a user or a role, using the resolved maps; `None` when
    /// `id` is in neither.
    ///
    /// The value itself is a bare snowflake, so the resolved data is the
    /// only place its kind shows.
    pub fn mentionable(&self, id: GenericId) -> Option<MentionableId> {
        let user_id: UserId = id.cast();
        if self.users.contains_key(&user_id) || self.members.contains_key(&user_id) {
            return Some(MentionableId::User(user_id));
        }
        let role_id: RoleId = id.cast();
        self.roles
            .contains_key(&role_id)
            .then_some(MentionableId::Role(role_id))
    }
}

// `Debug` is implemented by hand below so the token never reaches logs.
#[derive(Clone, Serialize, Deserialize)]
/// Typed Discord API object for `InteractionContextData`.
pub struct InteractionContextData {
    pub id: InteractionId,
    pub application_id: ApplicationId,
    /// Interaction token: authorizes responses and follow-ups for 15
    /// minutes. Redacted from `Debug` output.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_permissions: Option<Permissions>,
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

/// An option of a received application command (or a nested
/// subcommand/group). No `Default`: `type` and `name` are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandInteractionOption {
    #[serde(rename = "type")]
    pub kind: CommandOptionType,
    pub name: String,
    /// Options of a subcommand or subcommand group; empty for a value
    /// option.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandInteractionOption>,
    /// The value the user gave; absent for subcommands and groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<CommandOptionValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
}

impl CommandInteractionOption {
    pub fn is_focused(&self) -> bool {
        self.focused.unwrap_or(false)
    }
}

/// `data` of an application command or autocomplete interaction.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CommandInteractionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CommandId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Command type. Discord treats an absent `type` as
    /// [`CommandType::CHAT_INPUT`].
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<CommandType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandInteractionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedData>,
    /// Target of a user or message context menu command. Its kind depends
    /// on the command type, so this shared field is untyped; use
    /// [`UserContextMenuInteraction::target_id`] or
    /// [`MessageContextMenuInteraction::target_id`] for the typed ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<GenericId>,
    /// The guild the invoked command is registered to; absent for global
    /// commands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
}

/// `data` of a message component interaction. No `Default`: `custom_id`
/// and `component_type` are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentInteractionData {
    pub custom_id: String,
    pub component_type: ComponentType,
    /// Selected values of a select menu; empty for buttons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// The users, members, roles, and channels behind the selected values
    /// of a user, role, mentionable, or channel select. Discord omits it for
    /// buttons and string selects. The lookups below read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedData>,
}

impl ComponentInteractionData {
    /// The resolved data of the selection, when Discord sent any.
    pub fn resolved(&self) -> Option<&ResolvedData> {
        self.resolved.as_ref()
    }

    /// Users selected in a user or mentionable select (roles of a
    /// mentionable select are left out; see [`Self::get_roles`]).
    pub fn get_users(&self) -> impl Iterator<Item = &User> + '_ {
        resolve_values(&self.values, self.resolved.as_ref(), ResolvedData::user)
    }

    /// Partial members of the users selected in a user or mentionable
    /// select; only present for a message in a guild.
    pub fn get_members(&self) -> impl Iterator<Item = &Member> + '_ {
        resolve_values(&self.values, self.resolved.as_ref(), ResolvedData::member)
    }

    /// Roles selected in a role or mentionable select.
    pub fn get_roles(&self) -> impl Iterator<Item = &Role> + '_ {
        resolve_values(&self.values, self.resolved.as_ref(), ResolvedData::role)
    }

    /// Partial channels selected in a channel select.
    pub fn get_channels(&self) -> impl Iterator<Item = &Channel> + '_ {
        resolve_values(&self.values, self.resolved.as_ref(), ResolvedData::channel)
    }
}

/// Looks up each selected or uploaded value as an ID of kind `M` in
/// `resolved`, in order, skipping values that are not IDs or that Discord
/// did not resolve.
fn resolve_values<'a, M, T, F>(
    values: &'a [String],
    resolved: Option<&'a ResolvedData>,
    lookup: F,
) -> impl Iterator<Item = &'a T> + 'a
where
    M: IdMarker,
    T: 'a,
    F: Fn(&'a ResolvedData, Id<M>) -> Option<&'a T> + 'a,
{
    values.iter().filter_map(move |value| {
        let id = value.parse().ok()?;
        lookup(resolved?, id)
    })
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

impl UserContextMenuInteraction {
    /// The user the command was used on (`data.target_id`).
    pub fn target_id(&self) -> Option<UserId> {
        self.data.target_id.map(GenericId::cast)
    }

    /// The resolved target user, when Discord included it.
    pub fn target_user(&self) -> Option<&User> {
        let target_id = self.target_id()?;
        self.data.resolved.as_ref()?.user(target_id)
    }
}

impl MessageContextMenuInteraction {
    /// The message the command was used on (`data.target_id`).
    pub fn target_id(&self) -> Option<MessageId> {
        self.data.target_id.map(GenericId::cast)
    }

    /// The resolved target message, when Discord included it.
    pub fn target_message(&self) -> Option<&Message> {
        let target_id = self.target_id()?;
        self.data.resolved.as_ref()?.message(target_id)
    }
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
    /// `data.resolved`: the attachments behind File Upload values and the
    /// users, members, roles, and channels behind select values. Discord
    /// omits it when no such component was submitted. The lookups below
    /// read it per component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedData>,
}

impl ModalSubmitInteraction {
    /// The resolved data of the submission, when Discord sent any.
    pub fn resolved(&self) -> Option<&ResolvedData> {
        self.resolved.as_ref()
    }

    /// Files uploaded through File Upload `custom_id`, in upload order.
    ///
    /// Each uploaded ID is looked up in [`Self::resolved`]; an ID Discord
    /// did not resolve is skipped. Empty when there is no such component.
    pub fn get_attachments<'a>(
        &'a self,
        custom_id: &'a str,
    ) -> impl Iterator<Item = &'a Attachment> + 'a {
        self.resolved_values(custom_id, |resolved, id| resolved.attachment(id))
    }

    /// Users selected in user or mentionable select `custom_id` (roles of
    /// a mentionable select are left out; see [`Self::get_roles`]).
    pub fn get_users<'a>(&'a self, custom_id: &'a str) -> impl Iterator<Item = &'a User> + 'a {
        self.resolved_values(custom_id, |resolved, id| resolved.user(id))
    }

    /// Partial members of the users selected in user or mentionable select
    /// `custom_id`; only present for a modal submitted in a guild.
    pub fn get_members<'a>(&'a self, custom_id: &'a str) -> impl Iterator<Item = &'a Member> + 'a {
        self.resolved_values(custom_id, |resolved, id| resolved.member(id))
    }

    /// Roles selected in role or mentionable select `custom_id`.
    pub fn get_roles<'a>(&'a self, custom_id: &'a str) -> impl Iterator<Item = &'a Role> + 'a {
        self.resolved_values(custom_id, |resolved, id| resolved.role(id))
    }

    /// Partial channels selected in channel select `custom_id`.
    pub fn get_channels<'a>(
        &'a self,
        custom_id: &'a str,
    ) -> impl Iterator<Item = &'a Channel> + 'a {
        self.resolved_values(custom_id, |resolved, id| resolved.channel(id))
    }

    /// Looks up each submitted value of component `custom_id` (a select or
    /// File Upload) as an ID of kind `M` in the resolved data.
    fn resolved_values<'a, M, T, F>(
        &'a self,
        custom_id: &'a str,
        lookup: F,
    ) -> impl Iterator<Item = &'a T> + 'a
    where
        M: IdMarker,
        T: 'a,
        F: Fn(&'a ResolvedData, Id<M>) -> Option<&'a T> + 'a,
    {
        let values = self
            .submission
            .components
            .iter()
            .find_map(|component| submitted_values(component, custom_id))
            .unwrap_or_default();
        resolve_values(values, self.resolved.as_ref(), lookup)
    }
}

/// The `values` of a submitted select or File Upload named `custom_id`.
fn submitted_values<'a>(component: &'a V2ModalComponent, custom_id: &str) -> Option<&'a [String]> {
    match component {
        V2ModalComponent::StringSelect {
            custom_id: id,
            values,
        }
        | V2ModalComponent::UserSelect {
            custom_id: id,
            values,
        }
        | V2ModalComponent::RoleSelect {
            custom_id: id,
            values,
        }
        | V2ModalComponent::ChannelSelect {
            custom_id: id,
            values,
        }
        | V2ModalComponent::MentionableSelect {
            custom_id: id,
            values,
        }
        | V2ModalComponent::FileUpload {
            custom_id: id,
            values,
        } if id == custom_id => Some(values),
        _ => None,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `PingInteraction`.
pub struct PingInteraction {
    pub context: InteractionContextData,
}

/// A received interaction, typed by its `type` (and, for application
/// commands, by the command `type`).
///
/// `Serialize` and `Deserialize` use Discord's wire format: the interaction
/// object as Discord sends it in `INTERACTION_CREATE` and to an
/// interactions endpoint. Decode one with [`crate::decode_interaction`] or
/// any serde_json entry point; the decoder reads the payload once, with no
/// intermediate `serde_json::Value` (see `parsers/interaction` for how it
/// handles key order).
#[derive(Clone, Debug)]
#[non_exhaustive]
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
    /// An interaction this version of discord.rs cannot type: an unknown
    /// interaction `type`, or an application command whose command `type`
    /// is unknown.
    Unknown {
        /// Parsed interaction context shared with known variants.
        context: InteractionContextData,
        /// The interaction `type`. For an application command with an
        /// unknown command type this is
        /// [`InteractionType::APPLICATION_COMMAND`] and the command type is
        /// the `type` key of `raw_data` (2.x stored the command type here
        /// instead).
        kind: InteractionType,
        /// The interaction's `data` object as JSON text, exactly as
        /// received; `None` when Discord sent no `data`. Read it with
        /// `serde_json::from_str(raw.get())`.
        raw_data: Option<Box<RawValue>>,
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

    pub fn id(&self) -> InteractionId {
        self.context().id
    }

    pub fn token(&self) -> &str {
        &self.context().token
    }

    pub fn application_id(&self) -> ApplicationId {
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

/// An interaction callback (`POST /interactions/{id}/{token}/callback`).
/// No `Default`: `type` is required and has no protocol default.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackResponse {
    #[serde(rename = "type")]
    pub kind: InteractionResponseType,
    /// The callback data, whose shape depends on `kind` (message, choices,
    /// or modal). It stays a `Value` until the request models of 3.0 step 5
    /// replace this outgoing body with a typed response enum; the builders
    /// that fill it still produce `Value`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Interaction object returned inside an interaction callback result
/// (`POST /interactions/{id}/{token}/callback?with_response=true`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackInteraction {
    pub id: InteractionId,
    #[serde(rename = "type")]
    pub kind: InteractionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_id: Option<MessageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message_ephemeral: Option<bool>,
}

/// Resource created by an interaction response, returned when
/// `with_response=true` is passed to the callback endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackResource {
    /// The callback type that created this resource.
    #[serde(rename = "type")]
    pub kind: InteractionResponseType,
    /// The Activity launched by a `LAUNCH_ACTIVITY` response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_instance: Option<InteractionCallbackActivityInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
}

/// The Activity instance a `LAUNCH_ACTIVITY` interaction response started
/// (Discord: "Activity Instance Resource").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionCallbackActivityInstance {
    /// Instance ID of the Activity, the same ID as
    /// [`ActivityInstance::instance_id`](super::ActivityInstance::instance_id).
    pub id: String,
}

/// Full interaction callback result (`with_response=true`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionCallbackResult {
    pub interaction: InteractionCallbackInteraction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<InteractionCallbackResource>,
}
