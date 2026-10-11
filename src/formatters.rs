//! Discord message formatting helpers with `@discordjs/formatters` parity:
//! mentions, timestamps, markdown styles, lists, [`escape_markdown`], and
//! [`clean_content`].
//!
//! Every formatter takes `impl Display` and returns `impl Display`, so
//! nesting does not allocate until the result is written:
//!
//! ```
//! use discordrs::formatters::{bold, italic, timestamp, user_mention, TimestampStyle};
//!
//! let line = format!(
//!     "{} joined {}",
//!     bold(italic(user_mention(80351110224678912u64))),
//!     timestamp(1618953630).style(TimestampStyle::RelativeTime),
//! );
//! assert_eq!(line, "**_<@80351110224678912>_** joined <t:1618953630:R>");
//! ```
//!
//! Text transforms ([`escape_markdown`], [`clean_content`], ...) return
//! [`Cow`] and borrow the input when nothing changes.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: discord.js 사용자가 기대하는 포매터·이스케이프·cleanContent를
//!   같은 출력으로 제공한다.
//! - 기존 구현 및 제약 조건: 모델의 `mention()` 몇 개만 있고, 크레이트에 regex
//!   의존성이 없으며 Cargo.toml은 이 작업 범위 밖이다. discord.js 이스케이퍼는
//!   lookbehind 정규식이다.
//! - 검토한 주요 대안: (1) regex 크레이트 도입, (2) 문자 단위로 전부 이스케이프,
//!   (3) discord.js의 각 정규식 단계를 같은 순서의 수작업 스캐너로 옮김.
//! - 선택한 방식: (3). 기본값은 배포된 @discordjs/formatters 0.6.2(discord.js
//!   v14)를 따르고, 1.0 개발판의 quote/block_quote 옵션은 꺼진 채로 추가한다.
//! - 다른 대안 대신 이 방식을 선택한 이유: 의존성을 늘리지 않으면서 단계별
//!   상호작용(`***`의 토글 등)까지 같은 결과를 낸다. (2)는 bold만 끈 경우처럼
//!   옵션 조합에서 discord.js와 다른 결과를 낸다.
//! - 장점, 단점 및 영향: discord.js의 명백한 버그 네 가지는 따르지 않는다.
//!   같은 줄의 `<:` 뒤 모든 `_`를 건너뛰는 대신 실제 커스텀 이모지 이름만
//!   보호하고, `<https://...>` 안의 `_`도 보호하며, 마스킹 링크는 줄마다 첫
//!   하나가 아니라 전부 이스케이프하고, `clean_code_block_content`는 네 개
//!   이상의 백틱에서도 닫는 펜스가 남지 않게 끊는다. 공백·줄바꿈 판정은
//!   ECMAScript `\s`/LineTerminator 정의를 따른다(U+FEFF 포함, U+0085 제외).
//!   그 밖의 입력은 discord.js와 같은 문자열을 낸다.
//! ```

use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::GenericId;

// ---------------------------------------------------------------------------
// Display building blocks
// ---------------------------------------------------------------------------

/// `open` + content + `close`, written lazily.
struct Enclosed<T> {
    open: &'static str,
    content: T,
    close: &'static str,
}

impl<T: Display> Display for Enclosed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.open)?;
        self.content.fmt(f)?;
        f.write_str(self.close)
    }
}

fn enclose<T: Display>(open: &'static str, content: T, close: &'static str) -> impl Display {
    Enclosed {
        open,
        content,
        close,
    }
}

// ---------------------------------------------------------------------------
// Markdown styles
// ---------------------------------------------------------------------------

/// `**content**`
pub fn bold<T: Display>(content: T) -> impl Display {
    enclose("**", content, "**")
}

/// `_content_` (discord.js uses underscores for italics).
pub fn italic<T: Display>(content: T) -> impl Display {
    enclose("_", content, "_")
}

/// `__content__`
pub fn underline<T: Display>(content: T) -> impl Display {
    enclose("__", content, "__")
}

/// `~~content~~`
pub fn strikethrough<T: Display>(content: T) -> impl Display {
    enclose("~~", content, "~~")
}

/// `||content||`
pub fn spoiler<T: Display>(content: T) -> impl Display {
    enclose("||", content, "||")
}

/// `` `content` `` — content containing a backtick ends the span early,
/// as in discord.js.
pub fn inline_code<T: Display>(content: T) -> impl Display {
    enclose("`", content, "`")
}

/// A code block without a language. Run untrusted content through
/// [`clean_code_block_content`] first so a ```` ``` ```` inside it cannot
/// close the block.
pub fn code_block<T: Display>(content: T) -> impl Display {
    enclose("```\n", content, "\n```")
}

/// A code block with a syntax-highlighting language
/// (```` ```rust\ncontent\n``` ````).
pub fn code_block_with_language<L: Display, T: Display>(language: L, content: T) -> impl Display {
    struct CodeBlock<L, T>(L, T);
    impl<L: Display, T: Display> Display for CodeBlock<L, T> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "```{}\n{}\n```", self.0, self.1)
        }
    }
    CodeBlock(language, content)
}

/// `> content` — quotes one line.
pub fn quote<T: Display>(content: T) -> impl Display {
    enclose("> ", content, "")
}

/// `>>> content` — quotes everything until the end of the message.
pub fn block_quote<T: Display>(content: T) -> impl Display {
    enclose(">>> ", content, "")
}

/// `-# content` — small, muted text.
pub fn subtext<T: Display>(content: T) -> impl Display {
    enclose("-# ", content, "")
}

/// Heading size for [`heading`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HeadingLevel {
    /// `# `
    #[default]
    One,
    /// `## `
    Two,
    /// `### `
    Three,
}

/// `# content`, `## content`, or `### content`.
pub fn heading<T: Display>(content: T, level: HeadingLevel) -> impl Display {
    let open = match level {
        HeadingLevel::One => "# ",
        HeadingLevel::Two => "## ",
        HeadingLevel::Three => "### ",
    };
    enclose(open, content, "")
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

/// `[content](url)` — a masked link.
pub fn hyperlink<C: Display, U: Display>(content: C, url: U) -> impl Display {
    struct Link<C, U>(C, U);
    impl<C: Display, U: Display> Display for Link<C, U> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "[{}]({})", self.0, self.1)
        }
    }
    Link(content, url)
}

