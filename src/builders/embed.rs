//! [`EmbedBuilder`], which builds a typed [`Embed`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::DiscordError;
use crate::request::{Embed, EmbedAuthor, EmbedField, EmbedFooter, EmbedImage};

/// Builder for Discord rich embeds, paralleling discord.js's `EmbedBuilder`.
///
/// # Example
/// ```
/// use discordrs::builders::EmbedBuilder;
///
/// let embed = EmbedBuilder::new()
///     .title("Hello")
///     .description("World")
///     .color(0x5865F2)
///     .field("Name", "Value", false)
///     .footer("Footer text", None)
///     .build();
/// assert_eq!(embed.title.as_deref(), Some("Hello"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct EmbedBuilder {
    inner: Embed,
}

impl EmbedBuilder {
    /// An empty embed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the title (up to 256 characters).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.inner.title = Some(title.into());
        self
    }

    /// Sets the description (up to 4096 characters).
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.inner.description = Some(description.into());
        self
    }

    /// Sets the URL the title links to.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.inner.url = Some(url.into());
        self
    }

    /// Sets the sidebar colour as RGB.
    pub fn color(mut self, color: u32) -> Self {
        self.inner.color = Some(color);
        self
    }

    /// Appends a field (up to 25; name 1-256, value 1-1024 characters).
    pub fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.inner.fields.push(EmbedField::new(name, value, inline));
        self
    }

    /// Appends a field whose name and value are a zero-width space, used
    /// to break a row of inline fields.
    pub fn blank_field(self, inline: bool) -> Self {
        self.field("\u{200B}", "\u{200B}", inline)
    }

    /// Sets the author line.
    pub fn author(
        mut self,
        name: impl Into<String>,
        url: Option<String>,
        icon_url: Option<String>,
    ) -> Self {
        self.inner.author = Some(EmbedAuthor {
            url,
            icon_url,
            ..EmbedAuthor::new(name)
        });
        self
    }

    /// Sets the small image beside the description.
    pub fn thumbnail(mut self, url: impl Into<String>) -> Self {
        self.inner.thumbnail = Some(EmbedImage::new(url));
        self
    }

    /// Sets the large image.
    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.inner.image = Some(EmbedImage::new(url));
        self
    }

    /// Sets the footer.
    pub fn footer(mut self, text: impl Into<String>, icon_url: Option<String>) -> Self {
        self.inner.footer = Some(EmbedFooter {
            icon_url,
            ..EmbedFooter::new(text)
        });
        self
    }

    /// Sets the timestamp to the current UTC time in Discord-compatible ISO 8601 UTC format.
    ///
    /// ```
    /// use discordrs::builders::EmbedBuilder;
    ///
    /// let embed = EmbedBuilder::new().timestamp_now().build();
    /// let timestamp = embed.timestamp.unwrap_or_default();
    /// assert_eq!(timestamp.len(), 24);
    /// assert_eq!(&timestamp[10..11], "T");
    /// assert!(timestamp.ends_with('Z'));
    /// ```
    pub fn timestamp_now(mut self) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        self.inner.timestamp = Some(format_unix_timestamp(now));
        self
    }

    /// Sets an ISO 8601 timestamp.
    pub fn timestamp_iso(mut self, iso: impl Into<String>) -> Self {
        self.inner.timestamp = Some(iso.into());
        self
    }

    /// Validates the embed against Discord's limits without consuming the
    /// builder: title <= 256, description <= 4096, up to 25 fields (name
    /// 1-256, value 1-1024), footer text <= 2048, author name <= 256, and
    /// a combined total of at most 6000 characters.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// Validating counterpart to [`Self::build`]: fails locally with a
    /// descriptive [`DiscordError::Model`] instead of a Discord 400.
    pub fn try_build(self) -> Result<Embed, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }

    /// The embed, without checking limits.
    pub fn build(self) -> Embed {
        self.inner
    }
}

impl From<EmbedBuilder> for Embed {
    fn from(builder: EmbedBuilder) -> Self {
        builder.build()
    }
}

