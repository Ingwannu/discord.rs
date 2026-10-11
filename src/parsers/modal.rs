//! Submitted modal values (`MODAL_SUBMIT` interaction `data`).
//!
//! `V2ModalSubmission` and `V2ModalComponent` read and write Discord's wire
//! format directly: the decoder walks the submitted component tree (labels,
//! and action rows of older modals) as it is parsed and keeps only the
//! leaves, with no intermediate `serde_json::Value`.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 모달 제출 값을 JSON에서 바로 읽는다. 인터랙션 디코더(직접 디코딩)가 이
//!   경로를 쓰므로 Value 트리와 data 복제를 없애야 4단계 할당 목표를 맞춘다.
//! - 기존 구현 및 제약 조건: 2.x/3.0 초기 파서는 `&Value`를 걸으며 문자열을 복사했다. 키
//!   순서는 보장되지 않아 노드의 `type`이 `value`·`components`보다 뒤에 올 수 있다. 알 수 없는
//!   컴포넌트 타입·숫자가 아닌 `type`·객체가 아닌 노드는 무시하고, 문자열 필드는 숫자도
//!   받는(`"5"`/`5`) 관대한 규칙이 테스트로 고정돼 있다. 라디오 그룹 값은 문자열 또는 null만.
//! - 검토한 주요 대안: (1) 노드를 Value로 버퍼링 후 기존 로직, (2) 노드마다 RawValue로 잡고
//!   `type`을 본 뒤 다시 파싱, (3) 모든 키를 타입 무관하게 작은 중간 표현(LeafValue)으로 읽고
//!   맵 끝에서 `type`으로 판정, 자식 잎은 시드로 바로 결과 Vec에 넣기.
//! - 선택한 방식: (3).
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 없애려는 트리 그대로이고, (2)는 노드마다
//!   복사와 두 번 파싱이 든다. (3)은 한 번 읽고, 잎이 아닌 키는 할당 없이 건너뛴다.
//! - 장점, 단점 및 영향: 와이어 형식 Serialize/Deserialize(2.x의 외부 태그 형식은 더 읽지
//!   않음). 알 수 없는 컨테이너 타입의 자식 입력도 이제 수집된다(전에는 통째로 무시). 오류
//!   메시지는 같은 문구를 쓰되 serde 오류로 전달된다.
//! ```

use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{ComponentType, ResolvedData};
use crate::types::invalid_data_error;

use super::required_object_field;

#[derive(Clone, Debug, PartialEq, Eq)]
/// One submitted input of a modal (wire: a component object with
/// `custom_id` and `value`/`values`).
pub enum V2ModalComponent {
    /// Discord API enum variant `TextInput`.
    TextInput {
        /// Component custom ID.
        custom_id: String,
        /// Submitted text value.
        value: String,
    },
    /// Discord API enum variant `StringSelect`.
    StringSelect {
        /// Component custom ID.
        custom_id: String,
        /// Submitted select values.
        values: Vec<String>,
    },
    /// Discord API enum variant `UserSelect`.
    UserSelect {
        /// Component custom ID.
        custom_id: String,
        /// Submitted selected user IDs.
        values: Vec<String>,
    },
    /// Discord API enum variant `RoleSelect`.
    RoleSelect {
        /// Component custom ID.
        custom_id: String,
        /// Submitted selected role IDs.
        values: Vec<String>,
    },
    /// Discord API enum variant `ChannelSelect`.
    ChannelSelect {
        /// Component custom ID.
        custom_id: String,
        /// Submitted selected channel IDs.
        values: Vec<String>,
    },
    /// Discord API enum variant `MentionableSelect`.
    MentionableSelect {
        /// Component custom ID.
        custom_id: String,
        /// Submitted selected mentionable IDs.
        values: Vec<String>,
    },
    /// A submitted radio group (wire `value: ?string`).
    RadioGroup {
        /// Component custom ID.
        custom_id: String,
        /// The selected option's value, or `None` when the user selected
        /// nothing (Discord sends `null`; possible when the group is not
        /// `required`).
        value: Option<String>,
    },
    /// A submitted checkbox group (wire `values`).
    CheckboxGroup {
        /// Component custom ID.
        custom_id: String,
        /// Values of the checked options; empty when none is checked.
        values: Vec<String>,
    },
    /// Discord API enum variant `FileUpload`.
    FileUpload {
        /// Component custom ID.
        custom_id: String,
        /// Uploaded attachment IDs; the files themselves are in the
        /// interaction's resolved data
        /// ([`ModalSubmitInteraction::get_attachments`](crate::model::ModalSubmitInteraction::get_attachments)).
        values: Vec<String>,
    },
    /// A submitted checkbox (wire `value: bool`).
    Checkbox {
        /// Component custom ID.
        custom_id: String,
        /// Whether the checkbox was checked: the submitted `value`. (2.x
        /// read a `checked` key Discord never sends, so every checkbox
        /// submission failed to parse.)
        checked: bool,
    },
}