/// `[content](url "title")`; an empty `title` is omitted, as in discord.js.
pub fn hyperlink_with_title<'a, C: Display + 'a, U: Display + 'a>(
    content: C,
    url: U,
    title: &'a str,
) -> impl Display + 'a {
    struct TitledLink<'a, C, U>(C, U, &'a str);
    impl<C: Display, U: Display> Display for TitledLink<'_, C, U> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            if self.2.is_empty() {
                write!(f, "[{}]({})", self.0, self.1)
            } else {
                write!(f, "[{}]({} \"{}\")", self.0, self.1, self.2)
            }
        }
    }
    TitledLink(content, url, title)
}

/// `<url>` — a link without an embed preview.
pub fn hide_link_embed<U: Display>(url: U) -> impl Display {
    enclose("<", url, ">")
}

/// `https://discord.com/channels/{guild_id}/{channel_id}`
pub fn channel_link<G: Display, C: Display>(guild_id: G, channel_id: C) -> impl Display {
    struct ChannelLink<G, C>(G, C);
    impl<G: Display, C: Display> Display for ChannelLink<G, C> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "https://discord.com/channels/{}/{}", self.0, self.1)
        }
    }
    ChannelLink(guild_id, channel_id)
}

/// `https://discord.com/channels/@me/{channel_id}` for DM channels.
pub fn dm_channel_link<C: Display>(channel_id: C) -> impl Display {
    channel_link("@me", channel_id)
}

/// `https://discord.com/channels/{guild_id}/{channel_id}/{message_id}`
pub fn message_link<G: Display, C: Display, M: Display>(
    guild_id: G,
    channel_id: C,
    message_id: M,
) -> impl Display {
    struct MessageLink<G, C, M>(G, C, M);
    impl<G: Display, C: Display, M: Display> Display for MessageLink<G, C, M> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "https://discord.com/channels/{}/{}/{}",
                self.0, self.1, self.2
            )
        }
    }
    MessageLink(guild_id, channel_id, message_id)
}

/// `https://discord.com/channels/@me/{channel_id}/{message_id}`
pub fn dm_message_link<C: Display, M: Display>(channel_id: C, message_id: M) -> impl Display {
    message_link("@me", channel_id, message_id)
}

// ---------------------------------------------------------------------------
// Mentions and emoji
// ---------------------------------------------------------------------------

/// `<@id>`
pub fn user_mention<I: Display>(user_id: I) -> impl Display {
    enclose("<@", user_id, ">")
}

/// `<#id>`
pub fn channel_mention<I: Display>(channel_id: I) -> impl Display {
    enclose("<#", channel_id, ">")
}

/// `<@&id>`
pub fn role_mention<I: Display>(role_id: I) -> impl Display {
    enclose("<@&", role_id, ">")
}

/// `<id:linked-roles:id>` — opens the connection modal for a linked role.
pub fn linked_role_mention<I: Display>(role_id: I) -> impl Display {
    enclose("<id:linked-roles:", role_id, ">")
}

/// `<@$id>` — a game profile mention.
pub fn game_profile_mention<I: Display>(game_id: I) -> impl Display {
    enclose("<@$", game_id, ">")
}

/// `<:name:id>`
pub fn custom_emoji<N: Display, I: Display>(name: N, emoji_id: I) -> impl Display {
    struct CustomEmoji<N, I>(N, I);
    impl<N: Display, I: Display> Display for CustomEmoji<N, I> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "<:{}:{}>", self.0, self.1)
        }
    }
    CustomEmoji(name, emoji_id)
}

/// `<a:name:id>`
pub fn animated_emoji<N: Display, I: Display>(name: N, emoji_id: I) -> impl Display {
    struct AnimatedEmoji<N, I>(N, I);
    impl<N: Display, I: Display> Display for AnimatedEmoji<N, I> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "<a:{}:{}>", self.0, self.1)
        }
    }
    AnimatedEmoji(name, emoji_id)
}

/// A clickable slash command mention; see [`slash_command_mention`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommandMention<'a, I> {
    name: &'a str,
    group: Option<&'a str>,
    subcommand: Option<&'a str>,
    command_id: I,
}

impl<'a, I: Display> SlashCommandMention<'a, I> {
    /// Mentions a subcommand: `</name subcommand:id>`.
    pub fn subcommand(mut self, subcommand: &'a str) -> Self {
        self.subcommand = Some(subcommand);
        self.group = None;
        self
    }

    /// Mentions a subcommand inside a group: `</name group subcommand:id>`.
    pub fn subcommand_in_group(mut self, group: &'a str, subcommand: &'a str) -> Self {
        self.group = Some(group);
        self.subcommand = Some(subcommand);
        self
    }
}

impl<I: Display> Display for SlashCommandMention<'_, I> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "</{}", self.name)?;
        for part in [self.group, self.subcommand].into_iter().flatten() {
            write!(f, " {part}")?;
        }
        write!(f, ":{}>", self.command_id)
    }
}

/// `</name:id>` (discord.js `chatInputApplicationCommandMention`); chain
/// [`SlashCommandMention::subcommand`] or
/// [`SlashCommandMention::subcommand_in_group`] for subcommands.
pub fn slash_command_mention<I: Display>(name: &str, command_id: I) -> SlashCommandMention<'_, I> {
    SlashCommandMention {
        name,
        group: None,
        subcommand: None,
        command_id,
    }
}

/// Server-navigation mentions (`<id:...>`) that link to a tab of the current
/// server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GuildNavigation {
    /// `<id:customize>` — Channels & Roles (onboarding prompts).
    Customize,
    /// `<id:browse>` — Browse Channels.
    Browse,
    /// `<id:guide>` — Server Guide.
    Guide,
    /// `<id:linked-roles>` — Linked Roles.
    LinkedRoles,
}

impl Display for GuildNavigation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Customize => "<id:customize>",
            Self::Browse => "<id:browse>",
            Self::Guide => "<id:guide>",
            Self::LinkedRoles => "<id:linked-roles>",
        })
    }
}

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

/// How a `<t:...>` timestamp renders in the reader's locale and timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TimestampStyle {
    /// `t` — 16:20
    ShortTime,
    /// `T` — 16:20:30
    MediumTime,
    /// `d` — 20/04/2021
    ShortDate,
    /// `D` — April 20, 2021
    LongDate,
    /// `f` — April 20, 2021 at 16:20 (Discord's default)
    LongDateShortTime,
    /// `F` — Tuesday, April 20, 2021 at 16:20
    FullDateShortTime,
    /// `s` — 20/04/2021, 16:20
    ShortDateShortTime,
    /// `S` — 20/04/2021, 16:20:30
    ShortDateMediumTime,
    /// `R` — 4 years ago
    RelativeTime,
}

