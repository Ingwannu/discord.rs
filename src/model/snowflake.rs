use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
/// Typed Discord API object for `Snowflake`.
pub struct Snowflake {
    raw: String,
    numeric: Option<u64>,
}

impl Snowflake {
    /// Discord epoch: January 1, 2015 00:00:00 UTC in milliseconds.
    const DISCORD_EPOCH: u64 = 1_420_070_400_000;

    /// Creates a `new` value.
    pub fn new(value: impl Into<String>) -> Self {
        let raw = value.into();
        let numeric = raw.parse().ok();
        Self { raw, numeric }
    }

    /// Creates a `try_new` value.
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let snowflake = Self::new(value);
        if snowflake.is_valid() {
            Ok(snowflake)
        } else {
            Err("snowflake must contain only ASCII digits".to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.numeric
    }

    pub fn to_u64(&self) -> Option<u64> {
        self.as_u64()
    }

    pub fn is_valid(&self) -> bool {
        !self.raw.is_empty() && self.raw.chars().all(|ch| ch.is_ascii_digit())
    }

    /// Extracts the creation timestamp from this Snowflake as Unix milliseconds.
    ///
    /// Discord encodes the creation timestamp in the top 42 bits of every Snowflake ID.
    /// Returns `None` if the inner value is not a valid u64.
    pub fn timestamp(&self) -> Option<u64> {
        let raw = self.numeric?;
        // (raw >> 22) gives milliseconds since Discord epoch
        Some((raw >> 22) + Self::DISCORD_EPOCH)
    }
}

impl Display for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

impl From<u64> for Snowflake {
    fn from(value: u64) -> Self {
        Self {
            raw: value.to_string(),
            numeric: Some(value),
        }
    }
}

impl From<&str> for Snowflake {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Snowflake {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl FromStr for Snowflake {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl Serialize for Snowflake {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Snowflake {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SnowflakeVisitor;

        impl<'de> Visitor<'de> for SnowflakeVisitor {
            type Value = Snowflake;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Discord snowflake encoded as a string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value < 0 {
                    return Err(E::custom("snowflake cannot be negative"));
                }
                Ok(Snowflake::from(value as u64))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Snowflake::from(value))
            }
        }

        deserializer.deserialize_any(SnowflakeVisitor)
    }
}
