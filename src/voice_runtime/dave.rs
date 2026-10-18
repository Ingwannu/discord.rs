//! The DAVE (end-to-end encryption) session a voice runtime owns.
//!
//! [`DaveController`] is a synchronous state machine: it takes the DAVE
//! messages of the voice gateway (opcodes 21–31), returns the commands to
//! send back, and encrypts and decrypts media frames with `davey`. It knows
//! nothing about sockets, so every MLS transition can be tested without a
//! network; `voice_runtime.rs` feeds it from the gateway task and from the
//! handle's send and receive calls. The design, the opcode table, and the
//! Decision Log are in `docs/design/dave-voice.md`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::num::NonZeroU16;

use tokio::sync::watch;

use super::DAVE_MAGIC_MARKER;
use crate::error::DiscordError;
use crate::model::{ChannelId, UserId};
use crate::voice::player::OPUS_SILENCE_FRAME;
use crate::voice::VoiceGatewayCommand;

/// The DAVE protocol version this crate implements (`davey` supports
/// version 1 only).
pub(crate) const SUPPORTED_DAVE_PROTOCOL_VERSION: u16 = davey::DAVE_PROTOCOL_VERSION;

/// A transition with this ID is a (re)initialization that is executed at
/// once, without a ready/execute round trip (DAVE protocol, "Transition ID
/// 0"; discord.js `DAVESession.prepareTransition`).
const INIT_TRANSITION_ID: u64 = 0;

/// `dave_protocol_prepare_epoch` with this epoch announces a new MLS group:
/// the client resets its group state and sends a new key package (DAVE
/// protocol, "Sole Member Reset" and "Protocol Upgrade").
const NEW_GROUP_EPOCH: u64 = 1;

/// How long receivers keep accepting frames under the previous transition's
/// rules after a transition executes, in seconds (DAVE protocol: previous
/// keys are kept "for up to ten seconds"; discord.js `TRANSITION_EXPIRY`).
const TRANSITION_EXPIRY_SECS: u32 = 10;

/// How long unencrypted frames are accepted while a downgrade to protocol
/// version 0 is pending, in seconds. Transitions usually take about three
/// seconds; this covers delayed connections (discord.js
/// `TRANSITION_EXPIRY_PENDING_DOWNGRADE`).
const DOWNGRADE_PASSTHROUGH_SECS: u32 = 24;

/// Consecutive undecryptable frames, outside any pending transition, after
/// which the runtime assumes it lost the group's keys and rejoins. A few
/// frames normally fail right after joining or re-initializing (discord.js
/// `DEFAULT_DECRYPTION_FAILURE_TOLERANCE`).
const DECRYPTION_FAILURE_TOLERANCE: u32 = 36;

/// `dave_mls_proposals` operation types (DAVE protocol, opcode 27).
const PROPOSALS_APPEND: u8 = 0;
const PROPOSALS_REVOKE: u8 = 1;

/// What the voice runtime's DAVE layer is doing, from
/// [`VoiceRuntimeHandle::dave_status`](crate::VoiceRuntimeHandle::dave_status).
///
/// DAVE is Discord's end-to-end encryption for voice
/// (<https://daveprotocol.com/>). Discord requires it in every voice channel
/// except Stage channels; the runtime joins the call's MLS group, encrypts
/// outgoing audio, and decrypts incoming audio on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VoiceDaveStatus {
    /// The DAVE protocol version in effect. `0` means the call uses
    /// transport encryption only (Stage channels, or a call downgraded for
    /// a client without DAVE support).
    pub protocol_version: u16,
    /// Whether this client is a member of the call's MLS group and holds
    /// its media keys. A client alone in a DAVE call stays not ready until
    /// someone joins: the group is established by the first commit.
    pub ready: bool,
    /// The MLS epoch of the group, once ready.
    pub epoch: Option<u64>,
    /// The voice privacy code of the group (DAVE "Displayable Codes"), the
    /// code Discord clients show for the call. Changes with every epoch.
    pub voice_privacy_code: Option<String>,
    /// The last transition executed.
    pub last_transition_id: Option<u64>,
    /// The latest DAVE failure (a rejected commit or welcome, a proposal
    /// that did not apply, ...). Kept until another failure replaces it.
    pub last_error: Option<String>,
    /// Received packets that were dropped because they could not be
    /// decrypted: frames during a transition, from a user whose SSRC is not
    /// known yet, or while this client had no keys.
    pub undecryptable_packets: u64,
    /// How many times the runtime reported an invalid commit or welcome (or
    /// persistent decryption failures) and rejoined the group with a new
    /// key package.
    pub recoveries: u64,
    can_send_audio: bool,
}

impl VoiceDaveStatus {
    /// Whether audio can be sent now: the call uses transport encryption
    /// only, this client holds the group's keys, or the call is still
    /// upgrading from transport-only encryption (until the upgrade executes,
    /// peers accept unencrypted frames).
    ///
    /// While this is `false` the plain send methods return an error instead
    /// of sending audio unencrypted into an end-to-end encrypted call, and an
    /// [`AudioPlayer`](crate::voice::player::AudioPlayer) treats the
    /// connection as not playable.
    pub fn can_send_audio(&self) -> bool {
        self.can_send_audio
    }
}

/// A received frame after DAVE: the media, or why it was dropped.
pub(crate) struct InboundFrame {
    /// The Opus frame, or `None` when the packet must be skipped.
    pub(crate) media: Option<Vec<u8>>,
    /// Commands to send because decryption kept failing (a recovery).
    pub(crate) commands: Vec<VoiceGatewayCommand>,
}

/// Which MLS message a transition carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransitionMessage {
    /// `dave_mls_announce_commit_transition` (29).
    Commit,
    /// `dave_mls_welcome` (30).
    Welcome,
}

impl fmt::Display for TransitionMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Commit => "commit",
            Self::Welcome => "welcome",
        })
    }
}

fn dave_error(message: impl Into<String>) -> DiscordError {
    DiscordError::voice(message)
}