impl TimestampStyle {
    /// The style letter used in `<t:unix:STYLE>`.
    pub fn as_char(self) -> char {
        match self {
            Self::ShortTime => 't',
            Self::MediumTime => 'T',
            Self::ShortDate => 'd',
            Self::LongDate => 'D',
            Self::LongDateShortTime => 'f',
            Self::FullDateShortTime => 'F',
            Self::ShortDateShortTime => 's',
            Self::ShortDateMediumTime => 'S',
            Self::RelativeTime => 'R',
        }
    }

    /// Parses a style letter; `None` for letters Discord does not define.
    pub fn from_char(style: char) -> Option<Self> {
        Some(match style {
            't' => Self::ShortTime,
            'T' => Self::MediumTime,
            'd' => Self::ShortDate,
            'D' => Self::LongDate,
            'f' => Self::LongDateShortTime,
            'F' => Self::FullDateShortTime,
            's' => Self::ShortDateShortTime,
            'S' => Self::ShortDateMediumTime,
            'R' => Self::RelativeTime,
            _ => return None,
        })
    }
}

impl Display for TimestampStyle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

/// A `<t:unix>` or `<t:unix:STYLE>` timestamp; see [`timestamp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timestamp {
    seconds: i64,
    style: Option<TimestampStyle>,
}

impl Timestamp {
    /// A timestamp `seconds` after the Unix epoch (negative before it).
    pub fn new(seconds: i64) -> Self {
        Self {
            seconds,
            style: None,
        }
    }

    /// Sets the render style.
    pub fn style(mut self, style: TimestampStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// The Unix time in seconds.
    pub fn seconds(&self) -> i64 {
        self.seconds
    }
}

/// Floors to whole seconds like discord.js (`Math.floor(ms / 1000)`), also
/// before 1970.
impl From<SystemTime> for Timestamp {
    fn from(time: SystemTime) -> Self {
        let seconds = match time.duration_since(UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
            Err(before) => {
                let before = before.duration();
                let whole = i64::try_from(before.as_secs()).unwrap_or(i64::MAX);
                if before.subsec_nanos() > 0 {
                    whole.saturating_add(1).saturating_neg()
                } else {
                    whole.saturating_neg()
                }
            }
        };
        Self::new(seconds)
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.style {
            Some(style) => write!(f, "<t:{}:{style}>", self.seconds),
            None => write!(f, "<t:{}>", self.seconds),
        }
    }
}

/// `<t:unix_seconds>`; add `.style(...)` for `<t:unix_seconds:STYLE>`. Use
/// `Timestamp::from(system_time)` for a [`SystemTime`].
pub fn timestamp(unix_seconds: i64) -> Timestamp {
    Timestamp::new(unix_seconds)
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

/// An entry of a nested list: an item, or a sub-list indented one level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListItem<T> {
    /// A list item.
    Item(T),
    /// A sub-list.
    Nested(Vec<ListItem<T>>),
}

struct FlatList<'a, T> {
    items: &'a [T],
    start: Option<u64>,
}

impl<T: Display> Display for FlatList<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write_marker(f, self.start)?;
            write!(f, " {item}")?;
        }
        Ok(())
    }
}

struct NestedList<'a, T> {
    items: &'a [ListItem<T>],
    start: Option<u64>,
}

impl<T: Display> NestedList<'_, T> {
    /// discord.js `listCallback`: items at depth `d` are indented by
    /// `2 * (d - 1)` spaces and every level reuses the same marker.
    fn write_level(
        &self,
        f: &mut Formatter<'_>,
        items: &[ListItem<T>],
        depth: usize,
    ) -> fmt::Result {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            match item {
                ListItem::Item(content) => {
                    for _ in 1..depth {
                        f.write_str("  ")?;
                    }
                    write_marker(f, self.start)?;
                    write!(f, " {content}")?;
                }
                ListItem::Nested(children) => self.write_level(f, children, depth + 1)?,
            }
        }
        Ok(())
    }
}

impl<T: Display> Display for NestedList<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.write_level(f, self.items, 1)
    }
}

fn write_marker(f: &mut Formatter<'_>, start: Option<u64>) -> fmt::Result {
    match start {
        Some(number) => write!(f, "{number}."),
        None => f.write_str("-"),
    }
}

/// `- item` lines.
pub fn unordered_list<T: Display>(items: &[T]) -> impl Display + '_ {
    FlatList { items, start: None }
}

/// `start. item` lines. Every item gets the same number, as in discord.js;
/// Discord renumbers consecutive items when rendering. `start` is clamped
/// to at least 1.
pub fn ordered_list<T: Display>(items: &[T], start: u64) -> impl Display + '_ {
    FlatList {
        items,
        start: Some(start.max(1)),
    }
}

/// An unordered list whose [`ListItem::Nested`] entries are indented.
pub fn unordered_list_nested<T: Display>(items: &[ListItem<T>]) -> impl Display + '_ {
    NestedList { items, start: None }
}

/// An ordered list whose [`ListItem::Nested`] entries are indented; see
/// [`ordered_list`] for numbering.
pub fn ordered_list_nested<T: Display>(items: &[ListItem<T>], start: u64) -> impl Display + '_ {
    NestedList {
        items,
        start: Some(start.max(1)),
    }
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

macro_rules! option_setters {
    ($($field:ident),* $(,)?) => {
        $(
            #[doc = concat!("Sets [`EscapeMarkdownOptions::", stringify!($field), "`].")]
            pub fn $field(mut self, enabled: bool) -> Self {
                self.$field = enabled;
                self
            }
        )*
    };
}

