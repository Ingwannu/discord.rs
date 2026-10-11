//! Component Embed generator (`discordrs::request`) against the spec,
//! discord-api-docs `developers/link-previews/component-embeds.mdx` at
//! `3714862`: golden documents (the spec's own examples are the fixtures),
//! every limit at its boundary and one past it, `<script>` escaping, and the
//! fallible conversion from the message builders. The compile-time half
//! (no custom-ID button in a preview row) is `tests/ui/preview_row_custom_id_button.rs`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the test"
)]

use discordrs::request::{
    ComponentEmbed, ComponentEmbedBuilder, ComponentEmbedError, PreviewActionRow, PreviewComponent,
    PreviewContainer, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem, PreviewSection,
    PreviewSeparator, PreviewTextDisplay, PreviewThumbnail, SeparatorSpacing,
};
use discordrs::{
    text_input_style, ActionRowBuilder, ButtonBuilder, ContainerBuilder, DiscordError, Emoji,
    FileBuilder, MediaGalleryBuilder, MediaGalleryItem, SectionBuilder, SelectMenuBuilder,
    SeparatorBuilder, TextDisplayBuilder, TextInputBuilder, ThumbnailBuilder,
};
use serde_json::{json, Value};

const SPEC_FULL_EXAMPLE: &str = include_str!("fixtures/component_embed/spec_full_example.json");
const SPEC_PATCH_NOTES: &str = include_str!("fixtures/component_embed/spec_patch_notes.json");
const GOLDEN_FULL_EXAMPLE: &str = include_str!("fixtures/component_embed/full_example.golden.json");
const GOLDEN_PATCH_NOTES_TAG: &str =
    include_str!("fixtures/component_embed/patch_notes.golden.html");

const SITE: &str = "https://watchanimeattheoffice.com";
const PATCH_NOTES: &str = "# Patch Notes\n- Fixed a bug where certain treasure chests wouldn't open properly\n- Improved server stability during peak hours\n- Added a new type of gravity that will randomly apply when the moon is full\n- Every third thursday the furniture will scream its darkest secrets to nearby npcs";

/// The encoder's `\uXXXX` escape for one character.
fn escaped(character: char) -> String {
    format!("\\u{:04x}", u32::from(character))
}

/// Expected encoder output with the escapes spelled as `$lt`, `$gt`, `$amp`,
/// `$ls` (U+2028), and `$ps` (U+2029), which keeps long expectations readable.
fn with_escapes(template: &str) -> String {
    template
        .replace("$lt", &escaped('<'))
        .replace("$gt", &escaped('>'))
        .replace("$amp", &escaped('&'))
        .replace("$ls", &escaped('\u{2028}'))
        .replace("$ps", &escaped('\u{2029}'))
}

fn parse(json: &str) -> Value {
    serde_json::from_str(json).expect("valid JSON")
}

fn expect_error<T: std::fmt::Debug>(result: Result<T, ComponentEmbedError>) -> ComponentEmbedError {
    result.expect_err("expected a component embed error")
}

/// The spec's "Full Example", built with the builders.
fn full_example() -> ComponentEmbed {
    ComponentEmbed::builder()
        .spoiler(false)
        .accent_color(1_752_220)
        .add_component(
            PreviewSection::new(
                PreviewLinkButton::new(format!("{SITE}/posts/version2")).label("Open"),
            )
            .add_text(format!(
                "# **[New version out!]({SITE}/posts/version2)**\nThe dungeon is deceptively tougher."
            )),
        )
        .add_component(
            PreviewMediaGallery::new()
                .add_item(
                    PreviewMediaItem::new(format!("{SITE}/photos/1.png"))
                        .description("The main character entering the dungeon"),
                )
                .add_item(PreviewMediaItem::new(format!("{SITE}/photos/2.png")))
                .add_item(PreviewMediaItem::new(format!("{SITE}/photos/3.png"))),
        )
        .add_text("*Screenshots of new content*")
        .add_component(PreviewSeparator::new().spacing(SeparatorSpacing::Small))
        .add_component(
            PreviewActionRow::new()
                .add_button(PreviewLinkButton::new(format!("{SITE}/store")).label("Store"))
                .add_button(PreviewLinkButton::new(format!("{SITE}/community")).label("Community"))
                .add_button(
                    PreviewLinkButton::new(format!("{SITE}/patch-notes")).label("Patch Notes"),
                ),
        )
        .add_component(PreviewSeparator::new())
        .add_component(
            PreviewSection::new(PreviewThumbnail::new(format!("{SITE}/new-boss.png")).spoiler(true))
                .add_text(format!(
                    "## **[??????]({SITE}/me)**\nWho could this be?"
                )),
        )
        .build()
}

