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

use davey::aead::consts::{U12, U8};
use davey::aead::generic_array::GenericArray;
use davey::{AeadInPlace, KeyInit};
use hmac::{Hmac, Mac};
use openmls_rust_crypto::RustCrypto;
use sha2::Sha256;
use tokio::sync::watch;

use super::{DAVE_AUTH_TAG_LEN, DAVE_FOOTER_LEN, DAVE_MAGIC_MARKER};
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

/// The MLS exporter label each member's media key derives from, with the
/// member's user ID (little-endian) as context (DAVE protocol, "Sender Key
/// Derivation"; davey `USER_MEDIA_KEY_BASE_LABEL`).
const SENDER_KEY_LABEL: &str = "Discord Secure Frames v0";
/// AES-128-GCM key length, which is also the exported secret's length.
const SENDER_KEY_LEN: usize = 16;
/// Length of the ratchet's chaining secret: the SHA-256 output length of
/// DAVE v1's ciphersuite.
const RATCHET_SECRET_LEN: usize = 32;
/// The 32-bit truncated frame nonce carries the key generation in its top
/// byte (DAVE protocol, "Frame Encryption"; davey
/// `RATCHET_GENERATION_SHIFT_BITS`).
const GENERATION_SHIFT: u32 = 24;
/// The AES-GCM nonce is 12 bytes: zeros, then the truncated nonce
/// little-endian in the last four.
const FRAME_NONCE_LEN: usize = 12;
const FRAME_NONCE_OFFSET: usize = FRAME_NONCE_LEN - 4;

/// DAVE frame encryption: AES-128-GCM with the tag truncated to 8 bytes,
/// from davey's public truncated-tag AES-GCM.
type FrameCipher = davey::AesGcm<davey::aes::Aes128, U12, U8>;

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

/// This member's sending key for one MLS epoch: DAVE's hash ratchet over
/// the exported secret, and the frame counter.
///
/// [Decision Log]
/// - Purpose: frames must keep the previous epoch's key until the
///   transition executes (DAVE protocol, "Group Transitions": "media senders
///   will continue to use key ratchets from the previous epoch until the
///   transition is executed"); members that have not processed the commit
///   yet can only decrypt that.
/// - Constraint: `davey::DaveSession::process_commit` / `process_welcome`
///   replace the session's internal encryptor key at once
///   (`update_ratchets`), and davey has no API to defer that or to keep the
///   old key. libdave keeps the encryptor separate (`Encryptor::
///   SetKeyRatchet`) and leaves the switch to its caller.
/// - Alternatives: (1) process commits only on execute: receive keys come
///   late, and `transition_ready` would go out before the commit was
///   validated; (2) stop sending between commit and execute: audio stalls
///   whenever someone joins or leaves; (3) keep our own sending key, derived
///   the way davey and libdave derive it, and switch it on execute.
/// - Chosen: (3). After each commit or welcome the runtime exports the new
///   epoch's secret from davey's MLS group (the same `export_secret` call
///   davey makes) and keeps it until the transition executes; frames are
///   encrypted here. davey still runs MLS and decryption, whose decryptors
///   keep the previous epoch's keys for ten seconds.
/// - Trade-offs: about a hundred lines of key derivation and Opus frame
///   encryption mirror davey's; a test checks them byte for byte against
///   davey's own encryptor and against davey's ratchet test vector. Only Opus
///   is handled (it is encrypted whole: no unencrypted ranges). The counter
///   is checked, so after 2^32 - 1 frames in one epoch (about 2.7 years of
///   audio) sends fail instead of reusing a nonce; davey's own encryptor
///   would overflow there (a panic in debug builds).
struct SenderKey {
    /// The ratchet secret for `next_generation`.
    next_secret: Vec<u8>,
    next_generation: u32,
    /// The cipher for one generation, derived when first needed.
    cipher: Option<(u32, FrameCipher)>,
    /// The last truncated nonce used. Frames use 1, 2, ..., as davey's and
    /// libdave's encryptors do.
    nonce: u32,
}

