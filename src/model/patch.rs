//! Field states for partial updates: omitted, explicit `null`, or a value.
//!
//! The module is private; the user-facing guide (field table, required serde
//! attributes, pitfalls) is on [`Patch`], which is re-exported from
//! `discordrs::model` and the crate root.

// [Decision Log]
// - 목적과 의도: 생략·null·값을 타입으로 구분해 "null 불가 필드에 null"을 컴파일 시점에 막고,
//   부분 갱신이 생략된 필드를 덮어쓰지 않게 한다(docs/design/3.0-types-events-requests.md §3).
// - 기존 구현 및 제약 조건: 요청 필드는 Option<Option<T>>라 Some(None)이 null인지 실수인지
//   구분이 안 되고, bool 같은 null 불가 필드에도 Some(None)을 쓸 수 있다. 시리즈 1은 타입만
//   추가하고, 실제 필드 이전은 엔드포인트별 null 허용 여부를 확인하며 4단계에서 한다.
// - 검토한 주요 대안: (1) Option<Option<T>> 유지, (2) Missing/Null/Value 세 상태 enum 하나,
//   (3) Patch<T>와 Nullable<T> 분리 후 조합.
// - 선택한 방식: (3). 직렬화는 태그 없는 표현, 단독 Missing 직렬화는 오류, Default는
//   Missing(T: Default 불요).
// - 다른 대안 대신 이 방식을 선택한 이유: (2)는 Patch<bool>에서도 Null을 만들 수 있어
//   잘못된 null을 막지 못한다. 분리하면 null 가능성이 타입에 드러나고 Patch::null()은
//   Patch<Nullable<T>>에만 존재한다.
// - 장점, 단점 및 영향: null 가능 필드는 Patch::value(Nullable::Value(x))처럼 한 단계 더
//   감싸야 한다. serde가 필드 생략을 타입에 알려 주지 않으므로 #[serde(default)]를 빠뜨리면
//   생략이 null로 읽힌다(Patch 문서와 테스트로 고정).

use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A field that is either omitted or set to a value.
///
/// `Patch<T>` cannot hold `null`; use [`NullablePatch<T>`] for fields Discord
/// lets you clear.
///
/// Discord distinguishes three states for many request and event fields:
/// the key is absent (leave the stored value alone), the key is `null`
/// (clear it), or the key has a value (replace it). [`Patch<T>`] models
/// "absent or value" and [`Nullable<T>`] models "null or value"; a field that
/// can be all three is [`NullablePatch<T>`] = `Patch<Nullable<T>>`.
///
/// | Field | Type |
/// |---|---|
/// | message `content` on edit | `Patch<String>` |
/// | member `nick` (can be cleared) | `NullablePatch<String>` |
/// | a `bool` setting | `Patch<bool>` (Discord rejects `null`) |
/// | required-but-nullable snapshot field | `Nullable<T>` |
///
/// Struct fields of type `Patch<T>` (including `NullablePatch<T>`) must be
/// declared as
///
/// ```
/// # use discordrs::model::{NullablePatch, Patch};
/// # use serde::{Deserialize, Serialize};
/// #[derive(Default, Serialize, Deserialize)]
/// struct ModifyMember {
///     #[serde(default, skip_serializing_if = "Patch::is_missing")]
///     nick: NullablePatch<String>,
///     #[serde(default, skip_serializing_if = "Patch::is_missing")]
///     mute: Patch<bool>,
/// }
///
/// let body = ModifyMember { nick: Patch::null(), ..ModifyMember::default() };
/// assert_eq!(serde_json::to_string(&body).ok().as_deref(), Some(r#"{"nick":null}"#));
/// ```
///
/// Both attributes are needed. `skip_serializing_if` keeps `Missing` out of
/// the output; serializing a `Missing` that reaches the serializer is an
/// error rather than a silent `null`. `default` makes an absent key read as
/// `Missing`. Without it serde applies its `Option` rule to absent keys: a
/// `NullablePatch` field would read an absent key as an explicit `null`
/// (which would clear data), and a `Patch<T>` field would fail with
/// "missing field".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Patch<T> {
    /// The field is omitted: keep the current value.
    Missing,
    /// The field is present with this value.
    Value(T),
}

/// A value that may be explicitly `null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Nullable<T> {
    /// JSON `null`: no value, or "clear it" inside a patch.
    Null,
    /// A present value.
    Value(T),
}

/// A field that can be omitted, set to `null`, or set to a value.
pub type NullablePatch<T> = Patch<Nullable<T>>;

impl<T> Patch<T> {
    /// An omitted field.
    pub const fn missing() -> Self {
        Self::Missing
    }

    /// A field set to `value`.
    pub const fn value(value: T) -> Self {
        Self::Value(value)
    }

    /// Whether the field is omitted. Use as
    /// `skip_serializing_if = "Patch::is_missing"`.
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    /// Borrows the value, if present.
    pub const fn as_ref(&self) -> Patch<&T> {
        match self {
            Self::Missing => Patch::Missing,
            Self::Value(value) => Patch::Value(value),
        }
    }

