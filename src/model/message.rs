//! Messages, message references, pins, reactions, and message search.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{
    is_false, ApplicationId, Attachment, AuthorizingIntegrationOwners, BaseTheme, Channel,
    ChannelId, ChannelType, CreatePoll, Embed, GuildId, InteractionId, InteractionType, Member,
    MessageActivityType, MessageComponent, MessageId, MessageReferenceType, MessageType,
    PartialApplication, Poll, ReactionType, ResolvedData, RoleId, RoleSubscriptionListingId,
    Sticker, StickerId, StickerItem, ThreadMember, User, UserId, WebhookId,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Message`.
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub mention_roles: Vec<RoleId>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<MessageType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_id: Option<WebhookId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<PartialApplication>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<MessageActivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_everyone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_channels: Option<Vec<ChannelMention>>,
    #[serde(default)]
    pub sticker_items: Vec<StickerItem>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_subscription_data: Option<RoleSubscriptionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReference>,
    #[serde(default)]
    pub message_snapshots: Vec<MessageSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_message: Option<Box<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_metadata: Option<MessageInteractionMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<MessageInteraction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<MessageComponent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<Poll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<MessageCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_client_theme: Option<SharedClientTheme>,
}

/// Rich Presence activity of a message (`Message::activity`). No `Default`:
/// Discord always sends `type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageActivity {
    #[serde(rename = "type")]
    pub kind: MessageActivityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageCall`.
pub struct MessageCall {
    #[serde(default)]
    pub participants: Vec<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_timestamp: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshot`.
pub struct MessageSnapshot {
    pub message: MessageSnapshotMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshotMessage`.
pub struct MessageSnapshotMessage {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<MessageType>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub mention_roles: Vec<RoleId>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    #[serde(default)]
    pub sticker_items: Vec<StickerItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<MessageComponent>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RoleSubscriptionData`.
pub struct RoleSubscriptionData {
    pub role_subscription_listing_id: RoleSubscriptionListingId,
    pub tier_name: String,
    pub total_months_subscribed: u64,
    pub is_renewal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SharedClientTheme`.
pub struct SharedClientTheme {
    #[serde(default)]
    pub colors: Vec<String>,
    pub gradient_angle: u64,
    pub base_mix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_theme: Option<BaseTheme>,
}

/// Metadata about the interaction that caused a message
/// (`Message::interaction_metadata`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageInteractionMetadata {
    /// ID of the triggering interaction.
    pub id: InteractionId,
    /// Interaction type of the triggering interaction (required; 2.x read a
    /// missing `type` as `0`).
    #[serde(rename = "type")]
    pub kind: InteractionType,
    /// User who triggered the interaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    /// Installation contexts that authorized the interaction.
    #[serde(
        default,
        skip_serializing_if = "AuthorizingIntegrationOwners::is_empty"
    )]
    pub authorizing_integration_owners: AuthorizingIntegrationOwners,
    /// ID of the original response message (follow-up messages only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_response_message_id: Option<MessageId>,
    /// User targeted by a user context-menu command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<User>,
    /// Message targeted by a message context-menu command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_message_id: Option<MessageId>,
    /// Message containing the component that was interacted with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interacted_message_id: Option<MessageId>,
    /// Metadata of the interaction that spawned the modal, for modal
    /// submit interactions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggering_interaction_metadata: Option<Box<MessageInteractionMetadata>>,
}

/// Deprecated Discord `MessageInteraction` object still delivered on some
/// payloads (`Message::interaction`). Prefer
/// [`Message::interaction_metadata`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageInteraction {
    pub id: InteractionId,
    #[serde(rename = "type")]
    pub kind: InteractionType,
    #[serde(default)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MessagePin`.
pub struct MessagePin {
    pub pinned_at: String,
    pub message: Message,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChannelPins`.
pub struct ChannelPins {
    #[serde(default)]
    pub items: Vec<MessagePin>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `ChannelPinsQuery`.
pub struct ChannelPinsQuery {
    pub before: Option<String>,
    pub limit: Option<u64>,
}

/// Query options for `GET /channels/{id}/messages/{id}/reactions/{emoji}`
/// ([`RestClient::get_reactions_with_query`](crate::http::RestClient::get_reactions_with_query)).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GetReactionsQuery {
    /// Normal or burst (super) reactions; Discord defaults to
    /// [`ReactionType::NORMAL`] when omitted.
    pub kind: Option<ReactionType>,
    /// Return users after this user ID.
    pub after: Option<UserId>,
    /// Max number of users to return (1-100, Discord's default 25).
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AllowedMentions`.
pub struct AllowedMentions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<RoleId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_user: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SearchGuildMessagesResponse`.
pub struct SearchGuildMessagesResponse {
    #[serde(default)]
    pub doing_deep_historical_index: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents_indexed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub total_results: u64,
    #[serde(default)]
    pub messages: Vec<Vec<Message>>,
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub members: Vec<ThreadMember>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `SearchGuildMessagesQuery`.
pub struct SearchGuildMessagesQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub max_id: Option<MessageId>,
    pub min_id: Option<MessageId>,
    pub slop: Option<u64>,
    pub content: Option<String>,
    pub channel_ids: Vec<ChannelId>,
    pub author_types: Vec<String>,
    pub author_ids: Vec<UserId>,
    pub mentions: Vec<UserId>,
    pub mentions_role_ids: Vec<RoleId>,
    pub mention_everyone: Option<bool>,
    pub replied_to_user_ids: Vec<UserId>,
    pub replied_to_message_ids: Vec<MessageId>,
    pub pinned: Option<bool>,
    pub has: Vec<String>,
    pub embed_types: Vec<String>,
    pub embed_providers: Vec<String>,
    pub link_hostnames: Vec<String>,
    pub attachment_filenames: Vec<String>,
    pub attachment_extensions: Vec<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub include_nsfw: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ChannelMention`.
pub struct ChannelMention {
    pub id: ChannelId,
    pub guild_id: GuildId,
    #[serde(rename = "type")]
    pub kind: ChannelType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageReference`.
pub struct MessageReference {
    /// Reply (`DEFAULT`) or forward. Discord treats an absent `type` as
    /// [`MessageReferenceType::DEFAULT`].
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<MessageReferenceType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_if_not_exists: Option<bool>,
}

impl MessageReference {
    /// Builds a reply reference to a message in the same channel.
    pub fn reply(message_id: MessageId) -> Self {
        Self {
            kind: Some(MessageReferenceType::DEFAULT),
            message_id: Some(message_id),
            ..Self::default()
        }
    }

    /// Builds a forward reference; Discord attaches the referenced message
    /// to the new message as a `message_snapshots` entry.
    pub fn forward(channel_id: ChannelId, message_id: MessageId) -> Self {
        Self {
            kind: Some(MessageReferenceType::FORWARD),
            channel_id: Some(channel_id),
            message_id: Some(message_id),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ReactionCountDetails`.
pub struct ReactionCountDetails {
    #[serde(default)]
    pub burst: u64,
    #[serde(default)]
    pub normal: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateMessage`.
pub struct CreateMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker_ids: Option<Vec<StickerId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<CreatePoll>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub enforce_nonce: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Reaction`.
pub struct Reaction {
    #[serde(default)]
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count_details: Option<ReactionCountDetails>,
    #[serde(default)]
    pub me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
}
