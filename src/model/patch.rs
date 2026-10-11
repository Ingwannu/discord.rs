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
//   감싸야 한다.
//
// [Decision Log: 생략된 키 처리]
// - 목적과 의도: 생략된 키가 조용히 null(지우기)이 되지 않게 한다. 필수 null 가능 필드는
//   생략 시 오류, 패치 필드는 #[serde(default)]일 때만 Missing, default를 빠뜨리면 오류.
// - 기존 구현 및 제약 조건: serde derive는 생략된 필드에 대해 __private::de::missing_field로
//   만든 역직렬화기로 Deserialize를 호출한다. 이 역직렬화기는 deserialize_option에만
//   visit_none으로 답하고(Option 필드 생략 = None 규칙) 나머지 요청은 모두 "missing field"
//   오류를 낸다. Option<T>에 위임하면 생략이 null과 구분되지 않는다.
// - 검토한 주요 대안: (1) 문서로만 경고(첫 구현, 리뷰에서 거부), (2) 필드마다
//   deserialize_with를 강제하는 관례와 검사 매크로, (3) 역직렬화 진입을
//   deserialize_newtype_struct로 바꿔 missing_field 폴백이 오류를 내게 하기,
//   (4) serde_json::Value로 버퍼링 후 판별.
// - 선택한 방식: (3). Patch와 Nullable 모두 deserialize_newtype_struct로 들어가고, 실제
//   역직렬화기는 visit_newtype_struct(같은 역직렬화기)로 돌려준다. newtype을
//   deserialize_any로 넘기는 역직렬화기(serde의 value 모듈 등)를 위해 visit_* 폴백을 둔다.
// - 다른 대안 대신 이 방식을 선택한 이유: (2)는 수백 개 필드에 속성 하나만 빠져도 다시
//   조용한 null이 된다. (4)는 JSON 전용이고 할당이 생기며 빌린 문자열을 못 받는다. (3)은
//   타입 자체가 규칙을 강제하고, serde_json·serde_json::Value·serde의 Content(untagged,
//   flatten) 모두 newtype을 투명하게 처리하므로 와이어 형식이 바뀌지 않는다.
// - 장점, 단점 및 영향: 필드 속성은 `default`와 `skip_serializing_if`면 충분하고, 빠뜨리면
//   조용히 틀리는 대신 오류가 난다. serde의 비공개 missing_field 동작(Option만 특별 취급)에
//   기대므로 serde가 이를 바꾸면 absence 테스트가 깨져 알려 준다. 지원 형식은 JSON이다.
//   쓰기는 serialize_none/값 그대로, 읽기는 Option<T>와 newtype 진입이라, option이나
//   newtype을 JSON과 다르게 인코딩하는 형식(Some을 태그하는 형식, RON 기본 설정 등)에서는
//   쓴 것을 그대로 읽지 못할 수 있다.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, value, IntoDeserializer, Visitor};
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
/// Struct fields of type `Patch<T>` (including `NullablePatch<T>`) are
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
/// Both attributes are needed, and forgetting either one fails loudly
/// instead of changing data:
///
/// - `default` makes an absent key read as `Missing`. Without it an absent
///   key is a "missing field" error. It never becomes an explicit `null`,
///   not even for a `NullablePatch`, where `null` means "clear it".
/// - `skip_serializing_if` keeps `Missing` out of the output. A `Missing`
///   that reaches the serializer is an error, never a silent `null`.
///
/// A present key is read through `T`, so `null` is accepted only when `T`
/// accepts it: `Patch<bool>` rejects `null`, while `false`, `0`, `""`, and
/// `[]` are ordinary values.
///
/// [`Nullable<T>`] used on its own (a required field that may be `null`)
/// needs no attribute: `null` reads as `Null`, and an absent key is a
/// "missing field" error rather than `Null`.
///
/// Supported format: JSON through serde_json (from text, a reader, or a
/// `serde_json::Value`), including inside `#[serde(untagged)]`, internally
/// tagged, and `#[serde(flatten)]` containers. Other formats are not a goal.
/// The representation assumes JSON's encoding: `Nullable::Null` is written
/// with `serialize_none` and a value is written bare, but reading goes
/// through `Option<T>` and enters via `deserialize_newtype_struct`. A format
/// whose option or newtype encoding differs (for example one that tags
/// `Some`) may not read back what it wrote.
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
/// Absent keys become `Missing` through `#[serde(default)]`; without that
/// attribute they are a "missing field" error.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_present(deserializer)
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

