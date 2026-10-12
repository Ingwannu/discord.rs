//! Setter generation for the REST request bodies of phase 5 B (channel,
//! guild, moderation, media, scheduled events, commands).
//!
//! The bodies are plain structs with public fields and `#[non_exhaustive]`,
//! so new Discord fields can be added without a breaking change; code outside
//! the crate therefore builds them with `new(..)` plus these consuming
//! setters, or by assigning fields on a value from `new`.
//!
//! Field kinds (`Patch` / `NullablePatch` from [`crate::model`]):
//!
//! - `opt name: T` — `Option<T>` on a create body; the setter writes `Some`.
//! - `patch name: T` — `Patch<T>` on a modify body for a field Discord does
//!   not let you clear; the setter writes the value.
//! - `nullable name / clear_name: T` — `NullablePatch<T>`; `name` writes the
//!   value and `clear_name` writes `null` (Discord's "remove it").
//!
//! Every setter takes `impl Into<T>`.

macro_rules! request_setters {
    ($($kind:ident $field:ident $(/ $clear:ident)? : $ty:ty;)*) => {
        $(request_setters!(@$kind $field $(/ $clear)? : $ty);)*
    };
    (@opt $field:ident : $ty:ty) => {
        #[doc = concat!("Sets `", stringify!($field), "`.")]
        pub fn $field(mut self, value: impl Into<$ty>) -> Self {
            self.$field = Some(value.into());
            self
        }
    };
    (@patch $field:ident : $ty:ty) => {
        #[doc = concat!("Sets `", stringify!($field), "`.")]
        pub fn $field(mut self, value: impl Into<$ty>) -> Self {
            self.$field = $crate::model::Patch::Value(value.into());
            self
        }
    };
    (@nullable $field:ident / $clear:ident : $ty:ty) => {
        #[doc = concat!("Sets `", stringify!($field), "`.")]
        pub fn $field(mut self, value: impl Into<$ty>) -> Self {
            self.$field = $crate::model::Patch::Value($crate::model::Nullable::Value(value.into()));
            self
        }

        #[doc = concat!("Sends `\"", stringify!($field), "\": null`, which clears it.")]
        pub fn $clear(mut self) -> Self {
            self.$field = $crate::model::Patch::Value($crate::model::Nullable::Null);
            self
        }
    };
}

pub(crate) use request_setters;
