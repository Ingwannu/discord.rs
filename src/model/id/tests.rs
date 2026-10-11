//! Contract tests for typed IDs and the non-snowflake exception types
//! (docs/design/3.0-types-events-requests.md §1 and §5 "필수 테스트").

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasher, RandomState};
use std::mem::size_of;
use std::num::NonZeroU64;

use serde::de::value::{Error as ValueError, I64Deserializer};
use serde::Deserialize;
use serde_json::json;

use super::{
    ChannelId, ComponentId, GenericId, GuildId, GuildInstallOwner, InvalidIdKind, PollAnswerId,
    RoleId, UploadId, UserId, WidgetUserId,
};

const DOCS_EXAMPLE_ID: u64 = 175_928_847_299_117_063;

fn nonzero(value: u64) -> NonZeroU64 {
    match NonZeroU64::new(value) {
        Some(value) => value,
        None => panic!("test fixture must be nonzero"),
    }
}

fn guild(value: u64) -> GuildId {
    GuildId::from_nonzero(nonzero(value))
}

#[test]
fn try_new_rejects_zero_and_accepts_the_full_nonzero_range() {
    assert_eq!(
        GuildId::try_new(0).map_err(|error| error.kind()),
        Err(InvalidIdKind::Zero)
    );
    assert_eq!(GuildId::try_new(1).map(GuildId::get), Ok(1));
    assert_eq!(GuildId::try_new(u64::MAX).map(GuildId::get), Ok(u64::MAX));
    assert_eq!(
        GuildId::try_from(0_u64).map_err(|error| error.kind()),
        Err(InvalidIdKind::Zero)
    );
}

#[test]
fn parsing_rejects_zero_overflow_signs_whitespace_and_non_digits() {
    let cases: &[(&str, InvalidIdKind)] = &[
        ("", InvalidIdKind::Empty),
        ("0", InvalidIdKind::Zero),
        ("0000", InvalidIdKind::Zero),
        ("18446744073709551616", InvalidIdKind::Overflow),
        ("99999999999999999999999999", InvalidIdKind::Overflow),
        ("-1", InvalidIdKind::InvalidDigit),
        ("+1", InvalidIdKind::InvalidDigit),
        (" 1", InvalidIdKind::InvalidDigit),
        ("1 ", InvalidIdKind::InvalidDigit),
        ("1\n", InvalidIdKind::InvalidDigit),
        ("1.0", InvalidIdKind::InvalidDigit),
        ("1e3", InvalidIdKind::InvalidDigit),
        ("1_000", InvalidIdKind::InvalidDigit),
        ("0x10", InvalidIdKind::InvalidDigit),
        ("abc", InvalidIdKind::InvalidDigit),
        // Non-ASCII digits (fullwidth one, Arabic-Indic one).
        ("\u{ff11}", InvalidIdKind::InvalidDigit),
        ("\u{0661}", InvalidIdKind::InvalidDigit),
    ];
    for (input, expected) in cases {
        assert_eq!(
            input.parse::<GuildId>().map_err(|error| error.kind()),
            Err(*expected),
            "input {input:?}"
        );
        assert_eq!(
            GuildId::try_from(*input).map_err(|error| error.kind()),
            Err(*expected),
            "TryFrom<&str> for {input:?}"
        );
    }
}