/// Which markdown [`escape_markdown`] neutralizes. `Default` matches the
/// published `@discordjs/formatters` 0.6.2 (discord.js v14): inline styles
/// on, line-start syntax (headings, lists, masked links, quotes) off. Use
/// [`EscapeMarkdownOptions::all`] to escape everything.
///
/// ```
/// use discordrs::formatters::{escape_markdown, EscapeMarkdownOptions};
///
/// let options = EscapeMarkdownOptions::default().heading(true).bold(false);
/// assert_eq!(escape_markdown("# **hi**", options), "\\# **hi**");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct EscapeMarkdownOptions {
    /// ```` ``` ```` fences. Default `true`.
    pub code_block: bool,
    /// `` ` `` and ``` `` ``` inline code. Default `true`.
    pub inline_code: bool,
    /// `**bold**`. Default `true`.
    pub bold: bool,
    /// `*italic*` and `_italic_`. Default `true`.
    pub italic: bool,
    /// `__underline__`. Default `true`.
    pub underline: bool,
    /// `~~strikethrough~~`. Default `true`.
    pub strikethrough: bool,
    /// `||spoiler||`. Default `true`.
    pub spoiler: bool,
    /// When `false`, text between ```` ``` ```` fences is left as is.
    /// Default `true`.
    pub code_block_content: bool,
    /// When `false`, text between single backticks is left as is.
    /// Default `true`.
    pub inline_code_content: bool,
    /// Backslashes (so existing escapes stay visible). Default `true`.
    pub escape: bool,
    /// `#`, `##`, `###` headings at line start. Default `false`.
    pub heading: bool,
    /// `-` / `*` bullets at line start. Default `false`.
    pub bulleted_list: bool,
    /// `1.` numbering at line start. Default `false`.
    pub numbered_list: bool,
    /// `[text](url)` masked links. Default `false`.
    pub masked_link: bool,
    /// `> ` quotes at line start. Default `false`.
    pub quote: bool,
    /// `>>> ` block quotes at line start. Default `false`.
    pub block_quote: bool,
}

impl Default for EscapeMarkdownOptions {
    fn default() -> Self {
        Self {
            code_block: true,
            inline_code: true,
            bold: true,
            italic: true,
            underline: true,
            strikethrough: true,
            spoiler: true,
            code_block_content: true,
            inline_code_content: true,
            escape: true,
            heading: false,
            bulleted_list: false,
            numbered_list: false,
            masked_link: false,
            quote: false,
            block_quote: false,
        }
    }
}

impl EscapeMarkdownOptions {
    /// Escapes every supported construct.
    pub fn all() -> Self {
        Self {
            heading: true,
            bulleted_list: true,
            numbered_list: true,
            masked_link: true,
            quote: true,
            block_quote: true,
            ..Self::default()
        }
    }

    option_setters!(
        code_block,
        inline_code,
        bold,
        italic,
        underline,
        strikethrough,
        spoiler,
        code_block_content,
        inline_code_content,
        escape,
        heading,
        bulleted_list,
        numbered_list,
        masked_link,
        quote,
        block_quote,
    );
}

/// Escapes markdown in `text` (discord.js `escapeMarkdown`) so it renders
/// literally. Borrows `text` when nothing needed escaping.
///
/// ```
/// use discordrs::formatters::{escape_markdown, EscapeMarkdownOptions};
///
/// assert_eq!(
///     escape_markdown("**hi** `code` ~~x~~", EscapeMarkdownOptions::default()),
///     "\\*\\*hi\\*\\* \\`code\\` \\~\\~x\\~\\~"
/// );
/// ```
pub fn escape_markdown(text: &str, options: EscapeMarkdownOptions) -> Cow<'_, str> {
    if !options.code_block_content {
        let inner = EscapeMarkdownOptions {
            code_block: true,
            code_block_content: true,
            ..options
        };
        let joiner = if options.code_block {
            "\\`\\`\\`"
        } else {
            "```"
        };
        return keep_odd_segments(text, text.split("```"), joiner, inner);
    }
    if !options.inline_code_content {
        let inner = EscapeMarkdownOptions {
            inline_code: true,
            inline_code_content: true,
            ..options
        };
        let joiner = if options.inline_code { "\\`" } else { "`" };
        return keep_odd_segments(text, split_single_backticks(text), joiner, inner);
    }

    let passes: [(bool, Pass); 14] = [
        (options.escape, escape_escape_pass),
        (options.inline_code, inline_code_pass),
        (options.code_block, code_block_pass),
        (options.italic, italic_pass),
        (options.bold, bold_pass),
        (options.underline, underline_pass),
        (options.strikethrough, strikethrough_pass),
        (options.spoiler, spoiler_pass),
        (options.heading, heading_pass),
        (options.bulleted_list, bulleted_list_pass),
        (options.numbered_list, numbered_list_pass),
        (options.masked_link, masked_link_pass),
        (options.quote, quote_pass),
        (options.block_quote, block_quote_pass),
    ];
    let mut result = Cow::Borrowed(text);
    for (enabled, pass) in passes {
        if enabled {
            if let Some(changed) = pass(&result) {
                result = Cow::Owned(changed);
            }
        }
    }
    result
}

/// One discord.js escaper step: the rewritten text, or `None` when unchanged.
type Pass = fn(&str) -> Option<String>;

/// discord.js splits on the fence, escapes the even segments (outside code)
/// and the unterminated tail, keeps the odd ones verbatim, and rejoins.
fn keep_odd_segments<'t, 's>(
    text: &'t str,
    segments: impl Iterator<Item = &'s str>,
    joiner: &str,
    inner: EscapeMarkdownOptions,
) -> Cow<'t, str> {
    let segments: Vec<&str> = segments.collect();
    let last = segments.len().saturating_sub(1);
    let mut out = String::with_capacity(text.len());
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            out.push_str(joiner);
        }
        if index % 2 == 1 && index != last {
            out.push_str(segment);
        } else {
            out.push_str(&escape_markdown(segment, inner));
        }
    }
    if out == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(out)
    }
}

/// Splits at backticks that are not adjacent to another backtick (the
/// discord.js ``(?<=^|[^`])`(?=[^`]|$)`` separator).
fn split_single_backticks(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut pieces = Vec::new();
    let mut start = 0;
    for index in 0..bytes.len() {
        let single = bytes[index] == b'`'
            && (index == 0 || bytes[index - 1] != b'`')
            && bytes.get(index + 1) != Some(&b'`');
        if single {
            pieces.push(&text[start..index]);
            start = index + 1;
        }
    }
    pieces.push(&text[start..]);
    pieces.into_iter()
}

