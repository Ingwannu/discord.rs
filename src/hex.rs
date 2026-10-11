//! Minimal hex codec for the interactions endpoint's Ed25519 public key and
//! request signatures.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 서명 검증에 필요한 16진수 디코딩만을 위해 hex 크레이트를 의존성으로 두지
//!   않는다.
//! - 기존 구현 및 제약 조건: interactions 기능이 hex 0.4의 decode(공개 키·서명)와 테스트용
//!   encode만 썼고, From<hex::FromHexError> for DiscordError가 공개 API에 노출돼 있었다.
//! - 검토한 주요 대안: (1) hex 유지, (2) ed25519-dalek 등 기존 의존성의 hex 기능 재사용
//!   (해당 공개 API 없음), (3) 비공개 헬퍼 직접 구현.
//! - 선택한 방식: (3). 대소문자 모두 받고 홀수 길이와 16진수가 아닌 문자를 거부한다.
//! - 다른 대안 대신 이 방식을 선택한 이유: 필요한 기능이 수십 줄이고, 공개 오류 타입에
//!   외부 크레이트 타입이 새지 않는다.
//! - 장점, 단점 및 영향: 의존성 하나가 줄고 오류 메시지에 어느 입력(공개 키·서명)인지 담긴다.
//!   From<hex::FromHexError> for DiscordError는 사라진다(파괴적 변경, CHANGELOG 기록).
//!   성능은 64~128자 입력에서 의미 있는 차이가 없다.
//! ```

use std::fmt;

/// Why [`decode`] rejected its input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HexError {
    /// The input has an odd number of bytes, so it cannot encode whole bytes.
    OddLength,
    /// `character` at byte offset `index` is not a hex digit.
    InvalidCharacter { character: char, index: usize },
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OddLength => f.write_str("odd number of hex digits"),
            Self::InvalidCharacter { character, index } => {
                write!(f, "invalid hex character {character:?} at position {index}")
            }
        }
    }
}

impl std::error::Error for HexError {}

/// Decodes a hex string, accepting upper- and lowercase digits.
pub(crate) fn decode(input: &str) -> Result<Vec<u8>, HexError> {
    let (pairs, remainder) = input.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(HexError::OddLength);
    }
    let invalid = |index: usize| HexError::InvalidCharacter {
        // Every byte before `index` was an ASCII hex digit, so `index` is a
        // char boundary and the offending character can be reported whole.
        character: input
            .get(index..)
            .and_then(|rest| rest.chars().next())
            .unwrap_or(char::REPLACEMENT_CHARACTER),
        index,
    };
    let mut bytes = Vec::with_capacity(pairs.len());
    for (pair_index, &[high, low]) in pairs.iter().enumerate() {
        let index = pair_index * 2;
        let high = nibble(high).ok_or_else(|| invalid(index))?;
        let low = nibble(low).ok_or_else(|| invalid(index + 1))?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

/// Encodes bytes as lowercase hex (tests build signed requests with it).
#[cfg(test)]
pub(crate) fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, HexError};

    #[test]
    fn encode_is_lowercase_and_round_trips_every_byte() {
        let all: Vec<u8> = (0..=u8::MAX).collect();
        let encoded = encode(&all);
        assert_eq!(encoded.len(), 512);
        assert!(encoded.starts_with("000102"));
        assert!(encoded.ends_with("fdfeff"));
        assert!(!encoded.chars().any(|c| c.is_ascii_uppercase()));
        assert_eq!(decode(&encoded), Ok(all));
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn decode_accepts_upper_lower_and_mixed_case() {
        assert_eq!(decode("deadBEEF"), Ok(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(decode("DEADBEEF"), decode("deadbeef"));
        assert_eq!(decode("0a0B"), Ok(vec![0x0a, 0x0b]));
        assert_eq!(decode(""), Ok(Vec::new()));
    }

    #[test]
    fn decode_rejects_odd_length() {
        assert_eq!(decode("a"), Err(HexError::OddLength));
        assert_eq!(decode("abc"), Err(HexError::OddLength));
        assert_eq!(HexError::OddLength.to_string(), "odd number of hex digits");
    }

    #[test]
    fn decode_rejects_non_hex_characters_with_their_position() {
        assert_eq!(
            decode("zz"),
            Err(HexError::InvalidCharacter {
                character: 'z',
                index: 0
            })
        );
        assert_eq!(
            decode("0g"),
            Err(HexError::InvalidCharacter {
                character: 'g',
                index: 1
            })
        );
        assert_eq!(
            decode("00 1"),
            Err(HexError::InvalidCharacter {
                character: ' ',
                index: 2
            })
        );
        // Multi-byte characters are reported whole at their byte offset.
        let error = decode("00é").err();
        assert_eq!(
            error,
            Some(HexError::InvalidCharacter {
                character: 'é',
                index: 2
            })
        );
        assert_eq!(
            error.map(|error| error.to_string()),
            Some("invalid hex character 'é' at position 2".to_string())
        );
        // A sign or prefix is not part of the format.
        assert!(decode("0x00").is_err());
        assert!(decode("+0").is_err());
    }
}