    /// Transforms the value, keeping `Missing` as is.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Patch<U> {
        match self {
            Self::Missing => Patch::Missing,
            Self::Value(value) => Patch::Value(f(value)),
        }
    }

    /// Returns the value, or `None` if omitted.
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Missing => None,
            Self::Value(value) => Some(value),
        }
    }
}

impl<T> Patch<Nullable<T>> {
    /// A field set to explicit `null` ("clear it").
    ///
    /// Only nullable patches have this constructor, so `Patch<bool>` cannot
    /// be set to `null` by mistake.
    pub const fn null() -> Self {
        Self::Value(Nullable::Null)
    }
}

/// `Missing`, without requiring `T: Default`.
impl<T> Default for Patch<T> {
    fn default() -> Self {
        Self::Missing
    }
}

/// `None` is `Missing`; `Some(value)` is `Value(value)`. For a nullable patch
/// that means `None` omits the field; it never produces `null`.
impl<T> From<Option<T>> for Patch<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Value(value),
            None => Self::Missing,
        }
    }
}

impl<T> Nullable<T> {
    /// Whether this is `null`.
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Borrows the value, if not `null`.
    pub const fn as_ref(&self) -> Nullable<&T> {
        match self {
            Self::Null => Nullable::Null,
            Self::Value(value) => Nullable::Value(value),
        }
    }

    /// Transforms the value, keeping `Null` as is.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Nullable<U> {
        match self {
            Self::Null => Nullable::Null,
            Self::Value(value) => Nullable::Value(f(value)),
        }
    }

    /// Returns the value, or `None` if `null`.
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Null => None,
            Self::Value(value) => Some(value),
        }
    }
}

/// `None` is `Null`; `Some(value)` is `Value(value)`.
impl<T> From<Option<T>> for Nullable<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Value(value),
            None => Self::Null,
        }
    }
}

impl<T> From<Nullable<T>> for Option<T> {
    fn from(value: Nullable<T>) -> Self {
        value.into_option()
    }
}

/// Writes the value itself (no enum tag). `Missing` has no wire
/// representation and returns an error; skip it with
/// `skip_serializing_if = "Patch::is_missing"`.
impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Value(value) => value.serialize(serializer),
            Self::Missing => Err(S::Error::custom(
                "Patch::Missing has no wire representation; \
                 mark the field with skip_serializing_if = \"Patch::is_missing\"",
            )),
        }
    }
}

/// A present key is read through `T`, so `null` is accepted only when `T`
/// accepts it (for example `Nullable<_>`); `Patch<bool>` rejects `null`.
/// Absent keys become `Missing` through `#[serde(default)]`.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::Value)
    }
}

/// Writes `null` or the value itself.
impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Value(value) => value.serialize(serializer),
        }
    }
}