macro_rules! public_escaper {
    ($(#[$doc:meta])* $name:ident => $pass:ident) => {
        $(#[$doc])*
        pub fn $name(text: &str) -> Cow<'_, str> {
            match $pass(text) {
                Some(changed) => Cow::Owned(changed),
                None => Cow::Borrowed(text),
            }
        }
    };
}

public_escaper!(
    /// Escapes ```` ``` ```` fences (discord.js `escapeCodeBlock`).
    escape_code_block => code_block_pass
);
public_escaper!(
    /// Escapes runs of one or two backticks (discord.js `escapeInlineCode`).
    escape_inline_code => inline_code_pass
);
public_escaper!(
    /// Escapes `*` and `_` italics (discord.js `escapeItalic`); characters
    /// inside URLs and custom emoji names are left alone.
    escape_italic => italic_pass
);
public_escaper!(
    /// Escapes `**` (discord.js `escapeBold`).
    escape_bold => bold_pass
);
public_escaper!(
    /// Escapes `__` (discord.js `escapeUnderline`); URLs and custom emoji
    /// names are left alone.
    escape_underline => underline_pass
);
public_escaper!(
    /// Escapes `~~` (discord.js `escapeStrikethrough`).
    escape_strikethrough => strikethrough_pass
);
public_escaper!(
    /// Escapes `||` (discord.js `escapeSpoiler`).
    escape_spoiler => spoiler_pass
);
public_escaper!(
    /// Escapes backslashes (discord.js `escapeEscape`).
    escape_escape => escape_escape_pass
);
public_escaper!(
    /// Escapes `#`–`###` headings at line start, also after a bullet
    /// (discord.js `escapeHeading`).
    escape_heading => heading_pass
);
public_escaper!(
    /// Escapes `-` / `*` bullets at line start (discord.js
    /// `escapeBulletedList`).
    escape_bulleted_list => bulleted_list_pass
);
public_escaper!(
    /// Escapes the `.` of `1.` numbering at line start (discord.js
    /// `escapeNumberedList`).
    escape_numbered_list => numbered_list_pass
);
public_escaper!(
    /// Escapes the `[` of every `[text](url)` masked link (discord.js
    /// `escapeMaskedLink` escapes only the first per line).
    escape_masked_link => masked_link_pass
);
public_escaper!(
    /// Escapes `> ` quotes at line start (discord.js `escapeQuote`).
    escape_quote => quote_pass
);
public_escaper!(
    /// Escapes `>>> ` block quotes at line start (discord.js
    /// `escapeBlockQuote`).
    escape_block_quote => block_quote_pass
);

/// Breaks every run of three or more backticks with U+200B so untrusted
/// text cannot close a [`code_block`] (discord.js `cleanCodeBlockContent`).
///
/// A run of exactly three becomes ```` `\u{200b}`` ```` as in discord.js.
/// discord.js replaces each ```` ``` ```` only once, so a run of four
/// leaves ```` `\u{200b}``` ```` with a closing fence in it; here a zero
/// width space follows the first backtick and then every second one, so no
/// three backticks stay adjacent.
pub fn clean_code_block_content(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut edits = Edits::new(text);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let start = index;
        while bytes.get(index) == Some(&b'`') {
            index += 1;
        }
        let run = index - start;
        if run >= 3 {
            let mut broken = String::with_capacity(run * 4);
            broken.push('`');
            let mut remaining = run - 1;
            while remaining > 0 {
                let take = remaining.min(2);
                broken.push('\u{200b}');
                broken.push_str(&"``"[..take]);
                remaining -= take;
            }
            edits.replace(start, index, &broken);
        }
    }
    edits.finish().map_or(Cow::Borrowed(text), Cow::Owned)
}

/// Accumulates edits over `text`, copying untouched stretches once.
struct Edits<'a> {
    text: &'a str,
    out: String,
    copied: usize,
    changed: bool,
}

impl<'a> Edits<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            out: String::new(),
            copied: 0,
            changed: false,
        }
    }

    /// Replaces `text[start..end]` with `with`. Calls must move forward.
    fn replace(&mut self, start: usize, end: usize, with: &str) {
        if !self.changed {
            self.out.reserve(self.text.len() + 16);
            self.changed = true;
        }
        self.out.push_str(&self.text[self.copied..start]);
        self.out.push_str(with);
        self.copied = end;
    }

    fn insert(&mut self, at: usize, with: &str) {
        self.replace(at, at, with);
    }

    /// The edited text, or `None` when no edit was made.
    fn finish(mut self) -> Option<String> {
        if !self.changed {
            return None;
        }
        self.out.push_str(&self.text[self.copied..]);
        Some(self.out)
    }
}

fn replace_all(text: &str, pattern: &str, with: &str) -> Option<String> {
    text.contains(pattern).then(|| text.replace(pattern, with))
}

fn escape_escape_pass(text: &str) -> Option<String> {
    replace_all(text, "\\", "\\\\")
}

fn code_block_pass(text: &str) -> Option<String> {
    replace_all(text, "```", "\\`\\`\\`")
}

fn strikethrough_pass(text: &str) -> Option<String> {
    replace_all(text, "~~", "\\~\\~")
}

fn spoiler_pass(text: &str) -> Option<String> {
    replace_all(text, "||", "\\|\\|")
}

fn inline_code_pass(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut edits = Edits::new(text);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let start = index;
        while bytes.get(index) == Some(&b'`') {
            index += 1;
        }
        // Runs of three or more are code fences, left to `code_block`.
        match index - start {
            1 => edits.replace(start, index, "\\`"),
            2 => edits.replace(start, index, "\\`\\`"),
            _ => {}
        }
    }
    edits.finish()
}

fn italic_pass(text: &str) -> Option<String> {
    match single_marker_pass(text, b'*') {
        Some(stars) => Some(single_marker_pass(&stars, b'_').unwrap_or(stars)),
        None => single_marker_pass(text, b'_'),
    }
}

/// discord.js `escapeItalic` for one marker. A marker that starts a run
/// is escaped when it stands alone; a run of three toggles between
/// escaping its first and its last character (so `***a***` keeps bold);
/// a run of exactly two is bold/underline and left for those passes.
/// Underscores directly followed by `:digits>` belong to an emoji tag.
fn single_marker_pass(text: &str, marker: u8) -> Option<String> {
    if !text.as_bytes().contains(&marker) {
        return None;
    }
    let bytes = text.as_bytes();
    let shield = if marker == b'_' {
        Shield::Underscore
    } else {
        Shield::Star
    };
    let protected = protected_spans(text, shield);
    let mut edits = Edits::new(text);
    let mut toggle = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        let starts_run = bytes[index] == marker && (index == 0 || bytes[index - 1] != marker);
        if !starts_run || protected[index] || (marker == b'_' && emoji_id_tail(bytes, index + 1)) {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) != Some(&marker) {
            edits.insert(index, "\\");
            index += 1;
        } else if bytes.get(index + 2) == Some(&marker) {
            toggle += 1;
            let at = if toggle % 2 == 1 { index } else { index + 2 };
            edits.insert(at, "\\");
            index += 3;
        } else {
            index += 1;
        }
    }
    edits.finish()
}