/// Reads `null` as `Null` and anything else through `T`. An absent struct
/// field is a "missing field" error, not `Null` (unlike `Option<T>`).
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_present(deserializer)
    }
}

/// A field type whose `Deserialize` must reject serde's missing-field
/// fallback (see the "생략된 키 처리" Decision Log at the top of this file).
///
/// serde derive deserializes an absent field (one without
/// `#[serde(default)]`) from a stand-in deserializer that answers only
/// `deserialize_option` (with `visit_none`) and returns a "missing field"
/// error for every other request. These types therefore enter through
/// `deserialize_newtype_struct`: real deserializers hand themselves back via
/// `visit_newtype_struct`, while the stand-in reports the missing field.
trait PresentField<'de>: Sized {
    /// Newtype name passed to `deserialize_newtype_struct`. serde_json and
    /// serde's own deserializers ignore it.
    const NAME: &'static str;

    /// The key is present; `deserializer` may still hold `null`.
    fn from_present<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;

    /// The key is present and holds a non-null value.
    fn from_value<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;

    /// The key is present and holds `null` (seen as unit or none).
    fn from_null<E: de::Error>() -> Result<Self, E>;
}

impl<'de, T: Deserialize<'de>> PresentField<'de> for Patch<T> {
    const NAME: &'static str = "Patch";

    fn from_present<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }

    fn from_value<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }

    fn from_null<E: de::Error>() -> Result<Self, E> {
        // `T` decides whether null is acceptable: `Nullable<_>` says yes,
        // `bool` reports "invalid type: unit value".
        T::deserialize(value::UnitDeserializer::<E>::new()).map(Self::Value)
    }
}

impl<'de, T: Deserialize<'de>> PresentField<'de> for Nullable<T> {
    const NAME: &'static str = "Nullable";

    fn from_present<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(Self::from)
    }

    fn from_value<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }

    fn from_null<E: de::Error>() -> Result<Self, E> {
        Ok(Self::Null)
    }
}

fn deserialize_present<'de, F, D>(deserializer: D) -> Result<F, D::Error>
where
    F: PresentField<'de>,
    D: Deserializer<'de>,
{
    deserializer.deserialize_newtype_struct(F::NAME, PresentVisitor::<F>(PhantomData))
}

/// Feeds an already-visited primitive back into `F` as a deserializer.
fn from_primitive<'de, F, V, E>(value: V) -> Result<F, E>
where
    F: PresentField<'de>,
    V: IntoDeserializer<'de, E>,
    E: de::Error,
{
    F::from_value(value.into_deserializer())
}

struct PresentVisitor<F>(PhantomData<fn() -> F>);

