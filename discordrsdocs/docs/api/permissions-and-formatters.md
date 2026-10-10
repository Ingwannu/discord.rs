# Permissions and Formatters

Two pure, dependency-free modules added in 3.0 (unreleased):

- `discordrs::permissions` — the discord.js `channel.permissionsFor(member)` / `member.permissions` equivalent.
- `discordrs::formatters` — the `@discordjs/formatters` equivalent: mentions, timestamps, markdown styles, lists, `escape_markdown`, and `clean_content`.

## Permission computation

The calculation follows Discord's documented algorithm ([Permissions](https://docs.discord.com/developers/topics/permissions)):

1. The guild owner has every permission.
2. Base permissions are `@everyone` OR every role the member has. `ADMINISTRATOR` in the base grants every permission and skips overwrites.
3. Channel overwrites apply in a fixed order: the `@everyone` overwrite (deny, then allow), all of the member's role overwrites combined (deny, then allow), then the member's own overwrite (deny, then allow). The order of the overwrite list does not matter.
4. A member with an active timeout keeps only `VIEW_CHANNEL` and `READ_MESSAGE_HISTORY`. Owners and administrators are exempt.
5. Implicit rules that Discord documents:
   - Without `VIEW_CHANNEL`, every channel-scoped permission is dropped. Guild-wide permissions such as `KICK_MEMBERS` stay, as in discord.js.
   - In text, announcement, forum, media, voice, and stage channels, without `SEND_MESSAGES` the member also loses `MENTION_EVERYONE`, `SEND_TTS_MESSAGES`, `ATTACH_FILES`, and `EMBED_LINKS`.
   - Threads use the parent channel's overwrites. `SEND_MESSAGES` is not inherited, so it is removed, and `SEND_MESSAGES_IN_THREADS` gates the four permissions above instead.
   - In voice and stage channels, without `CONNECT` the member loses `MANAGE_CHANNELS`.

Not modeled, because Discord does not document them as permission bits: private-thread membership, visibility from being connected to a voice channel, other "such as" consequences of a missing `CONNECT`, and `SEND_POLLS` / `SEND_VOICE_MESSAGES` without `SEND_MESSAGES`. Bits that do not apply to a channel type (for example `CONNECT` in a text channel) are kept, as in discord.js.

### From models

```rust
use std::time::SystemTime;
use discordrs::bitfield::permissions;
use discordrs::model::{Channel, Guild, Member};
use discordrs::permissions::{permissions_for, ChannelPermissions, PermissionDataError};

fn can_send(
    guild: &Guild,
    member: &Member,
    channel: &Channel,
    parent: Option<&Channel>,
) -> Result<Option<bool>, PermissionDataError> {
    let result = permissions_for(guild, member, channel, parent, SystemTime::now())?;
    let needed = if channel.is_thread() {
        permissions::SEND_MESSAGES_IN_THREADS
    } else {
        permissions::SEND_MESSAGES
    };
    // `None` when the channel is obfuscated: the answer is unknown.
    Ok(result.has(needed))
}
```

`channel.permissions_for(&guild, &member, parent)` and `member.guild_permissions(&guild)` are the same calls with the current time. `guild_permissions(guild, member, now)` returns guild-level permissions.

Roles are resolved from `guild.roles`. Anything missing is a typed `PermissionDataError` that names the object: `MissingRole` (the `@everyone` role when `role_id == guild_id`), `MissingRolePermissions`, `MissingGuildOwner`, `MissingMemberUser` (message `member` objects have no `user`; copy `message.author` into it), `MissingParentChannel`, `MissingPermissionOverwrites` (partial channels such as interaction `resolved` entries), `NotGuildChannel`, `GuildMismatch`, `ParentMismatch`, `InvalidParentChannel`, and `InvalidTimestamp`. `PermissionDataError` converts into `DiscordError::Model` for `?`.

### Results

- `ChannelPermissions::Known(ComputedPermissions)` or `ChannelPermissions::Obfuscated`.
- `ComputedPermissions { permissions, source, timeout_applied }`, where `source` is `PermissionSource::{Owner, Administrator, Computed}`.
- `ComputedPermissions::has(required)` and `missing(required)` accept `Permissions` values (`permissions::SEND_MESSAGES | permissions::ATTACH_FILES`) or raw bits. Owners and administrators pass every check, including permissions newer than the crate.
- `ChannelPermissions::has(required)` returns `Option<bool>`: `None` means unknown.