impl V2ModalComponent {
    /// The component's `custom_id`.
    pub fn custom_id(&self) -> &str {
        match self {
            Self::TextInput { custom_id, .. }
            | Self::StringSelect { custom_id, .. }
            | Self::UserSelect { custom_id, .. }
            | Self::RoleSelect { custom_id, .. }
            | Self::ChannelSelect { custom_id, .. }
            | Self::MentionableSelect { custom_id, .. }
            | Self::RadioGroup { custom_id, .. }
            | Self::CheckboxGroup { custom_id, .. }
            | Self::FileUpload { custom_id, .. }
            | Self::Checkbox { custom_id, .. } => custom_id,
        }
    }

    /// The component type this submission came from.
    pub fn component_type(&self) -> ComponentType {
        match self {
            Self::TextInput { .. } => ComponentType::TEXT_INPUT,
            Self::StringSelect { .. } => ComponentType::STRING_SELECT,
            Self::UserSelect { .. } => ComponentType::USER_SELECT,
            Self::RoleSelect { .. } => ComponentType::ROLE_SELECT,
            Self::ChannelSelect { .. } => ComponentType::CHANNEL_SELECT,
            Self::MentionableSelect { .. } => ComponentType::MENTIONABLE_SELECT,
            Self::RadioGroup { .. } => ComponentType::RADIO_GROUP,
            Self::CheckboxGroup { .. } => ComponentType::CHECKBOX_GROUP,
            Self::FileUpload { .. } => ComponentType::FILE_UPLOAD,
            Self::Checkbox { .. } => ComponentType::CHECKBOX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// The submitted inputs of a modal: its `custom_id` and every input
/// component, in order, with the label and action row wrappers removed.
pub struct V2ModalSubmission {
    pub custom_id: String,
    pub components: Vec<V2ModalComponent>,
}

impl V2ModalSubmission {
    pub fn get_text(&self, custom_id: &str) -> Option<&str> {
        self.components
            .iter()
            .find_map(|component| match component {
                V2ModalComponent::TextInput {
                    custom_id: component_custom_id,
                    value,
                } if component_custom_id == custom_id => Some(value.as_str()),
                _ => None,
            })
    }

    pub fn get_select_values(&self, custom_id: &str) -> Option<&[String]> {
        self.components
            .iter()
            .find_map(|component| match component {
                V2ModalComponent::StringSelect {
                    custom_id: component_custom_id,
                    values,
                }
                | V2ModalComponent::UserSelect {
                    custom_id: component_custom_id,
                    values,
                }
                | V2ModalComponent::RoleSelect {
                    custom_id: component_custom_id,
                    values,
                }
                | V2ModalComponent::ChannelSelect {
                    custom_id: component_custom_id,
                    values,
                }
                | V2ModalComponent::MentionableSelect {
                    custom_id: component_custom_id,
                    values,
                }
                | V2ModalComponent::CheckboxGroup {
                    custom_id: component_custom_id,
                    values,
                } if component_custom_id == custom_id => Some(values.as_slice()),
                _ => None,
            })
    }

    pub fn get_file_values(&self, custom_id: &str) -> Option<&[String]> {
        self.components
            .iter()
            .find_map(|component| match component {
                V2ModalComponent::FileUpload {
                    custom_id: component_custom_id,
                    values,
                } if component_custom_id == custom_id => Some(values.as_slice()),
                _ => None,
            })
    }

    /// The selected option of radio group `custom_id`. `None` when there
    /// is no such radio group or nothing was selected.
    pub fn get_radio_value(&self, custom_id: &str) -> Option<&str> {
        self.components
            .iter()
            .find_map(|component| match component {
                V2ModalComponent::RadioGroup {
                    custom_id: component_custom_id,
                    value,
                } if component_custom_id == custom_id => value.as_deref(),
                _ => None,
            })
    }

    /// Whether checkbox `custom_id` was checked, or `None` when there is no
    /// such checkbox.
    pub fn get_checkbox(&self, custom_id: &str) -> Option<bool> {
        self.components
            .iter()
            .find_map(|component| match component {
                V2ModalComponent::Checkbox {
                    custom_id: component_custom_id,
                    checked,
                } if component_custom_id == custom_id => Some(*checked),
                _ => None,
            })
    }
}

/// Reads a modal submit's `data` object, or a whole interaction payload
/// whose `data` is one, from a `serde_json::Value`.
///
/// The submitted values only: the `resolved` attachments, users, roles, and
/// channels are on [`crate::model::ModalSubmitInteraction`], which
/// [`crate::decode_interaction`] returns. Decoding JSON text directly with
/// `serde_json::from_str::<V2ModalSubmission>` skips the `Value` tree.
pub fn parse_modal_submission(data: &Value) -> Result<V2ModalSubmission, DiscordError> {
    let modal_data = if data.get("custom_id").is_some() && data.get("components").is_some() {
        data
    } else {
        required_object_field(data, "data", "interaction")?
    };
    V2ModalSubmission::deserialize(modal_data)
        .map_err(|error| invalid_data_error(error.to_string()))
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

/// Writes Discord's modal submit `data` without `resolved` (that lives on
/// the interaction): `custom_id` and the leaves as top-level components,
/// which this decoder reads back unchanged.
impl Serialize for V2ModalSubmission {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ModalDataRef {
            submission: self,
            resolved: None,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for V2ModalSubmission {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ModalDataSeed {
            keep_resolved: false,
        }
        .deserialize(deserializer)
        .map(|(submission, _)| submission)
    }
}

/// Writes one leaf as Discord sends it: `type`, `custom_id`, and `value` or
/// `values`.
impl Serialize for V2ModalComponent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("type", &self.component_type())?;
        map.serialize_entry("custom_id", self.custom_id())?;
        match self {
            Self::TextInput { value, .. } => map.serialize_entry("value", value)?,
            Self::RadioGroup { value, .. } => map.serialize_entry("value", value)?,
            Self::Checkbox { checked, .. } => map.serialize_entry("value", checked)?,
            Self::StringSelect { values, .. }
            | Self::UserSelect { values, .. }
            | Self::RoleSelect { values, .. }
            | Self::ChannelSelect { values, .. }
            | Self::MentionableSelect { values, .. }
            | Self::CheckboxGroup { values, .. }
            | Self::FileUpload { values, .. } => map.serialize_entry("values", values)?,
        }
        map.end()
    }
}

/// Reads one submitted input. A label (or action row) holding exactly one
/// input reads as that input; anything else is an error.
impl<'de> Deserialize<'de> for V2ModalComponent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut leaves = Vec::with_capacity(1);
        NodeSeed {
            leaves: &mut leaves,
        }
        .deserialize(deserializer)?;
        let mut leaves = leaves.into_iter();
        match (leaves.next(), leaves.next()) {
            (Some(leaf), None) => Ok(leaf),
            _ => Err(de::Error::custom(
                "expected one submitted modal component (text input, select, radio group, \
                 checkbox group, checkbox, or file upload)",
            )),
        }
    }
}

/// Borrowed view used to write a modal submit `data` object, optionally
/// with the interaction's `resolved` data.
pub(crate) struct ModalDataRef<'a> {
    pub(crate) submission: &'a V2ModalSubmission,
    pub(crate) resolved: Option<&'a ResolvedData>,
}