impl SenderKey {
    fn new(secret: Vec<u8>) -> Self {
        Self {
            next_secret: secret,
            next_generation: 0,
            cipher: None,
            nonce: 0,
        }
    }

    /// Encrypts an Opus frame into a DAVE frame: ciphertext, 8-byte tag,
    /// ULEB128 nonce, supplemental size, magic marker (DAVE protocol, "Frame
    /// Format").
    fn encrypt_opus(&mut self, opus_frame: &[u8]) -> Result<Vec<u8>, DiscordError> {
        let nonce = self.nonce.checked_add(1).ok_or_else(|| {
            dave_error(
                "this epoch's DAVE frame counter is used up (2^32 - 1 frames); it starts over \
                 with the next MLS epoch",
            )
        })?;
        // Used up before encrypting, so a nonce is never used twice even if
        // this attempt fails.
        self.nonce = nonce;
        let cipher = self.cipher(nonce >> GENERATION_SHIFT)?;
        let mut frame_nonce = [0_u8; FRAME_NONCE_LEN];
        frame_nonce[FRAME_NONCE_OFFSET..].copy_from_slice(&nonce.to_le_bytes());

        let mut frame = Vec::with_capacity(opus_frame.len() + 16);
        frame.extend_from_slice(opus_frame);
        let tag = cipher
            .encrypt_in_place_detached(GenericArray::from_slice(&frame_nonce), &[], &mut frame)
            .map_err(|_| dave_error("DAVE could not encrypt an Opus frame"))?;
        frame.extend_from_slice(&tag);
        let nonce_len = write_uleb128(u64::from(nonce), &mut frame);
        let supplemental = u8::try_from(DAVE_AUTH_TAG_LEN + nonce_len + DAVE_FOOTER_LEN)
            .map_err(|_| dave_error("DAVE supplemental data does not fit its size byte"))?;
        frame.push(supplemental);
        frame.extend_from_slice(&DAVE_MAGIC_MARKER);
        Ok(frame)
    }

    /// The cipher for `generation`, advancing the ratchet to it. Generations
    /// only grow, since the nonce only grows.
    fn cipher(&mut self, generation: u32) -> Result<&FrameCipher, DiscordError> {
        if self
            .cipher
            .as_ref()
            .is_none_or(|(current, _)| *current != generation)
        {
            let mut key = None;
            while self.next_generation <= generation {
                let current = self.next_generation;
                let derived =
                    derive_tree_secret(&self.next_secret, "key", current, SENDER_KEY_LEN)?;
                self.next_secret =
                    derive_tree_secret(&self.next_secret, "secret", current, RATCHET_SECRET_LEN)?;
                self.next_generation = current.saturating_add(1);
                if current == generation {
                    key = Some(derived);
                }
            }
            let key = key.ok_or_else(|| dave_error("DAVE key generation went backwards"))?;
            let cipher = FrameCipher::new_from_slice(&key)
                .map_err(|_| dave_error("invalid DAVE frame key length"))?;
            self.cipher = Some((generation, cipher));
        }
        self.cipher
            .as_ref()
            .map(|(_, cipher)| cipher)
            .ok_or_else(|| dave_error("no DAVE frame cipher"))
    }
}

/// Exports `user_id`'s sending secret for the epoch `session`'s MLS group is
/// in, as davey does for its own encryptor and decryptors.
fn export_sender_secret(session: &davey::DaveSession, user_id: u64) -> Result<Vec<u8>, String> {
    let group = session.group().ok_or("the DAVE session has no MLS group")?;
    group
        .export_secret(
            &RustCrypto::default(),
            SENDER_KEY_LABEL,
            &user_id.to_le_bytes(),
            SENDER_KEY_LEN,
        )
        .map_err(|error| format!("could not export the DAVE sender secret: {error}"))
}

