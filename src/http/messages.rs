use crate::error::DiscordError;
use crate::model::{
    ChannelId, ChannelPins, ChannelPinsQuery, GetReactionsQuery, GuildId, Id, IdMarker, Message,
    MessageId, MessageReference, SearchGuildMessagesQuery, SearchGuildMessagesResponse, User,
    UserId,
};
use crate::request::{CreateMessage, EditMessage};

use super::body::{message_json_body, message_multipart_body};
use super::paths::query_string;
use super::{json_body, multipart_body, FileAttachment, RestClient, RouteId, NO_BODY};

impl RestClient {
    /// Sends a message. The client's default `allowed_mentions` applies when
    /// the body sets none. With `nonce` and `enforce_nonce` set, a send that
    /// fails ambiguously (a 5xx or a lost response) is retried within
    /// Discord's deduplication window, which cannot post a second message.
    pub async fn create_message(
        &self,
        channel_id: ChannelId,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let encoded = match body.check_voice_files([]) {
            Ok(()) => message_json_body(
                &body.allowed_mentions_or(self.default_allowed_mentions()),
                body.nonce_replay().is_some(),
            ),
            Err(error) => Err(error),
        };
        self.send(
            RouteId::CreateMessage,
            &format!("/channels/{}/messages", channel_id),
            encoded,
        )
        .await
    }

    /// Forwards a message into another channel using a
    /// `MessageReferenceType::FORWARD` reference — the equivalent of
    /// discord.js's `message.forward(channel)`.
    pub async fn forward_message(
        &self,
        from_channel_id: ChannelId,
        message_id: MessageId,
        to_channel_id: ChannelId,
    ) -> Result<Message, DiscordError> {
        let body = CreateMessage::new()
            .message_reference(MessageReference::forward(from_channel_id, message_id));
        self.create_message(to_channel_id, &body).await
    }

    /// [`Self::create_message`] with uploads: `payload_json` plus one
    /// `files[n]` part per file. Describe them in `body.attachments`. A
    /// voice message is checked locally first (one `audio/*` file).
    pub async fn create_message_with_files(
        &self,
        channel_id: ChannelId,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let encoded =
            match body.check_voice_files(files.iter().map(|file| file.content_type.as_deref())) {
                Ok(()) => message_multipart_body(
                    &body.allowed_mentions_or(self.default_allowed_mentions()),
                    files,
                    body.nonce_replay().is_some(),
                ),
                Err(error) => Err(error),
            };
        self.send(
            RouteId::CreateMessage,
            &format!("/channels/{}/messages", channel_id),
            encoded,
        )
        .await
    }