impl Serialize for ModalDataRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("custom_id", &self.submission.custom_id)?;
        map.serialize_entry("components", &self.submission.components)?;
        if let Some(resolved) = self.resolved {
            map.serialize_entry("resolved", resolved)?;
        }
        map.end()
    }
}

/// Decodes a modal submit `data` object into the submission and, when
/// `keep_resolved` is set, its `resolved` data (skipped otherwise, without
/// allocating).
pub(crate) struct ModalDataSeed {
    pub(crate) keep_resolved: bool,
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum ModalDataField {
    CustomId,
    Components,
    Resolved,
    #[serde(other)]
    Other,
}

impl<'de> DeserializeSeed<'de> for ModalDataSeed {
    type Value = (V2ModalSubmission, Option<ResolvedData>);

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ModalDataSeed {
    type Value = (V2ModalSubmission, Option<ResolvedData>);

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a modal submit data object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut custom_id = None;
        let mut components = None;
        let mut resolved = None;
        while let Some(field) = map.next_key::<ModalDataField>()? {
            match field {
                ModalDataField::CustomId => {
                    reject_duplicate(&custom_id, "custom_id")?;
                    custom_id = Some(map.next_value::<LeafValue>()?);
                }
                ModalDataField::Components => {
                    reject_duplicate(&components, "components")?;
                    let mut leaves = Vec::new();
                    let is_array = map.next_value_seed(ChildrenSeed {
                        leaves: &mut leaves,
                    })?;
                    components = Some(is_array.then_some(leaves));
                }
                ModalDataField::Resolved if self.keep_resolved => {
                    reject_duplicate(&resolved, "resolved")?;
                    resolved = Some(map.next_value::<Option<ResolvedData>>()?);
                }
                ModalDataField::Resolved | ModalDataField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        let custom_id = custom_id
            .and_then(LeafValue::into_text)
            .ok_or_else(|| de::Error::custom("missing or invalid modal_data.custom_id"))?;
        let components = components
            .flatten()
            .ok_or_else(|| de::Error::custom("missing or invalid modal_data.components"))?;
        Ok((
            V2ModalSubmission {
                custom_id,
                components,
            },
            resolved.flatten(),
        ))
    }
}

fn reject_duplicate<T, E: de::Error>(slot: &Option<T>, field: &'static str) -> Result<(), E> {
    match slot {
        Some(_) => Err(E::duplicate_field(field)),
        None => Ok(()),
    }
}

/// Decodes an array of component nodes, pushing every submitted input into
/// `leaves`. Returns `false` (after skipping it) when the value is not an
/// array, so the caller can name the field in its error.
struct ChildrenSeed<'a> {
    leaves: &'a mut Vec<V2ModalComponent>,
}

impl<'de> DeserializeSeed<'de> for ChildrenSeed<'_> {
    type Value = bool;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<bool, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ChildrenSeed<'_> {
    type Value = bool;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array of components")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<bool, A::Error> {
        while seq
            .next_element_seed(NodeSeed {
                leaves: &mut *self.leaves,
            })?
            .is_some()
        {}
        Ok(true)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<bool, A::Error> {
        skip_map(map).map(|()| false)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<bool, E> {
        Ok(false)
    }

    fn visit_unit<E: de::Error>(self) -> Result<bool, E> {
        Ok(false)
    }
}

fn skip_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

/// Decodes one component node. Inputs are pushed into `leaves` when the
/// node ends; children of labels (`component`) and action rows
/// (`components`) are pushed as they are read, so the order matches the
/// modal. Nodes that are not objects, have no numeric `type`, or have a
/// type that submits nothing (text display, ...) are skipped.
struct NodeSeed<'a> {
    leaves: &'a mut Vec<V2ModalComponent>,
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum NodeField {
    Type,
    CustomId,
    Value,
    Values,
    Components,
    Component,
    #[serde(other)]
    Other,
}

impl<'de> DeserializeSeed<'de> for NodeSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NodeSeed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a component object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut kind = None;
        let mut custom_id = None;
        let mut value = None;
        let mut values = None;
        let mut children_array = None;
        let mut has_child = false;
        while let Some(field) = map.next_key::<NodeField>()? {
            match field {
                NodeField::Type => kind = Some(map.next_value::<LenientU8>()?.0),
                NodeField::CustomId => custom_id = Some(map.next_value::<LeafValue>()?),
                NodeField::Value => value = Some(map.next_value::<LeafValue>()?),
                NodeField::Values => values = Some(map.next_value::<LeafValues>()?),
                NodeField::Components => {
                    children_array = Some(map.next_value_seed(ChildrenSeed {
                        leaves: &mut *self.leaves,
                    })?);
                }
                NodeField::Component => {
                    map.next_value_seed(NodeSeed {
                        leaves: &mut *self.leaves,
                    })?;
                    has_child = true;
                }
                NodeField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }

        let Some(kind) = kind.flatten().map(ComponentType::new) else {
            return Ok(());
        };
        let leaf = match kind {
            ComponentType::ACTION_ROW => {
                return match children_array {
                    Some(true) => Ok(()),
                    _ => Err(de::Error::custom(
                        "missing or invalid action_row.components",
                    )),
                };
            }
            ComponentType::LABEL => {
                return if has_child {
                    Ok(())
                } else {
                    Err(de::Error::custom("missing label.component"))
                };
            }
            ComponentType::TEXT_INPUT => V2ModalComponent::TextInput {
                custom_id: required_text(custom_id, "text_input.custom_id")?,
                value: required_text(value, "text_input.value")?,
            },
            ComponentType::STRING_SELECT => V2ModalComponent::StringSelect {
                custom_id: required_text(custom_id, "string_select.custom_id")?,
                values: required_values(values, "string_select")?,
            },
            ComponentType::USER_SELECT => V2ModalComponent::UserSelect {
                custom_id: required_text(custom_id, "user_select.custom_id")?,
                values: required_values(values, "user_select")?,
            },
            ComponentType::ROLE_SELECT => V2ModalComponent::RoleSelect {
                custom_id: required_text(custom_id, "role_select.custom_id")?,
                values: required_values(values, "role_select")?,
            },
            ComponentType::CHANNEL_SELECT => V2ModalComponent::ChannelSelect {
                custom_id: required_text(custom_id, "channel_select.custom_id")?,
                values: required_values(values, "channel_select")?,
            },
            ComponentType::MENTIONABLE_SELECT => V2ModalComponent::MentionableSelect {
                custom_id: required_text(custom_id, "mentionable_select.custom_id")?,
                values: required_values(values, "mentionable_select")?,
            },
            // Discord documents the value as `?string`: only a string or
            // null; a missing key reads as nothing selected.
            ComponentType::RADIO_GROUP => V2ModalComponent::RadioGroup {
                custom_id: required_text(custom_id, "radio_group.custom_id")?,
                value: match value {
                    None | Some(LeafValue::Null) => None,
                    Some(LeafValue::Str(text)) => Some(text),
                    Some(_) => {
                        return Err(de::Error::custom(
                            "invalid radio_group.value: expected a string or null",
                        ))
                    }
                },
            },
            ComponentType::CHECKBOX_GROUP => V2ModalComponent::CheckboxGroup {
                custom_id: required_text(custom_id, "checkbox_group.custom_id")?,
                values: required_values(values, "checkbox_group")?,
            },
            ComponentType::FILE_UPLOAD => V2ModalComponent::FileUpload {
                custom_id: required_text(custom_id, "file_upload.custom_id")?,
                values: required_values(values, "file_upload")?,
            },
            // Discord sends the state as `value` (a boolean); the variant
            // keeps the more descriptive name `checked`.
            ComponentType::CHECKBOX => V2ModalComponent::Checkbox {
                custom_id: required_text(custom_id, "checkbox.custom_id")?,
                checked: match value {
                    Some(LeafValue::Bool(checked)) => checked,
                    _ => return Err(de::Error::custom("missing or invalid checkbox.value")),
                },
            },
            _ => return Ok(()),
        };
        self.leaves.push(leaf);
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
}

fn required_text<E: de::Error>(value: Option<LeafValue>, field: &str) -> Result<String, E> {
    value
        .and_then(LeafValue::into_text)
        .ok_or_else(|| E::custom(format!("missing or invalid {field}")))
}

fn required_values<E: de::Error>(
    values: Option<LeafValues>,
    context: &str,
) -> Result<Vec<String>, E> {
    match values {
        Some(LeafValues::Strings(values)) => Ok(values),
        Some(LeafValues::NonString) => {
            Err(E::custom(format!("{context}.values must contain strings")))
        }
        Some(LeafValues::NotArray) | None => {
            Err(E::custom(format!("missing or invalid {context}.values")))
        }
    }
}

/// A component `type` that reads as `None` instead of failing when it is
/// not a number (or numeric string) that fits `u8`, so an unreadable node
/// is skipped like an unknown one.
struct LenientU8(Option<u8>);

impl<'de> Deserialize<'de> for LenientU8 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LenientU8Visitor;

