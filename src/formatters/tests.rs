use crate::model::test_id;
use std::borrow::Cow;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::Value;

use super::*;
use crate::model::{ChannelId, GenericId, RoleId, UserId};

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

#[test]
fn styles_wrap_content() {
    assert_eq!(bold("x").to_string(), "**x**");
    assert_eq!(italic("x").to_string(), "_x_");
    assert_eq!(underline("x").to_string(), "__x__");
    assert_eq!(strikethrough("x").to_string(), "~~x~~");
    assert_eq!(spoiler("x").to_string(), "||x||");
    assert_eq!(inline_code("x").to_string(), "`x`");
    assert_eq!(quote("x").to_string(), "> x");
    assert_eq!(block_quote("x\ny").to_string(), ">>> x\ny");
    assert_eq!(subtext("x").to_string(), "-# x");
}

#[test]
fn styles_nest_without_intermediate_strings() {
    let nested = bold(italic(underline(strikethrough(spoiler("deep")))));
    assert_eq!(nested.to_string(), "**___~~||deep||~~___**");
    assert_eq!(format!("{}", quote(bold(42))), "> **42**");
}

#[test]
fn styles_accept_empty_and_unicode_content() {
    assert_eq!(bold("").to_string(), "****");
    assert_eq!(italic("한국어 ✨").to_string(), "_한국어 ✨_");
    assert_eq!(heading("", HeadingLevel::One).to_string(), "# ");
}

#[test]
fn code_blocks_with_and_without_language() {
    assert_eq!(code_block("let x = 1;").to_string(), "```\nlet x = 1;\n```");
    assert_eq!(
        code_block_with_language("rust", "fn main() {}").to_string(),
        "```rust\nfn main() {}\n```"
    );
    assert_eq!(code_block("").to_string(), "```\n\n```");
}

#[test]
fn code_block_content_can_be_defused() {
    let untrusted = "a ``` b";
    assert_eq!(clean_code_block_content(untrusted), "a `\u{200b}`` b");
    assert!(matches!(
        clean_code_block_content("plain"),
        Cow::Borrowed("plain")
    ));
    assert_eq!(
        code_block(clean_code_block_content("```")).to_string(),
        "```\n`\u{200b}``\n```"
    );
}

/// Deviation 4: discord.js replaces each ```` ``` ```` once, so a run of
/// four or more backticks still contains a closing fence.
#[test]
fn backtick_runs_of_any_length_cannot_close_a_code_block() {
    // discord.js 0.6.2: "````" -> "`\u{200b}```" (a fence remains)
    assert_eq!(clean_code_block_content("```"), "`\u{200b}``");
    assert_eq!(clean_code_block_content("````"), "`\u{200b}``\u{200b}`");
    assert_eq!(clean_code_block_content("``x``"), "``x``");
    for run in 3..=10 {
        let untrusted = format!("a{}rm -rf /", "`".repeat(run));
        let cleaned = clean_code_block_content(&untrusted);
        assert!(!cleaned.contains("```"), "run of {run}: {cleaned:?}");
        assert_eq!(cleaned.matches('`').count(), run, "backticks are kept");
        assert!(cleaned.starts_with('a') && cleaned.ends_with("rm -rf /"));
        let block = code_block(cleaned).to_string();
        assert_eq!(block.matches("```").count(), 2, "only the wrapper's fences");
    }
}

#[test]
fn headings_have_three_levels() {
    assert_eq!(heading("a", HeadingLevel::One).to_string(), "# a");
    assert_eq!(heading("a", HeadingLevel::Two).to_string(), "## a");
    assert_eq!(heading("a", HeadingLevel::Three).to_string(), "### a");
    assert_eq!(HeadingLevel::default(), HeadingLevel::One);
}

// ---------------------------------------------------------------------------
// Links, mentions, emoji
// ---------------------------------------------------------------------------

