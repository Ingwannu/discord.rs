use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ChannelPins, ChannelPinsQuery, CreateMessage, Message, SearchGuildMessagesQuery,
    SearchGuildMessagesResponse, Snowflake, User,
};

use super::paths::query_string;
use super::{FileAttachment, RestClient};

impl RestClient {
    pub async fn create_message(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/messages", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_message_with_files(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        self.request_typed_multipart(
            Method::POST,
            &format!("/channels/{}/messages", channel_id.into()),
            body,
            files,
        )
        .await
    }

    pub async fn update_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/channels/{}/messages/{}",
                channel_id.into(),
                message_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn update_message_with_files(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        self.request_typed_multipart(
            Method::PATCH,
            &format!(
                "/channels/{}/messages/{}",
                channel_id.into(),
                message_id.into()
            ),
            body,
            files,
        )
        .await
    }

    pub async fn get_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/messages/{}",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_channel_messages(
        &self,
        channel_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<Vec<Message>, DiscordError> {
        let path = match limit {
            Some(limit) => format!("/channels/{}/messages?limit={}", channel_id.into(), limit),
            None => format!("/channels/{}/messages", channel_id.into()),
        };
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn search_guild_messages(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &SearchGuildMessagesQuery,
    ) -> Result<SearchGuildMessagesResponse, DiscordError> {
        let query = search_guild_messages_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/messages/search{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn bulk_delete_messages(
        &self,
        channel_id: impl Into<Snowflake>,
        message_ids: Vec<Snowflake>,
    ) -> Result<(), DiscordError> {
        let body = serde_json::json!({ "messages": message_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>() });
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/messages/bulk-delete", channel_id.into()),
            Some(&body),
        )
        .await
    }

    pub async fn add_reaction(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/@me",
            channel_id.into(),
            message_id.into(),
            emoji
        );
        self.request_no_content(Method::PUT, &path, Option::<&Value>::None)
            .await
    }

    pub async fn remove_reaction(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/@me",
            channel_id.into(),
            message_id.into(),
            emoji
        );
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub(crate) async fn edit_message_json(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!(
                "/channels/{}/messages/{}",
                channel_id.into(),
                message_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/messages/{}",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn crosspost_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!(
                "/channels/{}/messages/{}/crosspost",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_channel_messages_paginated(
        &self,
        channel_id: impl Into<Snowflake>,
        limit: Option<u64>,
        before: Option<Snowflake>,
        after: Option<Snowflake>,
        around: Option<Snowflake>,
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
            format!("/channels/{}/messages", channel_id.into())
        } else {
            format!(
                "/channels/{}/messages?{}",
                channel_id.into(),
                params.join("&")
            )
        };
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn get_reactions(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        emoji: &str,
        limit: Option<u64>,
        after: Option<Snowflake>,
    ) -> Result<Vec<User>, DiscordError> {
        let mut params = Vec::new();
        if let Some(limit) = limit {
            params.push(format!("limit={limit}"));
        }
        if let Some(after) = after {
            params.push(format!("after={after}"));
        }
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}{}",
            channel_id.into(),
            message_id.into(),
            emoji,
            query
        );
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn get_channel_pins(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ChannelPinsQuery,
    ) -> Result<ChannelPins, DiscordError> {
        let query = channel_pins_query(query);
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/messages/pins{query}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn remove_user_reaction(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        emoji: &str,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let path = format!(
            "/channels/{}/messages/{}/reactions/{}/{}",
            channel_id.into(),
            message_id.into(),
            emoji,
            user_id.into()
        );
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub async fn remove_all_reactions(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/messages/{}/reactions",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn remove_all_reactions_for_emoji(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        emoji: &str,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/messages/{}/reactions/{}",
                channel_id.into(),
                message_id.into(),
                emoji
            ),
            Option::<&Value>::None,
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

fn push_snowflakes(params: &mut Vec<String>, name: &str, values: &[Snowflake]) {
    params.extend(values.iter().map(|value| format!("{name}={value}")));
}

fn push_strings(params: &mut Vec<String>, name: &str, values: &[String]) {
    params.extend(values.iter().map(|value| format!("{name}={value}")));
}
