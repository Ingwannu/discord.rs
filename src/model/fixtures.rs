//! Test-only fixture constructors for models that lost `Default`.
//!
//! Persisted models with a required ID have no `Default` in 3.0 (no sentinel
//! IDs, docs/design/3.0-types-events-requests.md §1 "Default"), but tests
//! still want "a message with these three fields set". `T::fixture()` fills
//! the required IDs with [`FIXTURE_ID`], required discriminants with a
//! documented value (the 2.x `0` is not one for most of them), and
//! everything else with what `Default` used to give (empty strings, zero,
//! `None`), so tests read as
//! `Message { id: test_id(5), ..Message::fixture() }`.
//!
//! The JSON-backed fixtures deserialize a minimal payload instead of listing
//! every field, so a field added to a model does not need a fixture edit as
//! long as it is optional on the wire (the fixture tests below catch the
//! rest). None of this is compiled outside `cfg(test)`.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::{
    Attachment, AutocompleteInteraction, Channel, ChannelType, ChatInputCommandInteraction,
    CommandInteractionData, CommandInteractionOption, ComponentInteraction,
    ComponentInteractionData, Entitlement, ForumTag, Guild, Id, IdMarker, Integration,
    InteractionContextData, InteractionResponseState, Message, MessageContextMenuInteraction, Role,
    SoundboardSound, StageInstance, Sticker, StickerType, Subscription, SubscriptionStatus, User,
    UserContextMenuInteraction,
};

/// Placeholder for required IDs a test did not set. Large and distinctive
/// so it does not collide with the small literal IDs tests use.
pub(crate) const FIXTURE_ID: u64 = 999_999_999_999_999_999;

fn from_json<T: DeserializeOwned>(value: Value) -> T {
    match serde_json::from_value(value) {
        Ok(value) => value,
        Err(error) => panic!("fixture JSON must match the model: {error}"),
    }
}

fn placeholder<M: IdMarker>() -> Id<M> {
    super::test_id(FIXTURE_ID)
}

impl Message {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "channel_id": FIXTURE_ID, "content": "" }))
    }
}

impl User {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "username": "" }))
    }
}

impl Channel {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "type": 0 }))
    }
}

impl Role {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "name": "" }))
    }
}

impl Guild {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "name": "" }))
    }
}

impl StageInstance {
    pub(crate) fn fixture() -> Self {
        from_json(json!({
            "id": FIXTURE_ID,
            "guild_id": FIXTURE_ID,
            "channel_id": FIXTURE_ID,
            "topic": "",
            "privacy_level": 2,
        }))
    }
}

impl SoundboardSound {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "name": "", "sound_id": FIXTURE_ID, "volume": 0.0 }))
    }
}

impl Sticker {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "name": "", "type": 2, "format_type": 1 }))
    }
}

impl Attachment {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "filename": "" }))
    }
}

impl Subscription {
    pub(crate) fn fixture() -> Self {
        from_json(json!({
            "id": FIXTURE_ID,
            "user_id": FIXTURE_ID,
            "current_period_start": "",
            "current_period_end": "",
            "status": 0,
        }))
    }
}

impl Entitlement {
    pub(crate) fn fixture() -> Self {
        from_json(json!({
            "id": FIXTURE_ID,
            "sku_id": FIXTURE_ID,
            "application_id": FIXTURE_ID,
            "type": 1,
            "deleted": false,
        }))
    }
}

impl Integration {
    pub(crate) fn fixture() -> Self {
        from_json(json!({
            "id": FIXTURE_ID,
            "name": "",
            "type": "",
            "account": { "id": "", "name": "" },
        }))
    }
}

impl ForumTag {
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "id": FIXTURE_ID, "name": "" }))
    }
}

impl InteractionContextData {
    pub(crate) fn fixture() -> Self {
        Self {
            id: placeholder(),
            application_id: placeholder(),
            token: String::new(),
            guild_id: None,
            channel_id: None,
            user: None,
            member: None,
            app_permissions: None,
            locale: None,
            guild_locale: None,
            entitlements: None,
            context: None,
            authorizing_integration_owners: None,
            response_state: InteractionResponseState::new(),
        }
    }
}

impl ChatInputCommandInteraction {
    pub(crate) fn fixture() -> Self {
        Self {
            context: InteractionContextData::fixture(),
            data: CommandInteractionData::default(),
        }
    }
}

impl AutocompleteInteraction {
    pub(crate) fn fixture() -> Self {
        Self {
            context: InteractionContextData::fixture(),
            data: CommandInteractionData::default(),
        }
    }
}

impl UserContextMenuInteraction {
    pub(crate) fn fixture() -> Self {
        Self {
            context: InteractionContextData::fixture(),
            data: CommandInteractionData::default(),
        }
    }
}

impl MessageContextMenuInteraction {
    pub(crate) fn fixture() -> Self {
        Self {
            context: InteractionContextData::fixture(),
            data: CommandInteractionData::default(),
        }
    }
}

impl ComponentInteractionData {
    /// A button press with an empty `custom_id`.
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "custom_id": "", "component_type": 2 }))
    }
}

impl CommandInteractionOption {
    /// An unfocused `STRING` option with an empty name and no value.
    pub(crate) fn fixture() -> Self {
        from_json(json!({ "type": 3, "name": "" }))
    }
}

impl ComponentInteraction {
    pub(crate) fn fixture() -> Self {
        Self {
            context: InteractionContextData::fixture(),
            data: ComponentInteractionData::fixture(),
        }
    }
}

#[test]
fn every_fixture_builds() {
    assert_eq!(Message::fixture().id.get(), FIXTURE_ID);
    assert_eq!(User::fixture().username, "");
    assert_eq!(Channel::fixture().kind, ChannelType::GUILD_TEXT);
    assert_eq!(Role::fixture().name, "");
    assert_eq!(Guild::fixture().name, "");
    assert_eq!(StageInstance::fixture().topic, "");
    assert_eq!(SoundboardSound::fixture().sound_id.get(), FIXTURE_ID);
    assert_eq!(Sticker::fixture().kind, StickerType::GUILD);
    assert_eq!(Attachment::fixture().filename, "");
    assert_eq!(Subscription::fixture().status, SubscriptionStatus::ACTIVE);
    assert!(!Entitlement::fixture().deleted);
    assert_eq!(Integration::fixture().kind, "");
    assert_eq!(ForumTag::fixture().name, "");
    assert_eq!(ChatInputCommandInteraction::fixture().context.token, "");
    assert_eq!(ComponentInteraction::fixture().data.custom_id, "");
    assert_eq!(CommandInteractionOption::fixture().name, "");
}