        impl<'de> Visitor<'de> for LenientU8Visitor {
            type Value = LenientU8;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a component type")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LenientU8, E> {
                Ok(LenientU8(u8::try_from(value).ok()))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<LenientU8, E> {
                Ok(LenientU8(u8::try_from(value).ok()))
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<LenientU8, E> {
                Ok(LenientU8(None))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LenientU8, E> {
                Ok(LenientU8(value.parse().ok()))
            }

            fn visit_bool<E: de::Error>(self, _: bool) -> Result<LenientU8, E> {
                Ok(LenientU8(None))
            }

            fn visit_unit<E: de::Error>(self) -> Result<LenientU8, E> {
                Ok(LenientU8(None))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LenientU8, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(LenientU8(None))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<LenientU8, A::Error> {
                skip_map(map).map(|()| LenientU8(None))
            }
        }

        deserializer.deserialize_any(LenientU8Visitor)
    }
}

/// A leaf's `custom_id` or `value` as received, before the component type
/// says how to read it. Numbers keep their text: 2.x accepted `5` where a
/// string was expected, and the tests pin that.
enum LeafValue {
    Str(String),
    Number(String),
    Bool(bool),
    Null,
    Other,
}

impl LeafValue {
    /// A string or a number's text.
    fn into_text(self) -> Option<String> {
        match self {
            Self::Str(text) | Self::Number(text) => Some(text),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for LeafValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LeafValueVisitor;

        impl<'de> Visitor<'de> for LeafValueVisitor {
            type Value = LeafValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a submitted component value")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LeafValue, E> {
                Ok(LeafValue::Str(value.to_owned()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<LeafValue, E> {
                Ok(LeafValue::Str(value))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LeafValue, E> {
                Ok(LeafValue::Number(value.to_string()))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<LeafValue, E> {
                Ok(LeafValue::Number(value.to_string()))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<LeafValue, E> {
                Ok(LeafValue::Number(value.to_string()))
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<LeafValue, E> {
                Ok(LeafValue::Bool(value))
            }

            fn visit_unit<E: de::Error>(self) -> Result<LeafValue, E> {
                Ok(LeafValue::Null)
            }

            fn visit_none<E: de::Error>(self) -> Result<LeafValue, E> {
                Ok(LeafValue::Null)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LeafValue, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(LeafValue::Other)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<LeafValue, A::Error> {
                skip_map(map).map(|()| LeafValue::Other)
            }
        }

        deserializer.deserialize_any(LeafValueVisitor)
    }
}

/// A leaf's `values` list: its strings (and numbers' text), or which way
/// it was malformed.
enum LeafValues {
    Strings(Vec<String>),
    NonString,
    NotArray,
}

impl<'de> Deserialize<'de> for LeafValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LeafValuesVisitor;

        impl<'de> Visitor<'de> for LeafValuesVisitor {
            type Value = LeafValues;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an array of submitted values")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LeafValues, A::Error> {
                let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                let mut all_text = true;
                while let Some(entry) = seq.next_element::<LeafValue>()? {
                    match entry.into_text() {
                        Some(text) if all_text => values.push(text),
                        Some(_) => {}
                        None => all_text = false,
                    }
                }
                Ok(if all_text {
                    LeafValues::Strings(values)
                } else {
                    LeafValues::NonString
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<LeafValues, A::Error> {
                skip_map(map).map(|()| LeafValues::NotArray)
            }

            fn visit_str<E: de::Error>(self, _: &str) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }

            fn visit_u64<E: de::Error>(self, _: u64) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }

            fn visit_i64<E: de::Error>(self, _: i64) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }

            fn visit_f64<E: de::Error>(self, _: f64) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }

            fn visit_bool<E: de::Error>(self, _: bool) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }

            fn visit_unit<E: de::Error>(self) -> Result<LeafValues, E> {
                Ok(LeafValues::NotArray)
            }
        }

        deserializer.deserialize_any(LeafValuesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_modal_submission, V2ModalComponent, V2ModalSubmission};

    #[test]
    fn parse_modal_submission_preserves_file_upload_values() {
        let payload = json!({
            "type": 5,
            "data": {
                "custom_id": "bug_submit_modal",
                "components": [
                    {
                        "type": 18,
                        "label": "Screenshot",
                        "component": {
                            "type": 19,
                            "custom_id": "file_upload",
                            "values": ["123", "456"]
                        }
                    }
                ]
            }
        });

        let submission = parse_modal_submission(&payload).expect("modal submit should parse");

        assert_eq!(submission.custom_id, "bug_submit_modal");
        assert_eq!(
            submission.get_file_values("file_upload"),
            Some(&["123".to_string(), "456".to_string()][..])
        );
        assert_eq!(
            submission.components,
            vec![V2ModalComponent::FileUpload {
                custom_id: "file_upload".to_string(),
                values: vec!["123".to_string(), "456".to_string()],
            }]
        );
    }

    #[test]
    fn parse_modal_submission_keeps_existing_component_types() {
        let payload = json!({
            "custom_id": "mixed_modal",
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": "summary",
                            "value": "details"
                        }
                    ]
                },
                {
                    "type": 18,
                    "label": "Logs",
                    "component": {
                        "type": 19,
                        "custom_id": "attachments",
                        "values": ["789"]
                    }
                }
            ]
        });

        let submission = parse_modal_submission(&payload).expect("mixed modal should parse");

        assert_eq!(submission.get_text("summary"), Some("details"));
        assert_eq!(
            submission.get_file_values("attachments"),
            Some(&["789".to_string()][..])
        );
    }

