use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::DiscordError;
use crate::model::{
    ApplicationCommand, ApplicationCommandHandlerType, ApplicationCommandOption,
    ApplicationCommandOptionChoice, ApplicationIntegrationType, InteractionContextType,
    PermissionsBitField,
};

/// Discord's documented application-command limits, used by the builders'
/// `validate()` / `try_build()`.
///
/// Source: <https://docs.discord.com/developers/interactions/application-commands>
/// (command, option, and choice structures, "Application Command Naming",
/// and the per-command character total), checked against discord-api-docs
/// commit `3714862` (2026-10-09). Local validation is a fail-fast aid, not a
/// gate: when Discord raises a limit, change it here and nowhere else.
pub mod limits {
    /// Chat-input command, subcommand, group, and option names: 1-32
    /// characters. Context-menu names share the length limit.
    pub const MAX_NAME_LENGTH: usize = 32;
    /// Chat-input and entry-point command descriptions and option
    /// descriptions: 1-100 characters.
    pub const MAX_DESCRIPTION_LENGTH: usize = 100;
    /// Options, subcommands, or subcommand groups per level: up to 25.
    pub const MAX_OPTIONS: usize = 25;
    /// Choices per `STRING` / `INTEGER` / `NUMBER` option: up to 25.
    pub const MAX_CHOICES: usize = 25;
    /// Choice names: 1-100 characters.
    pub const MAX_CHOICE_NAME_LENGTH: usize = 100;
    /// String choice values: up to 100 characters.
    pub const MAX_CHOICE_STRING_VALUE_LENGTH: usize = 100;
    /// Upper bound of `min_length` (0-6000) and `max_length` (1-6000) on
    /// `STRING` options.
    pub const MAX_STRING_OPTION_LENGTH: u16 = 6000;
    /// Combined characters of a chat-input command's names, descriptions,
    /// and choice values across the whole option tree. For a localized field
    /// only its longest variant (default included) counts. The docs said
    /// 4000 when the 3.0 roadmap was written; they now say 8000.
    pub const MAX_COMMAND_TOTAL_LENGTH: usize = 8000;
    /// `file_types` entries on File Upload components and `ATTACHMENT`
    /// options: up to 10.
    pub const MAX_FILE_TYPES: usize = 10;
    /// Locale keys accepted in `name_localizations` and
    /// `description_localizations`.
    ///
    /// Source: <https://docs.discord.com/developers/reference#locales>
    /// (discord-api-docs `3714862`). Discord adds locales rarely; until this
    /// table is updated a new locale fails local validation (`build()` is
    /// unaffected).
    pub const LOCALES: [&str; 32] = [
        "id", "da", "de", "en-GB", "en-US", "es-ES", "es-419", "fr", "hr", "it", "lt", "hu", "nl",
        "no", "pl", "pt-BR", "ro", "fi", "sv-SE", "vi", "tr", "cs", "el", "bg", "ru", "uk", "hi",
        "th", "zh-CN", "ja", "zh-TW", "ko",
    ];
}

/// Crate-internal helpers shared by the builder validation methods.
///
/// Discord counts limits in Unicode codepoints, so all length checks use
/// `chars().count()` rather than byte length.
pub(crate) mod validation {
    use super::limits;
    use crate::error::DiscordError;

    /// Number of Unicode codepoints in `value` (Discord's counting unit).
    pub(crate) fn char_count(value: &str) -> usize {
        value.chars().count()
    }

    /// Ensures `value` is `min..=max` characters long.
    pub(crate) fn ensure_len(
        field: &str,
        value: &str,
        min: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        let count = char_count(value);
        if count < min || count > max {
            return Err(DiscordError::model(format!(
                "{field} must be {min}-{max} characters, got {count}"
            )));
        }
        Ok(())
    }

    /// Ensures `value` is at most `max` characters long.
    pub(crate) fn ensure_max_len(field: &str, value: &str, max: usize) -> Result<(), DiscordError> {
        let count = char_count(value);
        if count > max {
            return Err(DiscordError::model(format!(
                "{field} must be at most {max} characters, got {count}"
            )));
        }
        Ok(())
    }

    /// Ensures a collection holds `min..=max` items.
    pub(crate) fn ensure_count(
        field: &str,
        actual: usize,
        min: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        if actual < min || actual > max {
            return Err(DiscordError::model(format!(
                "{field} must contain {min}-{max} items, got {actual}"
            )));
        }
        Ok(())
    }

    /// Ensures a collection holds at most `max` items.
    pub(crate) fn ensure_max_count(
        field: &str,
        actual: usize,
        max: usize,
    ) -> Result<(), DiscordError> {
        if actual > max {
            return Err(DiscordError::model(format!(
                "{field} must contain at most {max} items, got {actual}"
            )));
        }
        Ok(())
    }

    /// Validates a `file_types` filter: at most [`limits::MAX_FILE_TYPES`]
    /// entries, each a dot-prefixed extension (`.pdf`) or a preset group name
    /// (`image`). Group names are not checked against a fixed list because
    /// Discord says the presets may change; only the shape is enforced, so
    /// local validation never rejects a value Discord would accept.
    pub(crate) fn ensure_file_types(
        field: &str,
        file_types: &[String],
    ) -> Result<(), DiscordError> {
        ensure_max_count(field, file_types.len(), limits::MAX_FILE_TYPES)?;
        for file_type in file_types {
            let valid = match file_type.strip_prefix('.') {
                Some(extension) => !extension.is_empty() && !extension.contains(['.', '/', ' ']),
                None => {
                    !file_type.is_empty()
                        && file_type
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                }
            };
            if !valid {
                return Err(DiscordError::model(format!(
                    "{field} entry {file_type:?} must be a dot-prefixed extension \
                     like \".pdf\" or a group name like \"image\""
                )));
            }
        }
        Ok(())
    }

    // Decision log: chat-input name check without a Unicode crate.
    // - Purpose: catch names Discord rejects with
    //   `^[-_\u02BC\p{L}\p{N}\p{sc=Deva}\p{sc=Thai}]{1,32}$` plus "use the
    //   lowercase variant of any letter that has one".
    // - Constraint: std has no General_Category or Script lookup, local
    //   validation must never be stricter than Discord, and std's Unicode
    //   tables change with the compiler (Rust 1.88 = Unicode 16, 1.99 =
    //   Unicode 17), so "not alphanumeric" also covers code points the
    //   compiler simply does not know yet (U+088F, U+1E6C0 on 1.88).
    // - Alternatives: the `regex` crate (exact, but a large dependency for
    //   one check and still tied to its own Unicode version); a
    //   general-category crate; rejecting everything not alphanumeric (an
    //   earlier version of this code, stricter than Discord on older
    //   compilers); rejecting only characters known to be disallowed.
    // - Chosen: reject only what is known to be disallowed: ASCII outside
    //   `[A-Za-z0-9_-]`, White_Space and control characters, and a fixed list
    //   of punctuation / symbol / emoji / private-use blocks whose every
    //   non-alphanumeric character is disallowed and whose free slots are
    //   reserved for more of the same. Alphanumerics and the Devanagari and
    //   Thai blocks always pass; any other code point passes because it may
    //   be a letter in a newer Unicode version. Lowercase = `to_lowercase()`
    //   maps the char to itself, the docs' "has a lowercase variant" rule.
    // - Why: the realistic mistakes (spaces, punctuation, emoji, ASCII
    //   apostrophe, uppercase) are still caught, and whether a character is
    //   rejected no longer depends on the compiler's Unicode version.
    // - Trade-offs: other characters Discord rejects pass locally and Discord
    //   reports them (combining accents such as U+0301, symbols outside the
    //   listed blocks, danda U+0964/U+0965, Other_Alphabetic symbols such as
    //   U+24B6). A brand-new cased letter is rejected as uppercase only on
    //   compilers whose tables know it; Discord rejects it either way. ASCII
    //   `'` is rejected on purpose: discord-api-docs PR #8567 (2026-08-27)
    //   says Discord's code accepts only U+02BC.

    /// Devanagari (U+0900-U+097F, Vedic Extensions, Devanagari Extended and
    /// Extended-A) and Thai (U+0E00-U+0E7F) blocks, covering Discord's
    /// `\p{sc=Deva}` and `\p{sc=Thai}` including their combining marks.
    fn is_devanagari_or_thai(ch: char) -> bool {
        matches!(
            ch,
            '\u{0900}'..='\u{097F}'
                | '\u{0E00}'..='\u{0E7F}'
                | '\u{1CD0}'..='\u{1CFF}'
                | '\u{A8E0}'..='\u{A8FF}'
                | '\u{11B00}'..='\u{11B5F}'
        )
    }

    /// Blocks in which every character that is not alphanumeric is
    /// punctuation, a symbol, an emoji, a variation selector, or private use
    /// (all outside Discord's set), and whose unassigned slots Unicode keeps
    /// for more of the same. Their letters and digits (`ª`, `²`, `❶`, `々`,
    /// ...) are long-assigned, so `is_alphanumeric` admits them first on
    /// every supported compiler.
    fn is_in_non_letter_block(ch: char) -> bool {
        matches!(
            ch,
            // Latin-1 punctuation and symbols, multiplication and division signs.
            '\u{00A0}'..='\u{00BF}' | '\u{00D7}' | '\u{00F7}'
                // General Punctuation, Currency Symbols.
                | '\u{2000}'..='\u{206F}' | '\u{20A0}'..='\u{20CF}'
                // Arrows, Mathematical Operators, Miscellaneous Technical.
                | '\u{2190}'..='\u{23FF}'
                // Box Drawing through Dingbats.
                | '\u{2500}'..='\u{27BF}'
                // CJK Symbols and Punctuation.
                | '\u{3000}'..='\u{303F}'
                // Private Use Area, Variation Selectors.
                | '\u{E000}'..='\u{F8FF}' | '\u{FE00}'..='\u{FE0F}'
                // Mahjong Tiles through Symbols and Pictographs Extended-A.
                | '\u{1F000}'..='\u{1FAFF}'
                // Supplementary Private Use Areas and their noncharacters.
                | '\u{F0000}'..='\u{10FFFF}'
        )
    }

