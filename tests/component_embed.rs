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

use discordrs::request::RawComponent;
use discordrs::request::{
    ComponentEmbed, ComponentEmbedBuilder, ComponentEmbedError, PreviewActionRow, PreviewComponent,
    PreviewContainer, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem, PreviewSection,
    PreviewSeparator, PreviewTextDisplay, PreviewThumbnail,
};
use discordrs::{
    ActionRowBuilder, ComponentId, ContainerBuilder, CustomButtonBuilder, DiscordError, Emoji,
    FileBuilder, LinkButtonBuilder, MediaGalleryBuilder, MediaGalleryItem, Nullable, Patch,
    PremiumButtonBuilder, SectionBuilder, SelectMenuBuilder, SeparatorBuilder, SeparatorSpacing,
    TextDisplayBuilder, ThumbnailBuilder,
};
use serde_json::{json, Value};

const SPEC_FULL_EXAMPLE: &str = include_str!("fixtures/component_embed/spec_full_example.json");
const SPEC_PATCH_NOTES: &str = include_str!("fixtures/component_embed/spec_patch_notes.json");
const GOLDEN_FULL_EXAMPLE: &str = include_str!("fixtures/component_embed/full_example.golden.json");
const GOLDEN_PATCH_NOTES_TAG: &str =
    include_str!("fixtures/component_embed/patch_notes.golden.html");

const SITE: &str = "https://watchanimeattheoffice.com";
const PATCH_NOTES: &str = "# Patch Notes\n- Fixed a bug where certain treasure chests wouldn't open properly\n- Improved server stability during peak hours\n- Added a new type of gravity that will randomly apply when the moon is full\n- Every third thursday the furniture will scream its darkest secrets to nearby npcs";

/// A raw component holding `value`.
fn raw(value: Value) -> RawComponent {
    RawComponent::from_value(&value).expect("valid JSON")
}

/// The encoder's `\uXXXX` escape for one character.
fn escaped(character: char) -> String {
    format!("\\u{:04x}", u32::from(character))
}

/// Expected encoder output with the escapes spelled as `$lt`, `$gt`, `$amp`,
/// `$ls` (U+2028), `$ps` (U+2029), `$fffe`, and `$ffff`, which keeps long
/// expectations readable.
fn with_escapes(template: &str) -> String {
    template
        .replace("$lt", &escaped('<'))
        .replace("$gt", &escaped('>'))
        .replace("$amp", &escaped('&'))
        .replace("$ls", &escaped('\u{2028}'))
        .replace("$ps", &escaped('\u{2029}'))
        .replace("$fffe", &escaped('\u{fffe}'))
        .replace("$ffff", &escaped('\u{ffff}'))
}

/// XML 1.0 `Char` production: what an XHTML page may contain at all.
fn is_xml_char(character: char) -> bool {
    matches!(
        u32::from(character),
        0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10_FFFF
    )
}

/// Text content that an XML parser accepts inside an element: only XML
/// characters, no markup start (`<`), no `&` (the encoder emits no
/// entities), and no CDATA end.
fn assert_xml_text(text: &str) {
    assert!(
        text.chars().all(is_xml_char),
        "non-XML character in {text:?}"
    );
    assert!(!text.contains('<') && !text.contains('&') && !text.contains("]]>"));
}

