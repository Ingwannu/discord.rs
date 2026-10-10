# Commands API

`discord.rs` now exposes typed application command builders instead of relying on raw `Vec<Value>` payload assembly.

## Main Builders

- `SlashCommandBuilder`
- `UserCommandBuilder`
- `MessageCommandBuilder`
- `PrimaryEntryPointCommandBuilder`
- `CommandOptionBuilder`

## Supported Patterns

- slash commands with typed options
- user and message context menu commands
- option choices, ranges, and string length constraints
- autocomplete flags
- localized names and descriptions for commands and options, and localized choice names (`name_localization`, `description_localization`, `localized_choice`)
- channel type filters on `CHANNEL` options (`channel_types`)
- local validation against Discord's limits (`validate()` / `try_build()`)

## Example

```rust
use discordrs::{option_type, CommandOptionBuilder, SlashCommandBuilder};

let command = SlashCommandBuilder::new("ticket", "Create a support ticket")
    .option(
        CommandOptionBuilder::new(option_type::STRING, "topic", "Ticket topic")
            .required(true)
            .autocomplete(true),
    )
    .build();
```

## Validation

`build()` never validates. `validate()` (borrows the builder) and `try_build()` check Discord's documented rules locally and return a `DiscordError::Model` that names the field, the rule, and the actual value, instead of a Discord 400 at registration time.

Local validation is a fail-fast aid, not a gate: it is deliberately **never stricter than Discord**. Where a rule is undocumented or ambiguous it lets the value through, so passing `validate()` does not guarantee that Discord accepts the command. All numeric limits and the locale table live in `discordrs::command::limits`, each with its documentation source.

`SlashCommandBuilder::validate()` checks (`CommandOptionBuilder::validate()` checks the same for one option subtree):

- **Names** (command, subcommands, groups, options, and their name localizations): 1-32 characters from Discord's set `^[-_\u02BC\p{L}\p{N}\p{sc=Deva}\p{sc=Thai}]{1,32}$`, using the lowercase variant of any letter that has one (`Éclair`, `ПРИВЕТ` fail; `차단`, `नमस्ते`, `straße` pass). The ASCII apostrophe `'` is rejected: Discord accepts only `ʼ` (U+02BC) (discord-api-docs PR #8567).
  The check uses only the standard library and rejects only characters it knows Discord rejects: ASCII other than letters, digits, `-`, and `_`; whitespace and control characters; and characters from punctuation, symbol, emoji, and private-use blocks. Every other code point passes, including ones the compiler's Unicode tables do not know yet, so the result does not depend on the Rust version (for example U+088F and U+1E6C0, added in Unicode 17, pass on Rust 1.88 and 1.99 alike). Some characters Discord rejects therefore pass locally, such as combining accents (U+0301); Discord reports those at registration.
- **Descriptions** and description localizations: 1-100 characters.
- **Localizations** of the command, its options, and choice names: keys must be one of Discord's 32 locales (`limits::LOCALES`, compared case-insensitively); values follow the rules of the field they localize (choice names 1-100 characters).
- **Each option level**: at most 25 entries; subcommands / groups never mixed with regular options; required options before optional ones; unique names. A localized option name must differ from the other options' default names and from their localized names in the same locale.
- **Nesting**: groups contain only subcommands; subcommands contain no subcommands or groups; other options have no children.
- **Choices**: at most 25, only on `STRING` / `INTEGER` / `NUMBER`, never together with `autocomplete: true`; names 1-100 characters, string values at most 100.
- **Ranges**: `min_value <= max_value`; `min_length` 0-6000, `max_length` 1-6000, `min_length <= max_length`.
- **`channel_types`**: only on `CHANNEL` options (an empty list is allowed anywhere). The values are not checked, so new channel types never fail locally.
- **`file_types`**: only on `ATTACHMENT`, at most 10 entries, each `.ext` or a group name.
- **Per-command total**: names, descriptions, choice names, and string choice values of the whole command may add up to at most 8000 characters, counting only the longest variant (default or localization) of each field (the docs said 4000 earlier). Numeric choice values are not counted, since Discord does not define how they count.

All lengths are Unicode codepoints, as Discord counts them.

`PrimaryEntryPointCommandBuilder` checks name and description lengths and its localizations (locale keys and lengths). `UserCommandBuilder` and `MessageCommandBuilder` check the name length only, because context-menu names may contain spaces and capitals.

```rust
use discordrs::{CommandOptionBuilder, SlashCommandBuilder};

let command = SlashCommandBuilder::new("ban", "Ban a member")
    .name_localization("ko", "차단")
    .option(
        CommandOptionBuilder::user("target", "Member to ban")
            .required(true)
            .name_localization("ko", "대상"),
    )
    .option(
        CommandOptionBuilder::string("reason", "Why")
            .max_length(512)
            .localized_choice("Spam", "spam", [("ko", "스팸")]),
    )
    .option(CommandOptionBuilder::channel("log", "Log channel").channel_types([0]))
    .try_build()?; // DiscordError::Model on the first broken rule
```

## Registration

- Build the command definition with the typed builders
- Send it with the REST layer instead of hand-written JSON
- Keep raw overwrite calls only for compatibility or uncovered routes

## Permissions

Application command permission reads are typed through:

- `get_guild_application_command_permissions(guild_id)`
- `get_application_command_permissions(guild_id, command_id)`

Permission writes use `EditApplicationCommandPermissions` and `ApplicationCommandPermission`. Discord requires an OAuth2 Bearer token for the write route, so `edit_application_command_permissions(...)` does not use the bot token.