fn patch_notes() -> ComponentEmbed {
    ComponentEmbed::builder().add_text(PATCH_NOTES).build()
}

/// Size of the document for one text display with `content`, unchecked.
fn text_embed(content: &str) -> ComponentEmbed {
    ComponentEmbed::builder().add_text(content).build()
}

fn thumbnail_embed(thumbnail: PreviewThumbnail) -> ComponentEmbed {
    ComponentEmbed::builder()
        .add_component(PreviewSection::new(thumbnail).add_text("t"))
        .build()
}

fn button_embed(button: PreviewLinkButton) -> ComponentEmbed {
    ComponentEmbed::builder()
        .add_component(PreviewActionRow::new().add_button(button))
        .build()
}

fn gallery(items: usize) -> PreviewMediaGallery {
    (0..items).fold(PreviewMediaGallery::new(), |gallery, index| {
        gallery.add_item(PreviewMediaItem::new(format!("https://e.co/{index}.png")))
    })
}

fn url_of_length(length: usize) -> String {
    let prefix = "https://e.co/";
    format!("{prefix}{}", "a".repeat(length - prefix.len()))
}

// ---------------------------------------------------------------------------
// Golden documents
// ---------------------------------------------------------------------------

#[test]
fn full_example_matches_the_spec_document() {
    let json = full_example().to_json().expect("the spec example is valid");
    assert_eq!(parse(&json), parse(SPEC_FULL_EXAMPLE));
    // Exact bytes: compact, `type` first, keys in a fixed order.
    assert_eq!(json, GOLDEN_FULL_EXAMPLE.trim_end());
}

#[test]
fn spec_documents_parse_into_the_same_model() {
    let parsed = ComponentEmbed::try_from(&parse(SPEC_FULL_EXAMPLE)).expect("spec example");
    assert_eq!(parsed, full_example());
    let parsed = ComponentEmbed::try_from(&parse(SPEC_PATCH_NOTES)).expect("spec example");
    assert_eq!(parsed, patch_notes());
}

#[test]
fn patch_notes_script_tag_is_golden() {
    let embed = patch_notes();
    assert_eq!(
        parse(&embed.to_json().expect("valid")),
        parse(SPEC_PATCH_NOTES)
    );
    assert_eq!(
        embed.to_script_tag().expect("valid"),
        GOLDEN_PATCH_NOTES_TAG.trim_end()
    );
}