/// discord.js `escapeBold`: every `**`; a `***` toggles between keeping the
/// first and the last star as an italic marker.
fn bold_pass(text: &str) -> Option<String> {
    double_marker_pass(text, b'*', "\\*\\*", "*\\*\\*", "\\*\\**", false)
}

/// discord.js `escapeUnderline`: like bold, but URLs and custom emoji names
/// are skipped and `__:digits>` is left as part of an emoji tag.
fn underline_pass(text: &str) -> Option<String> {
    double_marker_pass(text, b'_', "\\_\\_", "_\\_\\_", "\\_\\__", true)
}

fn double_marker_pass(
    text: &str,
    marker: u8,
    pair: &str,
    triple_odd: &str,
    triple_even: &str,
    emoji_aware: bool,
) -> Option<String> {
    let bytes = text.as_bytes();
    if !bytes.windows(2).any(|window| window == [marker, marker]) {
        return None;
    }
    let protected = if emoji_aware {
        protected_spans(text, Shield::Underline)
    } else {
        Vec::new()
    };
    let mut edits = Edits::new(text);
    let mut toggle = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        let is_pair = bytes[index] == marker && bytes.get(index + 1) == Some(&marker);
        let skipped = emoji_aware && protected[index];
        if !is_pair || skipped {
            index += 1;
            continue;
        }
        let tail = |at: usize| emoji_aware && emoji_id_tail(bytes, at);
        if bytes.get(index + 2) == Some(&marker) && !tail(index + 3) {
            toggle += 1;
            let with = if toggle % 2 == 1 {
                triple_odd
            } else {
                triple_even
            };
            edits.replace(index, index + 3, with);
            index += 3;
        } else if !tail(index + 2) {
            edits.replace(index, index + 2, pair);
            index += 2;
        } else {
            index += 1;
        }
    }
    edits.finish()
}

/// `:digits>` at `at`: the end of a `<:name:id>` emoji tag.
fn emoji_id_tail(bytes: &[u8], at: usize) -> bool {
    if bytes.get(at) != Some(&b':') {
        return false;
    }
    let digits = bytes[at + 1..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits > 0 && bytes.get(at + 1 + digits) == Some(&b'>')
}

/// Which discord.js lookbehind a pass mirrors when skipping URL and emoji
/// characters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shield {
    /// `escapeItalic` for `*`: bare URLs not preceded by `<`
    /// (`(?<!<)https?:\/\/\S*`) and `<scheme:/...>` autolinks.
    Star,
    /// `escapeItalic` for `_`: as `Star`, plus custom emoji names.
    Underscore,
    /// `escapeUnderline`: bare URLs with at least one character after `//`
    /// (`https?:\/\/\S+`, also inside `<...>`), plus custom emoji names.
    Underline,
}

/// Marks the bytes a pass must not escape so links and emoji keep working.
///
/// Mirrors the discord.js lookbehinds with two deliberate fixes: emoji
/// protection covers only the name inside a real `<:name:id>` /
/// `<a:name:id>` tag (discord.js skips every `_` after a `<:` on the line),
/// and underscores get the same autolink protection as stars (discord.js
/// requires a one-character scheme there, a typo of `[^\s:]+`).
fn protected_spans(text: &str, shield: Shield) -> Vec<bool> {
    let bytes = text.as_bytes();
    let mut protected = vec![false; bytes.len()];

    let mut search = 0;
    while let Some(offset) = text[search..].find("http") {
        let start = search + offset;
        search = start + 4;
        let Some(after_scheme) = ["http://", "https://"]
            .iter()
            .find(|scheme| text[start..].starts_with(*scheme))
            .map(|scheme| start + scheme.len())
        else {
            continue;
        };
        let after_angle = start > 0 && bytes[start - 1] == b'<';
        let from = match shield {
            Shield::Underline => after_scheme + 1,
            Shield::Star | Shield::Underscore if after_angle => continue,
            Shield::Star | Shield::Underscore => after_scheme,
        };
        let end = token_end(text, after_scheme, false);
        if from < end {
            protected[from..end].fill(true);
        }
    }

    for (open, _) in text.match_indices('<') {
        if shield != Shield::Underline {
            // `<[^\s:]+:\/[^\s>]*`: protection starts after the `:/`.
            let colon = text[open + 1..]
                .char_indices()
                .find(|&(_, ch)| ch == ':' || is_js_whitespace(ch))
                .filter(|&(position, ch)| ch == ':' && position > 0)
                .map(|(position, _)| open + 1 + position);
            if let Some(colon) = colon.filter(|&colon| text[colon..].starts_with(":/")) {
                let end = token_end(text, colon + 2, true);
                protected[colon + 2..end].fill(true);
            }
        }
        if shield != Shield::Star {
            if let Some((name_start, name_end)) = emoji_name_span(bytes, open) {
                protected[name_start..name_end].fill(true);
            }
        }
    }
    protected
}

/// End of the whitespace-free run starting at `from`; also stops at `>`
/// when `stop_at_angle` is set.
fn token_end(text: &str, from: usize, stop_at_angle: bool) -> usize {
    text[from..]
        .char_indices()
        .find(|&(_, ch)| is_js_whitespace(ch) || (stop_at_angle && ch == '>'))
        .map_or(text.len(), |(position, _)| from + position)
}

/// For a `<:name:id>` / `<a:name:id>` tag opening at `open`, the byte range
/// of `name`.
fn emoji_name_span(bytes: &[u8], open: usize) -> Option<(usize, usize)> {
    let mut index = open + 1;
    if bytes.get(index) == Some(&b'a') {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    let name_start = index + 1;
    let name_len = bytes[name_start..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count();
    let name_end = name_start + name_len;
    (name_len > 0 && emoji_id_tail(bytes, name_end)).then_some((name_start, name_end))
}

/// ECMAScript `\s`: WhiteSpace (TAB, VT, FF, U+FEFF, and the Zs spaces)
/// plus LineTerminator. The ports use this instead of
/// `char::is_whitespace`, which also counts U+0085 but not U+FEFF, so URL
/// ends and quote markers fall where discord.js puts them.
fn is_js_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\u{b}' | '\u{c}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    ) || is_js_line_terminator(ch)
}

/// ECMAScript LineTerminator: where multiline `^` matches and `.` stops.
fn is_js_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Byte offsets where a line starts (JavaScript's multiline `^`: the input
/// start and after every line terminator).
fn line_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(
        text.char_indices()
            .filter(|&(_, ch)| is_js_line_terminator(ch))
            .map(|(position, ch)| position + ch.len_utf8()),
    )
}

