//! Payloads: the JSON fixtures under `tests/fixtures/perf` and the
//! synthesized large GUILD_CREATE.

use serde_json::{json, Value};

pub(crate) const MESSAGE_CREATE_PLAIN: &str =
    include_str!("../fixtures/perf/message_create_plain.json");
const MESSAGE_CREATE_EMBEDS: &str = include_str!("../fixtures/perf/message_create_embeds.json");
pub(crate) const MESSAGE_CREATE_COMPONENTS_V2: &str =
    include_str!("../fixtures/perf/message_create_components_v2.json");
pub(crate) const MESSAGE_UPDATE_PARTIAL: &str =
    include_str!("../fixtures/perf/message_update_partial.json");
const PRESENCE_UPDATE: &str = include_str!("../fixtures/perf/presence_update.json");
const INTERACTION_CREATE_CHAT_INPUT: &str =
    include_str!("../fixtures/perf/interaction_create_chat_input.json");
const UNKNOWN_DISPATCH: &str = include_str!("../fixtures/perf/unknown_dispatch.json");
const GUILD_CREATE_TEMPLATE: &str = include_str!("../fixtures/perf/guild_create_template.json");

/// A dispatch name `decode_event` does not know (Discord sends it for guild
/// join requests; the crate does not decode it).
pub(crate) const UNKNOWN_EVENT_NAME: &str = "GUILD_JOIN_REQUEST_UPDATE";

/// A dispatch the crate does not decode, with an empty `d`: the fixed cost
/// of moving one frame through the pipeline.
pub(crate) const FLOOR_EVENT: &str = "PERF_REPORT_PROBE";

/// Collection sizes of the synthesized large GUILD_CREATE.
pub(crate) struct GuildShape {
    pub(crate) members: u64,
    pub(crate) channels: u64,
    pub(crate) roles: u64,
    pub(crate) presences: u64,
    pub(crate) voice_states: u64,
    pub(crate) emojis: u64,
}

/// A large guild as a bot with the GUILD_MEMBERS and GUILD_PRESENCES intents
/// receives it: Discord only includes online members, each with a presence.
pub(crate) const LARGE_GUILD: GuildShape = GuildShape {
    members: 1_000,
    channels: 200,
    roles: 300,
    presences: 1_000,
    voice_states: 50,
    emojis: 30,
};

/// Distinct large guilds in the GUILD_CREATE storm.
pub(crate) const STORM_GUILDS: u64 = 25;