#[test]
fn links() {
    assert_eq!(
        hyperlink("docs", "https://discord.com").to_string(),
        "[docs](https://discord.com)"
    );
    assert_eq!(
        hyperlink_with_title("docs", "https://discord.com", "Discord").to_string(),
        "[docs](https://discord.com \"Discord\")"
    );
    assert_eq!(
        hyperlink_with_title("docs", "https://discord.com", "").to_string(),
        "[docs](https://discord.com)"
    );
    assert_eq!(
        hide_link_embed("https://discord.com").to_string(),
        "<https://discord.com>"
    );
    assert_eq!(
        channel_link(1u64, 2u64).to_string(),
        "https://discord.com/channels/1/2"
    );
    assert_eq!(
        dm_channel_link(2u64).to_string(),
        "https://discord.com/channels/@me/2"
    );
    assert_eq!(
        message_link(1u64, 2u64, 3u64).to_string(),
        "https://discord.com/channels/1/2/3"
    );
    assert_eq!(
        dm_message_link(2u64, 3u64).to_string(),
        "https://discord.com/channels/@me/2/3"
    );
}

#[test]
fn mentions_accept_any_id_representation() {
    let id: GenericId = test_id(80351110224678912);
    assert_eq!(user_mention(&id).to_string(), "<@80351110224678912>");
    assert_eq!(
        user_mention(80351110224678912u64).to_string(),
        "<@80351110224678912>"
    );
    assert_eq!(
        user_mention("80351110224678912").to_string(),
        "<@80351110224678912>"
    );
    assert_eq!(channel_mention(1u64).to_string(), "<#1>");
    assert_eq!(role_mention(1u64).to_string(), "<@&1>");
    assert_eq!(linked_role_mention(1u64).to_string(), "<id:linked-roles:1>");
    assert_eq!(game_profile_mention(1u64).to_string(), "<@$1>");
}

#[test]
fn slash_command_mentions() {
    let id = 816437322781949972u64;
    assert_eq!(
        slash_command_mention("airhorn", id).to_string(),
        "</airhorn:816437322781949972>"
    );
    assert_eq!(
        slash_command_mention("foo", id)
            .subcommand("bar")
            .to_string(),
        "</foo bar:816437322781949972>"
    );
    assert_eq!(
        slash_command_mention("foo", id)
            .subcommand_in_group("group", "bar")
            .to_string(),
        "</foo group bar:816437322781949972>"
    );
    // Picking a plain subcommand afterwards drops the group.
    let mention = slash_command_mention("foo", id)
        .subcommand_in_group("group", "bar")
        .subcommand("baz");
    assert_eq!(mention.to_string(), "</foo baz:816437322781949972>");
}

#[test]
fn custom_emoji_formats() {
    assert_eq!(
        custom_emoji("mmLol", 216154654256398347u64).to_string(),
        "<:mmLol:216154654256398347>"
    );
    assert_eq!(
        animated_emoji("b1nzy", 392938283556143104u64).to_string(),
        "<a:b1nzy:392938283556143104>"
    );
}

#[test]
fn guild_navigation_mentions() {
    assert_eq!(GuildNavigation::Customize.to_string(), "<id:customize>");
    assert_eq!(GuildNavigation::Browse.to_string(), "<id:browse>");
    assert_eq!(GuildNavigation::Guide.to_string(), "<id:guide>");
    assert_eq!(
        GuildNavigation::LinkedRoles.to_string(),
        "<id:linked-roles>"
    );
}

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

#[test]
fn timestamps_with_every_style() {
    let styles = [
        (TimestampStyle::ShortTime, 't'),
        (TimestampStyle::MediumTime, 'T'),
        (TimestampStyle::ShortDate, 'd'),
        (TimestampStyle::LongDate, 'D'),
        (TimestampStyle::LongDateShortTime, 'f'),
        (TimestampStyle::FullDateShortTime, 'F'),
        (TimestampStyle::ShortDateShortTime, 's'),
        (TimestampStyle::ShortDateMediumTime, 'S'),
        (TimestampStyle::RelativeTime, 'R'),
    ];
    assert_eq!(timestamp(1618953630).to_string(), "<t:1618953630>");
    for (style, letter) in styles {
        assert_eq!(style.as_char(), letter);
        assert_eq!(TimestampStyle::from_char(letter), Some(style));
        assert_eq!(style.to_string(), letter.to_string());
        assert_eq!(
            timestamp(1618953630).style(style).to_string(),
            format!("<t:1618953630:{letter}>")
        );
    }
    assert_eq!(TimestampStyle::from_char('x'), None);
}

