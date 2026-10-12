//! Wire goldens for the phase 5 request models (`discordrs::request`):
//! every message body and component variant, serialized and compared with
//! `tests/fixtures/requests/golden.json`, which spells out the exact keys
//! and values (numeric `type`/`style`, omitted vs `null` vs `[]`, `false`
//! and `0` kept). Plus: the status card built with the 3.0 builders means
//! the same JSON as the 2.x body (`status_card_2x.json`, captured from
//! a68eb8d), and known numeric tags come first in every component object.
//!
//! To regenerate the golden file after an intended wire change, run with
//! `DISCORDRS_BLESS=1` and review the diff.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the test"
)]

use std::collections::BTreeMap;

use discordrs::bitfield::message_flags;
use discordrs::model::{
    AllowedMentions, AttachmentId, ChannelId, ChannelType, ComponentId, EmojiId, ForumTagId,
    MentionableId, MessageId, MessageReference, Nullable, Patch, PollLayoutType, RoleId,
    SeparatorSpacing, SharedClientTheme, SkuId, StickerId, UploadId, UserId,
};
use discordrs::request::{
    ActionRow, AttachmentUpload, ButtonRow, ChannelSelect, Component, Container,
    ContainerComponent, CreateFollowup, CreateMessage, CreatePoll, CreatePollAnswer, CustomButton,
    EditMessage, EditWebhookMessage, Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedImage,
    ExecuteWebhook, ExistingAttachment, FileComponent, InteractionMessage, LinkButton,
    MediaGallery, MentionableSelect, MessageAttachment, PremiumButton, RawComponent, RoleSelect,
    Section, SelectRow, Separator, StringSelect, TextDisplay, Thumbnail, UpdateMessage, UserSelect,
};
use discordrs::{
    ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    Emoji, LinkButtonBuilder, MediaGalleryBuilder, MediaGalleryItem, MessageBuilder,
    SectionBuilder, SelectOption, SeparatorBuilder, TextDisplayBuilder, ThumbnailBuilder,
};
use serde::Serialize;
use serde_json::Value;

const GOLDEN: &str = "tests/fixtures/requests/golden.json";
const STATUS_CARD_2X: &str = include_str!("fixtures/requests/status_card_2x.json");

fn user(value: u64) -> UserId {
    UserId::try_new(value).unwrap()
}

fn role(value: u64) -> RoleId {
    RoleId::try_new(value).unwrap()
}

fn channel(value: u64) -> ChannelId {
    ChannelId::try_new(value).unwrap()
}