    /// Whether `ch` is certainly outside Discord's name character set.
    /// Unknown code points are not rejected (see the decision log above).
    fn is_rejected_name_char(ch: char) -> bool {
        if ch.is_ascii() {
            return !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
        }
        if ch.is_alphanumeric() || is_devanagari_or_thai(ch) {
            return false;
        }
        ch.is_whitespace() || ch.is_control() || is_in_non_letter_block(ch)
    }

    fn has_lowercase_variant(ch: char) -> bool {
        !ch.to_lowercase().eq(std::iter::once(ch))
    }

    /// Ensures a chat-input command, subcommand, or option name (or one of
    /// its localizations) is valid: 1-32 characters from Discord's allowed
    /// set, using lowercase variants where a letter has one.
    pub(crate) fn ensure_command_name(field: &str, value: &str) -> Result<(), DiscordError> {
        ensure_len(field, value, 1, limits::MAX_NAME_LENGTH)?;
        if value.contains(' ') {
            return Err(DiscordError::model(format!(
                "{field} must not contain spaces, got \"{value}\""
            )));
        }
        if let Some(ch) = value.chars().find(|ch| is_rejected_name_char(*ch)) {
            let hint = if ch == '\'' {
                "; use \u{02BC} (U+02BC MODIFIER LETTER APOSTROPHE) instead of '"
            } else {
                ""
            };
            return Err(DiscordError::model(format!(
                "{field} contains {ch:?}, but only letters, numbers, '-', and '_' \
                 are allowed, got \"{value}\"{hint}"
            )));
        }
        if value.chars().any(has_lowercase_variant) {
            return Err(DiscordError::model(format!(
                "{field} must be lowercase, got \"{value}\""
            )));
        }
        Ok(())
    }

    /// Ensures `locale` is one of Discord's locale keys. Compared
    /// ASCII-case-insensitively because it is not documented whether Discord
    /// normalizes case, and local validation must not be stricter.
    pub(crate) fn ensure_locale(field: &str, locale: &str) -> Result<(), DiscordError> {
        if limits::LOCALES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(locale))
        {
            Ok(())
        } else {
            Err(DiscordError::model(format!(
                "{field} uses unsupported locale \"{locale}\"; see \
                 https://docs.discord.com/developers/reference#locales"
            )))
        }
    }
}

// Decision log: phase 7 chat-input validation (ROADMAP appendix 1).
// - Purpose: fail fast on commands Discord would reject with a 400, with an
//   error naming the field, rule, and value.
// - Constraint: `validate(&self)` / unchecked `build()` stay as they are, and
//   local rules must never be stricter than Discord: a false rejection blocks
//   a command that would register.
// - Alternatives: mirror every documented sentence as a hard rule; check only
//   rules the docs state unambiguously and allow everything else.
// - Chosen: the unambiguous rules only. Limits come from the current docs
//   (`3714862`), so the per-command total is 8000, not the roadmap's 4000.
//   Ambiguous points resolve toward allowing: localized-name uniqueness is
//   per level rather than command-wide, numeric choice values are not
//   counted toward the total, locale keys compare case-insensitively, and
//   autocomplete/min_value/min_length are not restricted to their option
//   types or numeric bounds.
// - Why: undercounting or skipping a rule only means Discord reports the
//   error instead of us; overcounting would break valid commands.
// - Trade-offs: some commands Discord rejects pass locally and fail at
//   registration instead. Channel type values in `channel_types` are not
//   checked against a list, so new channel types never fail locally.

/// How a command kind's names are checked.
#[derive(Clone, Copy)]
enum NameRule {
    /// Chat-input names: Discord's character set and lowercase rule.
    ChatInput,
    /// Entry-point and context-menu names: only the 1-32 length.
    LengthOnly,
}

impl NameRule {
    fn check(self, field: &str, value: &str) -> Result<(), DiscordError> {
        match self {
            Self::ChatInput => validation::ensure_command_name(field, value),
            Self::LengthOnly => validation::ensure_len(field, value, 1, limits::MAX_NAME_LENGTH),
        }
    }
}

/// Validates `name_localizations` / `description_localizations`: known
/// locale keys, and values under the same rules as the default field
/// ("Values follow the same restrictions as `name`/`description`").
fn validate_localizations(
    owner: &str,
    rule: NameRule,
    name_localizations: Option<&HashMap<String, String>>,
    description_localizations: Option<&HashMap<String, String>>,
) -> Result<(), DiscordError> {
    for (locale, name) in sorted_entries(name_localizations) {
        validation::ensure_locale(&format!("{owner} name_localizations"), locale)?;
        rule.check(&format!("{owner} name_localizations[{locale}]"), name)?;
    }
    for (locale, description) in sorted_entries(description_localizations) {
        validation::ensure_locale(&format!("{owner} description_localizations"), locale)?;
        validation::ensure_len(
            &format!("{owner} description_localizations[{locale}]"),
            description,
            1,
            limits::MAX_DESCRIPTION_LENGTH,
        )?;
    }
    Ok(())
}

/// Localization entries sorted by locale so the first reported error does
/// not depend on `HashMap` iteration order.
fn sorted_entries(map: Option<&HashMap<String, String>>) -> Vec<(&str, &str)> {
    let mut entries: Vec<(&str, &str)> = map
        .into_iter()
        .flatten()
        .map(|(locale, value)| (locale.as_str(), value.as_str()))
        .collect();
    entries.sort_unstable();
    entries
}

/// Characters a field contributes to the per-command total: the longest of
/// the default value and its localizations, as Discord counts it.
fn localized_char_count(default: &str, localizations: Option<&HashMap<String, String>>) -> usize {
    localizations
        .into_iter()
        .flat_map(HashMap::values)
        .map(|value| validation::char_count(value))
        .fold(validation::char_count(default), usize::max)
}

/// Characters an option subtree contributes to the per-command total.
///
/// Only string choice values are counted. Discord's docs say "value
/// properties" without defining how numbers count, and undercounting can only
/// make local validation more lenient than Discord, never stricter.
fn option_char_count(option: &ApplicationCommandOption) -> usize {
    let own = localized_char_count(&option.name, option.name_localizations.as_ref())
        + localized_char_count(
            &option.description,
            option.description_localizations.as_ref(),
        );
    let choices: usize = option
        .choices
        .iter()
        .map(|choice| {
            localized_char_count(&choice.name, choice.name_localizations.as_ref())
                + choice
                    .value
                    .as_str()
                    .map(validation::char_count)
                    .unwrap_or(0)
        })
        .sum();
    let nested: usize = option.options.iter().map(option_char_count).sum();
    own + choices + nested
}

/// Ensures option names are unique within one level, including localized
/// names.
///
/// Discord: "Localized option names [...] must be distinct from all other
/// default option names of that command, as well as all other option names
/// within that locale on that command." Applied per level (siblings), since
/// options in different subcommands may share default names and a
/// command-wide reading is not confirmed; when unsure, allow.
fn ensure_unique_option_names(
    level: &str,
    options: &[(&str, Option<&HashMap<String, String>>)],
) -> Result<(), DiscordError> {
    let mut defaults = HashSet::new();
    for (name, _) in options {
        if !defaults.insert(*name) {
            return Err(DiscordError::model(format!(
                "{level} has more than one option named \"{name}\""
            )));
        }
    }

    let mut localized: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (index, (name, localizations)) in options.iter().enumerate() {
        for (locale, localized_name) in sorted_entries(*localizations) {
            // Discord ignores a localization identical to the default.
            if localized_name == *name {
                continue;
            }
            let clashes_with_default = options
                .iter()
                .enumerate()
                .any(|(other, (other_name, _))| other != index && *other_name == localized_name);
            let per_locale = localized.entry(locale).or_default();
            if clashes_with_default || !per_locale.insert(localized_name) {
                return Err(DiscordError::model(format!(
                    "{level} option \"{name}\" name_localizations[{locale}] \
                     \"{localized_name}\" collides with another option name"
                )));
            }
        }
    }
    Ok(())
}

fn is_subcommand_like(option: &ApplicationCommandOption) -> bool {
    option.kind == option_type::SUB_COMMAND || option.kind == option_type::SUB_COMMAND_GROUP
}

/// Validates one level of options (a command's top level or a subcommand's
/// or group's children), then each option in it.
fn validate_option_level(
    level: &str,
    options: &[ApplicationCommandOption],
) -> Result<(), DiscordError> {
    validation::ensure_max_count(
        &format!("{level} options"),
        options.len(),
        limits::MAX_OPTIONS,
    )?;

    if options.iter().any(is_subcommand_like) {
        if let Some(plain) = options.iter().find(|option| !is_subcommand_like(option)) {
            return Err(DiscordError::model(format!(
                "{level} mixes subcommands or subcommand groups with the regular \
                 option \"{}\"; a level holds one kind or the other",
                plain.name
            )));
        }
    }

    // "Required options must be listed before optional options." `required`
    // does not apply to subcommands, so they are skipped.
    let mut first_optional: Option<&str> = None;
    for option in options.iter().filter(|option| !is_subcommand_like(option)) {
        let required = option.required == Some(true);
        match (required, first_optional) {
            (true, Some(optional)) => {
                return Err(DiscordError::model(format!(
                    "{level} lists required option \"{}\" after optional option \
                     \"{optional}\"; required options must come first",
                    option.name
                )));
            }
            (false, None) => first_optional = Some(&option.name),
            _ => {}
        }
    }

    let names: Vec<(&str, Option<&HashMap<String, String>>)> = options
        .iter()
        .map(|option| (option.name.as_str(), option.name_localizations.as_ref()))
        .collect();
    ensure_unique_option_names(level, &names)?;

    for option in options {
        validate_option(option)?;
    }
    Ok(())
}

