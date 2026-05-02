use std::fmt::Formatter;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
/// Typed Discord API object for `PermissionsBitField`.
pub struct PermissionsBitField(pub u64);

impl PermissionsBitField {
    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn contains(self, permission: u64) -> bool {
        self.0 & permission == permission
    }

    pub fn insert(&mut self, permission: u64) {
        self.0 |= permission;
    }

    pub fn remove(&mut self, permission: u64) {
        self.0 &= !permission;
    }
}

impl Serialize for PermissionsBitField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for PermissionsBitField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PermissionsVisitor;

        impl<'de> Visitor<'de> for PermissionsVisitor {
            type Value = PermissionsBitField;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Discord permission bitfield encoded as a string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PermissionsBitField(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value
                    .parse()
                    .map(PermissionsBitField)
                    .map_err(|error| E::custom(format!("invalid permission bitfield: {error}")))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_any(PermissionsVisitor)
    }
}
