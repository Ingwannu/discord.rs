//! The 2.x permission set name, kept as a deprecated alias.

/// The 2.x name of [`Permissions`](crate::bitfield::Permissions).
///
/// 2.x had two permission set types: this one (`PermissionsBitField(pub
/// u64)`, used by model fields) and `bitfield::Permissions` (with named
/// constants and set operations). Since 3.0 every field and parameter uses
/// [`Permissions`](crate::bitfield::Permissions); this alias only points old
/// code at it.
///
/// Migration: `PermissionsBitField(bits)` → `Permissions::from_bits(bits)`,
/// `.0` → `.bits()`, `set.insert(flag)` → `set |= flag`, and
/// `set.remove(flag)` (which changed `set` in place) → `set = set.remove(flag)`
/// (it returns the new set; the compiler warns when the result is dropped).
/// The JSON shape is unchanged: a decimal string, reading strings or
/// non-negative integers.
#[deprecated(
    since = "3.0.0",
    note = "use discordrs::Permissions (bitfield::Permissions); see the alias docs for the method changes"
)]
pub type PermissionsBitField = crate::bitfield::Permissions;