/// Validates choices, autocomplete, and value/length ranges of one option.
fn validate_option_constraints(option: &ApplicationCommandOption) -> Result<(), DiscordError> {
    let name = &option.name;
    validation::ensure_max_count(
        &format!("option \"{name}\" choices"),
        option.choices.len(),
        limits::MAX_CHOICES,
    )?;
    if !option.choices.is_empty() {
        if !matches!(
            option.kind,
            option_type::STRING | option_type::INTEGER | option_type::NUMBER
        ) {
            return Err(DiscordError::model(format!(
                "option \"{name}\" of type {} has choices, which only STRING, \
                 INTEGER, and NUMBER options accept",
                option.kind
            )));
        }
        if option.autocomplete == Some(true) {
            return Err(DiscordError::model(format!(
                "option \"{name}\" sets autocomplete and choices; Discord allows \
                 only one of them"
            )));
        }
    }
    for choice in &option.choices {
        validation::ensure_len(
            &format!("option \"{name}\" choice name"),
            &choice.name,
            1,
            limits::MAX_CHOICE_NAME_LENGTH,
        )?;
        for (locale, localized) in sorted_entries(choice.name_localizations.as_ref()) {
            let field = format!(
                "option \"{name}\" choice \"{}\" name_localizations",
                choice.name
            );
            validation::ensure_locale(&field, locale)?;
            validation::ensure_len(
                &format!("{field}[{locale}]"),
                localized,
                1,
                limits::MAX_CHOICE_NAME_LENGTH,
            )?;
        }
        if let Some(value) = choice.value.as_str() {
            validation::ensure_max_len(
                &format!("option \"{name}\" choice \"{}\" value", choice.name),
                value,
                limits::MAX_CHOICE_STRING_VALUE_LENGTH,
            )?;
        }
    }

    if let (Some(min), Some(max)) = (option.min_value, option.max_value) {
        if min > max {
            return Err(DiscordError::model(format!(
                "option \"{name}\" min_value {min} is greater than max_value {max}"
            )));
        }
    }
    if let Some(min_length) = option.min_length {
        if min_length > limits::MAX_STRING_OPTION_LENGTH {
            return Err(DiscordError::model(format!(
                "option \"{name}\" min_length must be 0-{}, got {min_length}",
                limits::MAX_STRING_OPTION_LENGTH
            )));
        }
    }
    if let Some(max_length) = option.max_length {
        if !(1..=limits::MAX_STRING_OPTION_LENGTH).contains(&max_length) {
            return Err(DiscordError::model(format!(
                "option \"{name}\" max_length must be 1-{}, got {max_length}",
                limits::MAX_STRING_OPTION_LENGTH
            )));
        }
    }
    if let (Some(min_length), Some(max_length)) = (option.min_length, option.max_length) {
        if min_length > max_length {
            return Err(DiscordError::model(format!(
                "option \"{name}\" min_length {min_length} is greater than \
                 max_length {max_length}"
            )));
        }
    }
    Ok(())
}

/// Recursively validates a chat-input command option against Discord limits.
fn validate_option(option: &ApplicationCommandOption) -> Result<(), DiscordError> {
    let name = &option.name;
    validation::ensure_command_name(&format!("option \"{name}\" name"), name)?;
    validation::ensure_len(
        &format!("option \"{name}\" description"),
        &option.description,
        1,
        limits::MAX_DESCRIPTION_LENGTH,
    )?;
    validate_localizations(
        &format!("option \"{name}\""),
        NameRule::ChatInput,
        option.name_localizations.as_ref(),
        option.description_localizations.as_ref(),
    )?;
    validate_option_constraints(option)?;
    // An empty list asks for nothing, so only a non-empty one is rejected.
    if option
        .channel_types
        .as_ref()
        .is_some_and(|channel_types| !channel_types.is_empty())
        && option.kind != option_type::CHANNEL
    {
        return Err(DiscordError::model(format!(
            "option \"{name}\" sets channel_types, which only CHANNEL options accept"
        )));
    }
    if let Some(file_types) = &option.file_types {
        if option.kind != option_type::ATTACHMENT {
            return Err(DiscordError::model(format!(
                "option \"{name}\" sets file_types, which only ATTACHMENT options accept"
            )));
        }
        validation::ensure_file_types(&format!("option \"{name}\" file_types"), file_types)?;
    }
    match option.kind {
        option_type::SUB_COMMAND_GROUP => {
            if let Some(nested) = option
                .options
                .iter()
                .find(|nested| nested.kind != option_type::SUB_COMMAND)
            {
                return Err(DiscordError::model(format!(
                    "subcommand group \"{name}\" may only contain subcommands, \
                     got option \"{}\" of type {}",
                    nested.name, nested.kind
                )));
            }
            validate_option_level(&format!("option \"{name}\""), &option.options)
        }
        option_type::SUB_COMMAND => {
            if let Some(nested) = option
                .options
                .iter()
                .find(|nested| is_subcommand_like(nested))
            {
                return Err(DiscordError::model(format!(
                    "subcommand \"{name}\" cannot contain nested subcommand \
                     or subcommand group \"{}\"",
                    nested.name
                )));
            }
            validate_option_level(&format!("option \"{name}\""), &option.options)
        }
        _ => {
            if !option.options.is_empty() {
                return Err(DiscordError::model(format!(
                    "option \"{name}\" of type {} cannot have nested options",
                    option.kind
                )));
            }
            Ok(())
        }
    }
}

/// Validates a whole chat-input command definition.
fn validate_chat_input_command(command: &CommandDefinition) -> Result<(), DiscordError> {
    validation::ensure_command_name("command name", &command.name)?;
    validation::ensure_len(
        "command description",
        &command.description,
        1,
        limits::MAX_DESCRIPTION_LENGTH,
    )?;
    validate_localizations(
        "command",
        NameRule::ChatInput,
        command.name_localizations.as_ref(),
        command.description_localizations.as_ref(),
    )?;
    validate_option_level("command", &command.options)?;

    let total = localized_char_count(&command.name, command.name_localizations.as_ref())
        + localized_char_count(
            &command.description,
            command.description_localizations.as_ref(),
        )
        + command.options.iter().map(option_char_count).sum::<usize>();
    if total > limits::MAX_COMMAND_TOTAL_LENGTH {
        return Err(DiscordError::model(format!(
            "command \"{}\" names, descriptions, and choice values total {total} \
             characters (longest localization per field); Discord allows at most {}",
            command.name,
            limits::MAX_COMMAND_TOTAL_LENGTH
        )));
    }
    Ok(())
}

/// Public module for `command_type` APIs.
pub mod command_type {
    /// Public constant for `CHAT_INPUT`.
    pub const CHAT_INPUT: u8 = 1;
    /// Public constant for `USER`.
    pub const USER: u8 = 2;
    /// Public constant for `MESSAGE`.
    pub const MESSAGE: u8 = 3;
    /// Public constant for `PRIMARY_ENTRY_POINT`.
    pub const PRIMARY_ENTRY_POINT: u8 = 4;
}

/// Public module for `option_type` APIs.
pub mod option_type {
    /// Public constant for `SUB_COMMAND`.
    pub const SUB_COMMAND: u8 = 1;
    /// Public constant for `SUB_COMMAND_GROUP`.
    pub const SUB_COMMAND_GROUP: u8 = 2;
    /// Public constant for `STRING`.
    pub const STRING: u8 = 3;
    /// Public constant for `INTEGER`.
    pub const INTEGER: u8 = 4;
    /// Public constant for `BOOLEAN`.
    pub const BOOLEAN: u8 = 5;
    /// Public constant for `USER`.
    pub const USER: u8 = 6;
    /// Public constant for `CHANNEL`.
    pub const CHANNEL: u8 = 7;
    /// Public constant for `ROLE`.
    pub const ROLE: u8 = 8;
    /// Public constant for `MENTIONABLE`.
    pub const MENTIONABLE: u8 = 9;
    /// Public constant for `NUMBER`.
    pub const NUMBER: u8 = 10;
    /// Public constant for `ATTACHMENT`.
    pub const ATTACHMENT: u8 = 11;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandDefinition`.
pub struct CommandDefinition {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types: Option<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<ApplicationCommandHandlerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
}

impl From<CommandDefinition> for ApplicationCommand {
    fn from(value: CommandDefinition) -> Self {
        ApplicationCommand {
            id: None,
            application_id: None,
            guild_id: None,
            kind: value.kind,
            name: value.name,
            name_localizations: value.name_localizations,
            description: value.description,
            description_localizations: value.description_localizations,
            options: value.options,
            default_member_permissions: value.default_member_permissions,
            dm_permission: value.dm_permission,
            integration_types: value.integration_types,
            contexts: value.contexts,
            handler: value.handler,
            version: None,
            nsfw: value.nsfw,
        }
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CommandOptionBuilder`.
pub struct CommandOptionBuilder {
    inner: ApplicationCommandOption,
}

impl CommandOptionBuilder {
    /// Creates a `new` value.
    pub fn new(kind: u8, name: &str, description: &str) -> Self {
        Self {
            inner: ApplicationCommandOption {
                kind,
                name: name.to_string(),
                description: description.to_string(),
                ..ApplicationCommandOption::default()
            },
        }
    }