/// The runtime's DAVE state for one voice connection.
///
/// Mirrors discord.js `DAVESession` (transitions, recovery, decryption
/// tolerance) with two deliberate differences, both explained in
/// `docs/design/dave-voice.md`: it does not send audio unencrypted while it
/// has no keys in an end-to-end encrypted call, and it keeps passthrough for
/// unencrypted frames open only for the transition expiry after an upgrade.
pub(crate) struct DaveController {
    user_id: u64,
    /// The MLS group ID; `None` makes DAVE impossible for this connection.
    channel_id: Option<u64>,
    /// The highest version this client advertised in IDENTIFY.
    advertised_version: u16,
    /// The version the session works toward: set by SESSION_DESCRIPTION, a
    /// new-group PREPARE_EPOCH, and EXECUTE_TRANSITION (discord.js
    /// `protocolVersion`).
    protocol_version: u16,
    /// The version of the last executed transition (or of
    /// SESSION_DESCRIPTION). Senders keep the previous transition's rules
    /// until the next one executes, so a 0 → 1 upgrade still sends
    /// unencrypted frames while the new group forms.
    active_version: u16,
    session: Option<davey::DaveSession>,
    /// The latest external sender package, applied again to a session that
    /// is created after it arrived (a call upgraded from version 0).
    external_sender: Option<Vec<u8>>,
    /// Prepared transitions: transition ID → protocol version.
    pending_transitions: HashMap<u64, u16>,
    last_transition_id: Option<u64>,
    /// Set from a recovery until the next transition executes, so a burst of
    /// failures reports the transition only once.
    reinitializing: bool,
    consecutive_failures: u32,
    /// Users the voice gateway reported as connected (CLIENTS_CONNECT minus
    /// CLIENT_DISCONNECT). Members must refuse proposals that add anyone else
    /// (DAVE protocol, "Validation").
    connected_users: HashSet<u64>,
    last_error: Option<String>,
    undecryptable_packets: u64,
    recoveries: u64,
    status_tx: watch::Sender<VoiceDaveStatus>,
}

impl fmt::Debug for DaveController {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaveController")
            .field("protocol_version", &self.protocol_version)
            .field("active_version", &self.active_version)
            .field("has_session", &self.session.is_some())
            .field("pending_transitions", &self.pending_transitions.len())
            .field("last_transition_id", &self.last_transition_id)
            .finish_non_exhaustive()
    }
}