#[test]
fn timestamps_from_system_time_floor_like_discord_js() {
    let after = UNIX_EPOCH + Duration::from_millis(1_618_953_630_999);
    assert_eq!(Timestamp::from(after).seconds(), 1_618_953_630);
    assert_eq!(Timestamp::from(UNIX_EPOCH).seconds(), 0);
    let before = UNIX_EPOCH - Duration::from_millis(1_500);
    assert_eq!(Timestamp::from(before).seconds(), -2);
    let whole_before = UNIX_EPOCH - Duration::from_secs(3);
    assert_eq!(Timestamp::from(whole_before).to_string(), "<t:-3>");
    assert_eq!(
        timestamp(-1)
            .style(TimestampStyle::RelativeTime)
            .to_string(),
        "<t:-1:R>"
    );
}

// ---------------------------------------------------------------------------
// Lists (expected strings produced by @discordjs/formatters 0.6.2)
// ---------------------------------------------------------------------------

#[test]
fn flat_lists_match_discord_js() {
    assert_eq!(unordered_list(&["a", "b"]).to_string(), "- a\n- b");
    assert_eq!(ordered_list(&["a", "b"], 1).to_string(), "1. a\n1. b");
    assert_eq!(ordered_list(&["a", "b"], 5).to_string(), "5. a\n5. b");
    assert_eq!(ordered_list(&["a"], 0).to_string(), "1. a");
    assert_eq!(unordered_list::<&str>(&[]).to_string(), "");
    assert_eq!(
        unordered_list(&[bold("x").to_string(), "y".to_string()]).to_string(),
        "- **x**\n- y"
    );
}

#[test]
fn nested_lists_match_discord_js() {
    use ListItem::{Item, Nested};
    let tree = [
        Item("a"),
        Nested(vec![Item("b"), Nested(vec![Item("c")]), Item("d")]),
        Item("e"),
    ];
    assert_eq!(
        unordered_list_nested(&tree).to_string(),
        "- a\n  - b\n    - c\n  - d\n- e"
    );
    assert_eq!(
        ordered_list_nested(&[Item("a"), Nested(vec![Item("b"), Item("c")])], 3).to_string(),
        "3. a\n  3. b\n  3. c"
    );
    assert_eq!(
        unordered_list_nested(&[Item("a"), Nested(vec![]), Item("b")]).to_string(),
        "- a\n\n- b"
    );
    assert_eq!(ordered_list_nested(&[Item("a")], 0).to_string(), "1. a");
}

// ---------------------------------------------------------------------------
// escape_markdown: golden corpus from @discordjs/formatters 0.6.2
// ---------------------------------------------------------------------------

fn options_from(value: &Value) -> EscapeMarkdownOptions {
    let mut options = EscapeMarkdownOptions::default();
    for (key, enabled) in value.as_object().expect("options object") {
        let enabled = enabled.as_bool().expect("boolean option");
        options = match key.as_str() {
            "codeBlock" => options.code_block(enabled),
            "inlineCode" => options.inline_code(enabled),
            "bold" => options.bold(enabled),
            "italic" => options.italic(enabled),
            "underline" => options.underline(enabled),
            "strikethrough" => options.strikethrough(enabled),
            "spoiler" => options.spoiler(enabled),
            "codeBlockContent" => options.code_block_content(enabled),
            "inlineCodeContent" => options.inline_code_content(enabled),
            "escape" => options.escape(enabled),
            "heading" => options.heading(enabled),
            "bulletedList" => options.bulleted_list(enabled),
            "numberedList" => options.numbered_list(enabled),
            "maskedLink" => options.masked_link(enabled),
            other => panic!("unknown option {other}"),
        };
    }
    options
}