    /// Edits a message. Omitted fields keep their value; list every
    /// attachment the message keeps in `body.attachments`. The client's
    /// default `allowed_mentions` applies when the body sets none.
    pub async fn edit_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        body: &EditMessage,
    ) -> Result<Message, DiscordError> {
        self.send(
            RouteId::EditMessage,
            &format!("/channels/{}/messages/{}", channel_id, message_id),
            json_body(&body.allowed_mentions_or(self.default_allowed_mentions())),
        )
        .await
    }

    /// [`Self::edit_message`] with new uploads (`files[n]`).
    pub async fn edit_message_with_files(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        body: &EditMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        self.send(
            RouteId::EditMessage,
            &format!("/channels/{}/messages/{}", channel_id, message_id),
            multipart_body(
                &body.allowed_mentions_or(self.default_allowed_mentions()),
                files,
            ),
        )
        .await
    }

    pub async fn get_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Message, DiscordError> {
        self.send(
            RouteId::GetChannelMessage,
            &format!("/channels/{}/messages/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_channel_messages(
        &self,
        channel_id: ChannelId,
        limit: Option<u64>,
    ) -> Result<Vec<Message>, DiscordError> {
        let path = match limit {
            Some(limit) => format!("/channels/{}/messages?limit={}", channel_id, limit),
            None => format!("/channels/{}/messages", channel_id),
        };
        self.send(RouteId::GetChannelMessages, &path, NO_BODY).await
    }

    pub async fn search_guild_messages(
        &self,
        guild_id: GuildId,
        query: &SearchGuildMessagesQuery,
    ) -> Result<SearchGuildMessagesResponse, DiscordError> {
        let query = search_guild_messages_query(query);
        self.send(
            RouteId::SearchGuildMessages,
            &format!("/guilds/{}/messages/search{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn bulk_delete_messages(
        &self,
        channel_id: ChannelId,
        message_ids: Vec<MessageId>,
    ) -> Result<(), DiscordError> {
        let body = serde_json::json!({ "messages": message_ids });
        self.send_unit(
            RouteId::BulkDeleteMessages,
            &format!("/channels/{}/messages/bulk-delete", channel_id),
            json_body(&body),
        )
        .await
    }

    pub async fn add_reaction(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/@me",
            channel_id, message_id, emoji
        );
        self.send_unit(RouteId::CreateReaction, &path, NO_BODY)
            .await
    }

    pub async fn remove_reaction(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/@me",
            channel_id, message_id, emoji
        );
        self.send_unit(RouteId::DeleteOwnReaction, &path, NO_BODY)
            .await
    }

    pub async fn delete_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteMessage,
            &format!("/channels/{}/messages/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn crosspost_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Message, DiscordError> {
        self.send(
            RouteId::CrosspostMessage,
            &format!("/channels/{}/messages/{}/crosspost", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_channel_messages_paginated(
        &self,
        channel_id: ChannelId,
        limit: Option<u64>,
        before: Option<MessageId>,
        after: Option<MessageId>,
        around: Option<MessageId>,
    ) -> Result<Vec<Message>, DiscordError> {
        let mut params = Vec::new();
        if let Some(limit) = limit {
            params.push(format!("limit={limit}"));
        }
        if let Some(before) = before {
            params.push(format!("before={before}"));
        }
        if let Some(after) = after {
            params.push(format!("after={after}"));
        }
        if let Some(around) = around {
            params.push(format!("around={around}"));
        }
        let path = if params.is_empty() {
            format!("/channels/{}/messages", channel_id)
        } else {
            format!("/channels/{}/messages?{}", channel_id, params.join("&"))
        };
        self.send(RouteId::GetChannelMessages, &path, NO_BODY).await
    }

    /// Users who reacted with `emoji` (normal reactions; see
    /// [`Self::get_reactions_with_query`] for burst reactions).
    pub async fn get_reactions(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
        limit: Option<u64>,
        after: Option<UserId>,
    ) -> Result<Vec<User>, DiscordError> {
        self.get_reactions_with_query(
            channel_id,
            message_id,
            emoji,
            &GetReactionsQuery {
                kind: None,
                after,
                limit,
            },
        )
        .await
    }

    /// Users who reacted with `emoji`, with every documented query option
    /// including the reaction `type` (normal or burst).
    pub async fn get_reactions_with_query(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
        query: &GetReactionsQuery,
    ) -> Result<Vec<User>, DiscordError> {
        let mut params = Vec::new();
        if let Some(kind) = query.kind {
            params.push(format!("type={}", kind.get()));
        }
        if let Some(limit) = query.limit {
            params.push(format!("limit={limit}"));
        }
        if let Some(after) = query.after {
            params.push(format!("after={after}"));
        }
        let query = query_string(params);
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}{}",
            channel_id, message_id, emoji, query
        );
        self.send(RouteId::GetReactions, &path, NO_BODY).await
    }

    pub async fn get_channel_pins(
        &self,
        channel_id: ChannelId,
        query: &ChannelPinsQuery,
    ) -> Result<ChannelPins, DiscordError> {
        let query = channel_pins_query(query);
        self.send(
            RouteId::GetChannelPins,
            &format!("/channels/{}/messages/pins{query}", channel_id),
            NO_BODY,
        )
        .await
    }

    pub async fn remove_user_reaction(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/{}",
            channel_id, message_id, emoji, user_id
        );
        self.send_unit(RouteId::DeleteUserReaction, &path, NO_BODY)
            .await
    }

    pub async fn remove_all_reactions(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteAllReactions,
            &format!("/channels/{}/messages/{}/reactions", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn remove_all_reactions_for_emoji(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteAllReactionsForEmoji,
            &format!(
                "/channels/{}/messages/{}/reactions/{}",
                channel_id, message_id, emoji
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn pin_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::PinMessage,
            &format!("/channels/{}/messages/pins/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn unpin_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::UnpinMessage,
            &format!("/channels/{}/messages/pins/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_pinned_messages(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Message>, DiscordError> {
        self.send(
            RouteId::GetPinnedMessagesDeprecated,
            &format!("/channels/{}/pins", channel_id),
            NO_BODY,
        )
        .await
    }

    /// Calls the legacy `pin_message` route.
    pub async fn pin_message_legacy(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::PinMessageDeprecated,
            &format!("/channels/{}/pins/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }

    /// Calls the legacy `unpin_message` route.
    pub async fn unpin_message_legacy(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::UnpinMessageDeprecated,
            &format!("/channels/{}/pins/{}", channel_id, message_id),
            NO_BODY,
        )
        .await
    }
}

fn channel_pins_query(query: &ChannelPinsQuery) -> String {
    let mut params = Vec::new();
    if let Some(before) = &query.before {
        params.push(format!("before={before}"));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    query_string(params)
}

fn search_guild_messages_query(query: &SearchGuildMessagesQuery) -> String {
    let mut params = Vec::new();
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(offset) = query.offset {
        params.push(format!("offset={offset}"));
    }
    if let Some(max_id) = &query.max_id {
        params.push(format!("max_id={max_id}"));
    }
    if let Some(min_id) = &query.min_id {
        params.push(format!("min_id={min_id}"));
    }
    if let Some(slop) = query.slop {
        params.push(format!("slop={slop}"));
    }
    if let Some(content) = &query.content {
        params.push(format!("content={content}"));
    }
    push_snowflakes(&mut params, "channel_id", &query.channel_ids);
    push_strings(&mut params, "author_type", &query.author_types);
    push_snowflakes(&mut params, "author_id", &query.author_ids);
    push_snowflakes(&mut params, "mentions", &query.mentions);
    push_snowflakes(&mut params, "mentions_role_id", &query.mentions_role_ids);
    if let Some(mention_everyone) = query.mention_everyone {
        params.push(format!("mention_everyone={mention_everyone}"));
    }
    push_snowflakes(
        &mut params,
        "replied_to_user_id",
        &query.replied_to_user_ids,
    );
    push_snowflakes(
        &mut params,
        "replied_to_message_id",
        &query.replied_to_message_ids,
    );
    if let Some(pinned) = query.pinned {
        params.push(format!("pinned={pinned}"));
    }
    push_strings(&mut params, "has", &query.has);
    push_strings(&mut params, "embed_type", &query.embed_types);
    push_strings(&mut params, "embed_provider", &query.embed_providers);
    push_strings(&mut params, "link_hostname", &query.link_hostnames);
    push_strings(
        &mut params,
        "attachment_filename",
        &query.attachment_filenames,
    );
    push_strings(
        &mut params,
        "attachment_extension",
        &query.attachment_extensions,
    );
    if let Some(sort_by) = &query.sort_by {
        params.push(format!("sort_by={sort_by}"));
    }
    if let Some(sort_order) = &query.sort_order {
        params.push(format!("sort_order={sort_order}"));
    }
    if let Some(include_nsfw) = query.include_nsfw {
        params.push(format!("include_nsfw={include_nsfw}"));
    }
    query_string(params)
}

fn push_snowflakes<M: IdMarker>(params: &mut Vec<String>, name: &str, values: &[Id<M>]) {
    params.extend(values.iter().map(|value| format!("{name}={value}")));
}

fn push_strings(params: &mut Vec<String>, name: &str, values: &[String]) {
    params.extend(values.iter().map(|value| format!("{name}={value}")));
}