fn format_unix_timestamp(timestamp: Duration) -> String {
    const SECONDS_PER_DAY: u64 = 86_400;
    const SECONDS_PER_HOUR: u64 = 3_600;
    const SECONDS_PER_MINUTE: u64 = 60;

    let total_seconds = timestamp.as_secs();
    let days = i64::try_from(total_seconds / SECONDS_PER_DAY).unwrap_or(i64::MAX);
    let seconds_of_day = total_seconds % SECONDS_PER_DAY;
    let hour = seconds_of_day / SECONDS_PER_HOUR;
    let minute = (seconds_of_day % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let second = seconds_of_day % SECONDS_PER_MINUTE;
    let milliseconds = timestamp.subsec_millis();
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milliseconds:03}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_phase = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_phase + 2) / 5 + 1;
    let month = month_phase + if month_phase < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, format_unix_timestamp, EmbedBuilder};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn format_unix_timestamp_formats_known_utc_instant() {
        let timestamp = format_unix_timestamp(Duration::from_millis(1_709_210_096_789));

        assert_eq!(timestamp, "2024-02-29T12:34:56.789Z");
    }

    #[test]
    fn timestamp_now_builds_iso8601_utc_timestamp() {
        let embed = EmbedBuilder::new().timestamp_now().build();
        let timestamp = embed.timestamp.expect("timestamp should be present");

        assert_eq!(timestamp.len(), 24);
        assert_eq!(timestamp.as_bytes()[4], b'-');
        assert_eq!(timestamp.as_bytes()[7], b'-');
        assert_eq!(timestamp.as_bytes()[10], b'T');
        assert_eq!(timestamp.as_bytes()[13], b':');
        assert_eq!(timestamp.as_bytes()[16], b':');
        assert_eq!(timestamp.as_bytes()[19], b'.');
        assert_eq!(timestamp.as_bytes()[23], b'Z');
        assert!(timestamp
            .bytes()
            .enumerate()
            .all(|(index, byte)| match index {
                4 | 7 => byte == b'-',
                10 => byte == b'T',
                13 | 16 => byte == b':',
                19 => byte == b'.',
                23 => byte == b'Z',
                _ => byte.is_ascii_digit(),
            }));
    }

    #[test]
    fn embed_build_serializes_rich_payload() {
        let embed = EmbedBuilder::new()
            .title("Release")
            .description("Shipped")
            .url("https://example.com/release")
            .color(0x5865F2)
            .field("Status", "Green", true)
            .blank_field(false)
            .author(
                "discordrs",
                Some("https://example.com".to_string()),
                Some("https://example.com/icon.png".to_string()),
            )
            .thumbnail("https://example.com/thumb.png")
            .image("https://example.com/image.png")
            .footer("Footer", Some("https://example.com/footer.png".to_string()))
            .timestamp_iso("2026-04-12T03:04:05.678Z")
            .build();

        assert_eq!(
            serde_json::to_value(&embed).unwrap(),
            json!({
                "title": "Release",
                "description": "Shipped",
                "url": "https://example.com/release",
                "color": 0x5865F2,
                "fields": [
                    {"name": "Status", "value": "Green", "inline": true},
                    {"name": "\u{200B}", "value": "\u{200B}", "inline": false}
                ],
                "author": {
                    "name": "discordrs",
                    "url": "https://example.com",
                    "icon_url": "https://example.com/icon.png"
                },
                "thumbnail": {"url": "https://example.com/thumb.png"},
                "image": {"url": "https://example.com/image.png"},
                "footer": {"text": "Footer", "icon_url": "https://example.com/footer.png"},
                "timestamp": "2026-04-12T03:04:05.678Z"
            })
        );
    }

    #[test]
    fn embed_build_omits_optional_nested_fields_when_not_set() {
        let embed = EmbedBuilder::new()
            .author("discordrs", None, None)
            .footer("Footer", None)
            .build();
        assert_eq!(
            serde_json::to_value(&embed).unwrap(),
            json!({"author": {"name": "discordrs"}, "footer": {"text": "Footer"}})
        );
        assert_eq!(
            serde_json::to_string(&EmbedBuilder::new().build()).unwrap(),
            "{}"
        );
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
    fn embed_try_build_matches_build_for_valid_embed() {
        let make = || {
            EmbedBuilder::new()
                .title("Release")
                .description("Shipped")
                .field("Status", "Green", true)
                .author("discordrs", None, None)
                .footer("Footer", None)
        };
        assert_eq!(make().build(), make().try_build().expect("valid embed"));
    }

    #[test]
    fn embed_validate_counts_codepoints_not_bytes() {
        // 256 codepoints but 512 UTF-8 bytes: must pass.
        EmbedBuilder::new()
            .title("é".repeat(256))
            .validate()
            .expect("codepoint-length title should be valid");
    }

    #[test]
    fn embed_try_build_rejects_oversized_parts() {
        expect_model_error(
            EmbedBuilder::new().title("t".repeat(257)).try_build(),
            "embed title must be at most 256 characters, got 257",
        );
        expect_model_error(
            EmbedBuilder::new()
                .description("d".repeat(4097))
                .try_build(),
            "embed description must be at most 4096 characters, got 4097",
        );
        expect_model_error(
            EmbedBuilder::new()
                .field("n".repeat(257), "value", false)
                .try_build(),
            "embed field 0 name must be 1-256 characters, got 257",
        );
        expect_model_error(
            EmbedBuilder::new().field("name", "", false).try_build(),
            "embed field 0 value must be 1-1024 characters, got 0",
        );
        expect_model_error(
            EmbedBuilder::new()
                .field("name", "v".repeat(1025), false)
                .try_build(),
            "embed field 0 value must be 1-1024 characters, got 1025",
        );
        expect_model_error(
            EmbedBuilder::new()
                .footer("f".repeat(2049), None)
                .try_build(),
            "embed footer text must be at most 2048 characters, got 2049",
        );
        expect_model_error(
            EmbedBuilder::new()
                .author("a".repeat(257), None, None)
                .try_build(),
            "embed author name must be at most 256 characters, got 257",
        );

        let mut overfull = EmbedBuilder::new();
        for index in 0..26 {
            overfull = overfull.field(format!("f{index}"), "v", false);
        }
        expect_model_error(
            overfull.try_build(),
            "embed fields must contain at most 25 items, got 26",
        );
    }

    #[test]
    fn embed_try_build_rejects_total_length_over_6000() {
        // Each part is individually valid, but the sum exceeds 6000.
        let embed = EmbedBuilder::new()
            .description("d".repeat(4000))
            .field("a", "v".repeat(1024), false)
            .field("b", "v".repeat(1024), false)
            .field("c", "v".repeat(1024), false);
        expect_model_error(
            embed.try_build(),
            "embed total length must be at most 6000 characters, got 7075",
        );
    }

    #[test]
    fn civil_from_days_handles_epoch_and_leap_day() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }
}