pub(crate) fn fixture(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn snowflake(n: u64) -> Value {
    Value::String((1_300_000_000_000_000_000_u64 + n).to_string())
}

/// A GUILD_CREATE `d` built from `guild_create_template.json`. Each guild
/// index owns a disjoint ID range, so a storm of distinct indices fills the
/// cache instead of replacing entries. Deterministic: no randomness.
pub(crate) fn synthesized_guild_create(index: u64, shape: &GuildShape) -> Value {
    let template = fixture(GUILD_CREATE_TEMPLATE);
    let t = &template["templates"];
    let base = index * 100_000_000;
    let guild_id = snowflake(base);
    let role_id = |i: u64| {
        if i == 0 {
            snowflake(base)
        } else {
            snowflake(base + 1_000_000 + i)
        }
    };
    let channel_id = |i: u64| snowflake(base + 2_000_000 + i);
    let user_id = |i: u64| snowflake(base + 10_000_000 + i);

    let mut guild = template["guild"].clone();
    guild["id"] = guild_id.clone();
    guild["name"] = json!(format!("Raid Night Collective #{index}"));
    guild["owner_id"] = user_id(0);
    guild["system_channel_id"] = channel_id(1);
    guild["rules_channel_id"] = channel_id(2);
    guild["public_updates_channel_id"] = channel_id(3);
    guild["member_count"] = json!(shape.members * 5);

    guild["roles"] = (0..shape.roles)
        .map(|i| {
            let mut role = t["role"].clone();
            role["id"] = role_id(i);
            role["name"] = json!(if i == 0 {
                "@everyone".to_string()
            } else {
                format!("role-{i}")
            });
            role["position"] = json!(i);
            role["hoist"] = json!(i % 10 == 0);
            role
        })
        .collect();

    guild["channels"] = (0..shape.channels)
        .map(|i| {
            // A category every 20 channels, then one voice channel per 5.
            let mut channel = if i % 20 == 0 {
                t["channel_category"].clone()
            } else if i % 5 == 0 {
                t["channel_voice"].clone()
            } else {
                let mut text = t["channel_text"].clone();
                text["last_message_id"] = snowflake(base + 50_000_000 + i);
                text
            };
            channel["id"] = channel_id(i);
            channel["name"] = json!(format!("channel-{i}"));
            channel["position"] = json!(i);
            if i % 20 != 0 {
                channel["parent_id"] = channel_id(i / 20 * 20);
                channel["permission_overwrites"] = json!([
                    {"type": 0, "id": guild_id.clone(), "deny": "2048", "allow": "0"},
                    {"type": 0, "id": role_id(1 + i % (shape.roles - 1)), "deny": "0", "allow": "3072"}
                ]);
            }
            channel
        })
        .collect();

    guild["members"] = (0..shape.members)
        .map(|i| {
            let mut member = t["member"].clone();
            member["user"]["id"] = user_id(i);
            member["user"]["username"] = json!(format!("raider{i}"));
            member["user"]["global_name"] = json!(format!("Raider {i}"));
            if i % 3 == 0 {
                member["user"]["avatar"] = Value::Null;
            }
            member["roles"] = json!([role_id(1 + i % 7), role_id(8 + i % 13)]);
            member["nick"] = if i % 4 == 0 {
                json!(format!("nick{i}"))
            } else {
                Value::Null
            };
            member
        })
        .collect();

    guild["presences"] = (0..shape.presences)
        .map(|i| {
            let mut presence = t["presence"].clone();
            presence["user"]["id"] = user_id(i);
            presence["status"] = json!(["online", "idle", "dnd"][usize::try_from(i % 3).unwrap()]);
            if i % 2 == 1 {
                // Every other member only shows a custom status.
                let custom = presence["activities"][0].clone();
                presence["activities"] = json!([custom]);
            }
            presence
        })
        .collect();

    guild["voice_states"] = (0..shape.voice_states)
        .map(|i| {
            let mut state = t["voice_state"].clone();
            state["user_id"] = user_id(i);
            // Channel indices 5, 25, 45, ... are voice channels.
            state["channel_id"] = channel_id(5 + 20 * (i % 10));
            state["session_id"] = json!(format!("{:032x}", base + i));
            state
        })
        .collect();

    guild["emojis"] = (0..shape.emojis)
        .map(|i| {
            let mut emoji = t["emoji"].clone();
            emoji["id"] = snowflake(base + 3_000_000 + i);
            emoji["name"] = json!(format!("emoji_{i}"));
            emoji
        })
        .collect();

    guild
}

/// A gateway dispatch frame as Discord sends it (`t`, `s`, `op`, `d`).
pub(crate) fn frame(event: &str, sequence: u64, d: &Value) -> String {
    format!(r#"{{"t":"{event}","s":{sequence},"op":0,"d":{d}}}"#)
}

pub(crate) struct InboundCase {
    pub(crate) name: &'static str,
    pub(crate) event: &'static str,
    pub(crate) d: Value,
    /// Timed iterations for the median.
    pub(crate) iterations: usize,
}

pub(crate) fn inbound_cases() -> Vec<InboundCase> {
    let case = |name, event, d, iterations| InboundCase {
        name,
        event,
        d,
        iterations,
    };
    vec![
        case(
            "message_create.plain",
            "MESSAGE_CREATE",
            fixture(MESSAGE_CREATE_PLAIN),
            2_000,
        ),
        case(
            "message_create.embeds",
            "MESSAGE_CREATE",
            fixture(MESSAGE_CREATE_EMBEDS),
            2_000,
        ),
        case(
            "message_create.components_v2",
            "MESSAGE_CREATE",
            fixture(MESSAGE_CREATE_COMPONENTS_V2),
            2_000,
        ),
        case(
            "message_update.partial",
            "MESSAGE_UPDATE",
            fixture(MESSAGE_UPDATE_PARTIAL),
            2_000,
        ),
        case(
            "presence_update",
            "PRESENCE_UPDATE",
            fixture(PRESENCE_UPDATE),
            2_000,
        ),
        case(
            "interaction_create.chat_input",
            "INTERACTION_CREATE",
            fixture(INTERACTION_CREATE_CHAT_INPUT),
            2_000,
        ),
        case(
            "unknown_dispatch",
            UNKNOWN_EVENT_NAME,
            fixture(UNKNOWN_DISPATCH),
            2_000,
        ),
        case("floor", FLOOR_EVENT, json!({}), 2_000),
        case(
            "guild_create.large",
            "GUILD_CREATE",
            synthesized_guild_create(0, &LARGE_GUILD),
            15,
        ),
    ]
}