fn line_end(text: &str, start: usize) -> usize {
    text[start..]
        .find(is_js_line_terminator)
        .map_or(text.len(), |position| start + position)
}

/// Runs `find` on every line and inserts a backslash at each returned
/// offset (relative to the line start).
fn per_line_pass(text: &str, find: impl Fn(&str) -> Vec<usize>) -> Option<String> {
    let mut edits = Edits::new(text);
    for start in line_starts(text) {
        let line = &text[start..line_end(text, start)];
        for offset in find(line) {
            edits.insert(start + offset, "\\");
        }
    }
    edits.finish()
}

fn count_leading(bytes: &[u8], from: usize, byte: u8) -> usize {
    bytes
        .get(from..)
        .map_or(0, |rest| rest.iter().take_while(|&&b| b == byte).count())
}

/// discord.js `^( {0,2})([*-] )?( *)(#{1,3} )`.
fn heading_pass(text: &str) -> Option<String> {
    fn hashes(bytes: &[u8], at: usize) -> bool {
        let count = count_leading(bytes, at, b'#');
        (1..=3).contains(&count) && bytes.get(at + count) == Some(&b' ')
    }
    per_line_pass(text, |line| {
        let bytes = line.as_bytes();
        let spaces = count_leading(bytes, 0, b' ');
        let bulleted = spaces <= 2
            && matches!(bytes.get(spaces), Some(b'*' | b'-'))
            && bytes.get(spaces + 1) == Some(&b' ');
        if bulleted {
            let after = spaces + 2 + count_leading(bytes, spaces + 2, b' ');
            if hashes(bytes, after) {
                return vec![after];
            }
        }
        if hashes(bytes, spaces) {
            vec![spaces]
        } else {
            Vec::new()
        }
    })
}

/// discord.js `^( *)([*-])( +)`.
fn bulleted_list_pass(text: &str) -> Option<String> {
    per_line_pass(text, |line| {
        let bytes = line.as_bytes();
        let spaces = count_leading(bytes, 0, b' ');
        if matches!(bytes.get(spaces), Some(b'*' | b'-')) && bytes.get(spaces + 1) == Some(&b' ') {
            vec![spaces]
        } else {
            Vec::new()
        }
    })
}

/// discord.js `^( *\d+)\.`.
fn numbered_list_pass(text: &str) -> Option<String> {
    per_line_pass(text, |line| {
        let bytes = line.as_bytes();
        let spaces = count_leading(bytes, 0, b' ');
        let digits = bytes[spaces..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digits > 0 && bytes.get(spaces + digits) == Some(&b'.') {
            vec![spaces + digits]
        } else {
            Vec::new()
        }
    })
}

/// Every `[` that opens `[text](url)` on its line: some `](` at least two
/// bytes later with a `)` at least one byte after the `(`.
fn masked_link_pass(text: &str) -> Option<String> {
    per_line_pass(text, |line| {
        let bytes = line.as_bytes();
        let Some(last_close) = bytes.iter().rposition(|&byte| byte == b')') else {
            return Vec::new();
        };
        let last_open = bytes
            .windows(2)
            .enumerate()
            .rfind(|(position, window)| *window == b"](" && position + 3 <= last_close)
            .map(|(position, _)| position);
        let Some(last_open) = last_open else {
            return Vec::new();
        };
        bytes
            .iter()
            .enumerate()
            .filter(|&(position, &byte)| byte == b'[' && position + 2 <= last_open)
            .map(|(position, _)| position)
            .collect()
    })
}

/// discord.js `^(\s*)>(\s+)`.
fn quote_pass(text: &str) -> Option<String> {
    quote_marker_pass(text, ">")
}

/// discord.js `^(\s*)>>>(\s+)`.
fn block_quote_pass(text: &str) -> Option<String> {
    quote_marker_pass(text, ">>>")
}

fn quote_marker_pass(text: &str, marker: &str) -> Option<String> {
    let mut edits = Edits::new(text);
    for start in line_starts(text) {
        let end = line_end(text, start);
        let indent = text[start..end]
            .char_indices()
            .find(|&(_, ch)| !is_js_whitespace(ch))
            .map_or(end - start, |(position, _)| position);
        let at = start + indent;
        // The whitespace after the marker may be the line break itself.
        let followed_by_space = text[at..]
            .strip_prefix(marker)
            .and_then(|rest| rest.chars().next())
            .is_some_and(is_js_whitespace);
        if followed_by_space {
            edits.insert(at, "\\");
        }
    }
    edits.finish()
}

// ---------------------------------------------------------------------------
// clean_content
// ---------------------------------------------------------------------------

/// What a mention refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MentionKind {
    /// `<@id>` or the deprecated `<@!id>`.
    User,
    /// `<@&id>`
    Role,
    /// `<#id>`
    Channel,
}

/// Supplies display names for [`clean_content`]. Every method defaults to
/// `None`, which leaves the mention untouched (discord.js does the same for
/// uncached targets).
///
/// Any `Fn(MentionKind, GenericId) -> Option<String>` closure is a
/// resolver; annotate the parameter types when passing a closure literal:
///
/// ```
/// use discordrs::formatters::{clean_content, MentionKind};
/// use discordrs::GenericId;
///
/// let resolver = |kind: MentionKind, id: GenericId| match (kind, id.get()) {
///     (MentionKind::User, 80351110224678912) => Some("Nelly".to_string()),
///     _ => None,
/// };
/// assert_eq!(
///     clean_content("hi <@80351110224678912>", &resolver),
///     "hi @Nelly"
/// );
/// ```
pub trait MentionResolver {
    /// Display name for a user (discord.js uses the member's display name,
    /// then the user's).
    fn user(&self, _id: GenericId) -> Option<String> {
        None
    }

    /// Name of a role.
    fn role(&self, _id: GenericId) -> Option<String> {
        None
    }

    /// Name of a channel.
    fn channel(&self, _id: GenericId) -> Option<String> {
        None
    }
}

impl<F> MentionResolver for F
where
    F: Fn(MentionKind, GenericId) -> Option<String>,
{
    fn user(&self, id: GenericId) -> Option<String> {
        self(MentionKind::User, id)
    }

    fn role(&self, id: GenericId) -> Option<String> {
        self(MentionKind::Role, id)
    }

    fn channel(&self, id: GenericId) -> Option<String> {
        self(MentionKind::Channel, id)
    }
}