/// Every case: a name and the value it serializes to.
fn cases() -> Vec<(&'static str, Value)> {
    fn v<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).expect("serializable")
    }

    let custom_full = {
        let mut button = CustomButton::new(ButtonStyle::DANGER, "delete");
        button.id = Some(ComponentId::new(0));
        button.label = Some(String::new());
        button.emoji = Some(Emoji::custom("trash", EmojiId::try_new(11).unwrap(), false));
        button.disabled = Some(false);
        button
    };
    let link_full = {
        let mut button = LinkButton::new("https://example.com");
        button.id = Some(ComponentId::new(3));
        button.label = Some("Docs".to_owned());
        button.emoji = Some(Emoji::unicode("📘"));
        button.disabled = Some(false);
        button
    };
    let premium_full = {
        let mut button = PremiumButton::new(SkuId::try_new(1_234_567_890_123).unwrap());
        button.id = Some(ComponentId::new(4));
        button.disabled = Some(true);
        button
    };
    let string_full = {
        let mut select = StringSelect::new("pick");
        select.id = Some(ComponentId::new(5));
        select.placeholder = Some(String::new());
        select.min_values = Some(0);
        select.max_values = Some(25);
        select.required = Some(false);
        select.disabled = Some(false);
        select.options = vec![
            SelectOption::new("One", "1"),
            SelectOption::new("Two", "2")
                .description("second")
                .emoji("2️⃣")
                .default_selected(false),
        ];
        select
    };
    let user_select = {
        let mut select = UserSelect::new("who");
        select.default_values = vec![user(7)];
        select
    };
    let role_select = {
        let mut select = RoleSelect::new("roles");
        select.default_values = vec![role(8)];
        select.max_values = Some(2);
        select
    };
    let mentionable_select = {
        let mut select = MentionableSelect::new("any");
        select.default_values = vec![MentionableId::User(user(7)), MentionableId::Role(role(8))];
        select.max_values = Some(2);
        select
    };
    let channel_select = {
        let mut select = ChannelSelect::new("where");
        select.default_values = vec![channel(9)];
        select.channel_types = Some(vec![ChannelType::GUILD_TEXT, ChannelType::PUBLIC_THREAD]);
        select
    };
    let channel_select_empty_types = {
        let mut select = ChannelSelect::new("where");
        select.channel_types = Some(Vec::new());
        select
    };
    let thumbnail_full = {
        let mut thumbnail = Thumbnail::new("attachment://t.png");
        thumbnail.id = Some(ComponentId::new(12));
        thumbnail.description = Some(String::new());
        thumbnail.spoiler = Some(false);
        thumbnail
    };
    let section_thumbnail = {
        let mut section = Section::new(thumbnail_full.clone());
        section.id = Some(ComponentId::new(13));
        section.components = vec![TextDisplay::new("a"), TextDisplay::new("b")];
        section
    };
    let section_button = {
        let mut section = Section::new(link_full.clone());
        section.components = vec![TextDisplay::new("a")];
        section
    };
    let gallery = {
        let mut gallery = MediaGallery::new();
        gallery.id = Some(ComponentId::new(14));
        gallery.items = vec![
            MediaGalleryItem::new("https://e.co/1.png"),
            MediaGalleryItem::new("https://e.co/2.png")
                .description("")
                .spoiler(false),
        ];
        gallery
    };
    let file_full = {
        let mut file = FileComponent::new("attachment://a.zip");
        file.id = Some(ComponentId::new(15));
        file.spoiler = Some(false);
        file
    };
    let separator_full = {
        let mut separator = Separator::new();
        separator.id = Some(ComponentId::new(16));
        separator.divider = Some(false);
        separator.spacing = Some(SeparatorSpacing::LARGE);
        separator
    };
    let raw = RawComponent::from_json(r#"{"type":99,"future":[1,null]}"#).unwrap();
    let button_row = ButtonRow::new()
        .add_button(custom_full.clone())
        .add_button(link_full.clone())
        .add_button(premium_full.clone());
    let container_full = {
        let mut container = Container::new();
        container.id = Some(ComponentId::new(17));
        container.accent_color = Some(0);
        container.spoiler = Some(false);
        container.components = vec![
            ContainerComponent::from(TextDisplay::new("in")),
            ContainerComponent::from(ActionRow::from(SelectRow::new(user_select.clone()))),
            ContainerComponent::from(section_button.clone()),
            ContainerComponent::from(gallery.clone()),
            ContainerComponent::from(file_full.clone()),
            ContainerComponent::from(Separator::new()),
            ContainerComponent::from(raw.clone()),
        ];
        container
    };

    let embed_full = {
        let mut embed = Embed::new();
        embed.title = Some("T".to_owned());
        embed.description = Some(String::new());
        embed.url = Some("https://e.co".to_owned());
        embed.timestamp = Some("2026-10-12T00:00:00.000Z".to_owned());
        embed.color = Some(0);
        embed.footer = Some(EmbedFooter::new("f"));
        embed.image = Some(EmbedImage::new("https://e.co/i.png"));
        embed.thumbnail = Some(EmbedImage::new("https://e.co/t.png"));
        let mut author = EmbedAuthor::new("a");
        author.url = Some("https://e.co/a".to_owned());
        author.icon_url = Some("https://e.co/a.png".to_owned());
        embed.author = Some(author);
        embed.fields = vec![EmbedField::new("n", "v", false)];
        embed
    };
    let poll_full = {
        let mut poll = CreatePoll::new("Q?")
            .add_answer("yes")
            .add_answer(CreatePollAnswer::new("no").emoji(Emoji::unicode("👎")))
            .duration(0)
            .allow_multiselect(false);
        poll.layout_type = Some(PollLayoutType::DEFAULT);
        poll
    };
    let upload_full = {
        let mut upload = AttachmentUpload::new(UploadId::new(0))
            .filename("voice.ogg")
            .description("")
            .spoiler(false);
        upload.title = Some("t".to_owned());
        upload.duration_secs = Some(0.0);
        upload.waveform = Some(String::new());
        upload
    };
    let existing_full = {
        let mut existing = ExistingAttachment::new(AttachmentId::try_new(42).unwrap());
        existing.description = Some("alt".to_owned());
        existing.is_spoiler = Some(true);
        existing
    };
    let mentions = AllowedMentions {
        parse: Vec::new(),
        roles: vec![role(8)],
        users: Vec::new(),
        replied_user: Some(false),
    };

    let create_full = CreateMessage::new()
        .content("")
        .enforce_nonce("n-1")
        .tts(false)
        .embeds(Vec::new())
        .allowed_mentions(mentions.clone())
        .message_reference(MessageReference::reply(MessageId::try_new(5).unwrap()))
        .components(Vec::new())
        .sticker_ids(vec![StickerId::try_new(6).unwrap()])
        .attachments(vec![
            MessageAttachment::from(existing_full.clone()),
            MessageAttachment::from(upload_full.clone()),
        ])
        .flags(0)
        .poll(poll_full.clone())
        .shared_client_theme(SharedClientTheme {
            colors: vec!["5865F2".to_owned()],
            gradient_angle: 0,
            base_mix: 58,
            base_theme: None,
        });
    let interaction_full = InteractionMessage::new()
        .tts(false)
        .content("hi")
        .add_embed(embed_full.clone())
        .allowed_mentions(AllowedMentions::default())
        .ephemeral(true)
        .components(vec![ActionRow::from(button_row.clone()).into()])
        .attachments(Vec::new())
        .poll(CreatePoll::new("Q").add_answer("A"));
    let followup_full = CreateFollowup::from(interaction_full.clone());
    let execute_full = ExecuteWebhook::new()
        .content("c")
        .username("u")
        .avatar_url("https://e.co/a.png")
        .tts(false)
        .embeds(Vec::new())
        .allowed_mentions(AllowedMentions::default())
        .components(Vec::new())
        .attachments(Vec::new())
        .flags(message_flags::SUPPRESS_NOTIFICATIONS)
        .thread_name("t")
        .applied_tags(vec![ForumTagId::try_new(10).unwrap()])
        .poll(CreatePoll::new("Q").add_answer("A"));
    let edit_values = EditMessage::new()
        .content("")
        .embeds(Vec::new())
        .flags(message_flags::SUPPRESS_EMBEDS)
        .allowed_mentions(AllowedMentions::default())
        .components(Vec::new())
        .attachments(vec![MessageAttachment::keep(
            AttachmentId::try_new(42).unwrap(),
        )]);
    let edit_nulls = {
        let mut edit = EditMessage::new().clear_content();
        edit.allowed_mentions = Patch::Value(Nullable::Null);
        edit.attachments = Patch::Value(Nullable::Null);
        edit.components = Patch::Value(Nullable::Null);
        edit.embeds = Patch::Value(Nullable::Null);
        edit.flags = Patch::Value(Nullable::Null);
        edit
    };
    let edit_webhook = EditWebhookMessage::new()
        .content("c")
        .clear_embeds()
        .add_component(TextDisplay::new("x"))
        .flags(message_flags::IS_COMPONENTS_V2)
        .clear_attachments()
        .poll(CreatePoll::new("Q").add_answer("A"));
    let update = UpdateMessage::new()
        .clear_content()
        .components_v2(vec![TextDisplay::new("x").into()])
        .clear_poll();

    vec![
        // Buttons
        (
            "button.custom.minimal",
            v(CustomButton::new(ButtonStyle::PRIMARY, "go")),
        ),
        ("button.custom.full", v(custom_full)),
        (
            "button.link.minimal",
            v(LinkButton::new("https://example.com")),
        ),
        ("button.link.full", v(link_full)),
        (
            "button.premium.minimal",
            v(PremiumButton::new(SkuId::try_new(1).unwrap())),
        ),
        ("button.premium.full", v(premium_full)),
        // Rows and selects
        ("row.buttons", v(button_row)),
        ("row.buttons.empty", v(ButtonRow::new())),
        ("select.string.minimal", v(StringSelect::new("pick"))),
        ("select.string.full", v(SelectRow::new(string_full))),
        ("select.user", v(user_select)),
        ("select.role", v(role_select)),
        ("select.mentionable", v(mentionable_select)),
        ("select.channel", v(channel_select)),
        ("select.channel.empty_types", v(channel_select_empty_types)),
        // Layout and content
        ("text_display", v(TextDisplay::new(""))),
        ("thumbnail.minimal", v(Thumbnail::new("https://e.co/t.png"))),
        ("thumbnail.full", v(thumbnail_full)),
        ("section.thumbnail", v(section_thumbnail)),
        ("section.button", v(section_button)),
        ("media_gallery", v(gallery)),
        ("media_gallery.empty", v(MediaGallery::new())),
        ("file.minimal", v(FileComponent::new("attachment://a.zip"))),
        ("file.full", v(file_full)),
        ("separator.minimal", v(Separator::new())),
        ("separator.full", v(separator_full)),
        ("container.minimal", v(Container::new())),
        ("container.full", v(container_full)),
        ("raw", v(Component::from(raw))),
        // Embeds, polls, attachments
        ("embed.empty", v(Embed::new())),
        ("embed.full", v(embed_full)),
        ("poll.full", v(poll_full)),
        (
            "attachment.existing.minimal",
            v(MessageAttachment::keep(AttachmentId::try_new(42).unwrap())),
        ),
        ("attachment.existing.full", v(existing_full)),
        (
            "attachment.upload.minimal",
            v(MessageAttachment::upload(UploadId::new(3))),
        ),
        ("attachment.upload.full", v(upload_full)),
        // Message bodies
        ("create_message.empty", v(CreateMessage::new())),
        ("create_message.full", v(create_full)),
        ("edit_message.missing", v(EditMessage::new())),
        ("edit_message.values", v(edit_values)),
        ("edit_message.nulls", v(edit_nulls)),
        (
            "edit_message.v2_transition",
            v(EditMessage::new()
                .clear_content()
                .clear_embeds()
                .clear_stickers()
                .clear_poll()
                .components_v2(vec![TextDisplay::new("x").into()])),
        ),
        ("interaction_message.empty", v(InteractionMessage::new())),
        ("interaction_message.full", v(interaction_full)),
        ("update_message.empty", v(UpdateMessage::new())),
        ("update_message.v2", v(update)),
        ("create_followup.full", v(followup_full)),
        ("execute_webhook.empty", v(ExecuteWebhook::new())),
        ("execute_webhook.full", v(execute_full)),
        ("edit_webhook_message.missing", v(EditWebhookMessage::new())),
        ("edit_webhook_message.values", v(edit_webhook)),
        (
            "edit_webhook_message.clear_poll",
            v(EditWebhookMessage::new().clear_poll()),
        ),
    ]
}

