//! Serde for model fields that hold shared (`Arc`) values: exactly the JSON
//! of the value itself.
//!
//! [Decision Log] Shared models
//! - Purpose: the cache and the events it stores share one allocation per
//!   user, channel, and role instead of a copy each (member to user copies
//!   were most of a large GUILD_CREATE's cache cost).
//! - Constraint: serde implements `Arc<T>` only with its `rc` feature, a
//!   crate-wide switch in `Cargo.toml`.
//! - Alternatives: (1) enable `rc`; (2) keep the values inline and wrap a
//!   copy in the cache; (3) these functions as `#[serde(with)]`.
//! - Chosen: (3). They reuse `T`'s (or `Option<T>`'s) decoder unchanged, the
//!   same instances every reader already has, then move the value into an
//!   `Arc`.
//! - Why: (1) changes how every `Arc` in a downstream build serializes for a
//!   handful of fields. (2) keeps the copies this change exists to remove.
//! - Trade-offs: one allocation per decoded value for the `Arc` (it replaces
//!   the cache's copy of the value's strings and lists); a struct holding the
//!   field shrinks by the value's inline size.

/// `#[serde(deserialize_with)]` for `Arc<T>` (the payloads using it are
/// decode-only).
pub(crate) mod arc {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer};

    pub(crate) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<T>, D::Error> {
        T::deserialize(deserializer).map(Arc::new)
    }
}

/// `#[serde(with)]` for `Option<Arc<T>>`; pair it with `default`.
pub(crate) mod option_arc {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Arc<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_deref().serialize(serializer)
    }

    pub(crate) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Arc<T>>, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| value.map(Arc::new))
    }
}

/// Serializes `Vec<Arc<T>>` and `Option<Vec<Arc<T>>>` as the plain lists;
/// decoding goes through [`ArcSeq`](super::gateway_payload::ArcSeq), which
/// wraps each element once as it is read.
pub(crate) mod seq_arc {
    use std::sync::Arc;

    use serde::{Serialize, Serializer};

    struct Items<'a, T>(&'a [Arc<T>]);

    impl<T: Serialize> Serialize for Items<'_, T> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.collect_seq(self.0.iter().map(|item| &**item))
        }
    }

    pub(crate) fn serialize<T: Serialize, S: Serializer>(
        items: &[Arc<T>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Items(items).serialize(serializer)
    }

    pub(crate) fn serialize_option<T: Serialize, S: Serializer>(
        items: &Option<Vec<Arc<T>>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match items {
            Some(items) => serializer.serialize_some(&Items(items)),
            None => serializer.serialize_none(),
        }
    }
}