#[test]
fn parsing_accepts_u64_max_and_canonicalizes_leading_zeros() {
    assert_eq!(
        "18446744073709551615".parse::<GuildId>().map(GuildId::get),
        Ok(u64::MAX)
    );

    let padded: GuildId = match "000123".parse() {
        Ok(id) => id,
        Err(error) => panic!("leading zeros must be accepted: {error}"),
    };
    assert_eq!(padded, guild(123));
    assert_eq!(padded.to_string(), "123");
    assert_eq!(serde_json::to_value(padded).ok(), Some(json!("123")));

    let from_json: GuildId = match serde_json::from_value(json!("0042")) {
        Ok(id) => id,
        Err(error) => panic!("leading zeros must be accepted from JSON: {error}"),
    };
    assert_eq!(
        serde_json::to_string(&from_json).ok().as_deref(),
        Some(r#""42""#)
    );
}

#[test]
fn serializes_as_decimal_string_and_reads_string_or_integer() {
    let id = guild(DOCS_EXAMPLE_ID);
    assert_eq!(
        serde_json::to_string(&id).ok().as_deref(),
        Some(r#""175928847299117063""#)
    );
    assert_eq!(
        serde_json::from_value::<GuildId>(json!("175928847299117063")).ok(),
        Some(id)
    );
    assert_eq!(
        serde_json::from_value::<GuildId>(json!(175_928_847_299_117_063_u64)).ok(),
        Some(id)
    );
    assert_eq!(
        serde_json::from_str::<GuildId>("18446744073709551615").ok(),
        Some(guild(u64::MAX))
    );
}

#[test]
fn deserialization_rejects_zero_negative_float_overflow_and_garbage() {
    let rejected = [
        "0",
        r#""0""#,
        "-1",
        r#""-1""#,
        "1.0",
        "1.5",
        "1e3",
        "18446744073709551616",
        r#""18446744073709551616""#,
        r#"" 1""#,
        r#""1 ""#,
        r#""+1""#,
        r#""""#,
        r#""abc""#,
        "null",
        "true",
        "[]",
        "{}",
    ];
    for input in rejected {
        assert!(
            serde_json::from_str::<GuildId>(input).is_err(),
            "{input} must be rejected"
        );
    }
}

#[test]
fn deserialization_accepts_positive_signed_integers_from_other_formats() {
    // serde_json reports non-negative numbers as u64, but other
    // deserializers may hand over small positive values through visit_i64.
    assert_eq!(
        GuildId::deserialize(I64Deserializer::<ValueError>::new(5)).ok(),
        Some(guild(5))
    );
    assert!(GuildId::deserialize(I64Deserializer::<ValueError>::new(0)).is_err());
    assert!(GuildId::deserialize(I64Deserializer::<ValueError>::new(-5)).is_err());
}

#[test]
fn json_map_keys_round_trip() {
    let mut names = HashMap::new();
    names.insert(guild(10), "ten".to_string());
    names.insert(guild(9), "nine".to_string());

    let text = match serde_json::to_string(&names) {
        Ok(text) => text,
        Err(error) => panic!("ID map keys must serialize: {error}"),
    };
    let decoded: HashMap<GuildId, String> = match serde_json::from_str(&text) {
        Ok(decoded) => decoded,
        Err(error) => panic!("ID map keys must deserialize: {error}"),
    };
    assert_eq!(decoded, names);

    let ordered: BTreeMap<UserId, u8> = match serde_json::from_value(json!({"10": 1, "9": 2})) {
        Ok(ordered) => ordered,
        Err(error) => panic!("ID map keys must deserialize from a Value: {error}"),
    };
    assert_eq!(
        serde_json::to_value(&ordered).ok(),
        Some(json!({"9": 2, "10": 1}))
    );
    assert!(serde_json::from_value::<HashMap<GuildId, u8>>(json!({"0": 1})).is_err());
    assert!(serde_json::from_value::<HashMap<GuildId, u8>>(json!({"x": 1})).is_err());
}

#[test]
fn ordering_is_numeric_not_lexicographic() {
    let mut ids = vec![guild(100), guild(9), guild(10)];
    ids.sort();
    assert_eq!(ids, vec![guild(9), guild(10), guild(100)]);
    assert!(guild(9) < guild(10));
    assert_eq!(guild(7).cmp(&guild(7)), std::cmp::Ordering::Equal);
    assert_eq!(
        guild(1).partial_cmp(&guild(2)),
        Some(std::cmp::Ordering::Less)
    );
}

#[test]
fn ids_and_optional_ids_are_eight_bytes() {
    assert_eq!(size_of::<GuildId>(), 8);
    assert_eq!(size_of::<Option<GuildId>>(), 8);
    assert_eq!(size_of::<GenericId>(), 8);
    assert_eq!(size_of::<Option<ChannelId>>(), 8);
}

#[test]
fn debug_names_the_alias_and_display_is_decimal() {
    assert_eq!(format!("{:?}", guild(123)), "GuildId(123)");
    assert_eq!(
        format!("{:?}", ChannelId::from_nonzero(nonzero(5))),
        "ChannelId(5)"
    );
    assert_eq!(format!("{:?}", guild(5).erase()), "GenericId(5)");
    assert_eq!(format!("{:?}", Some(guild(5))), "Some(GuildId(5))");
    assert_eq!(guild(DOCS_EXAMPLE_ID).to_string(), "175928847299117063");
    // Display forwards to the integer formatter, so width flags work.
    assert_eq!(format!("{:>5}", guild(42)), "   42");
}

#[test]
fn timestamp_matches_the_discord_docs_example() {
    // developers/reference.mdx "Snowflake ID Broken Down in Binary".
    assert_eq!(guild(DOCS_EXAMPLE_ID).timestamp(), 1_462_015_105_796);
    assert_eq!(guild(1).timestamp(), super::DISCORD_EPOCH_MS);
    assert_eq!(
        guild(u64::MAX).timestamp(),
        (u64::MAX >> 22) + super::DISCORD_EPOCH_MS
    );
}

#[test]
fn erase_and_cast_keep_the_number_and_change_only_the_marker() {
    let guild_id = guild(81_384_788_765_712_384);
    let everyone: RoleId = guild_id.cast();
    assert_eq!(everyone.get(), guild_id.get());
    assert_eq!(guild_id.erase().get(), guild_id.get());
    assert_eq!(everyone.erase(), guild_id.erase());
    assert_eq!(guild_id.erase().cast::<super::marker::Guild>(), guild_id);
}

#[test]
fn hashing_and_equality_use_only_the_number() {
    let state = RandomState::new();
    assert_eq!(
        state.hash_one(guild(DOCS_EXAMPLE_ID)),
        state.hash_one(DOCS_EXAMPLE_ID)
    );
    assert_eq!(guild(5), guild(5));
    assert_ne!(guild(5), guild(6));
}

#[test]
fn conversions_are_explicit_and_lossless() {
    let raw = nonzero(77);
    let id = GuildId::from(raw);
    assert_eq!(id.into_nonzero(), raw);
    assert_eq!(NonZeroU64::from(id), raw);
    assert_eq!(u64::from(id), 77);
    assert_eq!(GuildId::try_from(77_u64), Ok(id));
    assert_eq!(GuildId::try_from("77"), Ok(id));
    // Copy: the original stays usable after being passed by value.
    let copy = id;
    assert_eq!(copy.get(), id.get());
    #[allow(clippy::clone_on_copy, reason = "exercises the manual Clone impl")]
    let cloned = id.clone();
    assert_eq!(cloned, id);
}

#[test]
fn invalid_id_messages_explain_without_echoing_input() {
    let secretish = "Bot abc.def";
    let error = match secretish.parse::<GuildId>() {
        Ok(id) => panic!("{id} must not parse"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(!message.contains(secretish));
    assert!(message.contains("ASCII digits"));

    let messages = [
        GuildId::try_new(0).err().map(|error| error.to_string()),
        "".parse::<GuildId>().err().map(|error| error.to_string()),
        "18446744073709551616"
            .parse::<GuildId>()
            .err()
            .map(|error| error.to_string()),
    ];
    assert_eq!(
        messages,
        [
            Some("Discord ID must not be zero".to_string()),
            Some("Discord ID must not be empty".to_string()),
            Some("Discord ID does not fit in 64 bits".to_string()),
        ]
    );
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert!(boxed.source().is_none());
}

#[test]
fn invalid_id_converts_into_a_model_error_with_question_mark() {
    fn parse_config_guild(raw: &str) -> Result<GuildId, crate::DiscordError> {
        Ok(raw.parse::<GuildId>()?)
    }

    assert_eq!(parse_config_guild("42").ok(), Some(guild(42)));
    match parse_config_guild("Bot secret") {
        Err(crate::DiscordError::Model { message }) => {
            assert!(message.contains("ASCII digits"));
            assert!(!message.contains("secret"));
        }
        other => panic!("expected a model error, got {other:?}"),
    }
}

#[test]
fn widget_user_id_keeps_zero_and_opaque_text() {
    let zero: WidgetUserId = match serde_json::from_value(json!("0")) {
        Ok(id) => id,
        Err(error) => panic!("widget member id \"0\" must be accepted: {error}"),
    };
    assert_eq!(zero.as_str(), "0");
    assert_eq!(serde_json::to_value(&zero).ok(), Some(json!("0")));

    let anonymized = WidgetUserId::new("0001");
    assert_eq!(anonymized.to_string(), "0001");
    assert_eq!(
        serde_json::to_value(&anonymized).ok(),
        Some(json!("0001")),
        "widget IDs are opaque and must not be canonicalized"
    );
    assert_eq!(anonymized.into_string(), "0001");
}

#[test]
fn guild_install_owner_reads_zero_as_bot_dm() {
    for zero in [json!("0"), json!(0), json!("00")] {
        assert_eq!(
            serde_json::from_value::<GuildInstallOwner>(zero.clone()).ok(),
            Some(GuildInstallOwner::BotDm),
            "{zero}"
        );
    }
    assert_eq!(
        serde_json::to_value(GuildInstallOwner::BotDm).ok(),
        Some(json!("0"))
    );
    assert!(GuildInstallOwner::BotDm.is_bot_dm());
    assert_eq!(GuildInstallOwner::BotDm.guild_id(), None);
    assert_eq!(GuildInstallOwner::BotDm.to_string(), "0");
}

#[test]
fn guild_install_owner_reads_guild_ids_and_rejects_garbage() {
    let owner: GuildInstallOwner = match serde_json::from_value(json!("81384788765712384")) {
        Ok(owner) => owner,
        Err(error) => panic!("guild owner must parse: {error}"),
    };
    assert_eq!(
        owner,
        GuildInstallOwner::Guild(guild(81_384_788_765_712_384))
    );
    assert_eq!(owner.guild_id(), Some(guild(81_384_788_765_712_384)));
    assert!(!owner.is_bot_dm());
    assert_eq!(owner.to_string(), "81384788765712384");
    assert_eq!(
        serde_json::to_value(owner).ok(),
        Some(json!("81384788765712384"))
    );
    assert_eq!(
        "0".parse::<GuildInstallOwner>(),
        Ok(GuildInstallOwner::BotDm)
    );
    assert_eq!(
        "12".parse::<GuildInstallOwner>(),
        Ok(GuildInstallOwner::Guild(guild(12)))
    );

    for bad in [
        json!(""),
        json!("-1"),
        json!(-1),
        json!(" 0"),
        json!(1.0),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<GuildInstallOwner>(bad.clone()).is_err(),
            "{bad} must be rejected"
        );
    }

    let owners: HashMap<String, GuildInstallOwner> =
        match serde_json::from_value(json!({"0": "0", "1": "0"})) {
            Ok(owners) => owners,
            Err(error) => panic!("owner map must parse: {error}"),
        };
    assert_eq!(owners.get("0"), Some(&GuildInstallOwner::BotDm));
}

#[test]
fn local_integer_ids_allow_zero_and_serialize_as_integers() {
    assert_eq!(serde_json::to_value(UploadId::new(0)).ok(), Some(json!(0)));
    assert_eq!(
        serde_json::from_value::<UploadId>(json!(3)).ok(),
        Some(UploadId::new(3))
    );
    assert_eq!(UploadId::new(2).get(), 2);
    assert_eq!(UploadId::new(2).to_string(), "2");

    assert_eq!(
        serde_json::to_value(ComponentId::new(0)).ok(),
        Some(json!(0))
    );
    assert_eq!(
        serde_json::from_value::<ComponentId>(json!(4_294_967_295_u32)).ok(),
        Some(ComponentId::new(u32::MAX))
    );
    assert_eq!(ComponentId::new(7).get(), 7);
    assert_eq!(ComponentId::new(7).to_string(), "7");
    assert!(serde_json::from_value::<ComponentId>(json!(-1)).is_err());
    assert!(serde_json::from_value::<ComponentId>(json!(4_294_967_296_u64)).is_err());

    assert_eq!(
        serde_json::from_value::<PollAnswerId>(json!(1)).ok(),
        Some(PollAnswerId::new(1))
    );
    assert_eq!(
        serde_json::to_value(PollAnswerId::new(10)).ok(),
        Some(json!(10))
    );
    assert_eq!(PollAnswerId::new(10).get(), 10);
    assert_eq!(PollAnswerId::new(10).to_string(), "10");
}