    #[test]
    fn parse_modal_submission_covers_select_radio_checkbox_and_lookup_helpers() {
        let payload = json!({
            "custom_id": "settings_modal",
            "components": [
                {
                    "type": 18,
                    "label": "Roles",
                    "component": {
                        "type": 6,
                        "custom_id": "roles",
                        "values": ["mod", "admin"]
                    }
                },
                {
                    "type": 18,
                    "label": "Radio",
                    "component": {
                        "type": 21,
                        "custom_id": "visibility",
                        "value": "public"
                    }
                },
                {
                    "type": 18,
                    "label": "Checkbox",
                    "component": {
                        "type": 23,
                        "custom_id": "enabled",
                        "value": true
                    }
                }
            ]
        });

        let submission = parse_modal_submission(&payload).unwrap();
        assert_eq!(
            submission.get_select_values("roles"),
            Some(&["mod".to_string(), "admin".to_string()][..])
        );
        assert_eq!(submission.get_radio_value("visibility"), Some("public"));
        assert_eq!(submission.get_checkbox("enabled"), Some(true));
        assert_eq!(submission.get_checkbox("visibility"), None);
        assert_eq!(submission.get_text("roles"), None);
        assert_eq!(
            submission.components,
            vec![
                V2ModalComponent::RoleSelect {
                    custom_id: "roles".to_string(),
                    values: vec!["mod".to_string(), "admin".to_string()],
                },
                V2ModalComponent::RadioGroup {
                    custom_id: "visibility".to_string(),
                    value: Some("public".to_string()),
                },
                V2ModalComponent::Checkbox {
                    custom_id: "enabled".to_string(),
                    checked: true,
                }
            ]
        );
    }