impl DaveController {
    /// A controller for `user_id` in `channel_id` that advertised
    /// `advertised_version`. Call [`Self::on_session_description`] next.
    pub(crate) fn new(
        user_id: UserId,
        channel_id: Option<ChannelId>,
        advertised_version: u16,
    ) -> Self {
        let (status_tx, _) = watch::channel(VoiceDaveStatus {
            protocol_version: 0,
            ready: false,
            epoch: None,
            voice_privacy_code: None,
            last_transition_id: None,
            last_error: None,
            undecryptable_packets: 0,
            recoveries: 0,
            can_send_audio: true,
        });
        Self {
            user_id: user_id.get(),
            channel_id: channel_id.map(ChannelId::get),
            advertised_version,
            protocol_version: 0,
            active_version: 0,
            session: None,
            external_sender: None,
            pending_transitions: HashMap::new(),
            last_transition_id: None,
            reinitializing: false,
            consecutive_failures: 0,
            connected_users: HashSet::new(),
            last_error: None,
            undecryptable_packets: 0,
            recoveries: 0,
            status_tx,
        }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<VoiceDaveStatus> {
        self.status_tx.subscribe()
    }

    /// The DAVE protocol version in effect (0: transport encryption only).
    pub(crate) fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// SESSION_DESCRIPTION fixed the call's protocol version. With DAVE the
    /// client creates its session and sends a key package right away (DAVE
    /// protocol, "When the client sends its key package").
    pub(crate) fn on_session_description(&mut self, version: u16) -> Vec<VoiceGatewayCommand> {
        self.protocol_version = version;
        self.active_version = version;
        let commands = self.reinit();
        self.publish();
        commands
    }

    pub(crate) fn on_clients_connect(&mut self, user_ids: impl IntoIterator<Item = UserId>) {
        self.connected_users
            .extend(user_ids.into_iter().map(UserId::get));
    }

    pub(crate) fn on_client_disconnect(&mut self, user_id: UserId) {
        self.connected_users.remove(&user_id.get());
    }

    /// `dave_protocol_prepare_transition` (21).
    pub(crate) fn on_prepare_transition(
        &mut self,
        transition_id: u64,
        version: u16,
    ) -> Vec<VoiceGatewayCommand> {
        tracing::debug!(transition_id, version, "DAVE: preparing transition");
        self.pending_transitions.insert(transition_id, version);
        let commands = if transition_id == INIT_TRANSITION_ID {
            self.execute(transition_id);
            Vec::new()
        } else {
            if version == 0 {
                // A downgrade: peers switch to unencrypted frames when it
                // executes, which may reach us before our own execute.
                if let Some(session) = self.session.as_mut() {
                    session.set_passthrough_mode(true, Some(DOWNGRADE_PASSTHROUGH_SECS));
                }
            }
            vec![VoiceGatewayCommand::DaveProtocolTransitionReady { transition_id }]
        };
        self.publish();
        commands
    }

    /// `dave_protocol_execute_transition` (22).
    pub(crate) fn on_execute_transition(&mut self, transition_id: u64) {
        self.execute(transition_id);
        self.publish();
    }

    fn execute(&mut self, transition_id: u64) {
        let Some(version) = self.pending_transitions.remove(&transition_id) else {
            tracing::debug!(
                transition_id,
                "DAVE: execute for a transition that was not prepared"
            );
            return;
        };
        let previous = self.active_version;
        self.protocol_version = version;
        self.active_version = version;
        if previous == 0 && version > 0 {
            // The upgrade is done: peers encrypt from now on. Unencrypted
            // frames already in flight are accepted for the expiry, then
            // refused (libdave closes passthrough the same way; discord.js
            // leaves it open).
            if let Some(session) = self.session.as_mut() {
                session.set_passthrough_mode(false, Some(TRANSITION_EXPIRY_SECS));
            }
        }
        self.reinitializing = false;
        self.last_transition_id = Some(transition_id);
        tracing::debug!(
            transition_id,
            from = previous,
            to = version,
            "DAVE: transition executed"
        );
    }

    /// `dave_protocol_prepare_epoch` (24).
    pub(crate) fn on_prepare_epoch(
        &mut self,
        epoch: u64,
        version: u16,
    ) -> Vec<VoiceGatewayCommand> {
        tracing::debug!(epoch, version, "DAVE: preparing epoch");
        let commands = if epoch == NEW_GROUP_EPOCH {
            self.protocol_version = version;
            self.reinit()
        } else {
            Vec::new()
        };
        self.publish();
        commands
    }

    /// `dave_mls_external_sender_package` (25).
    pub(crate) fn on_external_sender(&mut self, external_sender: &[u8]) {
        self.external_sender = Some(external_sender.to_vec());
        if let Some(session) = self.session.as_mut() {
            if let Err(error) = session.set_external_sender(external_sender) {
                self.fail("could not apply the MLS external sender", error);
            }
        }
        self.publish();
    }

    /// `dave_mls_proposals` (27): `payload` is the operation type followed by
    /// the proposals.
    pub(crate) fn on_proposals(&mut self, payload: &[u8]) -> Vec<VoiceGatewayCommand> {
        let commands = self.process_proposals(payload);
        self.publish();
        commands
    }

    fn process_proposals(&mut self, payload: &[u8]) -> Vec<VoiceGatewayCommand> {
        let Some((&operation, proposals)) = payload.split_first() else {
            self.fail("MLS proposals", "the payload is empty");
            return Vec::new();
        };
        let operation = match operation {
            PROPOSALS_APPEND => davey::ProposalsOperationType::APPEND,
            PROPOSALS_REVOKE => davey::ProposalsOperationType::REVOKE,
            other => {
                self.fail("MLS proposals", format!("unknown operation type {other}"));
                return Vec::new();
            }
        };
        // This client is always expected in its own call.
        let recognized = self
            .connected_users
            .iter()
            .copied()
            .chain(std::iter::once(self.user_id))
            .collect::<Vec<_>>();
        let Some(session) = self.session.as_mut() else {
            tracing::debug!("DAVE: proposals without a session (protocol version 0)");
            return Vec::new();
        };
        match session.process_proposals(operation, proposals, Some(&recognized)) {
            Ok(Some(commit_welcome)) => vec![VoiceGatewayCommand::DaveMlsCommitWelcome {
                commit: commit_welcome.commit,
                welcome: commit_welcome.welcome,
            }],
            Ok(None) => Vec::new(),
            Err(error) => {
                self.fail("could not process MLS proposals", error);
                Vec::new()
            }
        }
    }

    /// `dave_mls_announce_commit_transition` (29): a 16-bit transition ID and
    /// the commit.
    pub(crate) fn on_announce_commit(&mut self, payload: &[u8]) -> Vec<VoiceGatewayCommand> {
        let commands = self.process_transition_message(TransitionMessage::Commit, payload);
        self.publish();
        commands
    }

    /// `dave_mls_welcome` (30): a 16-bit transition ID and the welcome.
    pub(crate) fn on_welcome(&mut self, payload: &[u8]) -> Vec<VoiceGatewayCommand> {
        let commands = self.process_transition_message(TransitionMessage::Welcome, payload);
        self.publish();
        commands
    }

    fn process_transition_message(
        &mut self,
        kind: TransitionMessage,
        payload: &[u8],
    ) -> Vec<VoiceGatewayCommand> {
        let Some((transition_id, message)) = payload.split_first_chunk::<2>() else {
            self.fail(
                format!("MLS {kind}"),
                "the payload is shorter than its transition ID",
            );
            return Vec::new();
        };
        let transition_id = u64::from(u16::from_be_bytes(*transition_id));
        let Some(session) = self.session.as_mut() else {
            tracing::debug!(%kind, transition_id, "DAVE: MLS message without a session");
            return Vec::new();
        };
        let result = match kind {
            TransitionMessage::Commit => session.process_commit(message).map_err(|e| e.to_string()),
            TransitionMessage::Welcome => {
                session.process_welcome(message).map_err(|e| e.to_string())
            }
        };
        if let Err(error) = result {
            self.fail(
                format!("MLS {kind} for transition {transition_id} was invalid"),
                error,
            );
            return self.recover(transition_id);
        }
        tracing::debug!(%kind, transition_id, "DAVE: MLS message processed");
        if self.active_version == 0 && self.protocol_version > 0 {
            // Upgrading: until the transition executes, peers still send
            // unencrypted frames, which the new group's receivers must take.
            if let Some(session) = self.session.as_mut() {
                session.set_passthrough_mode(true, None);
            }
        }
        if transition_id == INIT_TRANSITION_ID {
            // A (re)initialization: in effect at once, nothing to ack.
            self.reinitializing = false;
            self.active_version = self.protocol_version;
            self.last_transition_id = Some(transition_id);
            Vec::new()
        } else {
            self.pending_transitions
                .insert(transition_id, self.protocol_version);
            vec![VoiceGatewayCommand::DaveProtocolTransitionReady { transition_id }]
        }
    }

    /// Reports `transition_id` as invalid and rejoins with a new key package
    /// (DAVE protocol, "Recovery from invalid commit or welcome"; discord.js
    /// `recoverFromInvalidTransition`). A recovery already in progress is not
    /// repeated: the gateway removes us once and re-adds the new package.
    fn recover(&mut self, transition_id: u64) -> Vec<VoiceGatewayCommand> {
        if self.reinitializing {
            return Vec::new();
        }
        tracing::warn!(
            transition_id,
            "DAVE: reporting an invalid transition and rejoining the group"
        );
        self.reinitializing = true;
        self.consecutive_failures = 0;
        self.recoveries += 1;
        let mut commands = vec![VoiceGatewayCommand::DaveMlsInvalidCommitWelcome { transition_id }];
        commands.extend(self.reinit());
        commands
    }

    /// Starts over for `protocol_version`: a fresh session and key package
    /// for a DAVE version, or no group and open passthrough for version 0.
    fn reinit(&mut self) -> Vec<VoiceGatewayCommand> {
        let Some(version) = NonZeroU16::new(self.protocol_version) else {
            let reset = self.session.as_mut().map(|session| {
                let reset = session.reset();
                session.set_passthrough_mode(true, Some(TRANSITION_EXPIRY_SECS));
                reset
            });
            if let Some(Err(error)) = reset {
                self.fail("could not reset the DAVE session", error);
            }
            return Vec::new();
        };
        match self.fresh_key_package(version) {
            Ok(key_package) => vec![VoiceGatewayCommand::DaveMlsKeyPackage { key_package }],
            Err(error) => {
                self.fail("could not start a DAVE session", error);
                Vec::new()
            }
        }
    }

    fn fresh_key_package(&mut self, version: NonZeroU16) -> Result<Vec<u8>, String> {
        if version.get() > self.advertised_version {
            return Err(format!(
                "Discord negotiated DAVE protocol version {version}, but this connection \
                 advertised {} ({})",
                self.advertised_version,
                if self.channel_id.is_none() {
                    "VoiceRuntimeConfig::channel_id is not set"
                } else {
                    "max_dave_protocol_version is lower"
                }
            ));
        }
        let channel_id = self
            .channel_id
            .ok_or("DAVE needs the voice channel ID: set VoiceRuntimeConfig::channel_id")?;
        if let Some(session) = self.session.as_mut() {
            // Keeps the external sender and creates the pending group again.
            session
                .reinit(version, self.user_id, channel_id, None)
                .map_err(|error| error.to_string())?;
        } else {
            let mut session = davey::DaveSession::new(version, self.user_id, channel_id, None)
                .map_err(|error| error.to_string())?;
            if let Some(external_sender) = self.external_sender.as_deref() {
                session
                    .set_external_sender(external_sender)
                    .map_err(|error| error.to_string())?;
            }
            self.session = Some(session);
        }
        self.session
            .as_mut()
            .ok_or("the DAVE session was not created")?
            .create_key_package()
            .map_err(|error| error.to_string())
    }

    fn fail(&mut self, context: impl fmt::Display, error: impl fmt::Display) {
        let message = format!("{context}: {error}");
        tracing::warn!(error = %message, "DAVE failure");
        self.last_error = Some(message);
    }

    fn is_ready(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(davey::DaveSession::is_ready)
    }

    fn can_send_audio(&self) -> bool {
        self.protocol_version == 0 || self.active_version == 0 || self.is_ready()
    }

    /// Prepares an outbound Opus frame: DAVE-encrypted when the call is end
    /// to end encrypted, unchanged otherwise.
    ///
    /// # Errors
    ///
    /// When the call is end-to-end encrypted and this client holds no keys
    /// yet, so the frame cannot be sent without leaking it unencrypted.
    pub(crate) fn encrypt_outbound<'a>(
        &mut self,
        opus_frame: &'a [u8],
    ) -> Result<Cow<'a, [u8]>, DiscordError> {
        // Silence goes out unencrypted in every state (DAVE protocol,
        // "Silence packets"; discord.js `DAVESession.encrypt`), which keeps
        // silence tails working while a group re-forms.
        if opus_frame == OPUS_SILENCE_FRAME || self.protocol_version == 0 {
            return Ok(Cow::Borrowed(opus_frame));
        }
        if let Some(session) = self.session.as_mut().filter(|session| session.is_ready()) {
            return session
                .encrypt_opus(opus_frame)
                .map(|frame| Cow::Owned(frame.into_owned()))
                .map_err(|error| {
                    dave_error(format!("DAVE could not encrypt an Opus frame: {error}"))
                });
        }
        if self.active_version == 0 {
            // Still upgrading from transport-only encryption: the previous
            // rules apply until the upgrade executes.
            return Ok(Cow::Borrowed(opus_frame));
        }
        Err(self.not_ready_error())
    }

    fn not_ready_error(&self) -> DiscordError {
        let why = if self.channel_id.is_none() || self.advertised_version == 0 {
            "this connection cannot run DAVE (set VoiceRuntimeConfig::channel_id and keep \
             max_dave_protocol_version at its default)"
        } else if self.reinitializing {
            "it is rejoining the MLS group after an invalid transition"
        } else {
            "the MLS group is not established yet; it forms once another member is in the call"
        };
        dave_error(format!(
            "audio not sent: the call is end-to-end encrypted (DAVE) and this client holds no \
             media keys yet because {why}"
        ))
    }

    /// Removes DAVE encryption from a received frame of `user_id`.
    ///
    /// Frames that cannot be decrypted are dropped, not returned as errors:
    /// a few always fail around transitions (discord.js `DAVESession.decrypt`
    /// ignores them too), and an error would end receive loops that use `?`.
    pub(crate) fn decrypt_inbound(
        &mut self,
        user_id: Option<UserId>,
        frame: Vec<u8>,
    ) -> InboundFrame {
        let media = |media| InboundFrame {
            media: Some(media),
            commands: Vec::new(),
        };
        if frame.as_slice() == OPUS_SILENCE_FRAME {
            // Discord's SFU synthesizes silence for muted users without DAVE
            // (DAVE protocol, "Silence packets").
            return media(frame);
        }
        let Some(session) = self.session.as_mut() else {
            // DAVE never ran on this connection: transport encryption only,
            // and an Opus frame may end in any bytes.
            return media(frame);
        };
        let user = user_id.map(UserId::get);
        let can_decrypt = session.is_ready()
            && (self.protocol_version != 0
                || user.is_some_and(|user| session.can_passthrough(user)));
        // A DAVE frame always ends with its magic marker; an unencrypted
        // Opus frame from a call without DAVE (or during a transition) is
        // passed through. discord.js passes every frame through here, which
        // hands DAVE ciphertext to the Opus decoder.
        let is_dave_frame = frame.ends_with(&DAVE_MAGIC_MARKER);
        if !can_decrypt {
            return if is_dave_frame {
                self.drop_frame("no media keys yet")
            } else {
                media(frame)
            };
        }
        let Some(user) = user else {
            return self.drop_frame("the sender's SSRC has no known user yet");
        };
        match session.decrypt(user, davey::MediaType::AUDIO, &frame) {
            Ok(decrypted) => {
                self.consecutive_failures = 0;
                media(decrypted)
            }
            Err(error) => {
                tracing::debug!(user, %error, "DAVE: dropped a frame that did not decrypt");
                let commands = self.count_failure();
                let mut dropped = self.drop_frame("decryption failed");
                dropped.commands = commands;
                dropped
            }
        }
    }

    fn drop_frame(&mut self, why: &str) -> InboundFrame {
        tracing::trace!(why, "DAVE: dropped a received frame");
        self.undecryptable_packets = self.undecryptable_packets.saturating_add(1);
        self.publish();
        InboundFrame {
            media: None,
            commands: Vec::new(),
        }
    }

    /// Counts a decryption failure outside transitions and starts a recovery
    /// once they exceed the tolerance.
    fn count_failure(&mut self) -> Vec<VoiceGatewayCommand> {
        if self.reinitializing || !self.pending_transitions.is_empty() {
            return Vec::new();
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures <= DECRYPTION_FAILURE_TOLERANCE {
            return Vec::new();
        }
        self.consecutive_failures = 0;
        match self.last_transition_id {
            Some(transition_id) => {
                self.fail(
                    "DAVE frames keep failing to decrypt",
                    format!("rejoining the group (last transition {transition_id})"),
                );
                self.recover(transition_id)
            }
            None => {
                self.fail(
                    "DAVE frames keep failing to decrypt",
                    "no transition executed yet, so there is none to report",
                );
                Vec::new()
            }
        }
    }

    /// The pairwise verification code with `user_id` (DAVE "Verification
    /// Fingerprint", shown by Discord clients when verifying a user).
    pub(crate) fn verification_code(&self, user_id: UserId) -> Result<String, DiscordError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| dave_error("this voice connection has no DAVE session"))?;
        session
            .get_verification_code(user_id.get())
            .map_err(|error| dave_error(format!("no DAVE verification code: {error}")))
    }

    pub(crate) fn status(&self) -> VoiceDaveStatus {
        let ready = self.is_ready();
        let session = self.session.as_ref().filter(|_| ready);
        VoiceDaveStatus {
            protocol_version: self.protocol_version,
            ready,
            epoch: session
                .and_then(davey::DaveSession::epoch)
                .map(|epoch| epoch.as_u64()),
            voice_privacy_code: session
                .filter(|_| self.protocol_version > 0)
                .and_then(davey::DaveSession::voice_privacy_code)
                .map(str::to_owned),
            last_transition_id: self.last_transition_id,
            last_error: self.last_error.clone(),
            undecryptable_packets: self.undecryptable_packets,
            recoveries: self.recoveries,
            can_send_audio: self.can_send_audio(),
        }
    }

    /// Publishes the status when it changed. Called after every change; the
    /// hot path (a frame that decrypts) changes nothing and skips it.
    fn publish(&self) {
        let status = self.status();
        self.status_tx.send_if_modified(|current| {
            if *current == status {
                false
            } else {
                *current = status;
                true
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod test_gateway {
    //! The voice gateway's half of DAVE, for tests: an MLS external sender
    //! that signs Add proposals, built with OpenMLS as the voice gateway's
    //! external sender is (DAVE protocol, "External Sender").

    use openmls::prelude::tls_codec::DeserializeBytes as _;
    use openmls::prelude::tls_codec::Serialize as _;
    use openmls::prelude::{
        BasicCredential, ExternalProposal, ExternalSender, GroupEpoch, GroupId, KeyPackageIn,
        MlsMessageIn, OpenMlsProvider, ProtocolVersion, SenderExtensionIndex, SignatureScheme,
        VLBytes,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;

    use crate::voice::VoiceGatewayCommand;

    pub(crate) struct MockDaveGateway {
        signer: SignatureKeyPair,
        provider: OpenMlsRustCrypto,
        external_sender: Vec<u8>,
        channel_id: u64,
    }

    impl MockDaveGateway {
        pub(crate) fn new(channel_id: u64) -> Self {
            // DAVE v1 uses MLS_128_DHKEMP256_AES128GCM_SHA256_P256, whose
            // signatures are ECDSA P-256.
            let signer = SignatureKeyPair::new(SignatureScheme::ECDSA_SECP256R1_SHA256).unwrap();
            let credential = BasicCredential::new(b"voice-gateway".to_vec());
            let external_sender = ExternalSender::new(signer.public().into(), credential.into())
                .tls_serialize_detached()
                .unwrap();
            Self {
                signer,
                provider: OpenMlsRustCrypto::default(),
                external_sender,
                channel_id,
            }
        }

        /// The `dave_mls_external_sender_package` (25) payload.
        pub(crate) fn external_sender(&self) -> &[u8] {
            &self.external_sender
        }

        /// A `dave_mls_proposals` (27) payload that appends an external Add
        /// proposal for `key_package` to the channel's group at `epoch`.
        pub(crate) fn add_proposal(&self, key_package: &[u8], epoch: u64) -> Vec<u8> {
            let key_package = KeyPackageIn::tls_deserialize_exact_bytes(key_package)
                .unwrap()
                .validate(self.provider.crypto(), ProtocolVersion::Mls10)
                .unwrap();
            let proposal = ExternalProposal::new_add::<OpenMlsRustCrypto>(
                key_package,
                GroupId::from_slice(&self.channel_id.to_be_bytes()),
                GroupEpoch::from(epoch),
                &self.signer,
                SenderExtensionIndex::new(0),
            )
            .unwrap()
            .tls_serialize_detached()
            .unwrap();
            let mut payload = vec![super::PROPOSALS_APPEND];
            payload.extend(VLBytes::new(proposal).tls_serialize_detached().unwrap());
            payload
        }
    }

    /// Splits a `dave_mls_commit_welcome` (28) payload into the commit (an
    /// MLSMessage) and the welcome that follows it, as the voice gateway
    /// does before announcing the commit and sending the welcome.
    pub(crate) fn split_commit_welcome(payload: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let (_, welcome) = MlsMessageIn::tls_deserialize_bytes(payload).unwrap();
        let commit_len = payload.len() - welcome.len();
        (payload[..commit_len].to_vec(), welcome.to_vec())
    }

    /// The payload of `dave_mls_announce_commit_transition` (29) or
    /// `dave_mls_welcome` (30): a big-endian transition ID, then the message.
    pub(crate) fn transition(transition_id: u16, message: &[u8]) -> Vec<u8> {
        [&transition_id.to_be_bytes()[..], message].concat()
    }

    /// The key package among `commands`.
    pub(crate) fn key_package(commands: &[VoiceGatewayCommand]) -> Vec<u8> {
        commands
            .iter()
            .find_map(|command| match command {
                VoiceGatewayCommand::DaveMlsKeyPackage { key_package } => Some(key_package.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no key package in {commands:?}"))
    }

    /// The commit and welcome among `commands`.
    pub(crate) fn commit_welcome(commands: &[VoiceGatewayCommand]) -> (Vec<u8>, Option<Vec<u8>>) {
        commands
            .iter()
            .find_map(|command| match command {
                VoiceGatewayCommand::DaveMlsCommitWelcome { commit, welcome } => {
                    Some((commit.clone(), welcome.clone()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no commit in {commands:?}"))
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::test_gateway::{commit_welcome, key_package, transition, MockDaveGateway};
    use super::{DaveController, DECRYPTION_FAILURE_TOLERANCE};
    use crate::model::{test_id, UserId};
    use crate::voice::VoiceGatewayCommand;

    const BOT: u64 = 10;
    const PEER: u64 = 20;
    const CHANNEL: u64 = 500;
    const OPUS: [u8; 6] = [0x78, 1, 2, 3, 4, 5];
    const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];

    fn user(id: u64) -> Option<UserId> {
        Some(test_id(id))
    }

    fn controller(user_id: u64) -> DaveController {
        DaveController::new(test_id(user_id), Some(test_id(CHANNEL)), 1)
    }

    fn ready(transition_id: u64) -> VoiceGatewayCommand {
        VoiceGatewayCommand::DaveProtocolTransitionReady { transition_id }
    }

    /// The bot alone in a DAVE call, then a peer joins: the gateway asks the
    /// bot (its group's only member) to add the peer, the bot commits, the
    /// gateway announces the commit to the bot and welcomes the peer, both
    /// report ready, and the transition executes.
    fn established_call() -> (DaveController, DaveController, MockDaveGateway) {
        let gateway = MockDaveGateway::new(CHANNEL);
        let mut bot = controller(BOT);
        let bot_key_package = key_package(&bot.on_session_description(1));
        assert!(!bot_key_package.is_empty());
        bot.on_external_sender(gateway.external_sender());
        assert!(
            !bot.status().can_send_audio(),
            "alone in a DAVE call: no keys yet"
        );

        let mut peer = controller(PEER);
        let peer_key_package = key_package(&peer.on_session_description(1));
        peer.on_external_sender(gateway.external_sender());

        bot.on_clients_connect([test_id(PEER)]);
        let commands = bot.on_proposals(&gateway.add_proposal(&peer_key_package, 0));
        let (commit, welcome) = commit_welcome(&commands);
        let welcome = welcome.expect("an Add commit carries a welcome");

        assert_eq!(
            bot.on_announce_commit(&transition(7, &commit)),
            vec![ready(7)]
        );
        assert_eq!(peer.on_welcome(&transition(7, &welcome)), vec![ready(7)]);
        bot.on_execute_transition(7);
        peer.on_execute_transition(7);
        (bot, peer, gateway)
    }

    fn encrypt(controller: &mut DaveController, frame: &[u8]) -> Vec<u8> {
        controller.encrypt_outbound(frame).unwrap().into_owned()
    }

    fn decrypt(controller: &mut DaveController, from: u64, frame: Vec<u8>) -> Option<Vec<u8>> {
        let inbound = controller.decrypt_inbound(user(from), frame);
        assert!(inbound.commands.is_empty(), "{:?}", inbound.commands);
        inbound.media
    }

    #[test]
    fn dave_members_exchange_audio_after_commit_and_welcome() {
        let (mut bot, mut peer, _gateway) = established_call();

        for (status, other) in [(bot.status(), PEER), (peer.status(), BOT)] {
            assert!(status.ready && status.can_send_audio(), "{status:?}");
            assert_eq!(status.protocol_version, 1);
            assert_eq!(status.epoch, Some(1));
            assert_eq!(status.last_transition_id, Some(7));
            assert!(status.last_error.is_none(), "{:?}", status.last_error);
            assert!(status.voice_privacy_code.is_some(), "{other}");
        }
        // One group, so one privacy code; pairwise codes agree too.
        assert_eq!(
            bot.status().voice_privacy_code,
            peer.status().voice_privacy_code
        );
        assert_eq!(
            bot.verification_code(test_id(PEER)).unwrap(),
            peer.verification_code(test_id(BOT)).unwrap()
        );
        assert!(bot.verification_code(test_id(99)).is_err());

        // Opus → DAVE frame (magic marker last) → the other member's keys.
        let frame = encrypt(&mut bot, &OPUS);
        assert_ne!(frame, OPUS);
        assert!(frame.ends_with(&super::DAVE_MAGIC_MARKER));
        assert_eq!(decrypt(&mut peer, BOT, frame), Some(OPUS.to_vec()));
        let frame = encrypt(&mut peer, &OPUS);
        assert_eq!(decrypt(&mut bot, PEER, frame), Some(OPUS.to_vec()));

        // Every frame gets a fresh DAVE nonce.
        assert_ne!(encrypt(&mut bot, &OPUS), encrypt(&mut bot, &OPUS));

        // Silence is never encrypted, and passes in both directions.
        assert!(matches!(
            bot.encrypt_outbound(&SILENCE),
            Ok(Cow::Borrowed(_))
        ));
        assert_eq!(
            decrypt(&mut peer, BOT, SILENCE.to_vec()),
            Some(SILENCE.to_vec())
        );
    }

    #[test]
    fn dave_drops_frames_it_cannot_attribute_or_decrypt() {
        let (mut bot, mut peer, _gateway) = established_call();
        let frame = encrypt(&mut peer, &OPUS);

        // Before SPEAKING maps the SSRC there is no user, so no key.
        let inbound = bot.decrypt_inbound(None, frame.clone());
        assert!(inbound.media.is_none());
        // Plaintext is refused once the call is end-to-end encrypted.
        assert_eq!(decrypt(&mut bot, PEER, OPUS.to_vec()), None);
        // Tampered ciphertext fails authentication.
        let mut tampered = frame;
        tampered[0] ^= 0x01;
        assert_eq!(decrypt(&mut bot, PEER, tampered), None);
        assert_eq!(bot.status().undecryptable_packets, 3);
    }

    #[test]
    fn dave_without_keys_refuses_to_send_and_drops_dave_frames() {
        let mut bot = controller(BOT);
        bot.on_session_description(1);

        let error = bot.encrypt_outbound(&OPUS).unwrap_err().to_string();
        assert!(error.contains("not established yet"), "{error}");
        // Silence still goes out (and silence tails keep working).
        assert!(matches!(
            bot.encrypt_outbound(&SILENCE),
            Ok(Cow::Borrowed(_))
        ));

        // An unencrypted frame (a peer mid-transition) passes; a DAVE frame
        // cannot be decrypted without keys and is dropped instead of being
        // handed to the Opus decoder.
        assert_eq!(decrypt(&mut bot, PEER, OPUS.to_vec()), Some(OPUS.to_vec()));
        let dave_frame = [&OPUS[..], &[0; 9], &[12, 0xfa, 0xfa]].concat();
        assert_eq!(decrypt(&mut bot, PEER, dave_frame), None);
        assert_eq!(bot.status().undecryptable_packets, 1);
    }

    #[test]
    fn dave_reports_an_invalid_commit_once_and_rejoins_with_a_new_key_package() {
        let (mut bot, mut peer, gateway) = established_call();

        let commands = bot.on_announce_commit(&transition(9, b"not an MLS commit"));
        assert_eq!(
            commands[0],
            VoiceGatewayCommand::DaveMlsInvalidCommitWelcome { transition_id: 9 }
        );
        let new_key_package = key_package(&commands);
        let status = bot.status();
        assert_eq!(status.recoveries, 1);
        assert!(!status.ready && !status.can_send_audio());
        assert!(status
            .last_error
            .unwrap()
            .contains("transition 9 was invalid"));
        assert!(bot
            .encrypt_outbound(&OPUS)
            .unwrap_err()
            .to_string()
            .contains("rejoining"));

        // A second bad message while rejoining is not reported again.
        assert!(bot.on_welcome(&transition(10, b"garbage")).is_empty());
        assert_eq!(bot.status().recoveries, 1);

        // The gateway adds the new key package; the remaining member commits
        // and the bot is welcomed back.
        peer.on_clients_connect([test_id(BOT)]);
        let (commit, welcome) =
            commit_welcome(&peer.on_proposals(&gateway.add_proposal(&new_key_package, 1)));
        assert_eq!(
            peer.on_announce_commit(&transition(11, &commit)),
            vec![ready(11)]
        );
        assert_eq!(
            bot.on_welcome(&transition(11, &welcome.unwrap())),
            vec![ready(11)]
        );
        bot.on_execute_transition(11);
        peer.on_execute_transition(11);
        assert!(bot.status().can_send_audio());
        let frame = encrypt(&mut bot, &OPUS);
        assert_eq!(decrypt(&mut peer, BOT, frame), Some(OPUS.to_vec()));
    }

    #[test]
    fn dave_reports_an_invalid_welcome() {
        let gateway = MockDaveGateway::new(CHANNEL);
        let mut bot = controller(BOT);
        bot.on_session_description(1);
        bot.on_external_sender(gateway.external_sender());

        let commands = bot.on_welcome(&transition(3, b"not a welcome"));
        assert_eq!(
            commands[0],
            VoiceGatewayCommand::DaveMlsInvalidCommitWelcome { transition_id: 3 }
        );
        assert!(matches!(
            commands[1],
            VoiceGatewayCommand::DaveMlsKeyPackage { .. }
        ));
        // The new transition clears the recovery.
        assert!(bot.on_prepare_transition(0, 1).is_empty());
        assert!(!bot.reinitializing);
    }

    #[test]
    fn dave_recovers_after_too_many_undecryptable_frames_outside_transitions() {
        let (mut bot, mut peer, _gateway) = established_call();
        let mut bad = encrypt(&mut peer, &OPUS);
        bad[0] ^= 0x01;

        // During a pending transition failures are expected and not counted.
        assert_eq!(bot.on_prepare_transition(8, 1), vec![ready(8)]);
        for _ in 0..=DECRYPTION_FAILURE_TOLERANCE {
            assert!(bot
                .decrypt_inbound(user(PEER), bad.clone())
                .commands
                .is_empty());
        }
        bot.on_execute_transition(8);

        for _ in 0..DECRYPTION_FAILURE_TOLERANCE {
            assert!(bot
                .decrypt_inbound(user(PEER), bad.clone())
                .commands
                .is_empty());
        }
        // A good frame resets the count.
        let good = encrypt(&mut peer, &OPUS);
        assert_eq!(decrypt(&mut bot, PEER, good), Some(OPUS.to_vec()));
        for _ in 0..DECRYPTION_FAILURE_TOLERANCE {
            assert!(bot
                .decrypt_inbound(user(PEER), bad.clone())
                .commands
                .is_empty());
        }
        let commands = bot.decrypt_inbound(user(PEER), bad).commands;
        assert_eq!(
            commands[0],
            VoiceGatewayCommand::DaveMlsInvalidCommitWelcome { transition_id: 8 }
        );
        assert!(matches!(
            commands[1],
            VoiceGatewayCommand::DaveMlsKeyPackage { .. }
        ));
        assert_eq!(bot.status().recoveries, 1);
    }

    #[test]
    fn dave_downgrades_to_plaintext_and_upgrades_through_a_new_group() {
        let (mut bot, mut peer, gateway) = established_call();

        // Downgrade: ready at once, unencrypted frames accepted right away
        // (a peer may execute first), plaintext sent only after execute.
        assert_eq!(bot.on_prepare_transition(12, 0), vec![ready(12)]);
        assert_eq!(decrypt(&mut bot, PEER, OPUS.to_vec()), Some(OPUS.to_vec()));
        assert!(matches!(bot.encrypt_outbound(&OPUS), Ok(Cow::Owned(_))));
        bot.on_execute_transition(12);
        assert_eq!(bot.status().protocol_version, 0);
        assert!(bot.status().voice_privacy_code.is_none());
        assert!(matches!(bot.encrypt_outbound(&OPUS), Ok(Cow::Borrowed(_))));
        // Stragglers encrypted under the old group still decrypt.
        let straggler = encrypt(&mut peer, &OPUS);
        assert_eq!(decrypt(&mut bot, PEER, straggler), Some(OPUS.to_vec()));
        peer.on_prepare_transition(12, 0);
        peer.on_execute_transition(12);

        // Upgrade: a new group for version 1. Until it executes, frames go
        // out unencrypted, which the peers still accept.
        let bot_key_package = key_package(&bot.on_prepare_epoch(1, 1));
        let peer_key_package = key_package(&peer.on_prepare_epoch(1, 1));
        assert!(!bot_key_package.is_empty());
        let status = bot.status();
        assert!(!status.ready && status.can_send_audio(), "{status:?}");
        assert!(matches!(bot.encrypt_outbound(&OPUS), Ok(Cow::Borrowed(_))));

        let (commit, welcome) =
            commit_welcome(&bot.on_proposals(&gateway.add_proposal(&peer_key_package, 0)));
        assert_eq!(
            bot.on_announce_commit(&transition(13, &commit)),
            vec![ready(13)]
        );
        assert_eq!(
            peer.on_welcome(&transition(13, &welcome.unwrap())),
            vec![ready(13)]
        );
        // Keys exist now: encrypted from here, while a peer that has not
        // switched yet may still send plaintext until the upgrade executes.
        let frame = encrypt(&mut bot, &OPUS);
        assert_eq!(decrypt(&mut peer, BOT, frame), Some(OPUS.to_vec()));
        assert_eq!(decrypt(&mut bot, PEER, OPUS.to_vec()), Some(OPUS.to_vec()));
        bot.on_execute_transition(13);
        assert_eq!(bot.status().protocol_version, 1);
        assert_eq!(bot.status().last_transition_id, Some(13));
    }

    #[test]
    fn dave_session_upgraded_from_version_zero_uses_the_stored_external_sender() {
        let gateway = MockDaveGateway::new(CHANNEL);
        let mut bot = controller(BOT);
        assert!(bot.on_session_description(0).is_empty());
        // Version 0: everything passes, even a frame ending like a DAVE
        // frame (an Opus frame may end in any bytes).
        let looks_like_dave = [0x78, 0xfa, 0xfa];
        assert_eq!(
            decrypt(&mut bot, PEER, looks_like_dave.to_vec()),
            Some(looks_like_dave.to_vec())
        );
        assert!(matches!(bot.encrypt_outbound(&OPUS), Ok(Cow::Borrowed(_))));
        // Proposals and MLS messages need a session; without one they are
        // ignored.
        bot.on_external_sender(gateway.external_sender());
        assert!(bot.on_proposals(&[0]).is_empty());
        assert!(bot.on_welcome(&transition(1, b"x")).is_empty());

        let commands = bot.on_prepare_epoch(1, 1);
        key_package(&commands);
        // The external sender from before the session applies to it: the
        // bot can commit an Add for a peer.
        let mut peer = controller(PEER);
        let peer_key_package = key_package(&peer.on_session_description(1));
        bot.on_clients_connect([test_id(PEER)]);
        let commands = bot.on_proposals(&gateway.add_proposal(&peer_key_package, 0));
        commit_welcome(&commands);
        // An epoch other than 1 keeps the group.
        assert!(bot.on_prepare_epoch(2, 1).is_empty());
    }

    #[test]
    fn dave_refuses_proposals_for_users_not_in_the_call() {
        let gateway = MockDaveGateway::new(CHANNEL);
        let mut bot = controller(BOT);
        bot.on_session_description(1);
        bot.on_external_sender(gateway.external_sender());
        let mut peer = controller(PEER);
        let peer_key_package = key_package(&peer.on_session_description(1));

        // No CLIENTS_CONNECT for the peer (or it disconnected).
        bot.on_clients_connect([test_id(PEER)]);
        bot.on_client_disconnect(test_id(PEER));
        assert!(bot
            .on_proposals(&gateway.add_proposal(&peer_key_package, 0))
            .is_empty());
        let error = bot.status().last_error.unwrap();
        assert!(error.contains("could not process MLS proposals"), "{error}");
    }

    #[test]
    fn dave_reports_malformed_messages_without_answering_them() {
        let mut bot = controller(BOT);
        bot.on_session_description(1);
        for payload in [&[][..], &[7, 0]] {
            assert!(bot.on_proposals(payload).is_empty());
        }
        assert!(bot
            .status()
            .last_error
            .unwrap()
            .contains("unknown operation type 7"));
        // A revoke of nothing leaves nothing to commit.
        let gateway = MockDaveGateway::new(CHANNEL);
        bot.on_external_sender(gateway.external_sender());
        assert!(bot.on_proposals(&[1, 0]).is_empty());
        // Shorter than the transition ID.
        assert!(bot.on_announce_commit(&[0]).is_empty());
        assert!(bot
            .status()
            .last_error
            .unwrap()
            .contains("shorter than its transition ID"));
        // Execute for a transition nobody prepared changes nothing.
        bot.on_execute_transition(44);
        assert_eq!(bot.status().last_transition_id, None);
        // The external sender cannot change under an established group.
        let (mut member, _, gateway) = established_call();
        member.on_external_sender(gateway.external_sender());
        assert!(member
            .status()
            .last_error
            .unwrap()
            .contains("could not apply the MLS external sender"));
    }

    #[test]
    fn dave_cannot_run_without_a_channel_or_above_the_advertised_version() {
        let mut no_channel = DaveController::new(test_id(BOT), None, 0);
        assert!(no_channel.on_session_description(1).is_empty());
        let status = no_channel.status();
        assert!(!status.can_send_audio());
        assert!(status
            .last_error
            .unwrap()
            .contains("VoiceRuntimeConfig::channel_id is not set"));
        let error = no_channel.encrypt_outbound(&OPUS).unwrap_err().to_string();
        assert!(
            error.contains("set VoiceRuntimeConfig::channel_id"),
            "{error}"
        );
        assert!(no_channel.verification_code(test_id(PEER)).is_err());

        let mut capped = DaveController::new(test_id(BOT), Some(test_id(CHANNEL)), 0);
        assert!(capped.on_session_description(1).is_empty());
        assert!(capped
            .status()
            .last_error
            .unwrap()
            .contains("max_dave_protocol_version is lower"));
    }

    #[test]
    fn dave_status_is_published_when_it_changes() {
        let gateway = MockDaveGateway::new(CHANNEL);
        let mut bot = controller(BOT);
        let mut status = bot.subscribe();
        assert!(status.borrow_and_update().can_send_audio());
        bot.on_session_description(1);
        assert!(status.has_changed().unwrap());
        assert!(!status.borrow_and_update().can_send_audio());

        // Nothing visible changed.
        bot.on_clients_connect([test_id(PEER)]);
        bot.on_external_sender(gateway.external_sender());
        assert!(!status.has_changed().unwrap());

        let mut peer = controller(PEER);
        let peer_key_package = key_package(&peer.on_session_description(1));
        let (commit, _) =
            commit_welcome(&bot.on_proposals(&gateway.add_proposal(&peer_key_package, 0)));
        bot.on_announce_commit(&transition(0, &commit));
        // Transition 0 is in effect at once, without a ready.
        let current = status.borrow_and_update().clone();
        assert!(current.ready && current.can_send_audio());
        assert_eq!(current.last_transition_id, Some(0));
        assert!(format!("{bot:?}").contains("DaveController"));
    }
}