/// `escape_golden.json` holds discord.js outputs for every input × option
/// set; regenerate it with `generate_escape_golden.js` (see its header).
#[test]
fn escape_markdown_matches_discord_js_golden_corpus() {
    let golden: Value =
        serde_json::from_str(include_str!("escape_golden.json")).expect("golden corpus");
    let inputs = golden["inputs"].as_array().expect("inputs");
    let option_sets: Vec<EscapeMarkdownOptions> = golden["options"]
        .as_array()
        .expect("options")
        .iter()
        .map(options_from)
        .collect();
    let mut checked = 0;
    for (input, outputs) in inputs
        .iter()
        .zip(golden["escape_markdown"].as_array().expect("outputs"))
    {
        let input = input.as_str().expect("input");
        for (options, expected) in option_sets.iter().zip(outputs.as_array().expect("row")) {
            assert_eq!(
                escape_markdown(input, *options),
                expected.as_str().expect("expected"),
                "input {input:?} with {options:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, inputs.len() * option_sets.len());
    assert!(checked > 1_500);

    type Escaper = fn(&str) -> Cow<'_, str>;
    let escapers: [(&str, Escaper); 12] = [
        ("escapeCodeBlock", escape_code_block),
        ("escapeInlineCode", escape_inline_code),
        ("escapeItalic", escape_italic),
        ("escapeBold", escape_bold),
        ("escapeUnderline", escape_underline),
        ("escapeStrikethrough", escape_strikethrough),
        ("escapeSpoiler", escape_spoiler),
        ("escapeEscape", escape_escape),
        ("escapeHeading", escape_heading),
        ("escapeBulletedList", escape_bulleted_list),
        ("escapeNumberedList", escape_numbered_list),
        ("escapeMaskedLink", escape_masked_link),
    ];
    for (name, escaper) in escapers {
        let outputs = golden["escapers"][name]
            .as_array()
            .expect("escaper outputs");
        assert_eq!(outputs.len(), inputs.len(), "{name}");
        for (input, expected) in inputs.iter().zip(outputs) {
            let input = input.as_str().expect("input");
            assert_eq!(
                escaper(input),
                expected.as_str().expect("expected"),
                "{name}({input:?})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// escape_markdown: options, allocation, and deliberate deviations
// ---------------------------------------------------------------------------

#[test]
fn default_options_match_discord_js_v14() {
    let options = EscapeMarkdownOptions::default();
    assert!(options.code_block && options.inline_code && options.bold && options.italic);
    assert!(options.underline && options.strikethrough && options.spoiler && options.escape);
    assert!(options.code_block_content && options.inline_code_content);
    assert!(!options.heading && !options.bulleted_list && !options.numbered_list);
    assert!(!options.masked_link && !options.quote && !options.block_quote);

    let all = EscapeMarkdownOptions::all();
    assert!(all.heading && all.bulleted_list && all.numbered_list);
    assert!(all.masked_link && all.quote && all.block_quote && all.bold);
}

#[test]
fn every_option_can_be_switched_off() {
    let none = EscapeMarkdownOptions::default()
        .code_block(false)
        .inline_code(false)
        .bold(false)
        .italic(false)
        .underline(false)
        .strikethrough(false)
        .spoiler(false)
        .escape(false);
    let text = "**a** _b_ __c__ ~~d~~ ||e|| `f` ```g``` \\h";
    assert!(matches!(escape_markdown(text, none), Cow::Borrowed(_)));
    let all_off = none
        .heading(false)
        .bulleted_list(false)
        .numbered_list(false)
        .masked_link(false)
        .quote(false)
        .block_quote(false);
    assert!(matches!(
        escape_markdown("# - 1. > [a](b)", all_off),
        Cow::Borrowed(_)
    ));
}

#[test]
fn escape_markdown_borrows_when_nothing_changes() {
    let options = EscapeMarkdownOptions::all();
    assert!(matches!(escape_markdown("", options), Cow::Borrowed("")));
    assert!(matches!(
        escape_markdown("plain 한국어 text", options),
        Cow::Borrowed(_)
    ));
    assert!(matches!(
        escape_markdown("plain", options.code_block_content(false)),
        Cow::Borrowed(_)
    ));
    assert!(matches!(
        escape_markdown("plain", options.inline_code_content(false)),
        Cow::Borrowed(_)
    ));
    assert!(matches!(escape_markdown("*x*", options), Cow::Owned(_)));
    assert!(matches!(escape_bold("plain"), Cow::Borrowed(_)));
}

#[test]
fn code_content_can_be_preserved() {
    let text = "**out** ```**in**``` **out**";
    let keep_blocks = EscapeMarkdownOptions::default()
        .code_block(false)
        .code_block_content(false);
    assert_eq!(
        escape_markdown(text, keep_blocks),
        "\\*\\*out\\*\\* ```**in**``` \\*\\*out\\*\\*"
    );
    // An unterminated fence does not protect the tail.
    assert_eq!(
        escape_markdown("a ``` **b**", keep_blocks),
        "a ``` \\*\\*b\\*\\*"
    );

    let keep_inline = EscapeMarkdownOptions::default()
        .inline_code(false)
        .inline_code_content(false);
    assert_eq!(
        escape_markdown("*a* `*b*` *c*", keep_inline),
        "\\*a\\* `*b*` \\*c\\*"
    );
}

#[test]
fn emoji_names_and_urls_keep_their_underscores() {
    let options = EscapeMarkdownOptions::default();
    assert_eq!(
        escape_markdown("<:my_emoji:123456789012345678>", options),
        "<:my_emoji:123456789012345678>"
    );
    assert_eq!(
        escape_markdown("https://example.com/a_b*c", options),
        "https://example.com/a_b*c"
    );
}

#[test]
fn http_without_a_scheme_separator_is_plain_text() {
    assert_eq!(
        escape_italic("httpx_a_ http:/_b_"),
        "httpx\\_a\\_ http:/\\_b\\_"
    );
    assert_eq!(escape_underline("https://__a"), "https://\\_\\_a");
}

/// Deviation 1: discord.js leaves every `_` after a `<:` on the same line
/// unescaped (its `<a?:.+` lookbehind), so `_italic_` there would render.
#[test]
fn underscores_after_an_emoji_are_still_escaped() {
    let text = "<:e_m:123456789012345678> _italic_ __under__";
    // discord.js 0.6.2: "<:e_m:123456789012345678> _italic_ __under__"
    assert_eq!(
        escape_markdown(text, EscapeMarkdownOptions::default()),
        "<:e_m:123456789012345678> \\_italic\\_ \\_\\_under\\_\\_"
    );
    // A stray `<:` is not an emoji tag either.
    assert_eq!(escape_italic("x <: _y_"), "x <: \\_y\\_");
}

/// Deviation 2: discord.js protects `*` inside `<scheme:/...>` autolinks
/// but, through a regex typo, escapes `_` there, changing the link.
#[test]
fn underscores_inside_autolinks_are_protected_like_stars() {
    // discord.js 0.6.2: "<https://example.com/a\\_b*c>"
    assert_eq!(
        escape_italic("<https://example.com/a_b*c>"),
        "<https://example.com/a_b*c>"
    );
    // Text before the `:/` is not part of the link.
    assert_eq!(escape_italic("<_x:/_y>"), "<\\_x:/_y>");
}

/// Deviation 3: discord.js's greedy `\[.+]\(.+\)` escapes only the first
/// `[` per line, so later links on that line still render.
#[test]
fn every_masked_link_on_a_line_is_escaped() {
    // discord.js 0.6.2: "\\[a](b) [c](d)"
    assert_eq!(escape_masked_link("[a](b) [c](d)"), "\\[a](b) \\[c](d)");
    assert_eq!(escape_masked_link("[a](b)\n[c](d)"), "\\[a](b)\n\\[c](d)");
    assert_eq!(escape_masked_link("[a] (b) [c]"), "[a] (b) [c]");
    assert_eq!(escape_masked_link("[](x) [a]()"), "[](x) [a]()");
}

#[test]
fn heading_escapes_cover_bullets_and_limits() {
    assert_eq!(
        escape_heading("# a\n## b\n### c\n#### d"),
        "\\# a\n\\## b\n\\### c\n#### d"
    );
    assert_eq!(escape_heading("- # a"), "- \\# a");
    assert_eq!(escape_heading("   - # a"), "   - # a");
    assert_eq!(escape_heading("      # a"), "      \\# a");
    assert_eq!(escape_heading("#a"), "#a");
}

/// Expected strings from the discord.js 1.0 development line's
/// `^(\s*)>(\s+)` and `^(\s*)>>>(\s+)` (absent from 0.6.2).
#[test]
fn quote_escapes_match_discord_js_dev_line() {
    let quote_cases = [
        ("> a", "\\> a"),
        ("  > a", "  \\> a"),
        (">a", ">a"),
        (">", ">"),
        ("> ", "\\> "),
        (">>> a", ">>> a"),
        ("x > a", "x > a"),
        ("\t> a\n> b", "\t\\> a\n\\> b"),
        (">\nb", "\\>\nb"),
        (" >>> x", " >>> x"),
    ];
    for (input, expected) in quote_cases {
        assert_eq!(escape_quote(input), expected, "{input:?}");
    }
    let block_cases = [
        ("> a", "> a"),
        (">>> a", "\\>>> a"),
        (" >>> x", " \\>>> x"),
        (">>>x", ">>>x"),
        (">>>", ">>>"),
    ];
    for (input, expected) in block_cases {
        assert_eq!(escape_block_quote(input), expected, "{input:?}");
    }
    assert_eq!(
        escape_markdown("> **q**\n>>> b", EscapeMarkdownOptions::all()),
        "\\> \\*\\*q\\*\\*\n\\>>> b"
    );
}

/// discord.js regexes use ECMAScript `\s`: U+FEFF is whitespace, U+0085
/// is not (Rust's `char::is_whitespace` says the opposite).
#[test]
fn whitespace_follows_ecmascript_not_rust() {
    let options = EscapeMarkdownOptions::default();
    // U+FEFF ends the URL, so the italics after it are escaped.
    assert_eq!(
        escape_markdown("http://a\u{feff}_b_", options),
        "http://a\u{feff}\\_b\\_"
    );
    // U+0085 does not end it.
    assert_eq!(
        escape_markdown("http://a\u{0085}_b_", options),
        "http://a\u{0085}_b_"
    );
    assert_eq!(escape_quote("\u{feff}> quote"), "\u{feff}\\> quote");
    assert_eq!(escape_quote(">\u{feff}x"), "\\>\u{feff}x");
    assert_eq!(escape_quote("\u{0085}> quote"), "\u{0085}> quote");
    assert_eq!(escape_quote(">\u{0085}x"), ">\u{0085}x");
    assert_eq!(escape_block_quote("\u{a0}>>> x"), "\u{a0}\\>>> x");
}

#[test]
fn line_rules_see_every_line_terminator() {
    let options = EscapeMarkdownOptions::all();
    assert_eq!(
        escape_markdown("a\r- b\u{2028}1. c\u{2029}# d", options),
        "a\r\\- b\u{2028}1\\. c\u{2029}\\# d"
    );
}

#[test]
fn escaping_handles_multibyte_neighbours() {
    let options = EscapeMarkdownOptions::all();
    assert_eq!(
        escape_markdown("볼드**✨**_이탤릭_", options),
        "볼드\\*\\*✨\\*\\*\\_이탤릭\\_"
    );
    assert_eq!(
        escape_markdown("https://例え.jp/a_b　_c_", options),
        "https://例え.jp/a_b　\\_c\\_"
    );
}

#[test]
fn emoji_tail_requires_digits_and_a_closing_angle() {
    assert_eq!(escape_underline("a__:12>"), "a__:12>");
    assert_eq!(escape_underline("a__:>"), "a\\_\\_:>");
    assert_eq!(escape_underline("a__:12"), "a\\_\\_:12");
    assert_eq!(escape_italic("<a:_:1>"), "<a:_:1>");
    // Not an emoji tag, but discord.js's `(?!:\d+>)` lookahead still skips
    // an underscore right before `:digits>`.
    assert_eq!(escape_italic("<b:_:1>"), "<b:_:1>");
    assert_eq!(escape_italic("<b:_x:1>"), "<b:\\_x:1>");
}

// ---------------------------------------------------------------------------
// clean_content (expected strings from discord.js `cleanContent`)
// ---------------------------------------------------------------------------

struct Names;

impl MentionResolver for Names {
    fn user(&self, id: UserId) -> Option<String> {
        (id.get() == 80351110224678912).then(|| "Nelly".to_string())
    }

    fn role(&self, id: RoleId) -> Option<String> {
        (id.get() == 165511591545143296).then(|| "Mods".to_string())
    }

    fn channel(&self, id: ChannelId) -> Option<String> {
        (id.get() == 103735883630395392).then(|| "general".to_string())
    }
}

#[test]
fn clean_content_matches_discord_js() {
    let cases = [
        ("hi <@80351110224678912>!", "hi @Nelly!"),
        ("old <@!80351110224678912>", "old @Nelly"),
        ("role <@&165511591545143296>", "role @Mods"),
        ("chan <#103735883630395392>", "chan #general"),
        (
            "unknown <@123456789012345678> <@&123456789012345678> <#123456789012345678>",
            "unknown <@123456789012345678> <@&123456789012345678> <#123456789012345678>",
        ),
        ("</airhorn:816437322781949972>", "/airhorn"),
        ("</foo bar:123456789012345678>", "/foo bar"),
        ("</foo group bar:123456789012345678>", "/foo group bar"),
        ("</명령 하위:123456789012345678>", "/명령 하위"),
        (
            "<:mmLol:216154654256398347> <a:b1nzy:392938283556143104>",
            ":mmLol: :b1nzy:",
        ),
        ("<:_:216154654256398347>", ":_:"),
        ("short <@1234567890123456>", "short <@1234567890123456>"),
        (
            "game <@$1402418491272986635>",
            "game <@$1402418491272986635>",
        ),
        ("nested <<@80351110224678912>>", "nested <@Nelly>"),
        (
            "unterminated <@80351110224678912",
            "unterminated <@80351110224678912",
        ),
        ("`<@80351110224678912>`", "`@Nelly`"),
        ("everyone @everyone @here", "everyone @everyone @here"),
        (
            "emoji name with dash <:a-b:216154654256398347>",
            "emoji name with dash <:a-b:216154654256398347>",
        ),
        (
            "cmd empty <//:123456789012345678>",
            "cmd empty <//:123456789012345678>",
        ),
        (
            "<@80351110224678912><#103735883630395392>",
            "@Nelly#general",
        ),
        ("unicode ✨ <@80351110224678912> 😀", "unicode ✨ @Nelly 😀"),
        ("<@&80351110224678912>", "<@&80351110224678912>"),
        ("<a:x:216154654256398347", "<a:x:216154654256398347"),
        ("", ""),
    ];
    for (input, expected) in cases {
        assert_eq!(clean_content(input, &Names), expected, "{input:?}");
    }
}

#[test]
fn clean_content_accepts_twenty_digit_ids_but_not_longer() {
    let resolver = |target: MentionTarget| Some(format!("id{}", target.id().to_string().len()));
    assert_eq!(clean_content("<@12345678901234567890>", &resolver), "@id20");
    assert_eq!(
        clean_content("<@123456789012345678901>", &resolver),
        "<@123456789012345678901>"
    );
}

#[test]
fn clean_content_borrows_when_nothing_resolves() {
    let text = "no mentions <here> <@123456789012345678>";
    assert!(matches!(clean_content(text, &Names), Cow::Borrowed(_)));
    assert!(matches!(
        clean_content("<@80351110224678912>", &Names),
        Cow::Owned(_)
    ));
}

#[test]
fn clean_content_with_closures_and_default_resolvers() {
    let closure = |target: MentionTarget| match target {
        MentionTarget::User(id) => Some(format!("user{id}")),
        MentionTarget::Role(_) => Some("role".to_string()),
        _ => None,
    };
    assert_eq!(
        clean_content(
            "<@123456789012345678> <@&123456789012345678> <#123456789012345678>",
            &closure
        ),
        "@user123456789012345678 @role <#123456789012345678>"
    );

    struct Nothing;
    impl MentionResolver for Nothing {}
    assert_eq!(
        clean_content(
            "<@123456789012345678> <@&123456789012345678> <#123456789012345678> <:a:123456789012345678>",
            &Nothing
        ),
        "<@123456789012345678> <@&123456789012345678> <#123456789012345678> :a:"
    );

    let dynamic: &dyn MentionResolver = &Names;
    assert_eq!(clean_content("<@80351110224678912>", dynamic), "@Nelly");
}

// ---------------------------------------------------------------------------
// model_ext delegates
// ---------------------------------------------------------------------------

#[test]
fn member_mention_and_message_clean_content_delegate() {
    use crate::model::{Member, Message, User};

    let member = Member {
        user: Some(User {
            id: test_id(80351110224678912),
            ..User::fixture()
        }),
        ..Member::default()
    };
    assert_eq!(member.mention().as_deref(), Some("<@80351110224678912>"));
    assert_eq!(Member::default().mention(), None);

    let message = Message {
        content: "hey <@80351110224678912> in <#103735883630395392>".to_string(),
        ..Message::fixture()
    };
    assert_eq!(message.clean_content(&Names), "hey @Nelly in #general");
}

// ---------------------------------------------------------------------------
// Command-name class (expected results from discord.js on Node 24.16,
// Unicode 17.0; the table was also checked against discord.js for every
// Unicode scalar value)
// ---------------------------------------------------------------------------

#[test]
fn command_names_use_discord_js_character_class() {
    let cases = [
        // Script=Devanagari marks outside U+0900-U+097F are accepted.
        ("a\u{A8E0}", true),
        ("a\u{11B00}", true),
        // Other_Alphabetic but General_Category Mn, Script=Inherited.
        ("a\u{0345}", false),
        // Inside the Devanagari block but Script=Inherited / Common.
        ("a\u{0951}", false),
        ("a\u{0964}", false),
        // Inside the Thai block but Script=Common (currency sign).
        ("a\u{0E3F}", false),
        ("a\u{1CD0}", false),
        ("don\u{02BC}t", true),
        ("a\u{24B6}", false),
        ("e\u{0301}", false),
        ("สวัสดี", true),
        ("नमस्ते", true),
        ("한국어", true),
        ("x²", true),
        ("漢字", true),
        ("a😀", false),
        ("a\u{0085}", false),
        ("a-b_c d", true),
    ];
    let nobody = |_: MentionTarget| None;
    for (name, accepted) in cases {
        let input = format!("</{name}:123456789012345678>");
        let expected = if accepted {
            format!("/{name}")
        } else {
            input.clone()
        };
        assert_eq!(clean_content(&input, &nobody), expected, "{name:?}");
    }
}

#[test]
fn command_name_table_is_well_formed() {
    use super::command_name_ranges::{RANGES, UNICODE_VERSION};
    assert_eq!(UNICODE_VERSION, "17.0");
    assert!(!RANGES.is_empty());
    let mut previous_end: Option<u32> = None;
    for &(low, high) in RANGES {
        assert!(low <= high, "{low:#X}..={high:#X}");
        assert!(high <= 0x10FFFF);
        assert!(high < 0xD800 || low > 0xDFFF, "no surrogates");
        if let Some(end) = previous_end {
            assert!(low > end + 1, "sorted, disjoint, merged: {low:#X}");
        }
        previous_end = Some(high);
    }
    // Ends of the table and of a range are inclusive.
    assert!(super::is_command_name_char('-'));
    assert!(super::is_command_name_char('z'));
    assert!(!super::is_command_name_char('{'));
    assert!(!super::is_command_name_char(':'));
}