/// `visit_newtype_struct` is the normal path. The other methods are
/// fallbacks for deserializers that answer `deserialize_newtype_struct`
/// through `deserialize_any` (serde's `value` deserializers); each hands the
/// visited value back to `F` unchanged. Every data-model shape a
/// self-describing `deserialize_any` can report is covered, so a type that
/// decodes from such a deserializer on its own also decodes inside these
/// wrappers.
impl<'de, F: PresentField<'de>> Visitor<'de> for PresentVisitor<F> {
    type Value = F;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a present field value")
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<F, D::Error>
    where
        D: Deserializer<'de>,
    {
        F::from_present(deserializer)
    }

    fn visit_unit<E: de::Error>(self) -> Result<F, E> {
        F::from_null()
    }

    fn visit_none<E: de::Error>(self) -> Result<F, E> {
        F::from_null()
    }

    fn visit_some<D>(self, deserializer: D) -> Result<F, D::Error>
    where
        D: Deserializer<'de>,
    {
        F::from_value(deserializer)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<F, E> {
        F::from_value(value::BorrowedStrDeserializer::new(value))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<F, E> {
        from_primitive(value)
    }

    fn visit_borrowed_bytes<E: de::Error>(self, value: &'de [u8]) -> Result<F, E> {
        F::from_value(value::BorrowedBytesDeserializer::new(value))
    }

    fn visit_seq<A>(self, seq: A) -> Result<F, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        F::from_value(value::SeqAccessDeserializer::new(seq))
    }

    fn visit_map<A>(self, map: A) -> Result<F, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        F::from_value(value::MapAccessDeserializer::new(map))
    }

    fn visit_enum<A>(self, data: A) -> Result<F, A::Error>
    where
        A: de::EnumAccess<'de>,
    {
        F::from_value(value::EnumAccessDeserializer::new(data))
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

    fn decode_error<T: for<'de> Deserialize<'de> + std::fmt::Debug>(text: &str) -> String {
        match serde_json::from_str::<T>(text) {
            Ok(value) => panic!("{text} must not decode, got {value:?}"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn required_nullable_field_rejects_absence_instead_of_reading_null() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Snapshot {
            topic: Nullable<String>,
        }

        assert!(decode_error::<Snapshot>("{}").contains("missing field `topic`"));
        let from_value = serde_json::from_value::<Snapshot>(json!({}));
        assert!(
            from_value.is_err_and(|error| error.to_string().contains("missing field `topic`")),
            "absence must also fail when decoding from a serde_json::Value"
        );
        assert_eq!(
            serde_json::from_str::<Snapshot>(r#"{"topic":null}"#).ok(),
            Some(Snapshot {
                topic: Nullable::Null
            })
        );
        assert_eq!(
            serde_json::from_value::<Snapshot>(json!({"topic": "t"})).ok(),
            Some(Snapshot {
                topic: Nullable::Value("t".to_string())
            })
        );
        assert!(serde_json::from_str::<Snapshot>(r#"{"topic":5}"#).is_err());
    }

    #[test]
    fn patch_fields_without_serde_default_reject_absence_instead_of_clearing() {
        // What a migration gets if it drops `#[serde(default)]`: a loud
        // "missing field" error, never Missing-turned-null.
        #[derive(Debug, Deserialize)]
        struct Forgot {
            nick: NullablePatch<String>,
            flag: Patch<bool>,
            // Patch guards absence itself, even around an Option-like `T`.
            limit: Patch<Option<u8>>,
        }

        let cases = [
            (r#"{"flag":true,"limit":1}"#, "missing field `nick`"),
            (r#"{"nick":null,"limit":1}"#, "missing field `flag`"),
            (r#"{"nick":null,"flag":true}"#, "missing field `limit`"),
        ];
        for (text, expected) in cases {
            let message = decode_error::<Forgot>(text);
            assert!(message.contains(expected), "{text}: {message}");
        }

        let present: Forgot =
            match serde_json::from_str(r#"{"nick":null,"flag":false,"limit":null}"#) {
                Ok(present) => present,
                Err(error) => panic!("present keys must decode: {error}"),
            };
        assert_eq!(present.nick, Patch::null());
        assert_eq!(present.flag, Patch::value(false));
        assert_eq!(present.limit, Patch::value(None));
    }

    #[test]
    fn patch_fields_with_serde_default_read_absence_as_missing_from_text() {
        let body: Body = match serde_json::from_str("{}") {
            Ok(body) => body,
            Err(error) => panic!("absent keys must decode as Missing: {error}"),
        };
        assert_eq!(body, Body::default());
        assert!(body.nick.is_missing());
    }

    #[test]
    fn absence_is_detected_through_flatten_and_untagged() {
        #[derive(Debug, Deserialize)]
        struct Inner {
            #[serde(default)]
            nick: NullablePatch<String>,
            topic: Nullable<String>,
        }
        #[derive(Debug, Deserialize)]
        struct Outer {
            #[allow(dead_code, reason = "only the flattened fields are checked")]
            id: u8,
            #[serde(flatten)]
            inner: Inner,
        }
        #[derive(Debug, PartialEq, Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Snapshot { topic: Nullable<String> },
            Count(u8),
        }

        let outer: Outer = match serde_json::from_str(r#"{"id":1,"topic":null}"#) {
            Ok(outer) => outer,
            Err(error) => panic!("flattened fields must decode: {error}"),
        };
        assert!(outer.inner.nick.is_missing());
        assert_eq!(outer.inner.topic, Nullable::Null);
        let cleared: Outer = match serde_json::from_str(r#"{"id":1,"topic":"t","nick":null}"#) {
            Ok(outer) => outer,
            Err(error) => panic!("flattened fields must decode: {error}"),
        };
        assert_eq!(cleared.inner.nick, Patch::null());
        assert!(decode_error::<Outer>(r#"{"id":1}"#).contains("missing field `topic`"));

        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"topic":null}"#).ok(),
            Some(Wire::Snapshot {
                topic: Nullable::Null
            })
        );
        assert_eq!(
            serde_json::from_str::<Wire>(r#"{"topic":"t"}"#).ok(),
            Some(Wire::Snapshot {
                topic: Nullable::Value("t".to_string())
            })
        );
        assert!(
            serde_json::from_str::<Wire>("{}").is_err(),
            "an absent required nullable field must not match the variant as Null"
        );
    }

    #[test]
    fn standalone_values_decode_from_text_and_value_references() {
        assert_eq!(
            serde_json::from_str::<Nullable<u8>>("null").ok(),
            Some(Nullable::Null)
        );
        assert_eq!(
            serde_json::from_str::<NullablePatch<u8>>("null").ok(),
            Some(Patch::null())
        );
        let three = json!(3);
        assert_eq!(
            Nullable::<u8>::deserialize(&three).ok(),
            Some(Nullable::Value(3))
        );
        assert_eq!(Patch::<u8>::deserialize(&three).ok(), Some(Patch::Value(3)));
        assert!(Patch::<bool>::deserialize(&json!(null)).is_err());

        // Typed IDs nest inside both wrappers (e.g. a clearable parent_id).
        let parent = crate::model::ChannelId::try_new(5).ok();
        assert_eq!(
            serde_json::from_str::<NullablePatch<crate::model::ChannelId>>(r#""5""#).ok(),
            parent.map(|id| Patch::value(Nullable::Value(id)))
        );
        assert!(serde_json::from_str::<Nullable<crate::model::ChannelId>>(r#""0""#).is_err());
    }

    /// A deserializer whose `deserialize_any` reports a present optional,
    /// to reach the `visit_some` fallback.
    struct SomeDeserializer<D>(D);

    impl<'de, D: serde::Deserializer<'de>> serde::Deserializer<'de> for SomeDeserializer<D> {
        type Error = D::Error;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, D::Error>
        where
            V: serde::de::Visitor<'de>,
        {
            visitor.visit_some(self.0)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    #[test]
    fn fallback_path_serves_deserializers_that_route_newtypes_to_any() {
        use std::collections::HashMap;

        use serde::de::value::{
            BoolDeserializer, BorrowedBytesDeserializer, BorrowedStrDeserializer,
            BytesDeserializer, EnumAccessDeserializer, Error as ValueError, F64Deserializer,
            I128Deserializer, I64Deserializer, MapDeserializer, SeqDeserializer, StrDeserializer,
            StringDeserializer, U128Deserializer, U64Deserializer, UnitDeserializer,
        };

        // Null / unit.
        assert_eq!(
            Nullable::<u8>::deserialize(UnitDeserializer::<ValueError>::new()).ok(),
            Some(Nullable::Null)
        );
        assert_eq!(
            NullablePatch::<u8>::deserialize(UnitDeserializer::<ValueError>::new()).ok(),
            Some(Patch::null())
        );
        assert!(Patch::<bool>::deserialize(UnitDeserializer::<ValueError>::new()).is_err());

        // Primitives.
        assert_eq!(
            Patch::<bool>::deserialize(BoolDeserializer::<ValueError>::new(true)).ok(),
            Some(Patch::Value(true))
        );
        assert_eq!(
            Nullable::<i64>::deserialize(I64Deserializer::<ValueError>::new(-3)).ok(),
            Some(Nullable::Value(-3))
        );
        assert_eq!(
            Nullable::<u64>::deserialize(U64Deserializer::<ValueError>::new(7)).ok(),
            Some(Nullable::Value(7))
        );
        assert_eq!(
            Nullable::<f64>::deserialize(F64Deserializer::<ValueError>::new(1.5)).ok(),
            Some(Nullable::Value(1.5))
        );

        // Strings and bytes, owned and borrowed.
        assert_eq!(
            Nullable::<String>::deserialize(StrDeserializer::<ValueError>::new("a")).ok(),
            Some(Nullable::Value("a".to_string()))
        );
        assert_eq!(
            Nullable::<String>::deserialize(StringDeserializer::<ValueError>::new("b".into())).ok(),
            Some(Nullable::Value("b".to_string()))
        );
        assert_eq!(
            Nullable::<String>::deserialize(BytesDeserializer::<ValueError>::new(b"c")).ok(),
            Some(Nullable::Value("c".to_string()))
        );
        assert_eq!(
            Nullable::<&str>::deserialize(BorrowedStrDeserializer::<ValueError>::new("d")).ok(),
            Some(Nullable::Value("d"))
        );
        assert_eq!(
            Nullable::<&str>::deserialize(BorrowedBytesDeserializer::<ValueError>::new(b"e")).ok(),
            Some(Nullable::Value("e"))
        );

        // Sequences, maps, and a present optional.
        let seq = SeqDeserializer::<_, ValueError>::new(vec![1_u8, 2].into_iter());
        assert_eq!(
            Nullable::<Vec<u8>>::deserialize(seq).ok(),
            Some(Nullable::Value(vec![1, 2]))
        );
        let map = MapDeserializer::<_, ValueError>::new(vec![("k".to_string(), 1_u8)].into_iter());
        assert_eq!(
            Nullable::<HashMap<String, u8>>::deserialize(map).ok(),
            Some(Nullable::Value(HashMap::from([("k".to_string(), 1)])))
        );
        assert_eq!(
            Nullable::<u8>::deserialize(SomeDeserializer(U64Deserializer::<ValueError>::new(9)))
                .ok(),
            Some(Nullable::Value(9))
        );
        assert_eq!(
            Patch::<u8>::deserialize(SomeDeserializer(U64Deserializer::<ValueError>::new(9))).ok(),
            Some(Patch::Value(9))
        );

        // 128-bit integers and enums (regressions: these had no fallback).
        assert_eq!(
            Patch::<u128>::deserialize(U128Deserializer::<ValueError>::new(u128::MAX)).ok(),
            Some(Patch::Value(u128::MAX))
        );
        assert_eq!(
            Nullable::<i128>::deserialize(I128Deserializer::<ValueError>::new(i128::MIN)).ok(),
            Some(Nullable::Value(i128::MIN))
        );
        #[derive(Debug, PartialEq, Deserialize)]
        enum Color {
            Red,
            Green,
        }
        let red = EnumAccessDeserializer::new(StrDeserializer::<ValueError>::new("Red"));
        assert_eq!(
            Patch::<Color>::deserialize(red).ok(),
            Some(Patch::Value(Color::Red))
        );
        let green = EnumAccessDeserializer::new(StrDeserializer::<ValueError>::new("Green"));
        assert_eq!(
            Nullable::<Color>::deserialize(green).ok(),
            Some(Nullable::Value(Color::Green))
        );
        let unknown = EnumAccessDeserializer::new(StrDeserializer::<ValueError>::new("Blue"));
        assert!(Patch::<Color>::deserialize(unknown).is_err());

        // Error messages name what was expected.
        let error: ValueError = serde::de::Error::invalid_type(
            serde::de::Unexpected::Bool(true),
            &super::PresentVisitor::<Nullable<u8>>(std::marker::PhantomData),
        );
        assert!(error.to_string().contains("expected a present field value"));
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