/// MLS `DeriveTreeSecret` (RFC 9420 §8): `ExpandWithLabel(secret, label,
/// generation, length)` with HKDF-SHA256, as MLS++ computes it for DAVE's
/// sender ratchet (davey `mlspp_crypto`, libdave).
///
/// Written out because OpenMLS's HKDF refuses the 16-byte exported secret as
/// a pseudorandom key, which MLS++ accepts.
fn derive_tree_secret(
    secret: &[u8],
    label: &str,
    generation: u32,
    length: usize,
) -> Result<Vec<u8>, DiscordError> {
    // KDFLabel { uint16 length; opaque label<V>; opaque context<V> }, with
    // the "MLS 1.0 " label prefix and the generation as the context.
    let label = format!("MLS 1.0 {label}");
    let mut info = Vec::with_capacity(2 + 1 + label.len() + 1 + 4);
    let length_field =
        u16::try_from(length).map_err(|_| dave_error("DAVE derived key is too long"))?;
    info.extend_from_slice(&length_field.to_be_bytes());
    push_short_vl_bytes(&mut info, label.as_bytes())?;
    push_short_vl_bytes(&mut info, &generation.to_be_bytes())?;
    hkdf_expand_sha256(secret, &info, length)
}

/// Appends `bytes` as an MLS variable-length vector. Only the one-byte
/// length form (under 64 bytes, RFC 9420 §2.1.2) is needed for the fixed
/// labels and contexts here.
fn push_short_vl_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DiscordError> {
    let len = u8::try_from(bytes.len())
        .ok()
        .filter(|len| *len < 64)
        .ok_or_else(|| dave_error("DAVE key label is too long"))?;
    out.push(len);
    out.extend_from_slice(bytes);
    Ok(())
}