### Obfuscated channels

From 2026-11-16 Discord sends channels a bot cannot view with the `CHANNEL_OBFUSCATED` flag and a single synthetic `@everyone` `VIEW_CHANNEL` deny overwrite. Computing over that overwrite would be a wrong answer for every other member, so the result is `ChannelPermissions::Obfuscated` (also when only a thread's parent is obfuscated). For the bot itself, obfuscation already means it lacks `VIEW_CHANNEL` there. Owners and administrators still get `Known`, because their answer does not depend on overwrites.

### From the cache

```rust
use std::time::SystemTime;
use discordrs::model::Snowflake;
use discordrs::permissions::{cached_permissions_for, PermissionDataError};
use discordrs::CacheHandle;

async fn check(
    cache: &CacheHandle,
    guild: &Snowflake,
    user: &Snowflake,
    channel: &Snowflake,
) -> Result<(), PermissionDataError> {
    let result = cached_permissions_for(cache, guild, user, channel, SystemTime::now()).await?;
    println!("{result:?}");
    Ok(())
}
```

`cached_permissions_for` and `cached_guild_permissions` read the guild (for `owner_id`), the member, the channel, a thread's parent, and every role through `CacheHandle`'s public read API. Roles come from the role store, which role update events refresh, not from the cached guild's `roles` list (a GUILD_CREATE snapshot). Missing data is reported (`MissingGuild`, `MissingMember`, `MissingChannel`, `MissingRole`, ..., or `CacheDisabled` without the `cache` feature) instead of being filled with defaults. Each piece is a separate read, so a concurrent update can be seen half-applied.

### Explicit inputs

For data that is not in model form, the pure core takes everything explicitly and never fails:

```rust
use std::time::{Duration, SystemTime};
use discordrs::bitfield::permissions;
use discordrs::model::{PermissionOverwrite, Snowflake};
use discordrs::permissions::{ChannelClass, ChannelPermissionInput, MemberPermissionInput};

fn example(overwrites: &[PermissionOverwrite]) {
    let (guild, owner, member, role) = (
        Snowflake::from(1u64),
        Snowflake::from(2u64),
        Snowflake::from(3u64),
        Snowflake::from(4u64),
    );
    let now = SystemTime::now();
    let input = MemberPermissionInput::new(&guild, &owner, &member, permissions::VIEW_CHANNEL)
        .role(&role, permissions::SEND_MESSAGES)
        .communication_disabled_until(now + Duration::from_secs(60), now);

    let guild_level = input.guild_permissions();
    let in_channel = input.in_channel(&ChannelPermissionInput::new(ChannelClass::Text, overwrites));
    // For a thread pass ChannelClass::Thread with the parent's overwrites;
    // mark obfuscated channels with `.obfuscated(true)`.
    let _ = (guild_level, in_channel);
}
```

`ChannelPermissionInput::from_channel(guild_id, &channel, parent)` builds the channel side from a `Channel` with the same validation as `permissions_for`.

### Note on `ALL_PERMISSIONS`

Owners and administrators receive `discordrs::permissions::ALL_PERMISSIONS`, built from Discord's permission table (bits 0–46 and 48–52). It is not `Permissions::all()`, because the crate's `PermissionFlags` name table is shifted by one from bit 41 upward and lacks `PIN_MESSAGES` (51).

## Formatters

Every formatter takes `impl Display` and returns `impl Display`, so nesting allocates only when the result is written. Text transforms return `Cow<str>` and borrow the input when nothing changes.

```rust
use discordrs::formatters::{bold, italic, timestamp, user_mention, TimestampStyle};

let line = format!(
    "{} joined {}",
    bold(italic(user_mention(80351110224678912u64))),
    timestamp(1618953630).style(TimestampStyle::RelativeTime),
);
assert_eq!(line, "**_<@80351110224678912>_** joined <t:1618953630:R>");
```

| Function | Output |
|---|---|
| `user_mention(id)`, `channel_mention(id)`, `role_mention(id)` | `<@id>`, `<#id>`, `<@&id>` |
| `linked_role_mention(id)`, `game_profile_mention(id)` | `<id:linked-roles:id>`, `<@$id>` |
| `slash_command_mention(name, id)` (`.subcommand(sub)`, `.subcommand_in_group(group, sub)`) | `</name:id>`, `</name sub:id>`, `</name group sub:id>` |
| `custom_emoji(name, id)`, `animated_emoji(name, id)` | `<:name:id>`, `<a:name:id>` |
| `timestamp(secs)`, `.style(TimestampStyle::...)`, `Timestamp::from(SystemTime)` | `<t:secs>`, `<t:secs:R>` |
| `GuildNavigation::{Customize, Browse, Guide, LinkedRoles}` | `<id:customize>`, ... |
| `bold`, `italic`, `underline`, `strikethrough`, `spoiler`, `inline_code` | `**x**`, `_x_`, `__x__`, `~~x~~`, `\|\|x\|\|`, `` `x` `` |
| `code_block(x)`, `code_block_with_language(lang, x)` | ```` ```\nx\n``` ````, ```` ```lang\nx\n``` ```` |
| `quote`, `block_quote`, `subtext`, `heading(x, HeadingLevel::Two)` | `> x`, `>>> x`, `-# x`, `## x` |
| `hyperlink(text, url)`, `hyperlink_with_title(text, url, title)`, `hide_link_embed(url)` | `[text](url)`, `[text](url "title")`, `<url>` |
| `channel_link`, `dm_channel_link`, `message_link`, `dm_message_link` | `https://discord.com/channels/...` |
| `unordered_list(&items)`, `ordered_list(&items, start)`, `*_nested(&[ListItem])` | `- a`, `1. a` (indented sub-lists) |

`TimestampStyle` covers all nine documented styles: `t`, `T`, `d`, `D`, `f`, `F`, `s`, `S`, `R`. Ordered lists repeat the start number on every item, as in discord.js; Discord renumbers them when rendering. `clean_code_block_content(text)` keeps untrusted text from closing a code block.

### `escape_markdown`

```rust
use discordrs::formatters::{escape_markdown, EscapeMarkdownOptions};

assert_eq!(
    escape_markdown("**hi** `code`", EscapeMarkdownOptions::default()),
    "\\*\\*hi\\*\\* \\`code\\`"
);
let keep_code = EscapeMarkdownOptions::default()
    .code_block(false)
    .code_block_content(false);
assert_eq!(escape_markdown("**a** ```**b**```", keep_code), "\\*\\*a\\*\\* ```**b**```");
```

`EscapeMarkdownOptions::default()` matches the published `@discordjs/formatters` 0.6.2 (discord.js v14): code blocks, inline code, bold, italic, underline, strikethrough, spoiler, and backslashes on; headings, bulleted and numbered lists, masked links, quotes, and block quotes off. `EscapeMarkdownOptions::all()` turns everything on; each option has a chainable setter. The individual escapers (`escape_bold`, `escape_italic`, `escape_heading`, ...) are public too.

The output matches discord.js 0.6.2 for every case in the committed golden corpus (`src/formatters/escape_golden.json`, 1,710 option combinations plus 1,080 single-escaper cases, generated from the published package). Three discord.js regex bugs are deliberately not reproduced:

- discord.js leaves every `_` after a `<:` on the same line unescaped; only names inside real `<:name:id>` tags are protected here.
- discord.js escapes `_` inside `<scheme:/...>` autolinks (a one-character-scheme typo) while protecting `*`; both are protected here.
- discord.js escapes only the first masked link on a line; every one is escaped here.

### `clean_content`

```rust
use discordrs::formatters::{clean_content, MentionKind};
use discordrs::model::Snowflake;

let names = |kind: MentionKind, id: &Snowflake| match (kind, id.as_str()) {
    (MentionKind::User, "80351110224678912") => Some("Nelly".to_string()),
    _ => None,
};
assert_eq!(clean_content("hi <@80351110224678912>", &names), "hi @Nelly");
```

Like discord.js `cleanContent`: `<@id>` / `<@!id>` → `@name`, `<@&id>` → `@role`, `<#id>` → `#channel`, `</name sub:id>` → `/name sub`, and custom emoji → `:name:`. Names come from an explicit `MentionResolver` (implement it on a type, or pass a closure); there is no hidden cache lookup, and unknown IDs stay as written. IDs need 17–20 digits (discord.js accepts 17–19). `@everyone` / `@here` are not changed; use `AllowedMentions` to suppress pings. `message.clean_content(&resolver)` and `member.mention()` are thin model helpers over these functions.
