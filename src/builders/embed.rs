use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
/// ```
#[derive(Clone, Debug, Default)]
pub struct EmbedBuilder {
    title: Option<String>,
    description: Option<String>,
    url: Option<String>,
    color: Option<u32>,
    fields: Vec<EmbedFieldData>,
    author: Option<Value>,
    thumbnail: Option<Value>,
    image: Option<Value>,
    footer: Option<Value>,
    timestamp: Option<String>,
    video: Option<Value>,
    provider: Option<Value>,
}

#[derive(Clone, Debug)]
struct EmbedFieldData {
    name: String,
    value: String,
    inline: bool,
}

impl EmbedBuilder {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.fields.push(EmbedFieldData {
            name: name.into(),
            value: value.into(),
            inline,
        });
        self
    }

    pub fn blank_field(mut self, inline: bool) -> Self {
        self.fields.push(EmbedFieldData {
            name: "\u{200B}".to_string(),
            value: "\u{200B}".to_string(),
            inline,
        });
        self
    }

    pub fn author(
        mut self,
        name: impl Into<String>,
        url: Option<String>,
        icon_url: Option<String>,
    ) -> Self {
        let mut author = serde_json::json!({ "name": name.into() });
        if let Some(url) = url {
            author["url"] = Value::String(url);
        }
        if let Some(icon_url) = icon_url {
            author["icon_url"] = Value::String(icon_url);
        }
        self.author = Some(author);
        self
    }

    pub fn thumbnail(mut self, url: impl Into<String>) -> Self {
        self.thumbnail = Some(serde_json::json!({ "url": url.into() }));
        self
    }

    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.image = Some(serde_json::json!({ "url": url.into() }));
        self
    }

    pub fn footer(mut self, text: impl Into<String>, icon_url: Option<String>) -> Self {
        let mut footer = serde_json::json!({ "text": text.into() });
        if let Some(icon_url) = icon_url {
            footer["icon_url"] = Value::String(icon_url);
        }
        self.footer = Some(footer);
        self
    }

    /// Sets the timestamp to the current UTC time in Discord-compatible ISO 8601 UTC format.
    ///
    /// ```
    /// use discordrs::builders::EmbedBuilder;
    ///
    /// let embed = EmbedBuilder::new().timestamp_now().build();
    /// if let Some(timestamp) = embed.get("timestamp").and_then(|value| value.as_str()) {
    ///     assert_eq!(timestamp.len(), 24);
    ///     assert_eq!(&timestamp[10..11], "T");
    ///     assert!(timestamp.ends_with('Z'));
    /// } else {
    ///     panic!("timestamp should be present");
    /// }
    /// ```
    pub fn timestamp_now(mut self) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        self.timestamp = Some(format_unix_timestamp(now));
        self
    }

    pub fn timestamp_iso(mut self, iso: impl Into<String>) -> Self {
        self.timestamp = Some(iso.into());
        self
    }

    /// Build the embed into a serde_json::Value for use in API requests.
    pub fn build(self) -> Value {
        let mut embed = serde_json::json!({});

        if let Some(title) = self.title {
            embed["title"] = Value::String(title);
        }
        if let Some(description) = self.description {
            embed["description"] = Value::String(description);
        }
        if let Some(url) = self.url {
            embed["url"] = Value::String(url);
        }
        if let Some(color) = self.color {
            embed["color"] = Value::Number(color.into());
        }
        if !self.fields.is_empty() {
            embed["fields"] = Value::Array(
                self.fields
                    .into_iter()
                    .map(|f| {
                        serde_json::json!({
                            "name": f.name,
                            "value": f.value,
                            "inline": f.inline,
                        })
                    })
                    .collect(),
            );
        }
        if let Some(author) = self.author {
            embed["author"] = author;
        }
        if let Some(thumbnail) = self.thumbnail {
            embed["thumbnail"] = thumbnail;
        }
        if let Some(image) = self.image {
            embed["image"] = image;
        }
        if let Some(footer) = self.footer {
            embed["footer"] = footer;
        }
        if let Some(timestamp) = self.timestamp {
            embed["timestamp"] = Value::String(timestamp);
        }
        if let Some(video) = self.video {
            embed["video"] = video;
        }
        if let Some(provider) = self.provider {
            embed["provider"] = provider;
        }

        embed
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

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i32 + era as i32 * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_phase = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_phase + 2) / 5 + 1;
    let month = month_phase + if month_phase < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month as u32, day as u32)
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
        let timestamp = embed
            .get("timestamp")
            .and_then(|value| value.as_str())
            .expect("timestamp should be present");

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

        let fields = embed
            .get("fields")
            .and_then(|value| value.as_array())
            .expect("fields array");

        assert_eq!(
            embed.get("title").and_then(|value| value.as_str()),
            Some("Release")
        );
        assert_eq!(
            embed.get("description").and_then(|value| value.as_str()),
            Some("Shipped")
        );
        assert_eq!(
            embed.get("url").and_then(|value| value.as_str()),
            Some("https://example.com/release")
        );
        assert_eq!(
            embed.get("color").and_then(|value| value.as_u64()),
            Some(0x5865F2)
        );
        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields[0].get("name").and_then(|value| value.as_str()),
            Some("Status")
        );
        assert_eq!(
            fields[0].get("value").and_then(|value| value.as_str()),
            Some("Green")
        );
        assert_eq!(
            fields[0].get("inline").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            fields[1].get("name").and_then(|value| value.as_str()),
            Some("\u{200B}")
        );
        assert_eq!(
            fields[1].get("value").and_then(|value| value.as_str()),
            Some("\u{200B}")
        );
        assert_eq!(
            fields[1].get("inline").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            embed
                .get("author")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("discordrs")
        );
        assert_eq!(
            embed
                .get("author")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com")
        );
        assert_eq!(
            embed
                .get("author")
                .and_then(|value| value.get("icon_url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com/icon.png")
        );
        assert_eq!(
            embed
                .get("thumbnail")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com/thumb.png")
        );
        assert_eq!(
            embed
                .get("image")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com/image.png")
        );
        assert_eq!(
            embed
                .get("footer")
                .and_then(|value| value.get("text"))
                .and_then(|value| value.as_str()),
            Some("Footer")
        );
        assert_eq!(
            embed
                .get("footer")
                .and_then(|value| value.get("icon_url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com/footer.png")
        );
        assert_eq!(
            embed.get("timestamp").and_then(|value| value.as_str()),
            Some("2026-04-12T03:04:05.678Z")
        );
    }

    #[test]
    fn embed_build_omits_optional_nested_fields_when_not_set() {
        let embed = EmbedBuilder::new()
            .author("discordrs", None, None)
            .footer("Footer", None)
            .build();

        assert_eq!(
            embed
                .get("author")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("discordrs")
        );
        assert!(embed
            .get("author")
            .and_then(|value| value.get("url"))
            .is_none());
        assert!(embed
            .get("author")
            .and_then(|value| value.get("icon_url"))
            .is_none());
        assert_eq!(
            embed
                .get("footer")
                .and_then(|value| value.get("text"))
                .and_then(|value| value.as_str()),
            Some("Footer")
        );
        assert!(embed
            .get("footer")
            .and_then(|value| value.get("icon_url"))
            .is_none());
        assert!(embed.get("fields").is_none());
    }

    #[test]
    fn embed_build_includes_direct_video_and_provider_fields() {
        let embed = EmbedBuilder {
            video: Some(json!({ "url": "https://example.com/video.mp4" })),
            provider: Some(json!({ "name": "discordrs" })),
            ..EmbedBuilder::default()
        }
        .build();

        assert_eq!(
            embed
                .get("video")
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str()),
            Some("https://example.com/video.mp4")
        );
        assert_eq!(
            embed
                .get("provider")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("discordrs")
        );
    }

    #[test]
    fn civil_from_days_handles_epoch_and_leap_day() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }
}