#[test]
fn numeric_tags_and_link_button_keys() {
    let embed = ComponentEmbed::builder()
        .id(1)
        .accent_color(0)
        .add_component(
            PreviewSection::new(
                PreviewLinkButton::new("https://e.co")
                    .id(3)
                    .label("Go")
                    .emoji(Emoji::unicode("🔗"))
                    .disabled(true),
            )
            .id(2)
            .add_text_display(PreviewTextDisplay::new("x").id(4)),
        )
        .add_component(
            PreviewSeparator::new()
                .id(5)
                .divider(false)
                .spacing(SeparatorSpacing::Large),
        )
        .add_component(
            PreviewMediaGallery::new().id(6).add_item(
                PreviewMediaItem::new("https://e.co/a.png")
                    .description("alt")
                    .spoiler(true),
            ),
        )
        .build();
    let json = embed.to_json().expect("valid");
    assert_eq!(
        json,
        concat!(
            r#"{"component":{"type":17,"id":1,"accent_color":0,"components":["#,
            r#"{"type":9,"id":2,"components":[{"type":10,"id":4,"content":"x"}],"#,
            r#""accessory":{"type":2,"id":3,"style":5,"label":"Go","emoji":{"name":"🔗"},"#,
            r#""url":"https://e.co","disabled":true}},"#,
            r#"{"type":14,"id":5,"divider":false,"spacing":2},"#,
            r#"{"type":12,"id":6,"items":[{"media":{"url":"https://e.co/a.png"},"#,
            r#""description":"alt","spoiler":true}]}]}}"#
        )
    );
    // The only keys a button may carry ("Any other key invalidates the entire payload").
    let value = parse(&json);
    let button = &value["component"]["components"][0]["accessory"];
    let mut keys: Vec<&str> = button
        .as_object()
        .expect("button object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["disabled", "emoji", "id", "label", "style", "type", "url"]
    );
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

#[test]
fn script_breakout_sequences_are_escaped() {
    let hostile =
        "a</script><script>alert(1)</SCRIPT><!--<script>x & y > z ]]>\u{2028}\u{2029}é 한글 😀";
    let embed = text_embed(hostile);
    let json = embed.to_json().expect("valid");
    let expected = concat!(
        r#"{"component":{"type":17,"components":[{"type":10,"content":"#,
        r#""a$lt/script$gt$ltscript$gtalert(1)$lt/SCRIPT$gt"#,
        r#"$lt!--$ltscript$gtx $amp y $gt z ]]$gt$ls$ps"#,
        "é 한글 😀\"}]}}"
    );
    assert_eq!(json, with_escapes(expected));
    assert_eq!(escaped('<'), ["\\", "u003c"].concat());
    for forbidden in ['<', '>', '&', '\u{2028}', '\u{2029}'] {
        assert!(!json.contains(forbidden), "{forbidden:?} left unescaped");
    }
    // Still JSON for the same text, with non-ASCII kept as UTF-8.
    assert_eq!(
        parse(&json)["component"]["components"][0]["content"],
        hostile
    );
    assert!(json.contains("é 한글 😀"));

    let tag = embed.to_script_tag().expect("valid");
    assert!(tag.starts_with(r#"<script id="discord:component-embed" type="application/json">{"#));
    assert!(tag.ends_with("}</script>"));
    let lowercase = tag.to_ascii_lowercase();
    assert_eq!(lowercase.matches("</script").count(), 1, "{tag}");
    assert!(!tag.contains("<!--"));
    assert_eq!(tag.matches('<').count(), 2, "only the element's own tags");
}

#[test]
fn escapes_in_urls_and_labels_too() {
    let embed = button_embed(PreviewLinkButton::new("https://e.co/?a=1&b=<2>").label("<b>"));
    let json = embed.to_json().expect("valid");
    assert!(
        json.contains(&with_escapes(r#""url":"https://e.co/?a=1$ampb=$lt2$gt""#)),
        "{json}"
    );
    assert!(
        json.contains(&with_escapes(r#""label":"$ltb$gt""#)),
        "{json}"
    );
}

// ---------------------------------------------------------------------------
// Limits: boundary and boundary + 1
// ---------------------------------------------------------------------------

/// Builds a text embed whose escaped document is exactly `target` bytes,
/// spending `unit_bytes` per `unit` (escaped) and padding with ASCII.
fn text_embed_of_size(target: usize, unit: &str, unit_bytes: usize) -> ComponentEmbed {
    let overhead = text_embed("").to_json().expect("empty text").len();
    let budget = target - overhead;
    let content = format!(
        "{}{}",
        unit.repeat(budget / unit_bytes),
        "a".repeat(budget % unit_bytes)
    );
    let embed = text_embed(&content);
    assert_eq!(
        embed
            .to_json()
            .map(|json| json.len())
            .unwrap_or_else(|error| match error {
                ComponentEmbedError::PayloadTooLarge { bytes, .. } => bytes,
                other => panic!("unexpected {other:?}"),
            }),
        target
    );
    embed
}

#[test]
fn payload_limit_at_3000_bytes() {
    assert_eq!(ComponentEmbed::MAX_PAYLOAD_BYTES, 3_000);
    let at_limit = text_embed_of_size(3_000, "a", 1);
    assert_eq!(at_limit.to_json().expect("3000 bytes").len(), 3_000);
    assert!(at_limit.to_script_tag().is_ok());

    let over = text_embed_of_size(3_001, "a", 1);
    assert_eq!(
        expect_error(over.to_json()),
        ComponentEmbedError::PayloadTooLarge {
            bytes: 3_001,
            max: 3_000
        }
    );
    assert_eq!(
        expect_error(over.validate()),
        expect_error(over.to_script_tag())
    );
}

#[test]
fn payload_limit_counts_escapes_at_full_length() {
    // Each `<` is 1 byte of text but 6 bytes (`<`) of document.
    let at_limit = text_embed_of_size(3_000, "<", 6);
    let raw = serde_json::to_string(&at_limit).expect("plain JSON");
    assert!(
        raw.len() < 1_000,
        "unescaped it would be only {} bytes",
        raw.len()
    );
    assert!(at_limit.validate().is_ok());

    let over = text_embed_of_size(3_001, "<", 6);
    assert!(matches!(
        expect_error(over.validate()),
        ComponentEmbedError::PayloadTooLarge { bytes: 3_001, .. }
    ));

    // U+2028 is 3 bytes of UTF-8 but 6 bytes escaped.
    assert!(text_embed_of_size(3_000, "\u{2028}", 6).validate().is_ok());
    assert!(text_embed_of_size(3_001, "\u{2028}", 6).validate().is_err());
}

#[test]
fn payload_limit_counts_utf8_bytes_not_characters() {
    assert!(text_embed_of_size(3_000, "é", 2).validate().is_ok());
    assert!(text_embed_of_size(3_001, "é", 2).validate().is_err());
    assert!(text_embed_of_size(3_000, "😀", 4).validate().is_ok());
    assert!(text_embed_of_size(3_001, "😀", 4).validate().is_err());
}

#[test]
fn forty_components_including_the_container() {
    let with_separators = |count: usize| {
        (0..count).fold(ComponentEmbed::builder(), |builder, _| {
            builder.add_component(PreviewSeparator::new())
        })
    };
    assert!(with_separators(39).validate().is_ok());
    assert_eq!(
        expect_error(with_separators(40).validate()),
        ComponentEmbedError::TooManyComponents { count: 41, max: 40 }
    );

    // Buttons, section texts, and accessories count too: 1 + 6 rows × (1 + 5)
    // = 37, plus a section (1 + 1 text + 1 thumbnail) = 40.
    let row = (0..5).fold(PreviewActionRow::new(), |row, index| {
        row.add_button(PreviewLinkButton::new("https://e.co").label(format!("{index}")))
    });
    let nested = (0..6)
        .fold(ComponentEmbed::builder(), |builder, _| {
            builder.add_component(row.clone())
        })
        .add_component(
            PreviewSection::new(PreviewThumbnail::new("https://e.co/t.png")).add_text("t"),
        );
    assert!(nested.validate().is_ok());
    assert_eq!(
        expect_error(nested.add_text("one more").validate()),
        ComponentEmbedError::TooManyComponents { count: 41, max: 40 }
    );
}

#[test]
fn ten_gallery_items_across_the_embed() {
    let two_galleries = |first: usize, second: usize| {
        ComponentEmbed::builder()
            .add_component(gallery(first))
            .add_component(gallery(second))
    };
    assert!(two_galleries(5, 5).validate().is_ok());
    assert_eq!(
        expect_error(two_galleries(5, 6).validate()),
        ComponentEmbedError::TooManyGalleryItems { count: 11, max: 10 }
    );

    // Per gallery: 1-10 items.
    let one = |items: usize| ComponentEmbed::builder().add_component(gallery(items));
    assert!(one(1).validate().is_ok());
    assert!(one(10).validate().is_ok());
    for (items, count) in [(0, 0), (11, 11)] {
        assert_eq!(
            expect_error(one(items).validate()),
            ComponentEmbedError::ChildCountOutOfRange {
                path: "component.components[0].items".to_owned(),
                count,
                min: 1,
                max: 10,
            }
        );
    }

    // Thumbnails do not count toward the 10.
    let with_thumbnails = one(10).add_component(
        PreviewSection::new(PreviewThumbnail::new("https://e.co/t.png")).add_text("t"),
    );
    assert!(with_thumbnails.validate().is_ok());
}

#[test]
fn media_url_length_and_scheme() {
    assert_eq!(ComponentEmbed::MAX_MEDIA_URL_LENGTH, 2_048);
    assert!(thumbnail_embed(PreviewThumbnail::new(url_of_length(2_048)))
        .validate()
        .is_ok());
    assert_eq!(
        expect_error(thumbnail_embed(PreviewThumbnail::new(url_of_length(2_049))).validate()),
        ComponentEmbedError::UrlTooLong {
            path: "component.components[0].accessory.media.url".to_owned(),
            length: 2_049,
            max: 2_048,
        }
    );
    let gallery_url = |url: String| {
        ComponentEmbed::builder()
            .add_component(PreviewMediaGallery::new().add_item(PreviewMediaItem::new(url)))
            .build()
    };
    assert!(gallery_url(url_of_length(2_048)).validate().is_ok());
    assert!(matches!(
        expect_error(gallery_url(url_of_length(2_049)).validate()),
        ComponentEmbedError::UrlTooLong { path, length: 2_049, .. }
            if path == "component.components[0].items[0].media.url"
    ));

    for accepted in [
        "http://e.co/a.png",
        "https://e.co/a.png",
        "HTTPS://E.CO/A.PNG",
    ] {
        assert!(
            gallery_url(accepted.to_owned()).validate().is_ok(),
            "{accepted}"
        );
    }
    for rejected in [
        "ftp://e.co/a.png",
        "attachment://a.png",
        "data:image/png;base64,AAAA",
        "/images/a.png",
        "e.co/a.png",
    ] {
        assert_eq!(
            expect_error(gallery_url(rejected.to_owned()).validate()),
            ComponentEmbedError::MediaUrlNotHttp {
                path: "component.components[0].items[0].media.url".to_owned()
            },
            "{rejected}"
        );
    }
}

#[test]
fn media_description_length() {
    let thumbnail = |length: usize| {
        thumbnail_embed(PreviewThumbnail::new("https://e.co/t.png").description("é".repeat(length)))
    };
    assert!(thumbnail(1_024).validate().is_ok());
    assert_eq!(
        expect_error(thumbnail(1_025).validate()),
        ComponentEmbedError::TextTooLong {
            path: "component.components[0].accessory.description".to_owned(),
            length: 1_025,
            max: 1_024,
        }
    );
    let item = |length: usize| {
        ComponentEmbed::builder()
            .add_component(PreviewMediaGallery::new().add_item(
                PreviewMediaItem::new("https://e.co/a.png").description("d".repeat(length)),
            ))
            .build()
    };
    assert!(item(1_024).validate().is_ok());
    assert!(matches!(
        expect_error(item(1_025).validate()),
        ComponentEmbedError::TextTooLong { path, .. } if path == "component.components[0].items[0].description"
    ));
}

#[test]
fn link_button_url_label_and_content() {
    let with_url =
        |length: usize| button_embed(PreviewLinkButton::new(url_of_length(length)).label("b"));
    assert!(with_url(512).validate().is_ok());
    assert_eq!(
        expect_error(with_url(513).validate()),
        ComponentEmbedError::UrlTooLong {
            path: "component.components[0].components[0].url".to_owned(),
            length: 513,
            max: 512,
        }
    );

    // Labels count characters, not bytes.
    let with_label = |length: usize| {
        button_embed(PreviewLinkButton::new("https://e.co").label("é".repeat(length)))
    };
    assert!(with_label(80).validate().is_ok());
    assert_eq!(
        expect_error(with_label(81).validate()),
        ComponentEmbedError::TextTooLong {
            path: "component.components[0].components[0].label".to_owned(),
            length: 81,
            max: 80,
        }
    );

    // "with a `label`, an `emoji`, or both".
    assert_eq!(
        expect_error(button_embed(PreviewLinkButton::new("https://e.co")).validate()),
        ComponentEmbedError::ButtonWithoutLabelOrEmoji {
            path: "component.components[0].components[0]".to_owned()
        }
    );
    assert!(
        button_embed(PreviewLinkButton::new("https://e.co").emoji(Emoji::unicode("🔗")))
            .validate()
            .is_ok()
    );
    // Button URLs are only length-checked: Discord's link buttons accept
    // more than http(s).
    assert!(
        button_embed(PreviewLinkButton::new("discord://-/channels/@me").label("b"))
            .validate()
            .is_ok()
    );
}

#[test]
fn child_counts_of_rows_sections_and_the_container() {
    let row = |buttons: usize| {
        let row = (0..buttons).fold(PreviewActionRow::new(), |row, _| {
            row.add_button(PreviewLinkButton::new("https://e.co").label("b"))
        });
        ComponentEmbed::builder().add_component(row).build()
    };
    assert!(row(1).validate().is_ok());
    assert!(row(5).validate().is_ok());
    for count in [0, 6] {
        assert_eq!(
            expect_error(row(count).validate()),
            ComponentEmbedError::ChildCountOutOfRange {
                path: "component.components[0].components".to_owned(),
                count,
                min: 1,
                max: 5,
            }
        );
    }

    let section = |texts: usize| {
        let section = (0..texts).fold(
            PreviewSection::new(PreviewLinkButton::new("https://e.co").label("b")),
            |section, _| section.add_text("t"),
        );
        ComponentEmbed::builder().add_component(section).build()
    };
    assert!(section(1).validate().is_ok());
    assert!(section(3).validate().is_ok());
    for count in [0, 4] {
        assert_eq!(
            expect_error(section(count).validate()),
            ComponentEmbedError::ChildCountOutOfRange {
                path: "component.components[0].components".to_owned(),
                count,
                min: 1,
                max: 3,
            }
        );
    }

    assert_eq!(
        expect_error(ComponentEmbed::builder().validate()),
        ComponentEmbedError::EmptyContainer
    );
}

#[test]
fn accent_color_range() {
    let colored = |color: u32| ComponentEmbed::builder().accent_color(color).add_text("x");
    assert!(colored(0xFF_FFFF).validate().is_ok());
    assert_eq!(
        expect_error(colored(0x100_0000).validate()),
        ComponentEmbedError::AccentColorOutOfRange {
            value: 0x100_0000,
            max: 0xFF_FFFF
        }
    );
}

#[test]
fn component_ids_are_unique_except_zero() {
    let duplicate = ComponentEmbed::builder()
        .add_component(PreviewTextDisplay::new("a").id(7))
        .add_component(PreviewSeparator::new().id(7));
    assert_eq!(
        expect_error(duplicate.validate()),
        ComponentEmbedError::DuplicateComponentId {
            path: "component.components[1]".to_owned(),
            id: 7
        }
    );
    let nested = ComponentEmbed::builder().id(3).add_component(
        PreviewSection::new(PreviewLinkButton::new("https://e.co").label("b").id(3)).add_text("t"),
    );
    assert!(matches!(
        expect_error(nested.validate()),
        ComponentEmbedError::DuplicateComponentId { path, id: 3 } if path == "component.components[0].accessory"
    ));
    let zeros = ComponentEmbed::builder()
        .id(0)
        .add_component(PreviewTextDisplay::new("a").id(0))
        .add_component(PreviewSeparator::new().id(0));
    assert!(zeros.validate().is_ok());
}

#[test]
fn build_never_validates_and_try_build_does() {
    let builder = ComponentEmbedBuilder::new().accent_color(u32::MAX);
    let embed = builder.clone().build();
    assert_eq!(embed.component.accent_color, Some(u32::MAX));
    assert!(embed.component.components.is_empty());
    assert_eq!(
        expect_error(builder.clone().try_build()),
        expect_error(builder.validate())
    );
    let valid = ComponentEmbed::builder().add_text("ok");
    assert_eq!(valid.clone().try_build().expect("valid"), valid.build());
}

// ---------------------------------------------------------------------------
// Conversion from the message builders and raw payloads
// ---------------------------------------------------------------------------

#[test]
fn container_builder_with_preview_safe_parts_converts() {
    let container = ContainerBuilder::new()
        .accent_color(0x5865F2)
        .spoiler(false)
        // Raw JSON keeps this test independent of the builders' `id` setter type.
        .add_component(json!({"type": 10, "id": 1, "content": "hello"}))
        .add_section(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("read more"))
                .set_button_accessory(ButtonBuilder::new().label("Open").url("https://e.co/a")),
        )
        .add_section(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("pic"))
                .set_thumbnail_accessory(
                    ThumbnailBuilder::new("https://e.co/t.png")
                        .description("alt")
                        .spoiler(true),
                ),
        )
        .add_media_gallery(
            MediaGalleryBuilder::new()
                .add_item(MediaGalleryItem::new("https://e.co/1.png").description("one")),
        )
        .add_separator(SeparatorBuilder::new())
        .add_action_row(
            ActionRowBuilder::new().add_button(
                ButtonBuilder::new()
                    .label("Site")
                    .emoji_unicode("🔗")
                    .url("https://e.co")
                    .disabled(true),
            ),
        );
    let expected = ComponentEmbed::builder()
        .accent_color(0x5865F2)
        .spoiler(false)
        .add_component(PreviewTextDisplay::new("hello").id(1))
        .add_component(
            PreviewSection::new(PreviewLinkButton::new("https://e.co/a").label("Open"))
                .add_text("read more"),
        )
        .add_component(
            PreviewSection::new(
                PreviewThumbnail::new("https://e.co/t.png")
                    .description("alt")
                    .spoiler(true),
            )
            .add_text("pic"),
        )
        .add_component(
            PreviewMediaGallery::new()
                .add_item(PreviewMediaItem::new("https://e.co/1.png").description("one")),
        )
        // SeparatorBuilder spells out Discord's defaults; kept, not dropped.
        .add_component(
            PreviewSeparator::new()
                .divider(true)
                .spacing(SeparatorSpacing::Small),
        )
        .add_component(
            PreviewActionRow::new().add_button(
                PreviewLinkButton::new("https://e.co")
                    .label("Site")
                    .emoji(Emoji::unicode("🔗"))
                    .disabled(true),
            ),
        )
        .build();
    let converted = ComponentEmbed::try_from(container.clone()).expect("preview-safe container");
    assert_eq!(converted, expected);
    assert_eq!(
        PreviewContainer::try_from(container).expect("container"),
        expected.component
    );
    assert!(converted.validate().is_ok());
}

#[test]
fn custom_id_buttons_do_not_convert() {
    let in_row = ContainerBuilder::new().add_action_row(
        ActionRowBuilder::new()
            .add_button(ButtonBuilder::new().label("Site").url("https://e.co"))
            .add_button(ButtonBuilder::new().label("Vote").custom_id("vote")),
    );
    assert_eq!(
        expect_error(ComponentEmbed::try_from(in_row)),
        ComponentEmbedError::ButtonNotLink {
            path: "component.components[0].components[1]".to_owned(),
            style: 1
        }
    );
    let as_accessory = ContainerBuilder::new().add_section(
        SectionBuilder::new()
            .add_text_display(TextDisplayBuilder::new("t"))
            .set_button_accessory(ButtonBuilder::new().label("Vote").custom_id("vote")),
    );
    assert_eq!(
        expect_error(PreviewContainer::try_from(as_accessory)),
        ComponentEmbedError::ButtonNotLink {
            path: "component.components[0].accessory".to_owned(),
            style: 1
        }
    );
    // Premium (SKU) buttons are not link buttons either.
    let premium = json!({"type": 1, "components": [{"type": 2, "style": 6, "sku_id": "1"}]});
    assert!(matches!(
        expect_error(PreviewComponent::try_from(&premium)),
        ComponentEmbedError::ButtonNotLink { path, style: 6 } if path == "$.components[0]"
    ));
    // A link-styled button that also carries `custom_id` breaks the key rule.
    let extra_key =
        json!({"type": 2, "style": 5, "url": "https://e.co", "label": "x", "custom_id": "y"});
    let row = json!({"type": 1, "components": [extra_key]});
    assert_eq!(
        expect_error(PreviewComponent::try_from(&row)),
        ComponentEmbedError::UnsupportedField {
            path: "$.components[0]".to_owned(),
            field: "custom_id".to_owned()
        }
    );
}

#[test]
fn components_outside_the_preview_subset_do_not_convert() {
    let unsupported =
        |container: ContainerBuilder| match expect_error(PreviewContainer::try_from(container)) {
            ComponentEmbedError::UnsupportedComponent {
                path,
                component_type,
                ..
            } => (path, component_type),
            other => panic!("expected UnsupportedComponent, got {other:?}"),
        };
    let select = ContainerBuilder::new()
        .add_action_row(ActionRowBuilder::new().add_select_menu(SelectMenuBuilder::string("pick")));
    assert_eq!(
        unsupported(select),
        ("component.components[0].components[0]".to_owned(), 3)
    );
    let text_input =
        ContainerBuilder::new().add_action_row(ActionRowBuilder::new().add_text_input(
            TextInputBuilder::new("name", "Name", text_input_style::SHORT),
        ));
    assert_eq!(unsupported(text_input).1, 4);
    let file = ContainerBuilder::new().add_file(FileBuilder::new("attachment://a.zip"));
    assert_eq!(
        unsupported(file),
        ("component.components[0]".to_owned(), 13)
    );
    let bare_thumbnail =
        ContainerBuilder::new().add_component(ThumbnailBuilder::new("https://e.co/t.png").build());
    assert_eq!(unsupported(bare_thumbnail).1, 11);
    let raw = ContainerBuilder::new().add_component(json!({"type": 255, "name": "raw"}));
    assert_eq!(unsupported(raw).1, 255);

    let nested = ContainerBuilder::new().add_component(
        ContainerBuilder::new()
            .add_text_display(TextDisplayBuilder::new("inner"))
            .build(),
    );
    assert_eq!(
        expect_error(PreviewContainer::try_from(nested)),
        ComponentEmbedError::NestedContainer {
            path: "component.components[0]".to_owned()
        }
    );

    let not_a_container = ActionRowBuilder::new().build();
    assert!(matches!(
        expect_error(PreviewContainer::try_from(&not_a_container)),
        ComponentEmbedError::UnsupportedComponent {
            component_type: 1,
            allowed: "a container",
            ..
        }
    ));
}

#[test]
fn malformed_payloads_name_the_field() {
    let no_accessory = ContainerBuilder::new()
        .add_section(SectionBuilder::new().add_text_display(TextDisplayBuilder::new("t")));
    assert_eq!(
        expect_error(PreviewContainer::try_from(no_accessory)),
        ComponentEmbedError::InvalidPayload {
            path: "component.components[0]".to_owned(),
            reason: "missing required field `accessory`".to_owned()
        }
    );
    // Response-only media fields are not part of a preview payload.
    let gallery = json!({"type": 12, "items": [{"media": {"url": "https://e.co/a.png", "proxy_url": "https://p"}}]});
    assert_eq!(
        expect_error(PreviewComponent::try_from(&gallery)),
        ComponentEmbedError::UnsupportedField {
            path: "$.items[0].media".to_owned(),
            field: "proxy_url".to_owned()
        }
    );
    let separator = json!({"type": 14, "spacing": 3});
    assert!(matches!(
        expect_error(PreviewComponent::try_from(&separator)),
        ComponentEmbedError::InvalidPayload { path, .. } if path == "$.spacing"
    ));
    let document = json!({"component": {"type": 17, "components": []}, "extra": true});
    assert_eq!(
        expect_error(ComponentEmbed::try_from(&document)),
        ComponentEmbedError::UnsupportedField {
            path: "$".to_owned(),
            field: "extra".to_owned()
        }
    );
    // Conversion checks shape only; limits are checked on output.
    let empty = ComponentEmbed::try_from(&json!({"component": {"type": 17, "components": []}}))
        .expect("shape is fine");
    assert_eq!(
        expect_error(empty.to_json()),
        ComponentEmbedError::EmptyContainer
    );
}

// ---------------------------------------------------------------------------
// Link tag, errors
// ---------------------------------------------------------------------------

#[test]
fn link_tag_escapes_the_href_and_requires_an_absolute_http_url() {
    assert_eq!(
        ComponentEmbed::link_tag("https://cdn.example.com/embed.json?a=1&b=\"2\"<>")
            .expect("absolute"),
        "<link rel=\"discord:component-embed\" type=\"application/json\" \
         href=\"https://cdn.example.com/embed.json?a=1&amp;b=&quot;2&quot;&lt;&gt;\">"
    );
    for rejected in ["/embeds/article.json", "javascript:alert(1)", "embed.json"] {
        assert_eq!(
            expect_error(ComponentEmbed::link_tag(rejected)),
            ComponentEmbedError::InvalidLinkHref {
                href: rejected.to_owned()
            }
        );
    }
}

#[test]
fn errors_name_the_rule_and_convert_to_discord_error() {
    let error = expect_error(
        ComponentEmbed::builder()
            .add_component(gallery(6))
            .add_component(gallery(5))
            .validate(),
    );
    assert_eq!(
        error.to_string(),
        "component embed has 11 media gallery items across all galleries, over the limit of 10"
    );
    match DiscordError::from(error) {
        DiscordError::Model { message } => assert!(message.contains("media gallery items")),
        other => panic!("expected DiscordError::Model, got {other:?}"),
    }
    let unsupported = expect_error(PreviewComponent::try_from(
        &json!({"type": 3, "custom_id": "x"}),
    ));
    assert_eq!(
        unsupported.to_string(),
        "$: string select (type 3) is not allowed here in a component embed; expected an action \
         row, section, text display, media gallery, or separator"
    );
}