/// HKDF-Expand (RFC 5869 §2.3) with HMAC-SHA256.
fn hkdf_expand_sha256(prk: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, DiscordError> {
    let mut okm = Vec::with_capacity(length + 32);
    let mut previous: Vec<u8> = Vec::new();
    let mut counter: u8 = 0;
    while okm.len() < length {
        counter = counter
            .checked_add(1)
            .ok_or_else(|| dave_error("DAVE derived key is too long"))?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(prk)
            .map_err(|_| dave_error("invalid DAVE ratchet secret"))?;
        mac.update(&previous);
        mac.update(info);
        mac.update(&[counter]);
        previous = mac.finalize().into_bytes().to_vec();
        okm.extend_from_slice(&previous);
    }
    okm.truncate(length);
    Ok(okm)
}

/// Appends `value` as unsigned LEB128 and returns how many bytes it took.
fn write_uleb128(mut value: u64, out: &mut Vec<u8>) -> usize {
    let mut written = 0;
    loop {
        let low = value.to_le_bytes()[0] & 0x7f;
        value >>= 7;
        written += 1;
        if value == 0 {
            out.push(low);
            return written;
        }
        out.push(low | 0x80);
    }
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
    /// The key frames are sent with: the last executed epoch's.
    send_key: Option<SenderKey>,
    /// Sending secrets of epochs whose commit or welcome was processed,
    /// activated when their transition executes.
    pending_send_keys: HashMap<u64, Vec<u8>>,
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
            .field("has_send_key", &self.send_key.is_some())
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
            send_key: None,
            pending_send_keys: HashMap::new(),
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
        // Senders switch keys here, not when the commit was processed.
        let new_key = self.pending_send_keys.remove(&transition_id);
        if version == 0 {
            self.send_key = None;
        } else if let Some(secret) = new_key {
            self.send_key = Some(SenderKey::new(secret));
        }
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
        let secret = self
            .session
            .as_ref()
            .ok_or_else(|| "the DAVE session is gone".to_owned())
            .and_then(|session| export_sender_secret(session, self.user_id));
        let secret = match secret {
            Ok(secret) => Some(secret),
            Err(error) => {
                // Receiving still works; sending waits for the next epoch.
                self.fail(format!("transition {transition_id}"), error);
                None
            }
        };
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
            self.send_key = secret.map(SenderKey::new);
            Vec::new()
        } else {
            self.pending_transitions
                .insert(transition_id, self.protocol_version);
            if let Some(secret) = secret {
                self.pending_send_keys.insert(transition_id, secret);
            }
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
        // The group is left behind, and with it every key to send with.
        self.send_key = None;
        self.pending_send_keys.clear();
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
        self.protocol_version == 0 || self.active_version == 0 || self.send_key.is_some()
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
        if let Some(key) = self.send_key.as_mut() {
            return key.encrypt_opus(opus_frame).map(Cow::Owned);
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
        } else if !self.pending_send_keys.is_empty() {
            "it joined the MLS group, and the transition that puts its key in use has not \
             executed yet"
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
        // Both hold the new group's keys, but the upgrade has not executed:
        // senders keep the previous rules (unencrypted) until it does, and
        // receivers accept those frames.
        assert!(bot.status().ready);
        assert!(matches!(bot.encrypt_outbound(&OPUS), Ok(Cow::Borrowed(_))));
        assert!(matches!(peer.encrypt_outbound(&OPUS), Ok(Cow::Borrowed(_))));
        assert_eq!(decrypt(&mut bot, PEER, OPUS.to_vec()), Some(OPUS.to_vec()));
        assert_eq!(decrypt(&mut peer, BOT, OPUS.to_vec()), Some(OPUS.to_vec()));

        // From the execute on, frames are encrypted with the new group's keys.
        bot.on_execute_transition(13);
        peer.on_execute_transition(13);
        assert_eq!(bot.status().protocol_version, 1);
        assert_eq!(bot.status().last_transition_id, Some(13));
        let frame = encrypt(&mut bot, &OPUS);
        assert!(frame.ends_with(&super::DAVE_MAGIC_MARKER));
        assert_eq!(decrypt(&mut peer, BOT, frame), Some(OPUS.to_vec()));
        let frame = encrypt(&mut peer, &OPUS);
        assert_eq!(decrypt(&mut bot, PEER, frame), Some(OPUS.to_vec()));
    }

    /// The case group transitions exist for: a member that processed a commit
    /// must keep sending with the previous epoch's key until the transition
    /// executes, because the others may not have processed it yet.
    #[test]
    fn dave_keeps_sending_with_the_previous_epoch_until_the_transition_executes() {
        const CAROL: u64 = 30;
        let (mut bot, mut peer, gateway) = established_call();
        let mut carol = controller(CAROL);
        let carol_key_package = key_package(&carol.on_session_description(1));
        carol.on_external_sender(gateway.external_sender());
        bot.on_clients_connect([test_id(CAROL)]);
        peer.on_clients_connect([test_id(CAROL)]);

        // Both members get the proposal; the gateway announces the bot's
        // commit.
        let proposals = gateway.add_proposal(&carol_key_package, 1);
        let (commit, welcome) = commit_welcome(&bot.on_proposals(&proposals));
        commit_welcome(&peer.on_proposals(&proposals));
        assert_eq!(
            bot.on_announce_commit(&transition(21, &commit)),
            vec![ready(21)]
        );
        // The bot's group moved to epoch 2 (receive keys), but the peer has
        // not processed the commit: the bot's audio must still decrypt there.
        assert_eq!(bot.status().epoch, Some(2));
        let frame = encrypt(&mut bot, &OPUS);
        assert_eq!(decrypt(&mut peer, BOT, frame), Some(OPUS.to_vec()));

        assert_eq!(
            peer.on_announce_commit(&transition(21, &commit)),
            vec![ready(21)]
        );
        assert_eq!(
            carol.on_welcome(&transition(21, &welcome.unwrap())),
            vec![ready(21)]
        );
        // Everyone processed it; still the old key until execute. Carol was
        // not in that epoch, and has no key to send with yet herself.
        let frame = encrypt(&mut bot, &OPUS);
        assert_eq!(decrypt(&mut peer, BOT, frame.clone()), Some(OPUS.to_vec()));
        assert_eq!(decrypt(&mut carol, BOT, frame), None);
        let status = carol.status();
        assert!(status.ready && !status.can_send_audio(), "{status:?}");
        let error = carol.encrypt_outbound(&OPUS).unwrap_err().to_string();
        assert!(error.contains("has not executed yet"), "{error}");

        for member in [&mut bot, &mut peer, &mut carol] {
            member.on_execute_transition(21);
        }
        let frame = encrypt(&mut bot, &OPUS);
        assert_eq!(decrypt(&mut peer, BOT, frame.clone()), Some(OPUS.to_vec()));
        assert_eq!(decrypt(&mut carol, BOT, frame), Some(OPUS.to_vec()));
        let frame = encrypt(&mut carol, &OPUS);
        assert_eq!(decrypt(&mut bot, CAROL, frame.clone()), Some(OPUS.to_vec()));
        assert_eq!(decrypt(&mut peer, CAROL, frame), Some(OPUS.to_vec()));
    }

    /// The runtime's own sending key against davey's: the same epoch's first
    /// frame must come out byte for byte the same (AES-GCM is deterministic
    /// for one key and nonce), and the ratchet must reproduce davey's test
    /// vector.
    #[test]
    fn dave_sender_key_matches_davey_byte_for_byte() {
        let (mut bot, _peer, _gateway) = established_call();
        let ours = encrypt(&mut bot, &OPUS);
        // davey's own encryptor got the same epoch's key when it processed
        // the commit and has not been used: its first frame has nonce 1 too.
        let theirs = bot
            .session
            .as_mut()
            .unwrap()
            .encrypt_opus(&OPUS)
            .unwrap()
            .into_owned();
        assert_eq!(ours, theirs);

        // davey `hash_ratchet::tests::expected_result`.
        let secret = [
            206, 221, 97, 177, 184, 161, 202, 105, 4, 101, 84, 40, 44, 247, 11, 123,
        ];
        assert_eq!(
            super::derive_tree_secret(&secret, "key", 0, 16).unwrap(),
            [117, 48, 249, 169, 148, 94, 45, 46, 6, 208, 101, 31, 123, 42, 134, 75]
        );
        assert_eq!(
            super::derive_tree_secret(&secret, "nonce", 0, 12).unwrap(),
            [48, 30, 95, 75, 116, 9, 15, 152, 94, 114, 107, 178]
        );
    }

    #[test]
    fn dave_sender_key_moves_to_the_next_generation_and_never_reuses_a_nonce() {
        let mut key = super::SenderKey::new(vec![7; 16]);
        let first = key.encrypt_opus(&OPUS).unwrap();
        let parsed = super::super::parse_dave_frame(&first).unwrap();
        assert_eq!(parsed.nonce, 1);
        assert_eq!(parsed.ciphertext.len(), OPUS.len());

        // The top byte of the nonce is the key generation.
        key.nonce = (1 << 24) - 1;
        let next_generation = key.encrypt_opus(&OPUS).unwrap();
        assert_eq!(
            super::super::parse_dave_frame(&next_generation)
                .unwrap()
                .nonce,
            1 << 24
        );
        assert_eq!(
            key.cipher.as_ref().map(|(generation, _)| *generation),
            Some(1)
        );
        assert_eq!(key.next_generation, 2);

        // The last nonce of the epoch, then an error instead of a wrap.
        key.nonce = u32::MAX - 1;
        key.encrypt_opus(&OPUS).unwrap();
        let error = key.encrypt_opus(&OPUS).unwrap_err().to_string();
        assert!(error.contains("used up"), "{error}");
        assert_eq!(key.nonce, u32::MAX);

        let mut written = Vec::new();
        assert_eq!(super::write_uleb128(300, &mut written), 2);
        assert_eq!(written, [0xac, 0x02]);
        assert!(super::push_short_vl_bytes(&mut written, &[0; 64]).is_err());
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