#[test]
fn request_models_match_the_golden_wire_format() {
    let actual: BTreeMap<&str, Value> = cases().into_iter().collect();
    if std::env::var_os("DISCORDRS_BLESS").is_some() {
        let text = serde_json::to_string_pretty(&actual).unwrap();
        std::fs::write(GOLDEN, text + "\n").unwrap();
        return;
    }
    let golden: BTreeMap<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(GOLDEN).unwrap()).unwrap();
    for (name, value) in &actual {
        let expected = golden
            .get(*name)
            .unwrap_or_else(|| panic!("golden has no case {name}"));
        assert_eq!(value, expected, "case {name}");
    }
    let stale: Vec<&String> = golden
        .keys()
        .filter(|name| !actual.contains_key(name.as_str()))
        .collect();
    assert!(stale.is_empty(), "golden cases with no test: {stale:?}");
}

/// Every component object starts with its numeric `type` (and buttons
/// write `style` as a number), as Discord's examples do and as readers that
/// dispatch on the first key expect.
#[test]
fn component_objects_lead_with_a_numeric_type() {
    let message = MessageBuilder::new().components_v2(status_card()).build();
    let text = serde_json::to_string(&message).unwrap();
    // Every object with a `type` key opens with it.
    let component_openers = text.matches("{\"type\":").count();
    let type_keys = text.matches("\"type\":").count();
    assert_eq!(
        component_openers, type_keys,
        "a component object has a key before `type`: {text}"
    );
    assert!(!text.contains("\"type\":\""), "a string type tag: {text}");
    assert!(!text.contains("\"style\":\""), "a string style: {text}");
}