/// A tiny well-formedness check for one self-closing element with
/// double-quoted attributes, as XML requires: `<name a="v" b="v" />`, where
/// every `&` in a value starts a predefined entity reference and no value
/// contains `<` or `"`.
fn assert_well_formed_empty_element(tag: &str, name: &str) {
    let mut rest = tag
        .strip_prefix(&format!("<{name}"))
        .and_then(|rest| rest.strip_suffix(" />"))
        .unwrap_or_else(|| panic!("not a self-closing <{name} />: {tag}"));
    let mut names = Vec::new();
    while !rest.is_empty() {
        let attribute = rest.strip_prefix(' ').unwrap_or_else(|| panic!("{tag}"));
        let (attribute_name, after) = attribute
            .split_once("=\"")
            .unwrap_or_else(|| panic!("{tag}"));
        assert!(
            !attribute_name.is_empty()
                && attribute_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "{tag}"
        );
        assert!(
            !names.contains(&attribute_name),
            "duplicate attribute in {tag}"
        );
        names.push(attribute_name);
        let (value, after) = after.split_once('"').unwrap_or_else(|| panic!("{tag}"));
        assert!(!value.contains('<'), "{tag}");
        for (index, _) in value.match_indices('&') {
            let reference = value.get(index..).unwrap_or_default();
            assert!(
                ["&amp;", "&quot;", "&lt;", "&gt;", "&apos;"]
                    .iter()
                    .any(|entity| reference.starts_with(entity)),
                "bare & in {tag}"
            );
        }
        assert!(value.chars().all(is_xml_char), "{tag}");
        rest = after;
    }
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
        .add_component(PreviewSeparator::new().spacing(SeparatorSpacing::SMALL))
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
        .id(ComponentId::new(1))
        .accent_color(0)
        .add_component(
            PreviewSection::new(
                PreviewLinkButton::new("https://e.co")
                    .id(ComponentId::new(3))
                    .label("Go")
                    .emoji(Emoji::unicode("🔗"))
                    .disabled(true),
            )
            .id(ComponentId::new(2))
            .add_text_display(PreviewTextDisplay::new("x").id(ComponentId::new(4))),
        )
        .add_component(
            PreviewSeparator::new()
                .id(ComponentId::new(5))
                .divider(false)
                .spacing(SeparatorSpacing::LARGE),
        )
        .add_component(
            PreviewMediaGallery::new().id(ComponentId::new(6)).add_item(
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

#[test]
fn output_holds_only_xml_characters() {
    // Every C0 control, the two XML-forbidden noncharacters, and the edges of
    // the allowed ranges, including characters that stay raw.
    let mut content: String = (0_u8..0x20).map(char::from).collect();
    content.push_str("\u{7f}\u{85}\u{d7ff}\u{e000}\u{fdd0}\u{fffd}\u{fffe}\u{ffff}");
    content.push_str("\u{1fffe}\u{10ffff}");
    let embed = text_embed(&content);
    let json = embed.to_json().expect("valid");
    assert_xml_text(&json);
    assert_eq!(
        parse(&json)["component"]["components"][0]["content"],
        content.as_str()
    );

    let tag = embed.to_script_tag().expect("valid");
    let text = tag
        .strip_prefix(r#"<script id="discord:component-embed" type="application/json">"#)
        .and_then(|rest| rest.strip_suffix("</script>"))
        .expect("script element");
    assert_xml_text(text);

    let pair = text_embed("a\u{fffe}b\u{ffff}c").to_json().expect("valid");
    assert_eq!(
        pair,
        with_escapes(
            r#"{"component":{"type":17,"components":[{"type":10,"content":"a$fffeb$ffffc"}]}}"#
        )
    );
    // 3 bytes of UTF-8 become 6 bytes of escape.
    assert!(text_embed_of_size(3_000, "\u{ffff}", 6).validate().is_ok());
    assert!(text_embed_of_size(3_001, "\u{fffe}", 6).validate().is_err());
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
fn separator_spacing_is_small_or_large() {
    let spaced = |spacing: SeparatorSpacing| {
        ComponentEmbed::builder().add_component(PreviewSeparator::new().spacing(spacing))
    };
    assert!(spaced(SeparatorSpacing::SMALL).validate().is_ok());
    assert!(spaced(SeparatorSpacing::LARGE).validate().is_ok());
    for raw in [0, 3, u8::MAX] {
        assert_eq!(
            expect_error(spaced(SeparatorSpacing::new(raw)).validate()),
            ComponentEmbedError::InvalidSeparatorSpacing {
                path: "component.components[0].spacing".to_owned(),
                spacing: SeparatorSpacing::new(raw),
            }
        );
    }
    // An unknown value converts (the type is open) and is rejected on output.
    let document = json!({"component": {"type": 17, "components": [{"type": 14, "spacing": 3}]}});
    let converted = ComponentEmbed::try_from(&document).expect("shape is fine");
    assert_eq!(
        converted.component.components,
        [PreviewComponent::from(
            PreviewSeparator::new().spacing(SeparatorSpacing::new(3))
        )]
    );
    assert_eq!(
        expect_error(converted.to_json()).to_string(),
        "component.components[0].spacing must be 1 (small) or 2 (large), got 3"
    );
}

#[test]
fn component_ids_are_unique_except_zero() {
    let duplicate = ComponentEmbed::builder()
        .add_component(PreviewTextDisplay::new("a").id(ComponentId::new(7)))
        .add_component(PreviewSeparator::new().id(ComponentId::new(7)));
    assert_eq!(
        expect_error(duplicate.validate()),
        ComponentEmbedError::DuplicateComponentId {
            path: "component.components[1]".to_owned(),
            id: ComponentId::new(7)
        }
    );
    let nested = ComponentEmbed::builder()
        .id(ComponentId::new(3))
        .add_component(
            PreviewSection::new(
                PreviewLinkButton::new("https://e.co")
                    .label("b")
                    .id(ComponentId::new(3)),
            )
            .add_text("t"),
        );
    assert!(matches!(
        expect_error(nested.validate()),
        ComponentEmbedError::DuplicateComponentId { path, id }
            if path == "component.components[0].accessory" && id == ComponentId::new(3)
    ));
    let zeros = ComponentEmbed::builder()
        .id(ComponentId::new(0))
        .add_component(PreviewTextDisplay::new("a").id(ComponentId::new(0)))
        .add_component(PreviewSeparator::new().id(ComponentId::new(0)));
    assert!(zeros.validate().is_ok());
}

#[test]
fn build_never_validates_and_try_build_does() {
    let builder = ComponentEmbedBuilder::new().accent_color(u32::MAX);
    let embed = builder.clone().build();
    assert_eq!(
        embed.component.accent_color,
        Patch::Value(Nullable::Value(u32::MAX))
    );
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
        // A preview-safe raw child converts through the JSON reader.
        .add_component(raw(json!({"type": 10, "id": 1, "content": "hello"})))
        .add_section(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("read more"))
                .set_button_accessory(LinkButtonBuilder::new("https://e.co/a").label("Open")),
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
                LinkButtonBuilder::new("https://e.co")
                    .label("Site")
                    .emoji_unicode("🔗")
                    .disabled(true),
            ),
        );
    let expected = ComponentEmbed::builder()
        .accent_color(0x5865F2)
        .spoiler(false)
        .add_component(PreviewTextDisplay::new("hello").id(ComponentId::new(1)))
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
                .spacing(SeparatorSpacing::SMALL),
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
            .add_button(LinkButtonBuilder::new("https://e.co").label("Site"))
            .add_button(CustomButtonBuilder::new("vote").label("Vote")),
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
            .set_button_accessory(CustomButtonBuilder::new("vote").label("Vote")),
    );
    assert_eq!(
        expect_error(PreviewContainer::try_from(as_accessory)),
        ComponentEmbedError::ButtonNotLink {
            path: "component.components[0].accessory".to_owned(),
            style: 1
        }
    );
    // Premium (SKU) buttons are not link buttons either.
    let premium_row = ContainerBuilder::new().add_action_row(ActionRowBuilder::new().add_button(
        PremiumButtonBuilder::new(discordrs::model::SkuId::try_new(1).expect("nonzero")),
    ));
    assert_eq!(
        expect_error(PreviewContainer::try_from(premium_row)),
        ComponentEmbedError::ButtonNotLink {
            path: "component.components[0].components[0]".to_owned(),
            style: 6
        }
    );
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
    let select = ContainerBuilder::new().add_action_row(SelectMenuBuilder::string("pick"));
    assert_eq!(
        unsupported(select),
        ("component.components[0].components[0]".to_owned(), 3)
    );
    // A text input cannot be put in a message row; as raw JSON it still fails.
    let text_input = ContainerBuilder::new().add_component(raw(json!({
        "type": 1,
        "components": [{"type": 4, "custom_id": "name", "label": "Name", "style": 1}]
    })));
    assert_eq!(unsupported(text_input).1, 4);
    let file = ContainerBuilder::new().add_file(FileBuilder::new("attachment://a.zip"));
    assert_eq!(
        unsupported(file),
        ("component.components[0]".to_owned(), 13)
    );
    // A bare thumbnail and a nested container are not container children
    // in the typed model; raw JSON can still say them, and fails.
    let bare_thumbnail = ContainerBuilder::new().add_component(raw(
        json!({"type": 11, "media": {"url": "https://e.co/t.png"}}),
    ));
    assert_eq!(unsupported(bare_thumbnail).1, 11);
    let unknown = ContainerBuilder::new().add_component(raw(json!({"type": 255, "name": "raw"})));
    assert_eq!(unsupported(unknown).1, 255);

    let nested = ContainerBuilder::new().add_component(raw(json!({
        "type": 17,
        "components": [{"type": 10, "content": "inner"}]
    })));
    assert_eq!(
        expect_error(PreviewContainer::try_from(nested)),
        ComponentEmbedError::NestedContainer {
            path: "component.components[0]".to_owned()
        }
    );

    let not_a_container = serde_json::to_value(ActionRowBuilder::new().build()).expect("json");
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
    // The section builder cannot build a section without an accessory (it
    // tracks the accessory in its type); as JSON the field is named.
    let no_accessory = json!({"type": 9, "components": [{"type": 10, "content": "t"}]});
    assert_eq!(
        expect_error(PreviewComponent::try_from(&no_accessory)),
        ComponentEmbedError::InvalidPayload {
            path: "$".to_owned(),
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
    // Spacing must fit the open `SeparatorSpacing` (a `u8`); whether it is 1
    // or 2 is a limit, checked on output (`separator_spacing_is_small_or_large`).
    let separator = json!({"type": 14, "spacing": 300});
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

/// Sets (`Some`) or removes (`None`) `key` in the object at `pointer`.
fn with_field(mut document: Value, pointer: &str, key: &str, state: Option<Value>) -> Value {
    let object = document
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .expect("object at pointer");
    match state {
        Some(value) => {
            object.insert(key.to_owned(), value);
        }
        None => {
            object.remove(key);
        }
    }
    document
}

fn text_document() -> Value {
    json!({"component": {"type": 17, "components": [{"type": 10, "content": "x"}]}})
}

#[test]
fn nullable_fields_keep_absent_null_and_value_apart() {
    let cases = [
        (
            text_document(),
            "/component",
            "accent_color",
            json!(0x5865F2),
        ),
        (
            json!({"component": {"type": 17, "components": [{"type": 9,
                "components": [{"type": 10, "content": "t"}],
                "accessory": {"type": 11, "media": {"url": "https://e.co/t.png"}}}]}}),
            "/component/components/0/accessory",
            "description",
            json!("alt"),
        ),
        (
            json!({"component": {"type": 17, "components": [{"type": 12,
                "items": [{"media": {"url": "https://e.co/a.png"}}]}]}}),
            "/component/components/0/items/0",
            "description",
            json!("alt"),
        ),
    ];
    for (base, pointer, key, value) in cases {
        for state in [None, Some(Value::Null), Some(value)] {
            let document = with_field(base.clone(), pointer, key, state.clone());
            let embed = ComponentEmbed::try_from(&document).expect("convertible");
            let round_trip = parse(&embed.to_json().expect("valid"));
            assert_eq!(round_trip, document, "{pointer}/{key} = {state:?}");
        }
    }

    // The three states as the model holds them.
    let accent = |state: Option<Value>| {
        let document = with_field(text_document(), "/component", "accent_color", state);
        ComponentEmbed::try_from(&document)
            .expect("convertible")
            .component
            .accent_color
    };
    assert_eq!(accent(None), Patch::Missing);
    assert_eq!(accent(Some(Value::Null)), Patch::null());
    assert_eq!(accent(Some(json!(7))), Patch::Value(Nullable::Value(7)));

    // Built by hand: an explicit null is written, a missing field is not.
    let mut thumbnail = PreviewThumbnail::new("https://e.co/t.png");
    thumbnail.description = Patch::null();
    let json = thumbnail_embed(thumbnail).to_json().expect("valid");
    assert!(json.contains(r#""description":null"#), "{json}");
    let json = thumbnail_embed(PreviewThumbnail::new("https://e.co/t.png"))
        .to_json()
        .expect("valid");
    assert!(!json.contains("description"), "{json}");
}

#[test]
fn null_on_a_non_nullable_field_fails_with_its_path() {
    let null_path = |result: Result<PreviewComponent, ComponentEmbedError>| match result {
        Err(ComponentEmbedError::NullNotAllowed { path }) => path,
        other => panic!("expected NullNotAllowed, got {other:?}"),
    };
    let button = json!({"type": 2, "style": 5, "url": "https://e.co", "label": "b"});
    for key in ["id", "url", "label", "emoji", "disabled", "style", "type"] {
        let button = with_field(button.clone(), "", key, Some(Value::Null));
        let row = json!({"type": 1, "components": [button]});
        assert_eq!(
            null_path(PreviewComponent::try_from(&row)),
            format!("$.components[0].{key}")
        );
    }
    // Emoji `id` is nullable in Discord's emoji object, but the shared
    // `Emoji` type cannot hold an explicit null, so it fails rather than
    // being dropped.
    for key in ["id", "name", "animated"] {
        let emoji = with_field(json!({"name": "🔗"}), "", key, Some(Value::Null));
        let button = with_field(button.clone(), "", "emoji", Some(emoji));
        let row = json!({"type": 1, "components": [button]});
        assert_eq!(
            null_path(PreviewComponent::try_from(&row)),
            format!("$.components[0].emoji.{key}")
        );
    }
    let cases = [
        (json!({"type": 14, "spacing": null}), "$.spacing"),
        (json!({"type": 14, "divider": null}), "$.divider"),
        (json!({"type": 14, "id": null}), "$.id"),
        (json!({"type": 10, "content": null}), "$.content"),
        (json!({"type": 1, "components": null}), "$.components"),
        (json!({"type": 12, "items": null}), "$.items"),
        (
            json!({"type": 12, "items": [{"media": {"url": "https://e.co/a.png"}, "spoiler": null}]}),
            "$.items[0].spoiler",
        ),
        (
            json!({"type": 12, "items": [{"media": null}]}),
            "$.items[0].media",
        ),
        (
            json!({"type": 12, "items": [{"media": {"url": null}}]}),
            "$.items[0].media.url",
        ),
        (
            json!({"type": 9, "components": [{"type": 10, "content": "t"}], "accessory": null}),
            "$.accessory",
        ),
        (
            json!({"type": 9, "components": [{"type": 10, "content": "t"}],
                "accessory": {"type": 11, "media": {"url": "https://e.co/t.png"}, "spoiler": null}}),
            "$.accessory.spoiler",
        ),
    ];
    for (value, path) in cases {
        assert_eq!(
            null_path(PreviewComponent::try_from(&value)),
            path,
            "{value}"
        );
    }
    for (document, path) in [
        (json!({"component": null}), "$.component"),
        (
            json!({"component": {"type": 17, "spoiler": null, "components": []}}),
            "component.spoiler",
        ),
        (
            json!({"component": {"type": 17, "id": null, "components": []}}),
            "component.id",
        ),
    ] {
        assert_eq!(
            expect_error(ComponentEmbed::try_from(&document)),
            ComponentEmbedError::NullNotAllowed {
                path: path.to_owned()
            }
        );
    }
    // The same through the message builders' raw escape hatch.
    let container =
        ContainerBuilder::new().add_component(raw(json!({"type": 14, "spacing": null})));
    assert_eq!(
        expect_error(PreviewContainer::try_from(container)),
        ComponentEmbedError::NullNotAllowed {
            path: "component.components[0].spacing".to_owned()
        }
    );
}

// ---------------------------------------------------------------------------
// Link tag, errors

// ---------------------------------------------------------------------------

#[test]
fn link_tag_escapes_the_href_and_requires_an_absolute_http_url() {
    let href_of = |href: &str| {
        let tag = ComponentEmbed::link_tag(href).expect("absolute http(s) URL");
        assert_well_formed_empty_element(&tag, "link");
        assert!(tag.is_ascii(), "{tag:?}");
        tag.strip_prefix(r#"<link rel="discord:component-embed" type="application/json" href=""#)
            .and_then(|rest| rest.strip_suffix(r#"" />"#))
            .expect("link element")
            .to_owned()
    };
    // An already-normal URL comes out unchanged.
    assert_eq!(
        href_of("https://watchanimeattheoffice.com/embeds/article.json"),
        "https://watchanimeattheoffice.com/embeds/article.json"
    );
    // The parser's serialization, attribute-escaped: `&` becomes `&amp;`;
    // `"`, `<`, and `>` in a query are percent-encoded by the parser.
    assert_eq!(
        ComponentEmbed::link_tag("https://cdn.example.com/embed.json?a=1&b=\"2\"<>")
            .expect("absolute"),
        "<link rel=\"discord:component-embed\" type=\"application/json\" \
         href=\"https://cdn.example.com/embed.json?a=1&amp;b=%222%22%3C%3E\" />"
    );
    // Non-ASCII, including XML-forbidden U+FFFF, is percent-encoded; hosts
    // become punycode and lowercase.
    assert_eq!(
        href_of("https://example.com/a\u{ffff}b.json"),
        "https://example.com/a%EF%BF%BFb.json"
    );
    assert_eq!(
        href_of("HTTPS://B\u{fc}cher.Example/\u{e9}.json"),
        "https://xn--bcher-kva.example/%C3%A9.json"
    );
    assert_eq!(href_of("https://example.com"), "https://example.com/");

    for rejected in [
        "/embeds/article.json",
        "javascript:alert(1)",
        "embed.json",
        "ftp://example.com/embed.json",
        // Characters the parser would silently strip or remove.
        "\0https://example.com/embed.json",
        "https://example.com/embed.json\u{1f}",
        "https://exam\tple.com/embed.json",
        "https://example.com/em\nbed.json",
        "https://example.com/embed\u{7f}.json",
        " https://example.com/embed.json",
        "https://example.com/embed.json ",
    ] {
        assert_eq!(
            expect_error(ComponentEmbed::link_tag(rejected)),
            ComponentEmbedError::InvalidLinkHref {
                href: rejected.to_owned()
            },
            "{rejected:?}"
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