    #[test]
    fn parse_modal_submission_ignores_unknown_leaf_components_and_reports_missing_label_component()
    {
        let payload = json!({
            "custom_id": "mixed_modal",
            "components": [
                {
                    "type": 18,
                    "label": "Known",
                    "component": {
                        "type": 4,
                        "custom_id": "summary",
                        "value": "details"
                    }
                },
                {
                    "type": 255,
                    "custom_id": "unknown"
                }
            ]
        });

        let submission = parse_modal_submission(&payload).unwrap();
        assert_eq!(submission.get_text("summary"), Some("details"));
        assert_eq!(submission.components.len(), 1);

        let error = parse_modal_submission(&json!({
            "custom_id": "broken",
            "components": [{
                "type": 18,
                "label": "Missing nested component"
            }]
        }))
        .unwrap_err();
        assert!(error.to_string().contains("missing label.component"));
    }

    #[test]
    fn parse_modal_submission_covers_remaining_select_types_and_accessor_fallbacks() {
        let payload = json!({
            "custom_id": "picker_modal",
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 3,
                            "custom_id": "strings",
                            "values": ["alpha", "beta"]
                        },
                        {
                            "type": 5,
                            "custom_id": "users",
                            "values": ["100"]
                        },
                        {
                            "type": 8,
                            "custom_id": "channels",
                            "values": ["200"]
                        },
                        {
                            "type": 7,
                            "custom_id": "mentionables",
                            "values": ["300", "400"]
                        },
                        {
                            "type": 22,
                            "custom_id": "checks",
                            "values": ["x", "y"]
                        },
                        {
                            "type": "invalid"
                        }
                    ]
                }
            ]
        });

