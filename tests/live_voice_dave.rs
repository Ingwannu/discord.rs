#![cfg(all(feature = "voice", feature = "dave", feature = "voice-encode"))]

use std::env;
use std::num::NonZeroU16;
use std::time::Duration;

use discordrs::{connect_voice_runtime, VoiceDaveySession, VoiceRuntimeConfig, VoiceRuntimeState};
use tokio::sync::watch;
use tokio::time::{timeout, Instant};

struct LiveDaveConfig {
    server_id: String,
    user_id: String,
    session_id: String,
    token: String,
    endpoint: String,
    channel_id: u64,
}

impl LiveDaveConfig {
    fn from_env() -> Result<Self, String> {
        let required = [
            "DISCORDRS_LIVE_VOICE_SERVER_ID",
            "DISCORDRS_LIVE_VOICE_USER_ID",
            "DISCORDRS_LIVE_VOICE_SESSION_ID",
            "DISCORDRS_LIVE_VOICE_TOKEN",
            "DISCORDRS_LIVE_VOICE_ENDPOINT",
            "DISCORDRS_LIVE_VOICE_CHANNEL_ID",
        ];
        let missing = required
            .iter()
            .copied()
            .filter(|name| env_value(name).is_none())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "set required live DAVE env vars before running this ignored test: {}",
                missing.join(", ")
            ));
        }

        let channel_id = env_value("DISCORDRS_LIVE_VOICE_CHANNEL_ID")
            .expect("validated above")
            .parse()
            .map_err(|error| format!("DISCORDRS_LIVE_VOICE_CHANNEL_ID must be numeric: {error}"))?;

        Ok(Self {
            server_id: env_value("DISCORDRS_LIVE_VOICE_SERVER_ID").expect("validated above"),
            user_id: env_value("DISCORDRS_LIVE_VOICE_USER_ID").expect("validated above"),
            session_id: env_value("DISCORDRS_LIVE_VOICE_SESSION_ID").expect("validated above"),
            token: env_value("DISCORDRS_LIVE_VOICE_TOKEN").expect("validated above"),
            endpoint: env_value("DISCORDRS_LIVE_VOICE_ENDPOINT").expect("validated above"),
            channel_id,
        })
    }
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

async fn wait_for_state<F>(
    state_rx: &mut watch::Receiver<VoiceRuntimeState>,
    stage: &str,
    mut predicate: F,
) -> VoiceRuntimeState
where
    F: FnMut(&VoiceRuntimeState) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let state = state_rx.borrow().clone();
        if predicate(&state) {
            return state;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!(
                "timed out waiting for live voice DAVE state during {stage}; last state: {:?}",
                state
            );
        }

        match timeout(remaining, state_rx.changed()).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => panic!("voice state channel closed"),
            Err(_) => {
                let state = state_rx.borrow().clone();
                if predicate(&state) {
                    return state;
                }
                panic!(
                    "timed out waiting for live voice DAVE state during {stage}; last state: {:?}",
                    state
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires live Discord voice session env vars; see docs/api/voice.md"]
async fn live_voice_gateway_dave_mls_transition_smoke() {
    let config = LiveDaveConfig::from_env().expect(
        "live DAVE validation was explicitly requested with --ignored but is not configured",
    );

    let user_id = config
        .user_id
        .parse::<u64>()
        .expect("DISCORDRS_LIVE_VOICE_USER_ID must be a numeric snowflake");
    let mut dave = VoiceDaveySession::new(
        NonZeroU16::new(davey::DAVE_PROTOCOL_VERSION).unwrap(),
        user_id,
        config.channel_id,
    )
    .expect("failed to create DAVE session");

    let handle = connect_voice_runtime(
        VoiceRuntimeConfig::new(
            config.server_id,
            config.user_id,
            config.session_id,
            config.token,
            config.endpoint,
        )
        .max_dave_protocol_version(davey::DAVE_PROTOCOL_VERSION as u8),
    )
    .await
    .expect("failed to connect live voice runtime");

    let mut state_rx = handle.subscribe();
    let state = wait_for_state(&mut state_rx, "initial DAVE negotiation", |state| {
        state.dave.protocol_version.is_some() && state.dave.external_sender.is_some()
    })
    .await;

    let external_sender = state
        .dave
        .external_sender
        .as_deref()
        .expect("DAVE external sender should be present");
    dave.set_external_sender(external_sender)
        .expect("failed to apply DAVE external sender");
    let key_package = dave
        .create_key_package()
        .expect("failed to create DAVE key package");
    handle
        .send_dave_mls_key_package(key_package)
        .expect("failed to send DAVE key package");

    let state = wait_for_state(
        &mut state_rx,
        "post-key-package MLS proposal or welcome",
        |state| {
            !state.dave.proposals.is_empty()
                || state.dave.pending_commit.is_some()
                || state.dave.pending_welcome.is_some()
        },
    )
    .await;

    if let Some(proposals) = state.dave.proposals.last() {
        let Some((&operation, proposal_bytes)) = proposals.split_first() else {
            panic!("live DAVE proposals payload was empty");
        };
        let operation = match operation {
            0 => davey::ProposalsOperationType::APPEND,
            1 => davey::ProposalsOperationType::REVOKE,
            other => panic!("unknown DAVE proposals operation type {other}"),
        };
        if let Some(commit_welcome) = dave
            .process_proposals(operation, proposal_bytes, None)
            .expect("failed to process live DAVE proposals")
        {
            handle
                .send_dave_mls_commit_welcome(commit_welcome.commit, commit_welcome.welcome)
                .expect("failed to send DAVE commit/welcome");
        }
    }

    let state = wait_for_state(
        &mut state_rx,
        "commit or welcome transition payload",
        |state| state.dave.pending_commit.is_some() || state.dave.pending_welcome.is_some(),
    )
    .await;
    let transition_id = state
        .dave
        .transition_id
        .expect("DAVE transition id should be present during live transition");

    if let Some(commit) = state.dave.pending_commit.as_deref() {
        if dave.process_commit(commit).is_err() {
            handle
                .send_dave_mls_invalid_commit_welcome(transition_id)
                .expect("failed to send DAVE invalid commit/welcome");
            return;
        }
    }
    if let Some(welcome) = state.dave.pending_welcome.as_deref() {
        if dave.process_welcome(welcome).is_err() {
            handle
                .send_dave_mls_invalid_commit_welcome(transition_id)
                .expect("failed to send DAVE invalid commit/welcome");
            return;
        }
    }

    handle
        .send_dave_transition_ready(transition_id)
        .expect("failed to send DAVE transition ready");
    handle.close().await.expect("failed to close voice runtime");
}
