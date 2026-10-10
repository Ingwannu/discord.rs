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
- localized command names and descriptions
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

- **Names** (command, subcommands, groups, options, and name localizations): 1-32 characters from Discord's set `^[-_ʼ\p{L}\p{N}\p{sc=Deva}\p{sc=Thai}]{1,32}$`, using the lowercase variant of any letter that has one (`Éclair`, `ПРИВЕТ` fail; `차단`, `नमस्ते`, `straße` pass). The check uses only the standard library and accepts a slight superset of the regex. The ASCII apostrophe `'` is rejected: Discord accepts only `ʼ` (U+02BC) (discord-api-docs PR #8567).
- **Descriptions** and description localizations: 1-100 characters.
- **Localizations**: keys must be one of Discord's 32 locales (`limits::LOCALES`, compared case-insensitively); values follow the rules of the field they localize.
- **Each option level**: at most 25 entries; subcommands / groups never mixed with regular options; required options before optional ones; unique names (localized names are checked against the other default names and against each other per locale).
- **Nesting**: groups contain only subcommands; subcommands contain no subcommands or groups; other options have no children.
- **Choices**: at most 25, only on `STRING` / `INTEGER` / `NUMBER`, never together with `autocomplete: true`; names 1-100 characters, string values at most 100.
- **Ranges**: `min_value <= max_value`; `min_length` 0-6000, `max_length` 1-6000, `min_length <= max_length`.
- **`file_types`**: only on `ATTACHMENT`, at most 10 entries, each `.ext` or a group name.
- **Per-command total**: names, descriptions, and string choice values of the whole command may add up to at most 8000 characters, counting only the longest localization of each field (the docs said 4000 earlier). Numeric choice values are not counted, since Discord does not define how they count.

All lengths are Unicode codepoints, as Discord counts them.

`PrimaryEntryPointCommandBuilder` checks name and description lengths and its localizations (locale keys and lengths). `UserCommandBuilder` and `MessageCommandBuilder` check the name length only, because context-menu names may contain spaces and capitals.

```rust
use discordrs::{CommandOptionBuilder, SlashCommandBuilder};

let command = SlashCommandBuilder::new("ban", "Ban a member")
    .name_localization("ko", "차단")
    .option(CommandOptionBuilder::user("target", "Member to ban").required(true))
    .option(CommandOptionBuilder::string("reason", "Why").max_length(512))
    .try_build()?; // DiscordError::Model on the first broken rule
```

Not checked yet: option-level `name_localizations` / `description_localizations`, choice `name_localizations`, and `channel_types`. `ApplicationCommandOption` does not model these fields yet, so the builders cannot send them either.

## Registration

- Build the command definition with the typed builders
- Send it with the REST layer instead of hand-written JSON
- Keep raw overwrite calls only for compatibility or uncovered routes

## Permissions

Application command permission reads are typed through:

- `get_guild_application_command_permissions(guild_id)`
- `get_application_command_permissions(guild_id, command_id)`

Permission writes use `EditApplicationCommandPermissions` and `ApplicationCommandPermission`. Discord requires an OAuth2 Bearer token for the write route, so `edit_application_command_permissions(...)` does not use the bot token.