/// Replaces mentions with readable text, like discord.js `cleanContent`:
/// `<@id>` / `<@!id>` → `@name`, `<@&id>` → `@role`, `<#id>` → `#channel`
/// (names from `resolver`; unknown IDs stay as they are),
/// `</name sub:id>` → `/name sub`, and `<:name:id>` / `<a:name:id>` →
/// `:name:`. IDs must have 17–20 digits, matching real snowflakes (discord.js
/// accepts 17–19). Command names use discord.js's character class exactly,
/// as of Unicode 17.0. Mentions inside code are replaced too, as in
/// discord.js.
///
/// `@everyone` and `@here` are left alone; suppress pings with
/// `AllowedMentions` instead.
pub fn clean_content<'t, R>(text: &'t str, resolver: &R) -> Cow<'t, str>
where
    R: MentionResolver + ?Sized,
{
    let mut edits = Edits::new(text);
    let mut search = 0;
    while let Some(offset) = text[search..].find('<') {
        let open = search + offset;
        match parse_tag(text, open) {
            Some((end, tag)) => {
                if let Some(replacement) = tag.resolve(resolver) {
                    edits.replace(open, end, &replacement);
                }
                search = end;
            }
            None => search = open + 1,
        }
    }
    edits.finish().map_or(Cow::Borrowed(text), Cow::Owned)
}

enum Tag<'t> {
    Mention(MentionKind, GenericId),
    Command(&'t str),
    Emoji(&'t str),
}

impl Tag<'_> {
    fn resolve<R: MentionResolver + ?Sized>(&self, resolver: &R) -> Option<String> {
        match self {
            Self::Mention(MentionKind::User, id) => {
                resolver.user(*id).map(|name| format!("@{name}"))
            }
            Self::Mention(MentionKind::Role, id) => {
                resolver.role(*id).map(|name| format!("@{name}"))
            }
            Self::Mention(MentionKind::Channel, id) => {
                resolver.channel(*id).map(|name| format!("#{name}"))
            }
            Self::Command(name) => Some(format!("/{name}")),
            Self::Emoji(name) => Some(format!(":{name}:")),
        }
    }
}

/// Parses the tag starting at `text[open] == '<'`, returning the end offset
/// (after `>`) and the tag.
fn parse_tag(text: &str, open: usize) -> Option<(usize, Tag<'_>)> {
    let rest = &text[open + 1..];
    let (body_start, kind) = if let Some(after) = rest.strip_prefix("@!") {
        (after, Some(MentionKind::User))
    } else if let Some(after) = rest.strip_prefix("@&") {
        (after, Some(MentionKind::Role))
    } else if let Some(after) = rest.strip_prefix('@') {
        (after, Some(MentionKind::User))
    } else if let Some(after) = rest.strip_prefix('#') {
        (after, Some(MentionKind::Channel))
    } else {
        (rest, None)
    };
    let body_offset = text.len() - body_start.len();

    if let Some(kind) = kind {
        let (id, end) = snowflake_then_close(text, body_offset)?;
        // Twenty digits can exceed u64 and all-zero digits are not an ID.
        // Neither can name a real target, so the tag stays text, exactly
        // what an unresolved mention produces.
        return id.parse().ok().map(|id| (end, Tag::Mention(kind, id)));
    }

    if let Some(command) = rest.strip_prefix('/') {
        let name_len = command
            .char_indices()
            .find(|&(_, ch)| !is_command_name_char(ch))
            .map_or(command.len(), |(position, _)| position);
        let name_start = open + 2;
        let name_end = name_start + name_len;
        if name_len == 0 || !text[name_end..].starts_with(':') {
            return None;
        }
        let (_, end) = snowflake_then_close(text, name_end + 1)?;
        return Some((end, Tag::Command(&text[name_start..name_end])));
    }

    let emoji = rest.strip_prefix("a:").or_else(|| rest.strip_prefix(':'))?;
    let name_start = text.len() - emoji.len();
    let name_len = emoji
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    let name_end = name_start + name_len;
    if name_len == 0 || !text[name_end..].starts_with(':') {
        return None;
    }
    let (_, end) = snowflake_then_close(text, name_end + 1)?;
    Some((end, Tag::Emoji(&text[name_start..name_end])))
}

/// 17–20 ASCII digits followed by `>` at `from`.
fn snowflake_then_close(text: &str, from: usize) -> Option<(&str, usize)> {
    let digits = text[from..].bytes().take_while(u8::is_ascii_digit).count();
    let close = from + digits;
    ((17..=20).contains(&digits) && text[close..].starts_with('>'))
        .then(|| (&text[from..close], close + 1))
}

// Decision log: the command-name class of `clean_content`.
// - Purpose: rewrite `</name:id>` exactly when discord.js `cleanContent`
//   does; its class is `[-_\p{L}\p{N}\p{sc=Deva}\p{sc=Thai} ]`, i.e.
//   General_Category letter/number plus Script (not Script_Extensions)
//   Devanagari/Thai.
// - Constraint: std has no General_Category or Script lookup.
//   `char::is_alphanumeric` is the Alphabetic/Numeric property (it admits
//   Other_Alphabetic marks such as U+0345 and misses Devanagari marks such
//   as U+A8E0), and its tables follow the compiler's Unicode version, so
//   results would differ between Rust 1.88 and 1.99. No regex dependency.
// - Alternatives: `is_alphanumeric` plus blocks (the earlier version,
//   inexact); `command::validate`'s predicate (fail-open by design: it
//   rejects only characters known to be invalid, right for validation but
//   here an accepted character changes the output); the `regex` crate.
// - Chosen: a range table generated by running discord.js's own class on
//   every code point (`command_name_ranges.rs`, from
//   `generate_command_name_ranges.js`), searched with a binary search.
// - Trade-offs: identical on every compiler and equal to discord.js on a
//   Unicode 17.0 runtime (Node 24); runtimes on older Unicode versions
//   reject letters added since. About 16 KB of source; regenerate the file
//   when targeting a newer Unicode version.

/// discord.js `[-_\p{L}\p{N}\p{sc=Deva}\p{sc=Thai} ]`; see the decision
/// log above.
fn is_command_name_char(ch: char) -> bool {
    let code_point = u32::from(ch);
    command_name_ranges::RANGES
        .binary_search_by(|&(low, high)| {
            if high < code_point {
                std::cmp::Ordering::Less
            } else if low > code_point {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

mod command_name_ranges;

#[cfg(test)]
mod tests;