        let submission = parse_modal_submission(&payload).unwrap();
        assert_eq!(
            submission.get_select_values("strings"),
            Some(&["alpha".to_string(), "beta".to_string()][..])
        );
        assert_eq!(
            submission.get_select_values("users"),
            Some(&["100".to_string()][..])
        );
        assert_eq!(
            submission.get_select_values("channels"),
            Some(&["200".to_string()][..])
        );
        assert_eq!(
            submission.get_select_values("mentionables"),
            Some(&["300".to_string(), "400".to_string()][..])
        );
        assert_eq!(
            submission.get_select_values("checks"),
            Some(&["x".to_string(), "y".to_string()][..])
        );
        assert_eq!(submission.get_text("strings"), None);
        assert_eq!(submission.get_radio_value("strings"), None);
        assert_eq!(submission.get_file_values("strings"), None);
        assert_eq!(submission.get_select_values("missing"), None);
        assert_eq!(submission.components.len(), 5);
    }

    #[test]
    fn unchecked_checkboxes_and_empty_selections_parse() {
        // Discord: a checkbox's `value` is `false` when unchecked, a radio
        // group's `value` is `null` when nothing is selected, and a checkbox
        // group's `values` is `[]` when nothing is checked.
        let submission = parse_modal_submission(&json!({
            "custom_id": "optional_modal",
            "components": [
                { "type": 18, "id": 1, "component": {
                    "type": 23, "id": 2, "custom_id": "newsletter", "value": false
                }},
                { "type": 18, "id": 3, "component": {
                    "type": 21, "id": 4, "custom_id": "class", "value": null
                }},
                { "type": 18, "id": 5, "component": {
                    "type": 22, "id": 6, "custom_id": "days", "values": []
                }},
                { "type": 18, "id": 7, "component": {
                    "type": 21, "id": 8, "custom_id": "legacy_radio"
                }}
            ]
        }))
        .unwrap();
        assert_eq!(submission.get_checkbox("newsletter"), Some(false));
        assert_eq!(submission.get_radio_value("class"), None);
        assert_eq!(submission.get_select_values("days"), Some(&[][..]));
        assert_eq!(
            submission.components[1],
            V2ModalComponent::RadioGroup {
                custom_id: "class".to_string(),
                value: None,
            }
        );
        assert_eq!(
            submission.components[3],
            V2ModalComponent::RadioGroup {
                custom_id: "legacy_radio".to_string(),
                value: None,
            }
        );
    }

    #[test]
    fn modal_data_decodes_from_text_in_any_key_order() {
        // `type` after the fields it decides, and `components` before
        // `custom_id`: the decoder reads every key first and decides at
        // the end of each object.
        let submission: V2ModalSubmission = serde_json::from_str(
            r#"{"components":[
                {"component":{"value":"hello","custom_id":"note","type":4},"id":1,"type":18},
                {"components":[{"values":["a"],"custom_id":"pick","type":3}],"type":1},
                {"component":{"value":null,"custom_id":"radio","type":21},"type":18},
                {"component":{"value":false,"custom_id":"box","type":23},"type":18}
            ],"custom_id":"ordered","resolved":{"users":{}}}"#,
        )
        .unwrap();
        assert_eq!(submission.custom_id, "ordered");
        assert_eq!(submission.get_text("note"), Some("hello"));
        assert_eq!(
            submission.get_select_values("pick"),
            Some(&["a".to_string()][..])
        );
        assert_eq!(submission.get_radio_value("radio"), None);
        assert_eq!(submission.get_checkbox("box"), Some(false));
        assert_eq!(submission.components.len(), 4);
    }

    #[test]
    fn modal_submissions_write_discords_wire_format_and_read_it_back() {
        let submission = parse_modal_submission(&json!({
            "custom_id": "settings",
            "components": [
                { "type": 18, "component": { "type": 4, "custom_id": "name", "value": "Ari" } },
                { "type": 18, "component": { "type": 21, "custom_id": "class", "value": null } },
                { "type": 18, "component": { "type": 23, "custom_id": "news", "value": true } },
                { "type": 18, "component": { "type": 19, "custom_id": "files", "values": ["9"] } }
            ]
        }))
        .unwrap();
        let written = serde_json::to_value(&submission).unwrap();
        assert_eq!(
            written,
            json!({
                "custom_id": "settings",
                "components": [
                    { "type": 4, "custom_id": "name", "value": "Ari" },
                    { "type": 21, "custom_id": "class", "value": null },
                    { "type": 23, "custom_id": "news", "value": true },
                    { "type": 19, "custom_id": "files", "values": ["9"] }
                ]
            })
        );
        let again: V2ModalSubmission = serde_json::from_value(written).unwrap();
        assert_eq!(again, submission);

        // One component reads on its own, unwrapped from its label.
        let leaf: V2ModalComponent = serde_json::from_str(
            r#"{"type":18,"component":{"type":22,"custom_id":"days","values":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            leaf,
            V2ModalComponent::CheckboxGroup {
                custom_id: "days".to_string(),
                values: Vec::new(),
            }
        );
        assert_eq!(leaf.custom_id(), "days");
        assert!(serde_json::from_str::<V2ModalComponent>(
            r#"{"type":1,"components":[
                {"type":4,"custom_id":"a","value":"1"},{"type":4,"custom_id":"b","value":"2"}
            ]}"#
        )
        .is_err());
        assert!(serde_json::from_str::<V2ModalComponent>(r#"{"type":10,"content":"hi"}"#).is_err());
    }

    #[test]
    fn modal_parser_keeps_the_lenient_2x_shapes() {
        // Numbers where strings are expected read as their text, a numeric
        // string `type` reads as the number, and inputs nested in a
        // container type this version does not know are still collected.
        let submission = parse_modal_submission(&json!({
            "custom_id": 77,
            "components": [
                { "type": "1", "components": [{ "type": 4, "custom_id": 5, "value": 6 }] },
                { "type": 250, "components": [{ "type": 3, "custom_id": "s", "values": [1, "2"] }] },
                "not a component",
                { "type": 4.5, "custom_id": "ignored" }
            ]
        }))
        .unwrap();
        assert_eq!(submission.custom_id, "77");
        assert_eq!(submission.get_text("5"), Some("6"));
        assert_eq!(
            submission.get_select_values("s"),
            Some(&["1".to_string(), "2".to_string()][..])
        );
        assert_eq!(submission.components.len(), 2);

        let error = parse_modal_submission(&json!({
            "custom_id": "rows",
            "components": [{ "type": 1, "components": {} }]
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing or invalid action_row.components"));
        let error =
            parse_modal_submission(&json!({ "custom_id": "x", "components": {} })).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing or invalid modal_data.components"));
        let error = parse_modal_submission(&json!({
            "custom_id": "x",
            "components": [{ "type": 5, "custom_id": "u", "values": "1" }]
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing or invalid user_select.values"));
    }

    #[test]
    fn parse_modal_submission_reports_leaf_validation_errors() {
        let error = parse_modal_submission(&json!({
            "custom_id": "broken_text",
            "components": [{
                "type": 4,
                "custom_id": "summary"
            }]
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing or invalid text_input.value"));

        let error = parse_modal_submission(&json!({
            "custom_id": "broken_checkbox",
            "components": [{
                "type": 23,
                "custom_id": "enabled",
                "value": "yes"
            }]
        }))
        .unwrap_err();
        assert!(error.to_string().contains("checkbox.value"));

        // The 2.x key `checked` is not what Discord sends; it is not read.
        let error = parse_modal_submission(&json!({
            "custom_id": "legacy_checkbox",
            "components": [{
                "type": 23,
                "custom_id": "enabled",
                "checked": true
            }]
        }))
        .unwrap_err();
        assert!(error.to_string().contains("checkbox.value"));

        let error = parse_modal_submission(&json!({
            "custom_id": "broken_radio",
            "components": [{
                "type": 21,
                "custom_id": "class",
                "value": 3
            }]
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("invalid radio_group.value: expected a string or null"));

        let error = parse_modal_submission(&json!({
            "custom_id": "broken_select",
            "components": [{
                "type": 3,
                "custom_id": "strings",
                "values": [{}]
            }]
        }))
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("string_select.values must contain strings"));
    }
}