    pub fn required(mut self, required: bool) -> Self {
        self.inner.required = Some(required);
        self
    }

    pub fn autocomplete(mut self, enabled: bool) -> Self {
        self.inner.autocomplete = Some(enabled);
        self
    }

    pub fn choice(mut self, name: &str, value: impl Serialize) -> Self {
        self.inner
            .choices
            .push(ApplicationCommandOptionChoice::new(name, value));
        self
    }

    pub fn try_choice(
        mut self,
        name: &str,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        self.inner
            .choices
            .push(ApplicationCommandOptionChoice::try_new(name, value)?);
        Ok(self)
    }

    pub fn min_value(mut self, value: f64) -> Self {
        self.inner.min_value = Some(value);
        self
    }

    pub fn max_value(mut self, value: f64) -> Self {
        self.inner.max_value = Some(value);
        self
    }

    pub fn min_length(mut self, value: u16) -> Self {
        self.inner.min_length = Some(value);
        self
    }

    pub fn max_length(mut self, value: u16) -> Self {
        self.inner.max_length = Some(value);
        self
    }

    /// Adds a localized name for `locale`, under the same rules as the name.
    pub fn name_localization(mut self, locale: &str, name: &str) -> Self {
        self.inner
            .name_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), name.to_string());
        self
    }

    /// Adds a localized description for `locale` (1-100 characters).
    pub fn description_localization(mut self, locale: &str, description: &str) -> Self {
        self.inner
            .description_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), description.to_string());
        self
    }

    /// Adds a choice whose name is localized per locale key, for example
    /// `[("de", "Rot"), ("fr", "Rouge")]`.
    pub fn localized_choice<I, L, N>(
        mut self,
        name: &str,
        value: impl Serialize,
        name_localizations: I,
    ) -> Self
    where
        I: IntoIterator<Item = (L, N)>,
        L: Into<String>,
        N: Into<String>,
    {
        let mut choice = ApplicationCommandOptionChoice::new(name, value);
        choice.name_localizations = Some(
            name_localizations
                .into_iter()
                .map(|(locale, localized)| (locale.into(), localized.into()))
                .collect(),
        );
        self.inner.choices.push(choice);
        self
    }

    /// Restricts a `CHANNEL` option to these channel types (raw values, for
    /// example `0` for guild text and `2` for guild voice).
    pub fn channel_types<I>(mut self, channel_types: I) -> Self
    where
        I: IntoIterator<Item = u8>,
    {
        self.inner.channel_types = Some(channel_types.into_iter().collect());
        self
    }

    /// Restricts an `ATTACHMENT` option to file types: preset groups
    /// (`"image"`, `"video"`, `"audio"`) or dot-prefixed extensions
    /// (`".pdf"`), max 10.
    pub fn file_types<I, S>(mut self, file_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inner.file_types = Some(file_types.into_iter().map(Into::into).collect());
        self
    }

    pub fn option(mut self, option: CommandOptionBuilder) -> Self {
        self.inner.options.push(option.build());
        self
    }

    /// Creates a `subcommand` value.
    pub fn subcommand(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND, name, description)
    }

    /// Creates a `subcommand_group` value.
    pub fn subcommand_group(name: &str, description: &str) -> Self {
        Self::new(option_type::SUB_COMMAND_GROUP, name, description)
    }

    /// Creates a `string` value.
    pub fn string(name: &str, description: &str) -> Self {
        Self::new(option_type::STRING, name, description)
    }

    /// Creates a `integer` value.
    pub fn integer(name: &str, description: &str) -> Self {
        Self::new(option_type::INTEGER, name, description)
    }

    /// Creates a `boolean` value.
    pub fn boolean(name: &str, description: &str) -> Self {
        Self::new(option_type::BOOLEAN, name, description)
    }

    /// Creates a `user` value.
    pub fn user(name: &str, description: &str) -> Self {
        Self::new(option_type::USER, name, description)
    }

    /// Creates a `channel` value.
    pub fn channel(name: &str, description: &str) -> Self {
        Self::new(option_type::CHANNEL, name, description)
    }

    /// Creates a `role` value.
    pub fn role(name: &str, description: &str) -> Self {
        Self::new(option_type::ROLE, name, description)
    }

    /// Creates a `mentionable` value.
    pub fn mentionable(name: &str, description: &str) -> Self {
        Self::new(option_type::MENTIONABLE, name, description)
    }

    /// Creates a `number` value.
    pub fn number(name: &str, description: &str) -> Self {
        Self::new(option_type::NUMBER, name, description)
    }

    /// Creates a `attachment` value.
    pub fn attachment(name: &str, description: &str) -> Self {
        Self::new(option_type::ATTACHMENT, name, description)
    }

    pub fn build(self) -> ApplicationCommandOption {
        self.inner
    }

    /// Validates this option (recursively) against Discord's limits without
    /// consuming the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validate_option(&self.inner)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<ApplicationCommandOption, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SlashCommandBuilder`.
pub struct SlashCommandBuilder {
    inner: CommandDefinition,
}