/// Reads `null` as `Null` and anything else through `T`.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::{Nullable, NullablePatch, Patch};

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Body {
        #[serde(default, skip_serializing_if = "Patch::is_missing")]
        flag: Patch<bool>,
        #[serde(default, skip_serializing_if = "Patch::is_missing")]
        count: Patch<u64>,
        #[serde(default, skip_serializing_if = "Patch::is_missing")]
        text: Patch<String>,
        #[serde(default, skip_serializing_if = "Patch::is_missing")]
        list: Patch<Vec<u64>>,
        #[serde(default, skip_serializing_if = "Patch::is_missing")]
        nick: NullablePatch<String>,
    }

    fn to_json(body: &Body) -> serde_json::Value {
        match serde_json::to_value(body) {
            Ok(value) => value,
            Err(error) => panic!("body must serialize: {error}"),
        }
    }

    fn from_json(value: serde_json::Value) -> Body {
        match serde_json::from_value(value) {
            Ok(body) => body,
            Err(error) => panic!("body must deserialize: {error}"),
        }
    }

    #[test]
    fn missing_fields_are_omitted_and_read_back_as_missing() {
        assert_eq!(to_json(&Body::default()), json!({}));
        assert_eq!(from_json(json!({})), Body::default());
    }

    #[test]
    fn falsy_values_are_values_not_missing() {
        let body = Body {
            flag: Patch::value(false),
            count: Patch::value(0),
            text: Patch::value(String::new()),
            list: Patch::value(Vec::new()),
            nick: Patch::value(Nullable::Value(String::new())),
        };
        let wire = json!({"flag": false, "count": 0, "text": "", "list": [], "nick": ""});
        assert_eq!(to_json(&body), wire);
        assert_eq!(from_json(wire), body);
    }

    #[test]
    fn nullable_patch_distinguishes_missing_null_and_value() {
        let null = Body {
            nick: Patch::null(),
            ..Body::default()
        };
        assert_eq!(to_json(&null), json!({"nick": null}));
        assert_eq!(from_json(json!({"nick": null})), null);

        let value = Body {
            nick: Patch::value(Nullable::Value("n".to_string())),
            ..Body::default()
        };
        assert_eq!(to_json(&value), json!({"nick": "n"}));
        assert_eq!(from_json(json!({"nick": "n"})), value);

        assert!(from_json(json!({})).nick.is_missing());
    }

    #[test]
    fn null_is_rejected_for_non_nullable_patches() {
        for field in ["flag", "count", "text", "list"] {
            let mut wire = serde_json::Map::new();
            wire.insert(field.to_string(), serde_json::Value::Null);
            assert!(
                serde_json::from_value::<Body>(serde_json::Value::Object(wire)).is_err(),
                "null must be rejected for {field}"
            );
        }
    }

    #[test]
    fn serializing_a_standalone_missing_is_an_error() {
        let error = match serde_json::to_string(&Patch::<bool>::Missing) {
            Ok(text) => panic!("Missing must not serialize, got {text}"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("Patch::is_missing"));
        assert!(serde_json::to_value(vec![Patch::value(1), Patch::missing()]).is_err());
        assert_eq!(
            serde_json::to_value(Patch::value(5)).ok(),
            Some(json!(5)),
            "a standalone value is written untagged"
        );
    }

    #[test]
    fn standalone_nullable_reads_and_writes_null() {
        assert_eq!(
            serde_json::from_value::<Nullable<u8>>(json!(null)).ok(),
            Some(Nullable::Null)
        );
        assert_eq!(
            serde_json::from_value::<Nullable<u8>>(json!(3)).ok(),
            Some(Nullable::Value(3))
        );
        assert_eq!(
            serde_json::to_value(Nullable::<u8>::Null).ok(),
            Some(json!(null))
        );
        assert_eq!(
            serde_json::to_value(Nullable::Value(3_u8)).ok(),
            Some(json!(3))
        );
        assert!(serde_json::from_value::<Nullable<u8>>(json!("3")).is_err());
    }

    #[test]
    fn nullable_patch_without_serde_default_reads_absent_as_null() {
        // Pins the pitfall the module docs warn about: serde applies its
        // Option rule to absent keys, so `#[serde(default)]` is what keeps
        // "absent" from turning into "clear it".
        #[derive(Debug, Deserialize)]
        struct Forgot {
            nick: NullablePatch<String>,
        }
        #[derive(Debug, Deserialize)]
        struct ForgotBool {
            #[allow(dead_code, reason = "only the decode result matters")]
            flag: Patch<bool>,
        }

        let forgot: Forgot = match serde_json::from_value(json!({})) {
            Ok(forgot) => forgot,
            Err(error) => panic!("serde's Option rule applies: {error}"),
        };
        assert_eq!(forgot.nick, Patch::null());
        assert!(serde_json::from_value::<ForgotBool>(json!({})).is_err());
    }

    #[test]
    fn default_is_missing_without_t_default() {
        struct NoDefault;
        assert!(Patch::<NoDefault>::default().is_missing());
        assert!(Patch::<NoDefault>::missing().is_missing());
        assert!(!Patch::value(NoDefault).is_missing());
    }

    #[test]
    fn option_conversions_map_absence_per_layer() {
        assert_eq!(Patch::from(Some(1)), Patch::value(1));
        assert_eq!(Patch::<u8>::from(None), Patch::Missing);
        assert_eq!(Nullable::from(Some(1)), Nullable::Value(1));
        assert_eq!(Nullable::<u8>::from(None), Nullable::Null);
        assert_eq!(Option::from(Nullable::Value(2)), Some(2));
        assert_eq!(Option::<u8>::from(Nullable::Null), None);

        // For a nullable patch, `None` omits the field; it never means null.
        assert_eq!(
            NullablePatch::<u8>::from(None::<Nullable<u8>>),
            Patch::Missing
        );
    }

    #[test]
    fn accessors_and_combinators() {
        let patch = Patch::value(String::from("abc"));
        assert_eq!(patch.as_ref(), Patch::Value(&String::from("abc")));
        assert_eq!(patch.clone().map(|text| text.len()), Patch::Value(3));
        assert_eq!(patch.into_option().as_deref(), Some("abc"));
        assert_eq!(Patch::<u8>::Missing.as_ref(), Patch::Missing);
        assert_eq!(Patch::<u8>::Missing.map(|value| value + 1), Patch::Missing);
        assert_eq!(Patch::<u8>::Missing.into_option(), None);

        let nullable = Nullable::Value(4_u8);
        assert!(!nullable.is_null());
        assert!(Nullable::<u8>::Null.is_null());
        assert_eq!(nullable.as_ref(), Nullable::Value(&4));
        assert_eq!(Nullable::<u8>::Null.as_ref(), Nullable::Null);
        assert_eq!(nullable.map(|value| value * 2), Nullable::Value(8));
        assert_eq!(Nullable::<u8>::Null.map(|value| value * 2), Nullable::Null);
        assert_eq!(nullable.into_option(), Some(4));
        assert_eq!(Nullable::<u8>::Null.into_option(), None);
    }
}