/// The status card of the perf report, built with the 3.0 builders.
fn status_card() -> ComponentsV2Message {
    let container = ContainerBuilder::new()
        .accent_color(5_814_783)
        .add_text_display(TextDisplayBuilder::new(
            "## Server status\nAll shards healthy. Gateway latency is normal.",
        ))
        .add_separator(
            SeparatorBuilder::new()
                .divider(true)
                .spacing(SeparatorSpacing::SMALL),
        )
        .add_section(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new(
                    "**Shard 0-7**: 1,204 guilds, p50 41 ms",
                ))
                .add_text_display(TextDisplayBuilder::new(
                    "**Shard 8-15**: 1,188 guilds, p50 39 ms",
                ))
                .set_thumbnail_accessory(
                    ThumbnailBuilder::new("https://example.com/static/status-ok.png")
                        .description("status icon"),
                ),
        )
        .add_media_gallery(
            MediaGalleryBuilder::new()
                .add_item(
                    MediaGalleryItem::new("https://example.com/charts/latency-24h.png")
                        .description("latency chart"),
                )
                .add_item(
                    MediaGalleryItem::new("https://example.com/charts/memory-24h.png")
                        .description("memory chart"),
                ),
        )
        .add_separator(
            SeparatorBuilder::new()
                .divider(false)
                .spacing(SeparatorSpacing::LARGE),
        )
        .add_action_row(
            ActionRowBuilder::new()
                .add_button(
                    CustomButtonBuilder::new("status:refresh")
                        .style(ButtonStyle::PRIMARY)
                        .label("Refresh"),
                )
                .add_button(
                    CustomButtonBuilder::new("status:details")
                        .style(ButtonStyle::SECONDARY)
                        .label("Details"),
                )
                .add_button(
                    LinkButtonBuilder::new("https://example.com/dashboard").label("Dashboard"),
                ),
        );
    ComponentsV2Message::new()
        .add_container(container)
        .add_text_display(TextDisplayBuilder::new("-# Updated <t:1791796641:R>"))
}

/// The typed body means exactly what the 2.x builders sent (key order
/// aside): same keys, same values, nothing missing or added.
#[test]
fn status_card_means_the_same_as_the_2x_body() {
    let message = MessageBuilder::new().components_v2(status_card()).build();
    message.validate().expect("the status card is valid");
    let typed = serde_json::to_value(&message).unwrap();
    let expected: Value = serde_json::from_str(STATUS_CARD_2X).unwrap();
    assert_eq!(typed, expected);
    // Same byte length too: only key order differs.
    assert_eq!(
        serde_json::to_vec(&message).unwrap().len(),
        serde_json::to_vec(&expected).unwrap().len()
    );
}