impl SlashCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::CHAT_INPUT,
                name: name.to_string(),
                description: description.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn option(mut self, option: CommandOptionBuilder) -> Self {
        self.inner.options.push(option.build());
        self
    }

    pub fn string_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::string(name, description).required(required))
    }

    pub fn integer_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::integer(name, description).required(required))
    }

    pub fn boolean_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::boolean(name, description).required(required))
    }

    pub fn user_option(self, name: &str, description: &str, required: bool) -> Self {
        self.option(CommandOptionBuilder::user(name, description).required(required))
    }

    pub fn subcommand(self, option: CommandOptionBuilder) -> Self {
        self.option(option)
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn nsfw(mut self, enabled: bool) -> Self {
        self.inner.nsfw = Some(enabled);
        self
    }

    pub fn integration_types<I>(mut self, integration_types: I) -> Self
    where
        I: IntoIterator<Item = ApplicationIntegrationType>,
    {
        self.inner.integration_types = Some(integration_types.into_iter().collect());
        self
    }

    pub fn contexts<I>(mut self, contexts: I) -> Self
    where
        I: IntoIterator<Item = InteractionContextType>,
    {
        self.inner.contexts = Some(contexts.into_iter().collect());
        self
    }

    pub fn name_localization(mut self, locale: &str, name: &str) -> Self {
        self.inner
            .name_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), name.to_string());
        self
    }

    pub fn description_localization(mut self, locale: &str, description: &str) -> Self {
        self.inner
            .description_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), description.to_string());
        self
    }

    pub fn handler(mut self, handler: ApplicationCommandHandlerType) -> Self {
        self.inner.handler = Some(handler);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder.
    ///
    /// Checks names (character set, lowercase), descriptions, localizations
    /// of the command, its options, and choices (locale keys and values),
    /// and for every option level: at most 25 options, no subcommands mixed
    /// with regular options, required options first, unique names (localized
    /// ones too), choices (count, types, lengths, not with autocomplete),
    /// value/length ranges, `channel_types` only on `CHANNEL` options, and
    /// the per-command character total. Limits live in [`limits`]. Validation is deliberately never
    /// stricter than Discord, so passing it does not guarantee Discord
    /// accepts the command.
    pub fn validate(&self) -> Result<(), DiscordError> {
        validate_chat_input_command(&self.inner)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `PrimaryEntryPointCommandBuilder`.
pub struct PrimaryEntryPointCommandBuilder {
    inner: CommandDefinition,
}

impl PrimaryEntryPointCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::PRIMARY_ENTRY_POINT,
                name: name.to_string(),
                description: description.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn integration_types<I>(mut self, integration_types: I) -> Self
    where
        I: IntoIterator<Item = ApplicationIntegrationType>,
    {
        self.inner.integration_types = Some(integration_types.into_iter().collect());
        self
    }

    pub fn contexts<I>(mut self, contexts: I) -> Self
    where
        I: IntoIterator<Item = InteractionContextType>,
    {
        self.inner.contexts = Some(contexts.into_iter().collect());
        self
    }

    pub fn name_localization(mut self, locale: &str, name: &str) -> Self {
        self.inner
            .name_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), name.to_string());
        self
    }

    pub fn description_localization(mut self, locale: &str, description: &str) -> Self {
        self.inner
            .description_localizations
            .get_or_insert_with(HashMap::new)
            .insert(locale.to_string(), description.to_string());
        self
    }

    pub fn handler(mut self, handler: ApplicationCommandHandlerType) -> Self {
        self.inner.handler = Some(handler);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder.
    pub fn validate(&self) -> Result<(), DiscordError> {
        NameRule::LengthOnly.check("command name", &self.inner.name)?;
        validation::ensure_len(
            "command description",
            &self.inner.description,
            1,
            limits::MAX_DESCRIPTION_LENGTH,
        )?;
        validate_localizations(
            "command",
            NameRule::LengthOnly,
            self.inner.name_localizations.as_ref(),
            self.inner.description_localizations.as_ref(),
        )
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `UserCommandBuilder`.
pub struct UserCommandBuilder {
    inner: CommandDefinition,
}

impl UserCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::USER,
                name: name.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder. Context-menu names may contain spaces and
    /// mixed case, so only the length is checked.
    pub fn validate(&self) -> Result<(), DiscordError> {
        NameRule::LengthOnly.check("command name", &self.inner.name)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `MessageCommandBuilder`.
pub struct MessageCommandBuilder {
    inner: CommandDefinition,
}

impl MessageCommandBuilder {
    /// Creates a `new` value.
    pub fn new(name: &str) -> Self {
        Self {
            inner: CommandDefinition {
                kind: command_type::MESSAGE,
                name: name.to_string(),
                ..CommandDefinition::default()
            },
        }
    }

    pub fn default_member_permissions(mut self, permissions: PermissionsBitField) -> Self {
        self.inner.default_member_permissions = Some(permissions);
        self
    }

    pub fn dm_permission(mut self, enabled: bool) -> Self {
        self.inner.dm_permission = Some(enabled);
        self
    }

    pub fn build(self) -> CommandDefinition {
        self.inner
    }

    /// Validates the command definition against Discord's limits without
    /// consuming the builder. Context-menu names may contain spaces and
    /// mixed case, so only the length is checked.
    pub fn validate(&self) -> Result<(), DiscordError> {
        NameRule::LengthOnly.check("command name", &self.inner.name)
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<CommandDefinition, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        command_type, ensure_unique_option_names, limits, option_type, CommandOptionBuilder,
        MessageCommandBuilder, PrimaryEntryPointCommandBuilder, SlashCommandBuilder,
        UserCommandBuilder,
    };
    use crate::model::{
        ApplicationCommand, ApplicationCommandHandlerType, ApplicationIntegrationType,
        InteractionContextType, PermissionsBitField,
    };

    #[test]
    fn slash_command_builder_serializes_nested_options() {
        let command = SlashCommandBuilder::new("hello", "Say hello")
            .option(
                CommandOptionBuilder::new(option_type::STRING, "target", "Target user")
                    .required(true)
                    .choice("World", "world"),
            )
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], json!(command_type::CHAT_INPUT));
        assert_eq!(value["options"][0]["name"], json!("target"));
        assert_eq!(value["options"][0]["choices"][0]["value"], json!("world"));
    }

    #[test]
    fn slash_command_builder_exposes_common_option_shortcuts() {
        let command = SlashCommandBuilder::new("moderate", "Moderation command")
            .string_option("reason", "Reason", true)
            .boolean_option("silent", "Whether the reply should be hidden", false)
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["options"][0]["type"], json!(option_type::STRING));
        assert_eq!(value["options"][1]["type"], json!(option_type::BOOLEAN));
    }

    #[test]
    fn command_option_builder_shortcuts_cover_supported_types() {
        let cases = [
            (
                CommandOptionBuilder::subcommand("sub", "desc").build(),
                option_type::SUB_COMMAND,
            ),
            (
                CommandOptionBuilder::subcommand_group("group", "desc").build(),
                option_type::SUB_COMMAND_GROUP,
            ),
            (
                CommandOptionBuilder::string("string", "desc").build(),
                option_type::STRING,
            ),
            (
                CommandOptionBuilder::integer("integer", "desc").build(),
                option_type::INTEGER,
            ),
            (
                CommandOptionBuilder::boolean("boolean", "desc").build(),
                option_type::BOOLEAN,
            ),
            (
                CommandOptionBuilder::user("user", "desc").build(),
                option_type::USER,
            ),
            (
                CommandOptionBuilder::channel("channel", "desc").build(),
                option_type::CHANNEL,
            ),
            (
                CommandOptionBuilder::role("role", "desc").build(),
                option_type::ROLE,
            ),
            (
                CommandOptionBuilder::mentionable("mentionable", "desc").build(),
                option_type::MENTIONABLE,
            ),
            (
                CommandOptionBuilder::number("number", "desc").build(),
                option_type::NUMBER,
            ),
            (
                CommandOptionBuilder::attachment("attachment", "desc").build(),
                option_type::ATTACHMENT,
            ),
        ];

        for (option, expected_kind) in cases {
            assert_eq!(option.kind, expected_kind);
        }
    }

    #[test]
    fn command_option_builder_serializes_nested_constraints_and_choices() {
        let option = CommandOptionBuilder::subcommand_group("admin", "Admin tools")
            .option(
                CommandOptionBuilder::subcommand("ban", "Ban a member")
                    .option(CommandOptionBuilder::user("target", "Member").required(true))
                    .option(
                        CommandOptionBuilder::integer("days", "Delete days")
                            .autocomplete(true)
                            .min_value(1.0)
                            .max_value(7.0),
                    )
                    .option(
                        CommandOptionBuilder::string("reason", "Reason")
                            .min_length(3)
                            .max_length(120),
                    )
                    .option(CommandOptionBuilder::number("ratio", "Ratio").choice("Half", 0.5_f64))
                    .option(CommandOptionBuilder::attachment("proof", "Proof")),
            )
            .build();

        let value = serde_json::to_value(option).unwrap();
        let nested = &value["options"][0]["options"];

        assert_eq!(value["type"], json!(option_type::SUB_COMMAND_GROUP));
        assert_eq!(value["options"][0]["type"], json!(option_type::SUB_COMMAND));
        assert_eq!(nested[0]["required"], json!(true));
        assert_eq!(nested[1]["autocomplete"], json!(true));
        assert_eq!(nested[1]["min_value"], json!(1.0));
        assert_eq!(nested[1]["max_value"], json!(7.0));
        assert_eq!(nested[2]["min_length"], json!(3));
        assert_eq!(nested[2]["max_length"], json!(120));
        assert_eq!(nested[3]["choices"][0]["name"], json!("Half"));
        assert_eq!(nested[3]["choices"][0]["value"], json!(0.5));
        assert_eq!(nested[4]["type"], json!(option_type::ATTACHMENT));
    }

    #[test]
    fn command_definition_converts_into_application_command() {
        let permissions = PermissionsBitField(8);
        let command = SlashCommandBuilder::new("ban", "Ban a member")
            .integer_option("days", "Delete days", false)
            .default_member_permissions(permissions)
            .dm_permission(false)
            .nsfw(true)
            .integration_types([
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL,
            ])
            .contexts([
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM,
            ])
            .name_localization("ko", "李⑤떒")
            .description_localization("ko", "硫ㅻ쾭 李⑤떒")
            .handler(ApplicationCommandHandlerType::APP_HANDLER)
            .build();

        let application_command: ApplicationCommand = command.clone().into();

        assert_eq!(application_command.kind, command_type::CHAT_INPUT);
        assert_eq!(application_command.name, "ban");
        assert_eq!(application_command.description, "Ban a member");
        assert_eq!(application_command.options.len(), 1);
        assert_eq!(
            application_command
                .default_member_permissions
                .map(PermissionsBitField::bits),
            Some(8)
        );
        assert_eq!(application_command.dm_permission, Some(false));
        assert_eq!(application_command.nsfw, Some(true));
        assert_eq!(
            application_command.integration_types,
            Some(vec![
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL
            ])
        );
        assert_eq!(
            application_command.contexts,
            Some(vec![
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM
            ])
        );
        assert_eq!(
            application_command
                .name_localizations
                .as_ref()
                .and_then(|localizations| localizations.get("ko"))
                .map(String::as_str),
            Some("李⑤떒")
        );
        assert_eq!(
            application_command.handler,
            Some(ApplicationCommandHandlerType::APP_HANDLER)
        );
    }

    #[test]
    fn primary_entry_point_builder_serializes_activity_fields() {
        let command = PrimaryEntryPointCommandBuilder::new("launch", "Launch activity")
            .integration_types([
                ApplicationIntegrationType::GUILD_INSTALL,
                ApplicationIntegrationType::USER_INSTALL,
            ])
            .contexts([
                InteractionContextType::GUILD,
                InteractionContextType::BOT_DM,
            ])
            .handler(ApplicationCommandHandlerType::DISCORD_LAUNCH_ACTIVITY)
            .build();

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], json!(command_type::PRIMARY_ENTRY_POINT));
        assert_eq!(value["integration_types"], json!([0, 1]));
        assert_eq!(value["contexts"], json!([0, 1]));
        assert_eq!(value["handler"], json!(2));
    }

    fn expect_model_error<T: std::fmt::Debug>(
        result: Result<T, crate::error::DiscordError>,
        needle: &str,
    ) {
        let err = result.expect_err("expected validation failure").to_string();
        assert!(
            err.contains(needle),
            "error message {err:?} should contain {needle:?}"
        );
    }

    #[test]
    fn slash_command_try_build_matches_build_for_valid_command() {
        let make = || {
            SlashCommandBuilder::new("hello", "Say hello").option(
                CommandOptionBuilder::string("target", "Target user")
                    .required(true)
                    .choice("World", "world"),
            )
        };

        let built = serde_json::to_value(make().build()).unwrap();
        let validated = serde_json::to_value(make().try_build().expect("valid command")).unwrap();
        assert_eq!(built, validated);
    }

    #[test]
    fn slash_command_validate_accepts_multibyte_names_by_codepoint_count() {
        // 32 codepoints but 64 UTF-8 bytes: must pass because Discord counts codepoints.
        let name = "é".repeat(32);
        SlashCommandBuilder::new(&name, "desc")
            .validate()
            .expect("codepoint-length name should be valid");
    }

    #[test]
    fn slash_command_try_build_rejects_invalid_names() {
        expect_model_error(
            SlashCommandBuilder::new(&"a".repeat(45), "desc").try_build(),
            "command name must be 1-32 characters, got 45",
        );
        expect_model_error(
            SlashCommandBuilder::new("Hello", "desc").try_build(),
            "command name must be lowercase",
        );
        expect_model_error(
            SlashCommandBuilder::new("hi there", "desc").try_build(),
            "command name must not contain spaces",
        );
    }

    #[test]
    fn slash_command_try_build_rejects_bad_description_and_option_overflow() {
        expect_model_error(
            SlashCommandBuilder::new("hello", "").try_build(),
            "command description must be 1-100 characters, got 0",
        );
        expect_model_error(
            SlashCommandBuilder::new("hello", &"d".repeat(101)).try_build(),
            "command description must be 1-100 characters, got 101",
        );

        let mut command = SlashCommandBuilder::new("hello", "desc");
        for index in 0..26 {
            command = command.string_option(&format!("opt{index}"), "desc", false);
        }
        expect_model_error(
            command.try_build(),
            "command options must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn command_option_try_build_rejects_invalid_fields() {
        expect_model_error(
            CommandOptionBuilder::string("name", &"d".repeat(101)).try_build(),
            "option \"name\" description must be 1-100 characters, got 101",
        );
        expect_model_error(
            CommandOptionBuilder::string("Name", "desc").try_build(),
            "option \"Name\" name must be lowercase",
        );
        expect_model_error(
            CommandOptionBuilder::string("choices", "desc")
                .choice(&"c".repeat(101), 1)
                .try_build(),
            "option \"choices\" choice name must be 1-100 characters, got 101",
        );

        let mut option = CommandOptionBuilder::string("choices", "desc");
        for index in 0..26 {
            option = option.choice(&format!("choice{index}"), index);
        }
        expect_model_error(
            option.try_build(),
            "option \"choices\" choices must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn command_option_try_build_enforces_subcommand_nesting_rules() {
        expect_model_error(
            CommandOptionBuilder::subcommand("outer", "desc")
                .option(CommandOptionBuilder::subcommand("inner", "desc"))
                .try_build(),
            "subcommand \"outer\" cannot contain nested subcommand",
        );
        expect_model_error(
            CommandOptionBuilder::subcommand_group("group", "desc")
                .option(CommandOptionBuilder::string("plain", "desc"))
                .try_build(),
            "subcommand group \"group\" may only contain subcommands",
        );
        expect_model_error(
            CommandOptionBuilder::string("plain", "desc")
                .option(CommandOptionBuilder::string("nested", "desc"))
                .try_build(),
            "option \"plain\" of type 3 cannot have nested options",
        );

        // Group -> subcommand -> basic option is the valid shape.
        CommandOptionBuilder::subcommand_group("group", "desc")
            .option(
                CommandOptionBuilder::subcommand("sub", "desc")
                    .option(CommandOptionBuilder::string("plain", "desc")),
            )
            .try_build()
            .expect("valid nesting should build");
    }

    #[test]
    fn context_menu_and_entry_point_builders_validate_names() {
        let valid = UserCommandBuilder::new("Inspect Member")
            .try_build()
            .unwrap();
        assert_eq!(valid.name, "Inspect Member");

        expect_model_error(
            UserCommandBuilder::new(&"n".repeat(33)).try_build(),
            "command name must be 1-32 characters, got 33",
        );
        expect_model_error(
            MessageCommandBuilder::new("").try_build(),
            "command name must be 1-32 characters, got 0",
        );
        expect_model_error(
            PrimaryEntryPointCommandBuilder::new("launch", "").try_build(),
            "command description must be 1-100 characters, got 0",
        );
        PrimaryEntryPointCommandBuilder::new("launch", "Launch activity")
            .try_build()
            .expect("valid entry point command");
    }

    #[test]
    fn user_and_message_command_builders_apply_command_kinds_and_permissions() {
        let user_command = UserCommandBuilder::new("Inspect")
            .default_member_permissions(PermissionsBitField(16))
            .dm_permission(true)
            .build();
        let message_command = MessageCommandBuilder::new("Quote")
            .default_member_permissions(PermissionsBitField(32))
            .dm_permission(false)
            .build();

        let user_value = serde_json::to_value(user_command).unwrap();
        let message_value = serde_json::to_value(message_command).unwrap();

        assert_eq!(user_value["type"], json!(command_type::USER));
        assert_eq!(user_value["default_member_permissions"], json!("16"));
        assert_eq!(user_value["dm_permission"], json!(true));
        assert_eq!(message_value["type"], json!(command_type::MESSAGE));
        assert_eq!(message_value["default_member_permissions"], json!("32"));
        assert_eq!(message_value["dm_permission"], json!(false));
    }

    #[test]
    fn attachment_options_accept_file_types_and_validate_them() {
        let option =
            CommandOptionBuilder::attachment("file", "Upload a file").file_types(["image", ".pdf"]);
        let value = serde_json::to_value(option.clone().try_build().expect("valid")).expect("json");
        assert_eq!(value["file_types"], json!(["image", ".pdf"]));

        assert!(CommandOptionBuilder::attachment("file", "Upload a file")
            .file_types((0..11).map(|index| format!(".e{index}")))
            .validate()
            .is_err());
        assert!(
            CommandOptionBuilder::attachment("file", "Upload a file")
                .file_types(["pdf"])
                .validate()
                .is_ok(),
            "a bare token is treated as a group name, which Discord may add"
        );
        for bad in [".", "", "../x", "image/png"] {
            assert!(
                CommandOptionBuilder::attachment("file", "Upload a file")
                    .file_types([bad])
                    .validate()
                    .is_err(),
                "{bad:?}"
            );
        }
        assert!(
            CommandOptionBuilder::string("text", "Some text")
                .file_types(["image"])
                .validate()
                .is_err(),
            "file_types is only valid on ATTACHMENT options"
        );
        let untouched =
            serde_json::to_value(CommandOptionBuilder::attachment("file", "Upload a file").build())
                .expect("json");
        assert!(untouched.get("file_types").is_none());
    }

    // ---- phase 7: deeper chat-input validation -------------------------------

    fn sub(name: &str) -> CommandOptionBuilder {
        CommandOptionBuilder::subcommand(name, "desc")
    }

    #[test]
    fn chat_input_names_accept_everything_discord_regex_allows() {
        for name in [
            "a-b_c",
            "차단",
            "封禁",
            "बैन",
            "नमस्ते",
            "สวัสดี",
            "donʼt",
            "１２３",
            "straße",
            "ⅻ",
            "ǆ",
            "привет",
        ] {
            SlashCommandBuilder::new(name, "desc")
                .validate()
                .unwrap_or_else(|err| panic!("{name:?} should be valid: {err}"));
            CommandOptionBuilder::string(name, "desc")
                .validate()
                .unwrap_or_else(|err| panic!("option {name:?} should be valid: {err}"));
        }
    }

    #[test]
    fn chat_input_names_reject_characters_and_case_discord_rejects() {
        for (name, needle) in [
            ("a.b", "contains '.'"),
            ("a/b", "contains '/'"),
            ("fire🔥", "contains '🔥'"),
            ("tab\tname", "contains '\\t'"),
            ("ellipsis…", "contains '…'"),
            ("price€", "contains '€'"),
            ("a×b", "contains '×'"),
            ("zero\u{200D}width", "contains '\\u{200d}'"),
            ("ideographic\u{3000}space", "contains '\\u{3000}'"),
            ("private\u{E000}", "contains '\\u{e000}'"),
            ("heart\u{2764}\u{FE0F}", "contains '❤'"),
            ("emoji\u{1FAE0}", "contains '\u{1FAE0}'"),
            ("plane15\u{F0001}", "contains '\\u{f0001}'"),
            ("Éclair", "must be lowercase"),
            ("ПРИВЕТ", "must be lowercase"),
            ("ǅemal", "must be lowercase"),
            ("ΣΊΓΜΑ", "must be lowercase"),
        ] {
            expect_model_error(SlashCommandBuilder::new(name, "desc").try_build(), needle);
        }
        expect_model_error(
            SlashCommandBuilder::new("don't", "desc").try_build(),
            "use \u{02BC} (U+02BC MODIFIER LETTER APOSTROPHE) instead of '",
        );
        expect_model_error(
            CommandOptionBuilder::string("why?", "desc").try_build(),
            "option \"why?\" name contains '?'",
        );
    }

    #[test]
    fn names_do_not_depend_on_the_compilers_unicode_version() {
        // U+088F and U+1E6C0 are uncased letters (Lo) added in Unicode 17:
        // unknown to Rust 1.88's tables, letters on Rust 1.99. Both must pass
        // on every supported compiler, because unknown code points are not
        // rejected.
        for name in ["a\u{088F}", "\u{1E6C0}b", "\u{088F}\u{1E6C0}"] {
            SlashCommandBuilder::new(name, "desc")
                .validate()
                .unwrap_or_else(|err| panic!("{name:?} should be valid: {err}"));
        }
        // Known residual: characters outside the rejected classes pass locally
        // even when Discord rejects them, e.g. a combining accent.
        SlashCommandBuilder::new("e\u{0301}", "desc")
            .validate()
            .expect("unclassified characters are left to Discord");
    }

    #[test]
    fn name_and_description_boundaries_count_codepoints() {
        SlashCommandBuilder::new(&"가".repeat(32), &"é".repeat(100))
            .validate()
            .expect("32-codepoint name and 100-codepoint description");
        expect_model_error(
            SlashCommandBuilder::new(&"가".repeat(33), "desc").try_build(),
            "command name must be 1-32 characters, got 33",
        );
        expect_model_error(
            SlashCommandBuilder::new("", "desc").try_build(),
            "command name must be 1-32 characters, got 0",
        );
        expect_model_error(
            CommandOptionBuilder::string("x", &"é".repeat(101)).try_build(),
            "option \"x\" description must be 1-100 characters, got 101",
        );
    }

    #[test]
    fn option_levels_allow_exactly_25_options() {
        let mut command = SlashCommandBuilder::new("many", "desc");
        let mut subcommand = sub("inner");
        for index in 0..25 {
            command = command.string_option(&format!("opt{index}"), "desc", false);
            subcommand = subcommand.option(CommandOptionBuilder::string(&format!("o{index}"), "d"));
        }
        command.validate().expect("25 top-level options");
        subcommand
            .clone()
            .validate()
            .expect("25 subcommand options");

        expect_model_error(
            subcommand
                .option(CommandOptionBuilder::string("o25", "d"))
                .try_build(),
            "option \"inner\" options must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn subcommands_cannot_share_a_level_with_regular_options() {
        expect_model_error(
            SlashCommandBuilder::new("mixed", "desc")
                .subcommand(sub("list"))
                .string_option("query", "desc", false)
                .try_build(),
            "command mixes subcommands or subcommand groups with the regular option \"query\"",
        );
        expect_model_error(
            SlashCommandBuilder::new("mixed", "desc")
                .string_option("query", "desc", false)
                .option(CommandOptionBuilder::subcommand_group("admin", "desc").option(sub("ban")))
                .try_build(),
            "regular option \"query\"",
        );
        SlashCommandBuilder::new("grouped", "desc")
            .subcommand(sub("list"))
            .option(CommandOptionBuilder::subcommand_group("admin", "desc").option(sub("ban")))
            .validate()
            .expect("subcommands and groups may share a level");
    }

    #[test]
    fn required_options_must_come_before_optional_ones_per_level() {
        expect_model_error(
            SlashCommandBuilder::new("order", "desc")
                .string_option("note", "desc", false)
                .user_option("target", "desc", true)
                .try_build(),
            "command lists required option \"target\" after optional option \"note\"",
        );
        expect_model_error(
            SlashCommandBuilder::new("order", "desc")
                .option(CommandOptionBuilder::string("unset", "desc"))
                .user_option("target", "desc", true)
                .try_build(),
            "after optional option \"unset\"",
        );
        expect_model_error(
            SlashCommandBuilder::new("order", "desc")
                .subcommand(
                    sub("ban")
                        .option(CommandOptionBuilder::integer("days", "desc"))
                        .option(CommandOptionBuilder::user("target", "desc").required(true)),
                )
                .try_build(),
            "option \"ban\" lists required option \"target\" after optional option \"days\"",
        );
        SlashCommandBuilder::new("order", "desc")
            .user_option("target", "desc", true)
            .string_option("reason", "desc", true)
            .boolean_option("silent", "desc", false)
            .integer_option("days", "desc", false)
            .validate()
            .expect("required first, then optional");
    }

    #[test]
    fn option_names_must_be_unique_within_a_level() {
        expect_model_error(
            SlashCommandBuilder::new("dupe", "desc")
                .string_option("name", "desc", false)
                .string_option("name", "other", false)
                .try_build(),
            "command has more than one option named \"name\"",
        );
        expect_model_error(
            CommandOptionBuilder::subcommand_group("admin", "desc")
                .option(sub("ban"))
                .option(sub("ban"))
                .try_build(),
            "option \"admin\" has more than one option named \"ban\"",
        );
        SlashCommandBuilder::new("mod", "desc")
            .subcommand(sub("ban").option(CommandOptionBuilder::user("target", "d")))
            .subcommand(sub("kick").option(CommandOptionBuilder::user("target", "d")))
            .validate()
            .expect("different subcommands may reuse option names");
    }

    fn localizations(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(locale, value)| (locale.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn localized_option_names_must_not_collide_within_a_level() {
        let target_de = localizations(&[("de", "ziel"), ("fr", "cible")]);
        let reason_de = localizations(&[("de", "grund"), ("fr", "cible")]);
        let clash_default = localizations(&[("de", "reason")]);
        let clash_locale = localizations(&[("de", "ziel")]);
        let identical = localizations(&[("de", "target")]);

        ensure_unique_option_names("command", &[("target", Some(&target_de)), ("reason", None)])
            .expect("distinct localized names");
        ensure_unique_option_names("command", &[("target", Some(&identical)), ("reason", None)])
            .expect("a localization identical to its default is ignored");

        expect_model_error(
            ensure_unique_option_names(
                "command",
                &[("target", Some(&clash_default)), ("reason", None)],
            ),
            "command option \"target\" name_localizations[de] \"reason\" collides",
        );
        expect_model_error(
            ensure_unique_option_names(
                "command",
                &[
                    ("target", Some(&target_de)),
                    ("reason", Some(&clash_locale)),
                ],
            ),
            "option \"reason\" name_localizations[de] \"ziel\" collides",
        );
        expect_model_error(
            ensure_unique_option_names(
                "command",
                &[("target", Some(&target_de)), ("reason", Some(&reason_de))],
            ),
            "name_localizations[fr] \"cible\" collides",
        );
    }

    #[test]
    fn choices_follow_type_autocomplete_and_length_rules() {
        let mut option = CommandOptionBuilder::string("pick", "desc");
        for index in 0..25 {
            option = option.choice(&format!("c{index}"), format!("v{index}"));
        }
        option.validate().expect("exactly 25 choices");

        CommandOptionBuilder::string("pick", "desc")
            .choice(&"n".repeat(100), "é".repeat(100))
            .validate()
            .expect("100-character choice name and string value");
        expect_model_error(
            CommandOptionBuilder::string("pick", "desc")
                .choice("long", "é".repeat(101))
                .try_build(),
            "option \"pick\" choice \"long\" value must be at most 100 characters, got 101",
        );
        expect_model_error(
            CommandOptionBuilder::string("pick", "desc")
                .choice("", "v")
                .try_build(),
            "option \"pick\" choice name must be 1-100 characters, got 0",
        );

        CommandOptionBuilder::integer("count", "desc")
            .choice("one", 1)
            .validate()
            .expect("integer choices");
        CommandOptionBuilder::number("ratio", "desc")
            .choice("half", 0.5)
            .validate()
            .expect("number choices");
        expect_model_error(
            CommandOptionBuilder::boolean("flag", "desc")
                .choice("yes", true)
                .try_build(),
            "option \"flag\" of type 5 has choices, which only STRING, INTEGER, and NUMBER",
        );
        expect_model_error(
            CommandOptionBuilder::string("topic", "desc")
                .autocomplete(true)
                .choice("a", "a")
                .try_build(),
            "option \"topic\" sets autocomplete and choices",
        );
        CommandOptionBuilder::string("topic", "desc")
            .autocomplete(false)
            .choice("a", "a")
            .validate()
            .expect("autocomplete(false) with choices is fine");
        CommandOptionBuilder::string("topic", "desc")
            .autocomplete(true)
            .validate()
            .expect("autocomplete without choices");
    }

    #[test]
    fn value_and_length_ranges_are_checked() {
        CommandOptionBuilder::integer("n", "desc")
            .min_value(5.0)
            .max_value(5.0)
            .validate()
            .expect("min_value == max_value");
        expect_model_error(
            CommandOptionBuilder::number("n", "desc")
                .min_value(2.5)
                .max_value(1.0)
                .try_build(),
            "option \"n\" min_value 2.5 is greater than max_value 1",
        );
        CommandOptionBuilder::string("s", "desc")
            .min_length(0)
            .max_length(1)
            .validate()
            .expect("lowest bounds");
        CommandOptionBuilder::string("s", "desc")
            .min_length(6000)
            .max_length(6000)
            .validate()
            .expect("highest bounds");
        expect_model_error(
            CommandOptionBuilder::string("s", "desc")
                .min_length(6001)
                .try_build(),
            "option \"s\" min_length must be 0-6000, got 6001",
        );
        expect_model_error(
            CommandOptionBuilder::string("s", "desc")
                .max_length(0)
                .try_build(),
            "option \"s\" max_length must be 1-6000, got 0",
        );
        expect_model_error(
            CommandOptionBuilder::string("s", "desc")
                .max_length(6001)
                .try_build(),
            "option \"s\" max_length must be 1-6000, got 6001",
        );
        expect_model_error(
            CommandOptionBuilder::string("s", "desc")
                .min_length(10)
                .max_length(9)
                .try_build(),
            "option \"s\" min_length 10 is greater than max_length 9",
        );
    }

    #[test]
    fn command_localizations_use_known_locales_and_field_rules() {
        SlashCommandBuilder::new("ban", "Ban a member")
            .name_localization("ko", "차단")
            .name_localization("en-us", "ban")
            .description_localization("es-419", &"é".repeat(100))
            .validate()
            .expect("known locales (case-insensitive) and valid values");

        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .name_localization("en", "ban")
                .try_build(),
            "command name_localizations uses unsupported locale \"en\"",
        );
        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .description_localization("kr", "차단")
                .try_build(),
            "command description_localizations uses unsupported locale \"kr\"",
        );
        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .name_localization("de", "Bannen")
                .try_build(),
            "command name_localizations[de] must be lowercase",
        );
        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .name_localization("fr", "bannir membre")
                .try_build(),
            "command name_localizations[fr] must not contain spaces",
        );
        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .description_localization("ja", &"説".repeat(101))
                .try_build(),
            "command description_localizations[ja] must be 1-100 characters, got 101",
        );
        expect_model_error(
            SlashCommandBuilder::new("ban", "desc")
                .description_localization("ja", "")
                .try_build(),
            "command description_localizations[ja] must be 1-100 characters, got 0",
        );

        PrimaryEntryPointCommandBuilder::new("launch", "Launch")
            .name_localization("de", "Spiel Starten")
            .validate()
            .expect("entry-point names only have a length rule");
        expect_model_error(
            PrimaryEntryPointCommandBuilder::new("launch", "Launch")
                .name_localization("xx", "start")
                .try_build(),
            "command name_localizations uses unsupported locale \"xx\"",
        );
        expect_model_error(
            PrimaryEntryPointCommandBuilder::new("launch", "Launch")
                .description_localization("de", "")
                .try_build(),
            "command description_localizations[de] must be 1-100 characters, got 0",
        );
    }

    /// Builds a command whose first two options count 7937 characters
    /// (mixing ASCII and multibyte text), followed by `last`.
    fn command_with(last: CommandOptionBuilder) -> SlashCommandBuilder {
        // name 4 + description 100 = 104
        let mut command = SlashCommandBuilder::new("poll", &"d".repeat(100));
        // Each full option: name 32 + description 100 + 25 choices x (100 + 100)
        // = 5132; the first one brings the total to 5236.
        let mut full = CommandOptionBuilder::string(&"a".repeat(32), &"é".repeat(100));
        for index in 0..25 {
            full = full.choice(&format!("{index:0>100}"), "가".repeat(100));
        }
        command = command.option(full);
        // Second option: name 1 + description 100 + 13 x 200 = 2701 -> 7937.
        let mut partial = CommandOptionBuilder::string("b", &"설".repeat(100));
        for index in 0..13 {
            partial = partial.choice(&format!("{index:0>100}"), "x".repeat(100));
        }
        command.option(partial).option(last)
    }

    /// Builds a command whose counted total is exactly `8000 + extra`.
    fn command_with_total(extra: usize) -> SlashCommandBuilder {
        // Integer choice values are not counted, so only the name (1),
        // description (3 + extra), and choice name (59) count -> 8000.
        command_with(
            CommandOptionBuilder::integer("c", &"z".repeat(3 + extra)).choice(&"q".repeat(59), 1),
        )
    }

    #[test]
    fn command_total_length_allows_exactly_8000_characters() {
        command_with_total(0)
            .validate()
            .expect("exactly 8000 counted characters");
        expect_model_error(
            command_with_total(1).try_build(),
            "command \"poll\" names, descriptions, and choice values total 8001 characters",
        );
        assert_eq!(limits::MAX_COMMAND_TOTAL_LENGTH, 8000);
    }

    #[test]
    fn command_total_length_counts_the_longest_localization_once() {
        // Localizations no longer than the default add nothing...
        command_with_total(0)
            .name_localization("ko", "투표")
            .description_localization("de", &"ü".repeat(100))
            .description_localization("fr", &"e".repeat(99))
            .validate()
            .expect("max across locales, not the sum");
        // ...a longer one replaces the default's count.
        expect_model_error(
            command_with_total(0)
                .name_localization("de", "abstimmung")
                .try_build(),
            "total 8006 characters",
        );
    }

    #[test]
    fn locale_table_matches_discord_reference() {
        assert_eq!(limits::LOCALES.len(), 32);
        let unique: std::collections::HashSet<&str> = limits::LOCALES.iter().copied().collect();
        assert_eq!(unique.len(), limits::LOCALES.len());
        for locale in [
            "en-US", "en-GB", "es-419", "pt-BR", "sv-SE", "zh-CN", "zh-TW", "ko",
        ] {
            assert!(limits::LOCALES.contains(&locale), "{locale}");
        }
    }

    #[test]
    fn try_choice_reports_serialization_errors_and_message_commands_build() {
        let option = CommandOptionBuilder::string("pick", "desc")
            .try_choice("a", "a")
            .expect("string value serializes");
        option.validate().expect("valid choice");

        let unserializable = std::collections::BTreeMap::from([((1, 2), 3)]);
        assert!(CommandOptionBuilder::integer("pick", "desc")
            .try_choice("map", unserializable)
            .is_err());

        let quote = MessageCommandBuilder::new("Quote Message")
            .try_build()
            .expect("context-menu names may contain spaces and capitals");
        assert_eq!(quote.kind, command_type::MESSAGE);
    }

    #[test]
    fn command_total_length_counts_option_and_choice_localizations() {
        let last = || CommandOptionBuilder::integer("c", "zzz");
        command_with(
            last()
                .description_localization("de", "yyy")
                .localized_choice(&"q".repeat(59), 1, [("de", "r".repeat(59))]),
        )
        .validate()
        .expect("localizations as long as their defaults add nothing");
        expect_model_error(
            command_with(
                last()
                    .name_localization("de", "cc")
                    .choice(&"q".repeat(59), 1),
            )
            .try_build(),
            "total 8001 characters",
        );
        expect_model_error(
            command_with(last().localized_choice(&"q".repeat(59), 1, [("de", "r".repeat(60))]))
                .try_build(),
            "total 8001 characters",
        );
    }

    #[test]
    fn option_and_choice_localizations_follow_field_rules() {
        let option = CommandOptionBuilder::string("color", "Pick a color")
            .name_localization("de", "farbe")
            .description_localization("de", &"ä".repeat(100))
            .localized_choice("Red", "red", [("de", "Rot"), ("fr", "Rouge")]);
        option.clone().validate().expect("valid localizations");
        let value = serde_json::to_value(option.build()).expect("json");
        assert_eq!(value["name_localizations"]["de"], json!("farbe"));
        assert_eq!(
            value["choices"][0]["name_localizations"]["fr"],
            json!("Rouge")
        );

        let plain = serde_json::to_value(
            CommandOptionBuilder::string("color", "desc")
                .choice("Red", "red")
                .build(),
        )
        .expect("json");
        assert!(plain.get("name_localizations").is_none());
        assert!(plain.get("description_localizations").is_none());
        assert!(plain.get("channel_types").is_none());
        assert!(plain["choices"][0].get("name_localizations").is_none());

        let decoded: crate::model::ApplicationCommandOption = serde_json::from_value(json!({
            "type": 3,
            "name": "color",
            "description": "desc",
            "name_localizations": null,
            "choices": [{ "name": "Red", "value": "red", "name_localizations": { "de": "Rot" } }]
        }))
        .expect("decodes Discord's shape");
        assert!(decoded.name_localizations.is_none());
        assert_eq!(
            decoded.choices[0]
                .name_localizations
                .as_ref()
                .and_then(|names| names.get("de"))
                .map(String::as_str),
            Some("Rot")
        );

        expect_model_error(
            CommandOptionBuilder::string("color", "desc")
                .name_localization("de", "Farbe")
                .try_build(),
            "option \"color\" name_localizations[de] must be lowercase",
        );
        expect_model_error(
            CommandOptionBuilder::string("color", "desc")
                .name_localization("xx", "farbe")
                .try_build(),
            "option \"color\" name_localizations uses unsupported locale \"xx\"",
        );
        expect_model_error(
            CommandOptionBuilder::string("color", "desc")
                .description_localization("de", &"ä".repeat(101))
                .try_build(),
            "option \"color\" description_localizations[de] must be 1-100 characters, got 101",
        );
        CommandOptionBuilder::string("color", "desc")
            .localized_choice("Red", "red", [("de", "r".repeat(100))])
            .validate()
            .expect("100-character localized choice name");
        expect_model_error(
            CommandOptionBuilder::string("color", "desc")
                .localized_choice("Red", "red", [("de", "r".repeat(101))])
                .try_build(),
            "option \"color\" choice \"Red\" name_localizations[de] must be 1-100 characters, got 101",
        );
        expect_model_error(
            CommandOptionBuilder::string("color", "desc")
                .localized_choice("Red", "red", [("zz", "rot")])
                .try_build(),
            "option \"color\" choice \"Red\" name_localizations uses unsupported locale \"zz\"",
        );
    }

    #[test]
    fn localized_option_names_must_not_collide_through_the_builders() {
        expect_model_error(
            SlashCommandBuilder::new("mod", "desc")
                .option(CommandOptionBuilder::user("target", "d").name_localization("de", "reason"))
                .string_option("reason", "d", false)
                .try_build(),
            "command option \"target\" name_localizations[de] \"reason\" collides",
        );
        expect_model_error(
            SlashCommandBuilder::new("mod", "desc")
                .subcommand(
                    sub("ban")
                        .option(
                            CommandOptionBuilder::user("target", "d")
                                .name_localization("de", "ziel"),
                        )
                        .option(
                            CommandOptionBuilder::string("reason", "d")
                                .name_localization("de", "ziel"),
                        ),
                )
                .try_build(),
            "option \"ban\" option \"reason\" name_localizations[de] \"ziel\" collides",
        );
        SlashCommandBuilder::new("mod", "desc")
            .option(CommandOptionBuilder::user("target", "d").name_localization("de", "ziel"))
            .option(CommandOptionBuilder::string("reason", "d").name_localization("fr", "ziel"))
            .validate()
            .expect("the same localized name in different locales does not collide");
    }

    #[test]
    fn channel_types_are_only_accepted_on_channel_options() {
        let option = CommandOptionBuilder::channel("where", "Target channel").channel_types([0, 2]);
        option.clone().validate().expect("CHANNEL option");
        let value = serde_json::to_value(option.build()).expect("json");
        assert_eq!(value["channel_types"], json!([0, 2]));

        expect_model_error(
            CommandOptionBuilder::string("where", "desc")
                .channel_types([0])
                .try_build(),
            "option \"where\" sets channel_types, which only CHANNEL options accept",
        );
        CommandOptionBuilder::string("where", "desc")
            .channel_types([])
            .validate()
            .expect("an empty list restricts nothing");
    }
}
