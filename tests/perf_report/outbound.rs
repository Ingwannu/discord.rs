//! Outbound request bodies: a Components V2 message and an interaction
//! reply, built and encoded the way the REST client does.

use std::hint::black_box;
use std::time::Instant;

use discordrs::{
    ActionRowBuilder, ButtonBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder,
    InteractionResponseBuilder, MediaGalleryBuilder, MediaGalleryItem, MessageBuilder,
    SectionBuilder, SeparatorBuilder, SeparatorSpacing, TextDisplayBuilder, ThumbnailBuilder,
};

use crate::alloc::{Recorder, Uncounted};
use crate::report::{Report, Scenario, DETERMINISTIC};

/// The status card of `message_create_components_v2.json`, built with the
/// crate's builders.
pub(crate) fn status_card() -> ComponentsV2Message {
    let container = ContainerBuilder::new()
        .accent_color(5_814_783)
        .add_text_display(TextDisplayBuilder::new(
            "## Server status\nAll shards healthy. Gateway latency is normal.",
        ))
        .add_separator(
            SeparatorBuilder::new()
                .divider(true)
                .spacing(SeparatorSpacing::SMALL),
        )
        .add_section(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new(
                    "**Shard 0-7**: 1,204 guilds, p50 41 ms",
                ))
                .add_text_display(TextDisplayBuilder::new(
                    "**Shard 8-15**: 1,188 guilds, p50 39 ms",
                ))
                .set_thumbnail_accessory(
                    ThumbnailBuilder::new("https://example.com/static/status-ok.png")
                        .description("status icon"),
                ),
        )
        .add_media_gallery(
            MediaGalleryBuilder::new()
                .add_item(
                    MediaGalleryItem::new("https://example.com/charts/latency-24h.png")
                        .description("latency chart"),
                )
                .add_item(
                    MediaGalleryItem::new("https://example.com/charts/memory-24h.png")
                        .description("memory chart"),
                ),
        )
        .add_separator(
            SeparatorBuilder::new()
                .divider(false)
                .spacing(SeparatorSpacing::LARGE),
        )
        .add_action_row(
            ActionRowBuilder::new()
                .add_button(
                    ButtonBuilder::new()
                        .style(ButtonStyle::PRIMARY)
                        .label("Refresh")
                        .custom_id("status:refresh"),
                )
                .add_button(
                    ButtonBuilder::new()
                        .style(ButtonStyle::SECONDARY)
                        .label("Details")
                        .custom_id("status:details"),
                )
                .add_button(
                    ButtonBuilder::new()
                        .style(ButtonStyle::LINK)
                        .label("Dashboard")
                        .url("https://example.com/dashboard"),
                ),
        );
    ComponentsV2Message::new()
        .add_container(container)
        .add_text_display(TextDisplayBuilder::new("-# Updated <t:1791796641:R>"))
}

pub(crate) fn run_outbound(report: &mut Report) {
    pub(crate) const ITERATIONS: usize = 5_000;

    // POST /channels/{id}/messages: `create_message` -> `request_typed` ->
    // http/body.rs `serialize_body` (`serde_json::to_value`) -> reqwest
    // `.json(&value)` (`serde_json::to_vec`).
    let create_message = |rec: &mut Recorder| {
        let body = rec.step("build", || {
            MessageBuilder::new().components_v2(status_card()).build()
        });
        let value = rec.step("serialize_body", || serde_json::to_value(&body).unwrap());
        let bytes = rec.step("encode", || serde_json::to_vec(&value).unwrap());
        drop(value);
        drop(body);
        bytes
    };
    // POST /interactions/{id}/{token}/callback:
    // `InteractionResponseBuilder::channel_message` (to_value of the
    // message) -> `create_interaction_response_typed` -> `serialize_body`
    // -> `to_vec`.
    let interaction_reply = |rec: &mut Recorder| {
        let body = rec.step("build", || {
            InteractionResponseBuilder::channel_message(
                MessageBuilder::new()
                    .components_v2(status_card())
                    .ephemeral(true),
            )
            .unwrap()
            .build()
        });
        let value = rec.step("serialize_body", || serde_json::to_value(&body).unwrap());
        let bytes = rec.step("encode", || serde_json::to_vec(&value).unwrap());
        drop(value);
        drop(body);
        bytes
    };
    // What the same body costs encoded straight to bytes (phase 5 typed
    // request bodies).
    let direct = |rec: &mut Recorder| {
        let body = rec.step("build", || {
            MessageBuilder::new().components_v2(status_card()).build()
        });
        let bytes = rec.step("encode", || serde_json::to_vec(&body).unwrap());
        drop(body);
        bytes
    };

    pub(crate) type Case<'a> = (&'a str, &'a str, &'a dyn Fn(&mut Recorder) -> Vec<u8>);
    let cases: [Case<'_>; 3] = [
        (
            "outbound.create_message.components_v2",
            "outbound",
            &create_message,
        ),
        (
            "outbound.interaction_reply.components_v2",
            "outbound",
            &interaction_reply,
        ),
        (
            "ref.outbound.create_message.direct_to_vec",
            "reference",
            &direct,
        ),
    ];
    for (name, group, run) in cases {
        let mut rec = Recorder::counting();
        let bytes = run(&mut rec);
        let body_bytes = bytes.len();
        drop(bytes);
        let (sample, steps) = rec.finish();
        let uncounted = Uncounted::new();
        let mut timings = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let mut idle = Recorder::idle();
            let start = Instant::now();
            drop(black_box(run(&mut idle)));
            timings.push(start.elapsed());
        }
        drop(uncounted);
        report.push(
            Scenario::new(name, group)
                .set("body_bytes", body_bytes)
                .sample(sample)
                .steps(&steps, &[])
                .timings(timings)
                .deterministic(DETERMINISTIC),
        );
    }
}
