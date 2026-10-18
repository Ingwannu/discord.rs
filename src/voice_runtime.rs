use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::num::NonZeroU16;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use futures_util::{SinkExt, StreamExt};
use opus_decoder::OpusDecoder;
#[cfg(feature = "voice-encode")]
use opus_rs::{Application as OpusApplication, OpusEncoder as RawOpusEncoder};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{interval, sleep, timeout, Duration, Instant};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use crate::error::DiscordError;
use crate::model::Lenient;
use crate::model::{ChannelId, GuildId, UserId};
use crate::types::invalid_data_error;
use crate::voice::player::OPUS_SILENCE_FRAME;
use crate::voice::{
    VoiceEncryptionMode, VoiceGatewayCommand, VoiceGatewayOpcode, VoiceGatewayReady,
    VoiceSelectProtocolCommand, VoiceSpeakingCommand, VoiceSpeakingFlags, VoiceUdpDiscoveryPacket,
};

#[path = "voice_runtime/dave.rs"]
mod dave;

pub use dave::VoiceDaveStatus;
use dave::{DaveController, SUPPORTED_DAVE_PROTOCOL_VERSION};

// Voice opcodes (Discord docs: Opcodes and Status Codes › Voice).
const VOICE_OP_IDENTIFY: u64 = 0;
const VOICE_OP_READY: u64 = 2;
const VOICE_OP_HEARTBEAT: u64 = 3;
const VOICE_OP_SESSION_DESCRIPTION: u64 = 4;
const VOICE_OP_HEARTBEAT_ACK: u64 = 6;
const VOICE_OP_RESUME: u64 = 7;
const VOICE_OP_HELLO: u64 = 8;
const VOICE_OP_RESUMED: u64 = 9;
const VOICE_OP_CLIENTS_CONNECT: u64 = 11;
const VOICE_OP_CLIENT_DISCONNECT: u64 = 13;
const VOICE_OP_DAVE_PREPARE_TRANSITION: u64 = 21;
const VOICE_OP_DAVE_EXECUTE_TRANSITION: u64 = 22;
const VOICE_OP_DAVE_PREPARE_EPOCH: u64 = 24;
const VOICE_OP_DAVE_MLS_EXTERNAL_SENDER: u64 = 25;
const VOICE_OP_DAVE_MLS_PROPOSALS: u64 = 27;
const VOICE_OP_DAVE_MLS_ANNOUNCE_COMMIT_TRANSITION: u64 = 29;
const VOICE_OP_DAVE_MLS_WELCOME: u64 = 30;
const VOICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const VOICE_HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const VOICE_READY_TIMEOUT: Duration = Duration::from_secs(10);
const VOICE_UDP_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const VOICE_SESSION_DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(10);
/// Close code for a client without the DAVE support the channel requires
/// (Discord docs: Opcodes and Status Codes › Voice Close Event Codes).
const VOICE_CLOSE_DAVE_REQUIRED: u16 = 4017;
const DAVE_MAGIC_MARKER: [u8; 2] = [0xfa, 0xfa];
const RTP_VERSION: u8 = 2;
const RTP_PAYLOAD_TYPE_OPUS: u8 = 120;
/// Fixed RTP header: flags, payload type, sequence, timestamp, SSRC
/// (RFC 3550 §5.1).
const RTP_HEADER_LEN: usize = 12;
/// RTP counts in 32-bit words: one per CSRC, one for the extension header,
/// and the extension's length field (RFC 3550 §5.1, §5.3.1).
const RTP_WORD_LEN: usize = 4;
/// The header extension's preamble: 16-bit profile plus 16-bit length in
/// words (RFC 3550 §5.3.1).
const RTP_EXTENSION_PREAMBLE_LEN: usize = 4;
/// Both rtpsize AEAD modes take the 256-bit `secret_key` from
/// SESSION_DESCRIPTION (Discord docs: Voice Connections › Transport
/// Encryption Modes).
const VOICE_SECRET_KEY_LEN: usize = 32;
/// The rtpsize modes append a 32-bit packet counter to every packet
/// ("32-bit incremental integer value, appended to payload"); see
/// [`rtpsize_nonce`] for where it goes in the AEAD nonce.
const RTPSIZE_NONCE_SUFFIX_LEN: usize = 4;
/// AES-256-GCM and Poly1305 both append a 16-byte tag to the ciphertext.
const AEAD_TAG_LEN: usize = 16;
/// AES-256-GCM nonce length.
const AES_GCM_NONCE_LEN: usize = 12;
/// XChaCha20-Poly1305 nonce length.
const XCHACHA20_NONCE_LEN: usize = 24;
/// DAVE frames end with an 8-byte truncated AES-GCM tag, a ULEB128 nonce and
/// unencrypted ranges, a supplemental-size byte, and the magic marker (DAVE
/// protocol, "Frame format": <https://daveprotocol.com/>).
const DAVE_AUTH_TAG_LEN: usize = 8;
/// The supplemental-size byte plus the magic marker.
const DAVE_FOOTER_LEN: usize = 1 + DAVE_MAGIC_MARKER.len();
/// Smallest supplemental section: the tag, a one-byte nonce, and the footer.
const DAVE_MIN_SUPPLEMENTAL_LEN: usize = DAVE_AUTH_TAG_LEN + 1 + DAVE_FOOTER_LEN;
/// Voice gateway version requested by default, the one Discord recommends
/// (Discord docs: Voice Connections › Voice Gateway Versioning).
const DEFAULT_VOICE_GATEWAY_VERSION: u8 = 8;
/// Oldest version Discord lists as available (1–3 are deprecated); lower
/// requests are raised to it.
const MIN_VOICE_GATEWAY_VERSION: u8 = 4;
const DISCORD_OPUS_SAMPLE_RATE: u32 = 48_000;
const DISCORD_OPUS_CHANNELS: usize = 2;
#[cfg(feature = "voice-encode")]
const DISCORD_OPUS_SAMPLES_PER_CHANNEL: usize = 960;
#[cfg(feature = "voice-encode")]
const DISCORD_OPUS_STEREO_FRAME_SAMPLES: usize =
    DISCORD_OPUS_CHANNELS * DISCORD_OPUS_SAMPLES_PER_CHANNEL;
const DISCORD_OPUS_FRAME_MS: u64 = 20;

/// [`SUPPORTED_DAVE_PROTOCOL_VERSION`] as IDENTIFY's `u8` field.
fn supported_dave_version() -> u8 {
    u8::try_from(SUPPORTED_DAVE_PROTOCOL_VERSION).unwrap_or(u8::MAX)
}

#[derive(Clone, PartialEq, Eq)]
/// Typed Discord API object for `VoiceRuntimeConfig`.
pub struct VoiceRuntimeConfig {
    pub server_id: GuildId,
    pub user_id: UserId,
    pub session_id: String,
    pub token: String,
    pub endpoint: String,
    pub gateway_version: u8,
    pub preferred_mode: Option<VoiceEncryptionMode>,
    /// The highest DAVE (end-to-end encryption) protocol version to
    /// advertise; `None` leaves the field out of IDENTIFY, which Discord
    /// reads as 0. Defaults to the version this crate implements. The
    /// runtime caps it at that version, and advertises 0 when
    /// [`channel_id`](#structfield.channel_id) is unset.
    pub max_dave_protocol_version: Option<u8>,
    /// The voice channel being joined. DAVE uses it as the MLS group ID, so
    /// without it the runtime cannot take part in end-to-end encryption and
    /// Discord refuses the connection in every channel except Stage
    /// channels (close code 4017). [`VoiceManager::runtime_config`] and
    /// `Context::connect_voice_runtime` fill it in.
    ///
    /// [`VoiceManager::runtime_config`]: crate::voice::VoiceManager::runtime_config
    pub channel_id: Option<ChannelId>,
}

impl fmt::Debug for VoiceRuntimeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceRuntimeConfig")
            .field("server_id", &self.server_id)
            .field("user_id", &self.user_id)
            .field("session_id", &self.session_id)
            .field("token", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .field("gateway_version", &self.gateway_version)
            .field("preferred_mode", &self.preferred_mode)
            .field("max_dave_protocol_version", &self.max_dave_protocol_version)
            .field("channel_id", &self.channel_id)
            .finish()
    }
}

impl VoiceRuntimeConfig {
    /// Creates a `new` value.
    pub fn new(
        server_id: GuildId,
        user_id: UserId,
        session_id: impl Into<String>,
        token: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            server_id,
            user_id,
            session_id: session_id.into(),
            token: token.into(),
            endpoint: endpoint.into(),
            gateway_version: DEFAULT_VOICE_GATEWAY_VERSION,
            preferred_mode: None,
            max_dave_protocol_version: Some(supported_dave_version()),
            channel_id: None,
        }
    }

    /// Sets the voice channel being joined, which DAVE end-to-end encryption
    /// needs (see [`Self::channel_id`](#structfield.channel_id)).
    pub fn channel_id(mut self, channel_id: ChannelId) -> Self {
        self.channel_id = Some(channel_id);
        self
    }

    /// The DAVE version IDENTIFY advertises: the configured maximum, capped
    /// at the version this crate implements, and 0 without a channel ID.
    ///
    /// Advertising a version the runtime cannot run would let Discord
    /// negotiate DAVE for a connection that can then neither send nor
    /// receive audio; advertising 0 makes Discord refuse it with close code
    /// 4017 instead, which the handshake reports.
    fn advertised_dave_version(&self) -> Option<u8> {
        let requested = self.max_dave_protocol_version?;
        if self.channel_id.is_none() {
            return Some(0);
        }
        Some(requested.min(supported_dave_version()))
    }

    pub fn gateway_version(mut self, gateway_version: u8) -> Self {
        self.gateway_version = gateway_version.max(MIN_VOICE_GATEWAY_VERSION);
        self
    }

    pub fn preferred_mode(mut self, preferred_mode: VoiceEncryptionMode) -> Self {
        self.preferred_mode = Some(preferred_mode);
        self
    }

    pub fn max_dave_protocol_version(mut self, version: u8) -> Self {
        self.max_dave_protocol_version = Some(version);
        self
    }

    pub fn websocket_url(&self) -> String {
        let mut endpoint = if self.endpoint.contains("://") {
            self.endpoint.clone()
        } else {
            format!("wss://{}", self.endpoint)
        };

        if !endpoint.contains("?v=") {
            let separator = if endpoint.contains('?') { "&" } else { "/?" };
            endpoint.push_str(separator);
            endpoint.push_str(&format!(
                "v={}",
                self.gateway_version.max(MIN_VOICE_GATEWAY_VERSION)
            ));
        }

        endpoint
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `VoiceSessionDescription`.
pub struct VoiceSessionDescription {
    pub mode: VoiceEncryptionMode,
    #[serde(default)]
    pub secret_key: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dave_protocol_version: Option<u8>,
}

impl fmt::Debug for VoiceSessionDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSessionDescription")
            .field("mode", &self.mode)
            .field(
                "secret_key",
                &format_args!("<redacted:{} bytes>", self.secret_key.len()),
            )
            .field("audio_codec", &self.audio_codec)
            .field("dave_protocol_version", &self.dave_protocol_version)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `VoiceDaveState`.
pub struct VoiceDaveState {
    pub protocol_version: Option<u8>,
    pub transition_id: Option<u64>,
    pub epoch: Option<u64>,
    pub passthrough: bool,
    #[serde(default)]
    pub external_sender: Option<Vec<u8>>,
    #[serde(default)]
    pub proposals: Vec<Vec<u8>>,
    #[serde(default)]
    pub pending_commit: Option<Vec<u8>>,
    #[serde(default)]
    pub pending_welcome: Option<Vec<u8>>,
}

impl fmt::Debug for VoiceDaveState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let proposal_lengths = self.proposals.iter().map(Vec::len).collect::<Vec<_>>();
        f.debug_struct("VoiceDaveState")
            .field("protocol_version", &self.protocol_version)
            .field("transition_id", &self.transition_id)
            .field("epoch", &self.epoch)
            .field("passthrough", &self.passthrough)
            .field(
                "external_sender",
                &self
                    .external_sender
                    .as_ref()
                    .map(|sender| format!("<redacted:{} bytes>", sender.len())),
            )
            .field("proposal_lengths", &proposal_lengths)
            .field(
                "pending_commit_len",
                &self.pending_commit.as_ref().map(Vec::len),
            )
            .field(
                "pending_welcome_len",
                &self.pending_welcome.as_ref().map(Vec::len),
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceRuntimeState`.
pub struct VoiceRuntimeState {
    pub config: VoiceRuntimeConfig,
    pub heartbeat_interval_ms: u64,
    pub last_sequence: Option<i64>,
    pub ready: VoiceGatewayReady,
    pub discovery: VoiceUdpDiscoveryPacket,
    pub selected_mode: VoiceEncryptionMode,
    pub session_description: Option<VoiceSessionDescription>,
    pub ssrc_users: HashMap<u32, UserId>,
    pub speaking: HashMap<u32, VoiceSpeakingUpdate>,
    pub dave: VoiceDaveState,
    pub resumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceRawUdpPacket`.
pub struct VoiceRawUdpPacket {
    pub bytes: Vec<u8>,
    pub version: Option<u8>,
    pub payload_type: Option<u8>,
    pub sequence: Option<u16>,
    pub timestamp: Option<u32>,
    pub ssrc: Option<u32>,
}

impl VoiceRawUdpPacket {
    fn from_bytes(bytes: Vec<u8>) -> Self {
        let (version, payload_type, sequence, timestamp, ssrc) = if bytes.len() >= RTP_HEADER_LEN {
            (
                Some(bytes[0] >> 6),
                Some(bytes[1] & 0x7f),
                Some(u16::from_be_bytes([bytes[2], bytes[3]])),
                Some(u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])),
                Some(u32::from_be_bytes([
                    bytes[8], bytes[9], bytes[10], bytes[11],
                ])),
            )
        } else {
            (None, None, None, None, None)
        };

        Self {
            bytes,
            version,
            payload_type,
            sequence,
            timestamp,
            ssrc,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceRtpHeader`.
pub struct VoiceRtpHeader {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    /// Where the media payload starts once the packet is decrypted: the
    /// fixed header, the CSRCs, and the whole header extension (preamble
    /// and body). In the rtpsize transport modes the extension body is
    /// encrypted together with the payload, so in the raw packet
    /// `bytes[header_len..]` is not where the ciphertext starts when
    /// `extension` is set.
    pub header_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceReceivedPacket`.
pub struct VoiceReceivedPacket {
    pub raw: VoiceRawUdpPacket,
    pub rtp: VoiceRtpHeader,
    pub user_id: Option<UserId>,
    /// The media payload after transport decryption: an Opus frame, or the
    /// DAVE-decrypted Opus frame from the `_with_dave` receivers. The RTP
    /// header extension body and any RTP padding are already removed.
    ///
    /// A padding-only packet (P bit set and a padding count equal to the
    /// whole payload, which RFC 3550 §5.1 allows) yields an empty frame
    /// when DAVE is not active; with DAVE active it is reported as a DAVE
    /// frame that is too short. A malformed padding count (zero, or longer
    /// than the payload) is always an error.
    pub opus_frame: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceDecodedPacket`.
pub struct VoiceDecodedPacket {
    pub packet: VoiceReceivedPacket,
    pub sample_rate: u32,
    pub channels: usize,
    pub samples_per_channel: usize,
    pub pcm: Vec<i16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceOpusFrame`.
pub struct VoiceOpusFrame {
    pub bytes: Vec<u8>,
    pub duration: Duration,
}

impl VoiceOpusFrame {
    /// Creates a `new` value.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            duration: Duration::from_millis(DISCORD_OPUS_FRAME_MS),
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }
}

#[cfg(feature = "voice-encode")]
#[derive(Clone, Debug, PartialEq)]
/// Typed Discord API object for `PcmFrame`.
pub struct PcmFrame {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: usize,
    duration: Duration,
}

#[cfg(feature = "voice-encode")]
impl PcmFrame {
    /// Creates a `new` value.
    pub fn new(
        samples: impl Into<Vec<f32>>,
        sample_rate: u32,
        channels: usize,
        duration: Duration,
    ) -> Result<Self, DiscordError> {
        let samples = samples.into();
        if sample_rate != DISCORD_OPUS_SAMPLE_RATE {
            return Err(invalid_data_error(format!(
                "PCM frame sample_rate must be {DISCORD_OPUS_SAMPLE_RATE} Hz"
            )));
        }
        if channels != DISCORD_OPUS_CHANNELS {
            return Err(invalid_data_error(format!(
                "PCM frame channels must be {DISCORD_OPUS_CHANNELS}"
            )));
        }
        if duration != Duration::from_millis(DISCORD_OPUS_FRAME_MS) {
            return Err(invalid_data_error(format!(
                "PCM frame duration must be {DISCORD_OPUS_FRAME_MS}ms"
            )));
        }
        if samples.len() != DISCORD_OPUS_STEREO_FRAME_SAMPLES {
            return Err(invalid_data_error(format!(
                "PCM frame must contain {DISCORD_OPUS_STEREO_FRAME_SAMPLES} interleaved f32 samples"
            )));
        }

        Ok(Self {
            samples,
            sample_rate,
            channels,
            duration,
        })
    }

    /// Creates a `discord_stereo_20ms` value.
    pub fn discord_stereo_20ms(samples: impl Into<Vec<f32>>) -> Result<Self, DiscordError> {
        Self::new(
            samples,
            DISCORD_OPUS_SAMPLE_RATE,
            DISCORD_OPUS_CHANNELS,
            Duration::from_millis(DISCORD_OPUS_FRAME_MS),
        )
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn samples_per_channel(&self) -> usize {
        self.samples.len() / self.channels
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }
}

#[cfg(feature = "voice-encode")]
/// Trait for `AudioSource` behavior.
pub trait AudioSource {
    /// Returns the next Discord-sized PCM frame, or `None` when the source is exhausted.
    fn next_pcm_frame(&mut self) -> Result<Option<PcmFrame>, DiscordError>;
}

#[cfg(feature = "voice-encode")]
/// Typed Discord API object for `AudioMixer`.
pub struct AudioMixer {
    sources: Vec<Box<dyn AudioSource + Send>>,
    volume: f32,
}

#[cfg(feature = "voice-encode")]
impl AudioMixer {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            volume: 1.0,
        }
    }

    /// Creates a `with_volume` value.
    pub fn with_volume(volume: f32) -> Self {
        Self {
            sources: Vec::new(),
            volume,
        }
    }

    pub fn push_source<S>(&mut self, source: S)
    where
        S: AudioSource + Send + 'static,
    {
        self.sources.push(Box::new(source));
    }

    pub fn active_sources(&self) -> usize {
        self.sources.len()
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }
}

#[cfg(feature = "voice-encode")]
impl Default for AudioMixer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "voice-encode")]
impl AudioSource for AudioMixer {
    fn next_pcm_frame(&mut self) -> Result<Option<PcmFrame>, DiscordError> {
        let mut mixed = vec![0.0_f32; DISCORD_OPUS_STEREO_FRAME_SAMPLES];
        let mut mixed_any = false;
        let mut index = 0;

        while index < self.sources.len() {
            match self.sources[index].next_pcm_frame()? {
                Some(frame) => {
                    if frame.sample_rate() != DISCORD_OPUS_SAMPLE_RATE
                        || frame.channels() != DISCORD_OPUS_CHANNELS
                        || frame.duration() != Duration::from_millis(DISCORD_OPUS_FRAME_MS)
                        || frame.samples().len() != DISCORD_OPUS_STEREO_FRAME_SAMPLES
                    {
                        return Err(invalid_data_error(
                            "audio mixer only accepts 48kHz stereo 20ms PCM frames",
                        ));
                    }
                    for (target, sample) in mixed.iter_mut().zip(frame.samples()) {
                        *target += sample * self.volume;
                    }
                    mixed_any = true;
                    index += 1;
                }
                None => {
                    self.sources.remove(index);
                }
            }
        }

        if !mixed_any {
            return Ok(None);
        }

        for sample in &mut mixed {
            *sample = sample.clamp(-1.0, 1.0);
        }

        PcmFrame::discord_stereo_20ms(mixed).map(Some)
    }
}

#[cfg(feature = "voice-encode")]
/// Typed Discord API object for `VoiceOpusEncoder`.
pub struct VoiceOpusEncoder {
    encoder: RawOpusEncoder,
    bitrate_bps: i32,
    use_cbr: bool,
    output: Vec<u8>,
}

#[cfg(feature = "voice-encode")]
impl VoiceOpusEncoder {
    /// Creates a `new` value.
    pub fn new(bitrate_bps: i32, use_cbr: bool) -> Result<Self, DiscordError> {
        let mut encoder = RawOpusEncoder::new(
            DISCORD_OPUS_SAMPLE_RATE.cast_signed(), // 48 000 fits an i32
            DISCORD_OPUS_CHANNELS,
            OpusApplication::Audio,
        )
        .map_err(|error| invalid_data_error(format!("failed to create Opus encoder: {error}")))?;
        encoder.bitrate_bps = bitrate_bps;
        encoder.use_cbr = use_cbr;

        Ok(Self {
            encoder,
            bitrate_bps,
            use_cbr,
            output: vec![0_u8; 4_096],
        })
    }

    /// Creates a `discord_music` value.
    pub fn discord_music() -> Result<Self, DiscordError> {
        Self::new(128_000, true)
    }

    pub fn with_bitrate_bps(mut self, bitrate_bps: i32) -> Self {
        self.encoder.bitrate_bps = bitrate_bps;
        self.bitrate_bps = bitrate_bps;
        self
    }

    pub fn with_cbr(mut self, use_cbr: bool) -> Self {
        self.encoder.use_cbr = use_cbr;
        self.use_cbr = use_cbr;
        self
    }

    pub fn sample_rate(&self) -> u32 {
        DISCORD_OPUS_SAMPLE_RATE
    }

    pub fn channels(&self) -> usize {
        DISCORD_OPUS_CHANNELS
    }

    pub fn samples_per_channel(&self) -> usize {
        DISCORD_OPUS_SAMPLES_PER_CHANNEL
    }

    pub fn bitrate_bps(&self) -> i32 {
        self.bitrate_bps
    }

    pub fn use_cbr(&self) -> bool {
        self.use_cbr
    }

    pub fn encode_pcm_frame(&mut self, frame: &PcmFrame) -> Result<VoiceOpusFrame, DiscordError> {
        if frame.sample_rate() != DISCORD_OPUS_SAMPLE_RATE
            || frame.channels() != DISCORD_OPUS_CHANNELS
            || frame.samples_per_channel() != DISCORD_OPUS_SAMPLES_PER_CHANNEL
            || frame.duration() != Duration::from_millis(DISCORD_OPUS_FRAME_MS)
        {
            return Err(invalid_data_error(
                "Opus encoder requires 48kHz stereo 20ms PCM frames",
            ));
        }

        let written = self
            .encoder
            .encode(
                frame.samples(),
                DISCORD_OPUS_SAMPLES_PER_CHANNEL,
                &mut self.output,
            )
            .map_err(|error| invalid_data_error(format!("failed to encode Opus frame: {error}")))?;
        Ok(VoiceOpusFrame {
            bytes: self.output[..written].to_vec(),
            duration: frame.duration(),
        })
    }

    pub fn encode_source_frames<S>(
        &mut self,
        source: &mut S,
        max_frames: Option<usize>,
    ) -> Result<Vec<VoiceOpusFrame>, DiscordError>
    where
        S: AudioSource + ?Sized,
    {
        let mut frames = Vec::new();
        while max_frames.is_none_or(|max| frames.len() < max) {
            let Some(frame) = source.next_pcm_frame()? else {
                break;
            };
            frames.push(self.encode_pcm_frame(&frame)?);
        }
        Ok(frames)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceOutboundPacket`.
pub struct VoiceOutboundPacket {
    pub rtp: VoiceRtpHeader,
    pub nonce_suffix: [u8; 4],
    pub opus_frame: Vec<u8>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceOutboundRtpState`.
pub struct VoiceOutboundRtpState {
    pub sequence: u16,
    pub timestamp: u32,
    pub nonce_suffix: u32,
    pub ssrc: u32,
    pub payload_type: u8,
    pub sample_rate: u32,
}

impl VoiceOutboundRtpState {
    /// Creates a `new` value.
    pub fn new(ssrc: u32) -> Self {
        Self {
            sequence: 0,
            timestamp: 0,
            nonce_suffix: low_32_bits(initial_voice_heartbeat_nonce()),
            ssrc,
            payload_type: RTP_PAYLOAD_TYPE_OPUS,
            sample_rate: DISCORD_OPUS_SAMPLE_RATE,
        }
    }

    /// Creates a `with_counters` value.
    pub fn with_counters(ssrc: u32, sequence: u16, timestamp: u32, nonce_suffix: u32) -> Self {
        Self {
            sequence,
            timestamp,
            nonce_suffix,
            ssrc,
            payload_type: RTP_PAYLOAD_TYPE_OPUS,
            sample_rate: DISCORD_OPUS_SAMPLE_RATE,
        }
    }

    pub fn build_packet(
        &mut self,
        opus_frame: &[u8],
        duration: Duration,
        mode: &VoiceEncryptionMode,
        secret_key: &[u8],
    ) -> Result<VoiceOutboundPacket, DiscordError> {
        let packet = self.seal_packet(opus_frame, Duration::ZERO, mode, secret_key)?;
        self.commit_packet(Duration::ZERO, duration);
        Ok(packet)
    }

    /// Encrypts a packet stamped `gap` after the current timestamp.
    ///
    /// Sealing consumes a nonce but not the sequence number or timestamp;
    /// [`Self::commit_packet`] advances those once the packet was really
    /// sent. A packet that is sealed but never sent therefore leaves no hole
    /// in the sequence or the media timeline, while its nonce is never used
    /// again: reusing a nonce for a different payload would break the AEAD.
    fn seal_packet(
        &mut self,
        opus_frame: &[u8],
        gap: Duration,
        mode: &VoiceEncryptionMode,
        secret_key: &[u8],
    ) -> Result<VoiceOutboundPacket, DiscordError> {
        if opus_frame.is_empty() {
            return Err(invalid_data_error("opus frame must not be empty"));
        }

        let sequence = self.sequence;
        let timestamp = self
            .timestamp
            .wrapping_add(gap_increment(self.sample_rate, gap));
        let nonce_suffix = self.nonce_suffix.to_be_bytes();
        let packet = encrypt_transport_payload(
            VoiceOutboundEncryptParams {
                sequence,
                timestamp,
                ssrc: self.ssrc,
                payload_type: self.payload_type,
                nonce_suffix,
            },
            opus_frame,
            mode,
            secret_key,
        )?;
        self.nonce_suffix = self.nonce_suffix.wrapping_add(1);
        let rtp = parse_rtp_header(&packet)?;

        Ok(VoiceOutboundPacket {
            rtp,
            nonce_suffix,
            opus_frame: opus_frame.to_vec(),
            bytes: packet,
        })
    }

    /// Records a sent packet that followed `gap` of untransmitted media and
    /// carried `duration` of audio.
    fn commit_packet(&mut self, gap: Duration, duration: Duration) {
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self
            .timestamp
            .wrapping_add(gap_increment(self.sample_rate, gap))
            .wrapping_add(timestamp_increment(self.sample_rate, duration));
    }
}

/// Typed Discord API object for `VoiceOpusDecoder`.
pub struct VoiceOpusDecoder {
    decoder: OpusDecoder,
    sample_rate: u32,
    channels: usize,
    max_samples_per_channel: usize,
}

impl VoiceOpusDecoder {
    /// Creates a `new` value.
    pub fn new(sample_rate: u32, channels: usize) -> Result<Self, DiscordError> {
        let decoder = OpusDecoder::new(sample_rate, channels).map_err(|error| {
            invalid_data_error(format!("failed to create Opus decoder: {error}"))
        })?;
        let max_samples_per_channel = decoder.max_frame_size_per_channel();
        Ok(Self {
            decoder,
            sample_rate,
            channels,
            max_samples_per_channel,
        })
    }

    /// Creates a `discord_default` value.
    pub fn discord_default() -> Result<Self, DiscordError> {
        Self::new(DISCORD_OPUS_SAMPLE_RATE, DISCORD_OPUS_CHANNELS)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn decode_opus_frame(
        &mut self,
        opus_frame: &[u8],
        fec: bool,
    ) -> Result<(usize, Vec<i16>), DiscordError> {
        let mut pcm = vec![0_i16; self.max_samples_per_channel * self.channels];
        let samples_per_channel = self
            .decoder
            .decode(opus_frame, &mut pcm, fec)
            .map_err(|error| invalid_data_error(format!("failed to decode Opus frame: {error}")))?;
        pcm.truncate(samples_per_channel * self.channels);
        Ok((samples_per_channel, pcm))
    }

    pub fn decode_packet(
        &mut self,
        packet: VoiceReceivedPacket,
    ) -> Result<VoiceDecodedPacket, DiscordError> {
        let (samples_per_channel, pcm) = self.decode_opus_frame(&packet.opus_frame, false)?;
        Ok(VoiceDecodedPacket {
            packet,
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples_per_channel,
            pcm,
        })
    }

    pub fn reset(&mut self) {
        self.decoder.reset();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceDaveUnencryptedRange`.
pub struct VoiceDaveUnencryptedRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `VoiceDaveFrame`.
pub struct VoiceDaveFrame {
    pub bytes: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub auth_tag: [u8; 8],
    pub nonce: u32,
    pub unencrypted_ranges: Vec<VoiceDaveUnencryptedRange>,
    pub supplemental_size: u8,
}

/// Trait for `VoiceDaveFrameDecryptor` behavior.
pub trait VoiceDaveFrameDecryptor {
    /// Decrypts one DAVE media frame into an Opus payload.
    fn decrypt_frame(
        &mut self,
        rtp: &VoiceRtpHeader,
        user_id: Option<UserId>,
        frame: &VoiceDaveFrame,
    ) -> Result<Vec<u8>, DiscordError>;
}

/// Trait for `VoiceDaveFrameEncryptor` behavior.
pub trait VoiceDaveFrameEncryptor {
    /// Encrypts one Opus payload for DAVE media transport.
    fn encrypt_opus(&mut self, opus_frame: &[u8]) -> Result<Vec<u8>, DiscordError>;
}

/// Typed Discord API object for `VoiceDaveySession`.
pub struct VoiceDaveySession {
    session: davey::DaveSession,
}

/// Type alias for `VoiceDaveyDecryptor`.
pub type VoiceDaveyDecryptor = VoiceDaveySession;

impl VoiceDaveySession {
    /// Creates a `new` value.
    pub fn new(
        protocol_version: NonZeroU16,
        user_id: u64,
        channel_id: u64,
    ) -> Result<Self, DiscordError> {
        let session = davey::DaveSession::new(protocol_version, user_id, channel_id, None)
            .map_err(|error| {
                invalid_data_error(format!("failed to create DAVE session: {error:?}"))
            })?;
        Ok(Self { session })
    }

    pub fn session(&self) -> &davey::DaveSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut davey::DaveSession {
        &mut self.session
    }

    pub fn is_ready(&self) -> bool {
        self.session.is_ready()
    }

    pub fn voice_privacy_code(&self) -> Option<&str> {
        self.session.voice_privacy_code()
    }

    pub fn set_external_sender(&mut self, external_sender: &[u8]) -> Result<(), DiscordError> {
        self.session
            .set_external_sender(external_sender)
            .map_err(|error| {
                invalid_data_error(format!("failed to set DAVE external sender: {error:?}"))
            })
    }

    pub fn create_key_package(&mut self) -> Result<Vec<u8>, DiscordError> {
        self.session.create_key_package().map_err(|error| {
            invalid_data_error(format!("failed to create DAVE key package: {error:?}"))
        })
    }

    pub fn process_welcome(&mut self, welcome: &[u8]) -> Result<(), DiscordError> {
        self.session.process_welcome(welcome).map_err(|error| {
            invalid_data_error(format!("failed to process DAVE welcome: {error:?}"))
        })
    }

    pub fn process_commit(&mut self, commit: &[u8]) -> Result<(), DiscordError> {
        self.session.process_commit(commit).map_err(|error| {
            invalid_data_error(format!("failed to process DAVE commit: {error:?}"))
        })
    }

    pub fn process_proposals(
        &mut self,
        operation_type: davey::ProposalsOperationType,
        proposals: &[u8],
        expected_user_ids: Option<&[u64]>,
    ) -> Result<Option<davey::CommitWelcome>, DiscordError> {
        self.session
            .process_proposals(operation_type, proposals, expected_user_ids)
            .map_err(|error| {
                invalid_data_error(format!("failed to process DAVE proposals: {error:?}"))
            })
    }

    pub fn set_passthrough_mode(&mut self, enabled: bool, transition_expiry: Option<u32>) {
        self.session
            .set_passthrough_mode(enabled, transition_expiry);
    }

    pub fn encrypt_opus(&mut self, opus_frame: &[u8]) -> Result<Vec<u8>, DiscordError> {
        self.session
            .encrypt_opus(opus_frame)
            .map(|frame| frame.into_owned())
            .map_err(|error| {
                invalid_data_error(format!("failed to encrypt DAVE Opus frame: {error:?}"))
            })
    }
}

impl VoiceDaveFrameEncryptor for VoiceDaveySession {
    fn encrypt_opus(&mut self, opus_frame: &[u8]) -> Result<Vec<u8>, DiscordError> {
        VoiceDaveySession::encrypt_opus(self, opus_frame)
    }
}

impl VoiceDaveFrameDecryptor for VoiceDaveySession {
    fn decrypt_frame(
        &mut self,
        _rtp: &VoiceRtpHeader,
        user_id: Option<UserId>,
        frame: &VoiceDaveFrame,
    ) -> Result<Vec<u8>, DiscordError> {
        let user_id = user_id
            .map(UserId::get)
            .ok_or_else(|| invalid_data_error("DAVE frame decrypt requires a mapped user_id"))?;
        self.session
            .decrypt(user_id, davey::MediaType::AUDIO, &frame.bytes)
            .map_err(|error| invalid_data_error(format!("failed to decrypt DAVE frame: {error:?}")))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `VoiceSpeakingUpdate`.
pub struct VoiceSpeakingUpdate {
    pub speaking: u64,
    pub ssrc: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
}

fn parse_rtp_header(bytes: &[u8]) -> Result<VoiceRtpHeader, DiscordError> {
    if bytes.len() < RTP_HEADER_LEN {
        return Err(invalid_data_error(
            "voice RTP packet is shorter than 12 bytes",
        ));
    }

    let version = bytes[0] >> 6;
    let padding = bytes[0] & 0x20 != 0;
    let extension = bytes[0] & 0x10 != 0;
    let csrc_count = usize::from(bytes[0] & 0x0f);
    let marker = bytes[1] & 0x80 != 0;
    let payload_type = bytes[1] & 0x7f;
    let sequence = u16::from_be_bytes([bytes[2], bytes[3]]);
    let timestamp = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let ssrc = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

    let mut header_len = RTP_HEADER_LEN + csrc_count * RTP_WORD_LEN;
    if bytes.len() < header_len {
        return Err(invalid_data_error(
            "voice RTP packet has truncated CSRC list",
        ));
    }

    if extension {
        if bytes.len() < header_len + RTP_WORD_LEN {
            return Err(invalid_data_error(
                "voice RTP packet has truncated extension header",
            ));
        }
        let extension_words = usize::from(u16::from_be_bytes([
            bytes[header_len + 2],
            bytes[header_len + 3],
        ]));
        header_len += RTP_WORD_LEN + extension_words * RTP_WORD_LEN;
        if bytes.len() < header_len {
            return Err(invalid_data_error(
                "voice RTP packet has truncated extension payload",
            ));
        }
    }

    Ok(VoiceRtpHeader {
        version,
        padding,
        extension,
        marker,
        payload_type,
        sequence,
        timestamp,
        ssrc,
        header_len,
    })
}

fn parse_uleb128(bytes: &[u8]) -> Result<(u64, usize), DiscordError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;

    for (index, byte) in bytes.iter().enumerate() {
        let chunk = u64::from(byte & 0x7f);
        if shift >= 64 && chunk != 0 {
            return Err(invalid_data_error("DAVE ULEB128 value overflows u64"));
        }
        value |= chunk
            .checked_shl(shift)
            .ok_or_else(|| invalid_data_error("DAVE ULEB128 value overflows u64"))?;
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
        shift += 7;
    }

    Err(invalid_data_error("truncated DAVE ULEB128 value"))
}

fn parse_dave_frame(frame: &[u8]) -> Result<VoiceDaveFrame, DiscordError> {
    if frame.len() < DAVE_MIN_SUPPLEMENTAL_LEN {
        return Err(invalid_data_error("DAVE frame is too short"));
    }
    if frame[frame.len() - 2..] != DAVE_MAGIC_MARKER {
        return Err(invalid_data_error("DAVE frame is missing magic marker"));
    }

    let supplemental_size = frame[frame.len() - DAVE_FOOTER_LEN];
    let supplemental_size_usize = usize::from(supplemental_size);
    if supplemental_size_usize < DAVE_MIN_SUPPLEMENTAL_LEN || supplemental_size_usize > frame.len()
    {
        return Err(invalid_data_error("invalid DAVE supplemental data size"));
    }

    let supplemental_start = frame.len() - supplemental_size_usize;
    let supplemental = &frame[supplemental_start..frame.len() - DAVE_FOOTER_LEN];
    if supplemental.len() < DAVE_AUTH_TAG_LEN + 1 {
        return Err(invalid_data_error("DAVE supplemental data is truncated"));
    }

    let mut auth_tag = [0_u8; DAVE_AUTH_TAG_LEN];
    auth_tag.copy_from_slice(&supplemental[..DAVE_AUTH_TAG_LEN]);
    let (nonce, nonce_len) = parse_uleb128(&supplemental[DAVE_AUTH_TAG_LEN..])?;
    let nonce =
        u32::try_from(nonce).map_err(|_| invalid_data_error("DAVE frame nonce exceeds 32 bits"))?;

    let mut ranges = Vec::new();
    let mut cursor = DAVE_AUTH_TAG_LEN + nonce_len;
    while cursor < supplemental.len() {
        let (offset, offset_len) = parse_uleb128(&supplemental[cursor..])?;
        cursor += offset_len;
        if cursor >= supplemental.len() {
            return Err(invalid_data_error(
                "DAVE unencrypted range is missing length",
            ));
        }
        let (len, len_len) = parse_uleb128(&supplemental[cursor..])?;
        cursor += len_len;
        ranges.push(VoiceDaveUnencryptedRange { offset, len });
    }

    Ok(VoiceDaveFrame {
        bytes: frame.to_vec(),
        ciphertext: frame[..supplemental_start].to_vec(),
        auth_tag,
        nonce,
        unencrypted_ranges: ranges,
        supplemental_size,
    })
}

/// Typed Discord API object for `VoiceRuntimeHandle`.
///
/// A connected voice session. When Discord negotiates DAVE end-to-end
/// encryption for the call, the runtime runs the MLS group on its own: the
/// plain send methods ([`Self::send_opus_frame`],
/// [`Self::send_opus_frame_after_gap`], [`Self::play_opus_frames`], and an
/// [`AudioPlayer`](crate::voice::player::AudioPlayer) subscribed with
/// `subscribe_runtime`) encrypt with DAVE, and the plain receive methods
/// ([`Self::recv_voice_packet`], [`Self::recv_decoded_voice_packet`])
/// decrypt each sender's frames. [`Self::dave_status`] shows where the
/// session is.
pub struct VoiceRuntimeHandle {
    state_rx: watch::Receiver<VoiceRuntimeState>,
    command_tx: mpsc::UnboundedSender<VoiceGatewayCommand>,
    close_tx: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), DiscordError>>,
    /// Closes when the gateway task ends, however it ends.
    task_done: watch::Receiver<()>,
    udp_socket: Arc<UdpSocket>,
    // Concurrent `&self` senders must take sequence, timestamp, and nonce in
    // send order, so the lock is held across the UDP send (tokio mutex).
    outbound_rtp: Mutex<VoiceOutboundRtpState>,
    // The DAVE session, shared with the gateway task (MLS messages) and used
    // by every send and receive here; a std lock, never held across an
    // await. Senders take it inside `outbound_rtp`, so DAVE frame counters
    // go out in RTP order.
    dave: Arc<StdMutex<DaveController>>,
    /// Published by `dave` after every change; read without its lock, so
    /// [`Self::is_playable`] never waits for MLS work.
    dave_status: watch::Receiver<VoiceDaveStatus>,
}

/// Locks the DAVE controller.
///
/// A poisoned lock means `davey` panicked in the middle of an operation;
/// its state may be stale, but every later call still checks it and fails
/// with an error instead of panicking again, so the guard is used anyway.
fn lock_dave(dave: &StdMutex<DaveController>) -> MutexGuard<'_, DaveController> {
    dave.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a send puts inside transport encryption.
enum OutboundMedia<'a> {
    /// An Opus frame the runtime's DAVE session encrypts when the call is
    /// end-to-end encrypted.
    Opus(&'a [u8]),
    /// A frame a caller already encrypted with its own DAVE encryptor.
    DaveFrame(&'a [u8]),
}

impl VoiceRuntimeHandle {
    pub fn state(&self) -> VoiceRuntimeState {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<VoiceRuntimeState> {
        self.state_rx.clone()
    }

    pub fn udp_socket(&self) -> Arc<UdpSocket> {
        Arc::clone(&self.udp_socket)
    }

    /// The state of the call's DAVE end-to-end encryption.
    pub fn dave_status(&self) -> VoiceDaveStatus {
        self.dave_status.borrow().clone()
    }

    /// A watch receiver that sees every change of [`Self::dave_status`].
    pub fn subscribe_dave_status(&self) -> watch::Receiver<VoiceDaveStatus> {
        self.dave_status.clone()
    }

    /// Whether this connection can send audio now: its gateway task is
    /// running and [`VoiceDaveStatus::can_send_audio`] holds. Never blocks.
    ///
    /// In an end-to-end encrypted call that is the case once this client is
    /// a member of the call's MLS group, which needs another member in the
    /// call. An [`AudioPlayer`](crate::voice::player::AudioPlayer) counts
    /// only playable connections as subscribed.
    pub fn is_playable(&self) -> bool {
        !self.task.is_finished() && self.dave_status.borrow().can_send_audio()
    }

    /// Waits until [`Self::is_playable`] holds.
    ///
    /// Wrap it in `tokio::time::timeout`: alone in a DAVE call, the MLS
    /// group (and with it the media keys) only forms once someone joins.
    ///
    /// # Errors
    ///
    /// When the voice gateway connection ends first.
    pub async fn wait_until_playable(&self) -> Result<(), DiscordError> {
        let closed =
            || DiscordError::voice("the voice connection closed before it could send audio");
        let mut status = self.dave_status.clone();
        let mut done = self.task_done.clone();
        loop {
            if self.task.is_finished() {
                return Err(closed());
            }
            if status.borrow_and_update().can_send_audio() {
                return Ok(());
            }
            tokio::select! {
                changed = status.changed() => {
                    // The controller, and with it the sender, lives as long
                    // as this handle, so this only fails if it is dropped.
                    changed.map_err(|_| DiscordError::voice("the DAVE status is gone"))?;
                }
                // Nothing is ever sent on it: it errs once the gateway task
                // has ended and dropped the sender.
                _ = done.changed() => return Err(closed()),
            }
        }
    }

    /// The DAVE verification code shared with `user_id`, the code Discord
    /// clients show when verifying a call member (DAVE "Verification
    /// Fingerprint").
    ///
    /// # Errors
    ///
    /// When the call has no DAVE session, or `user_id` is not in the MLS
    /// group.
    pub fn dave_verification_code(&self, user_id: UserId) -> Result<String, DiscordError> {
        lock_dave(&self.dave).verification_code(user_id)
    }

    /// Hands commands the DAVE session produced outside the gateway task
    /// (a recovery triggered by failed decryption) to that task.
    fn send_dave_commands(&self, commands: Vec<VoiceGatewayCommand>) {
        for command in commands {
            if self.command_tx.send(command).is_err() {
                tracing::debug!("voice gateway task stopped; DAVE command not sent");
            }
        }
    }

    /// The transport mode and key from SESSION_DESCRIPTION.
    fn transport_keys(&self) -> Result<(VoiceEncryptionMode, Vec<u8>), DiscordError> {
        let state = self.state_rx.borrow();
        let description = state
            .session_description
            .as_ref()
            .ok_or_else(|| invalid_data_error("missing voice session description"))?;
        Ok((description.mode.clone(), description.secret_key.clone()))
    }

    /// Parses and transport-decrypts a received packet, and looks up its
    /// sender.
    fn open_transport(
        &self,
        raw: &VoiceRawUdpPacket,
    ) -> Result<(VoiceRtpHeader, Vec<u8>, Option<UserId>), DiscordError> {
        let (mode, secret_key) = self.transport_keys()?;
        let rtp = parse_rtp_header(&raw.bytes)?;
        let frame = decrypt_transport_payload(&raw.bytes, &rtp, &mode, &secret_key)?;
        let user_id = self.state_rx.borrow().ssrc_users.get(&rtp.ssrc).copied();
        Ok((rtp, frame, user_id))
    }

    pub async fn recv_raw_udp_packet(
        &self,
        max_len: usize,
    ) -> Result<VoiceRawUdpPacket, DiscordError> {
        if max_len == 0 {
            return Err(invalid_data_error("max_len must be greater than zero"));
        }

        let mut buffer = vec![0_u8; max_len];
        let received = self.udp_socket.recv(&mut buffer).await?;
        buffer.truncate(received);
        Ok(VoiceRawUdpPacket::from_bytes(buffer))
    }

    /// Receives the next packet with media and removes transport encryption
    /// and, in an end-to-end encrypted call, DAVE encryption.
    ///
    /// The sender's DAVE keys are found through the SSRC → user mapping
    /// from SPEAKING updates. Packets that cannot be decrypted are skipped
    /// rather than returned as errors (a few always fail around MLS
    /// transitions, and an error would end receive loops that use `?`);
    /// [`VoiceDaveStatus::undecryptable_packets`] counts them. The Opus
    /// silence frame (`F8 FF FE`), which Discord sends unencrypted for muted
    /// users, is returned as is. Transport errors (a malformed packet, a
    /// failed AEAD check) are still returned.
    ///
    /// Cancel safe: a cancelled call loses at most the packet it was
    /// skipping.
    pub async fn recv_voice_packet(
        &self,
        max_len: usize,
    ) -> Result<VoiceReceivedPacket, DiscordError> {
        loop {
            let raw = self.recv_raw_udp_packet(max_len).await?;
            let (rtp, frame, user_id) = self.open_transport(&raw)?;
            let inbound = lock_dave(&self.dave).decrypt_inbound(user_id, frame);
            self.send_dave_commands(inbound.commands);
            if let Some(opus_frame) = inbound.media {
                return Ok(VoiceReceivedPacket {
                    raw,
                    rtp,
                    user_id,
                    opus_frame,
                });
            }
        }
    }

    /// Receives one packet and removes transport encryption and, on a DAVE
    /// session, DAVE encryption through `dave_decryptor` instead of the
    /// runtime's own DAVE session.
    ///
    /// A payload that is exactly the Opus silence frame (`F8 FF FE`) is
    /// returned unchanged without calling `dave_decryptor`: Discord sends it
    /// unencrypted for muted users, as the DAVE protocol allows.
    pub async fn recv_voice_packet_with_dave<D>(
        &self,
        max_len: usize,
        dave_decryptor: &mut D,
    ) -> Result<VoiceReceivedPacket, DiscordError>
    where
        D: VoiceDaveFrameDecryptor,
    {
        let raw = self.recv_raw_udp_packet(max_len).await?;
        let (rtp, transport_frame, user_id) = self.open_transport(&raw)?;
        // Why the silence check: Discord's SFU sends the 3-byte Opus silence
        // packet for a muted sender without DAVE encryption, even inside an
        // E2EE session, and the DAVE decrypting transform must let a frame
        // that is exactly those bytes through (DAVE protocol, "Silence
        // packets": <https://daveprotocol.com/#silence-packets>; discord.js
        // `DAVESession.decrypt` does the same). Parsing it as a DAVE frame
        // failed with "too short" and ended every receive loop that used `?`.
        // Only the exact frame passes, as the spec requires. The spec notes
        // that whoever can inject packets could inject silence; here that
        // means the SFU, because transport AEAD already rejected anything
        // sealed without the session key.
        let dave_active = lock_dave(&self.dave).protocol_version() > 0;
        let opus_frame = if dave_active && transport_frame.as_slice() != OPUS_SILENCE_FRAME {
            let dave_frame = parse_dave_frame(&transport_frame)?;
            dave_decryptor.decrypt_frame(&rtp, user_id, &dave_frame)?
        } else {
            transport_frame
        };

        Ok(VoiceReceivedPacket {
            raw,
            rtp,
            user_id,
            opus_frame,
        })
    }

    /// [`Self::recv_voice_packet`], then Opus decoding.
    pub async fn recv_decoded_voice_packet(
        &self,
        decoder: &mut VoiceOpusDecoder,
        max_len: usize,
    ) -> Result<VoiceDecodedPacket, DiscordError> {
        let packet = self.recv_voice_packet(max_len).await?;
        decoder.decode_packet(packet)
    }

    pub async fn recv_decoded_voice_packet_with_dave<D>(
        &self,
        decoder: &mut VoiceOpusDecoder,
        max_len: usize,
        dave_decryptor: &mut D,
    ) -> Result<VoiceDecodedPacket, DiscordError>
    where
        D: VoiceDaveFrameDecryptor,
    {
        let packet = self
            .recv_voice_packet_with_dave(max_len, dave_decryptor)
            .await?;
        decoder.decode_packet(packet)
    }

    pub fn send(&self, command: VoiceGatewayCommand) -> Result<(), DiscordError> {
        self.command_tx
            .send(command)
            .map_err(|error| invalid_data_error(format!("failed to send voice command: {error}")))
    }

    pub fn set_speaking(&self, flags: VoiceSpeakingFlags, delay: u32) -> Result<(), DiscordError> {
        let ssrc = self.state_rx.borrow().ready.ssrc;
        self.send(VoiceGatewayCommand::Speaking(
            VoiceSpeakingCommand::new(ssrc).speaking(flags).delay(delay),
        ))
    }

    /// Sends a raw `dave_protocol_ready_for_transition`. The runtime already
    /// answers every transition itself; this is for diagnostics.
    pub fn send_dave_transition_ready(&self, transition_id: u64) -> Result<(), DiscordError> {
        self.send(VoiceGatewayCommand::DaveProtocolTransitionReady { transition_id })
    }

    /// Sends a raw MLS key package. The runtime already sends its own; a
    /// second one on the same connection confuses the voice gateway.
    pub fn send_dave_mls_key_package(
        &self,
        key_package: impl Into<Vec<u8>>,
    ) -> Result<(), DiscordError> {
        self.send(VoiceGatewayCommand::DaveMlsKeyPackage {
            key_package: key_package.into(),
        })
    }

    /// Sends a raw MLS commit/welcome. The runtime already commits pending
    /// proposals itself.
    pub fn send_dave_mls_commit_welcome(
        &self,
        commit: impl Into<Vec<u8>>,
        welcome: Option<Vec<u8>>,
    ) -> Result<(), DiscordError> {
        self.send(VoiceGatewayCommand::DaveMlsCommitWelcome {
            commit: commit.into(),
            welcome,
        })
    }

    /// Sends a raw `dave_mls_invalid_commit_welcome`. The runtime already
    /// reports invalid transitions and rejoins itself.
    pub fn send_dave_mls_invalid_commit_welcome(
        &self,
        transition_id: u64,
    ) -> Result<(), DiscordError> {
        self.send(VoiceGatewayCommand::DaveMlsInvalidCommitWelcome { transition_id })
    }

    /// Sends one Opus frame, DAVE-encrypted when the call is end-to-end
    /// encrypted.
    ///
    /// # Errors
    ///
    /// Besides transport failures: when the call is end-to-end encrypted and
    /// this client holds no media keys yet ([`Self::is_playable`] is
    /// `false`). Nothing is sent or used up then, not even a nonce.
    pub async fn send_opus_frame(
        &self,
        opus_frame: &[u8],
        duration: Duration,
    ) -> Result<VoiceOutboundPacket, DiscordError> {
        self.send_media(OutboundMedia::Opus(opus_frame), Duration::ZERO, duration)
            .await
    }

    /// Like [`Self::send_opus_frame`], for a frame that follows `gap` of
    /// media that was never transmitted, such as frames an audio player
    /// dropped while this connection lagged.
    ///
    /// The RTP timestamp moves across the gap, because RFC 7587 timestamps
    /// count 48 kHz sampling time, while the sequence number and nonce
    /// advance by one as for any packet.
    ///
    /// Like `send_opus_frame`, this is cancel safe for the RTP state: the
    /// sequence number and timestamp only advance once the datagram has been
    /// sent, so a send that fails or is dropped before completing leaves no
    /// hole in either. Each attempt still uses up a fresh nonce, and in a
    /// DAVE call a DAVE frame counter, which receivers tolerate like packet
    /// loss.
    pub async fn send_opus_frame_after_gap(
        &self,
        opus_frame: &[u8],
        duration: Duration,
        gap: Duration,
    ) -> Result<VoiceOutboundPacket, DiscordError> {
        self.send_media(OutboundMedia::Opus(opus_frame), gap, duration)
            .await
    }

    async fn send_media(
        &self,
        media: OutboundMedia<'_>,
        gap: Duration,
        duration: Duration,
    ) -> Result<VoiceOutboundPacket, DiscordError> {
        let (mode, secret_key) = self.transport_keys()?;
        // The RTP state stays locked across the UDP send and is committed
        // only after it. `UdpSocket::send` is cancel safe (a cancelled send
        // transmitted nothing), so a failed or cancelled send leaves the
        // sequence and timeline untouched, and concurrent senders cannot
        // reorder packets.
        let mut outbound_rtp = self.outbound_rtp.lock().await;
        // DAVE runs inside the RTP lock so its frame counter follows RTP
        // order; the std lock is released before the send below.
        let payload: Cow<'_, [u8]> = match media {
            OutboundMedia::Opus(frame) => {
                if frame.is_empty() {
                    return Err(invalid_data_error("opus frame must not be empty"));
                }
                lock_dave(&self.dave).encrypt_outbound(frame)?
            }
            OutboundMedia::DaveFrame(frame) => {
                if lock_dave(&self.dave).protocol_version() == 0 {
                    return Err(invalid_data_error(
                        "DAVE encrypted Opus send requires an active DAVE session",
                    ));
                }
                Cow::Borrowed(frame)
            }
        };
        let packet = outbound_rtp.seal_packet(&payload, gap, &mode, &secret_key)?;
        self.udp_socket.send(&packet.bytes).await?;
        outbound_rtp.commit_packet(gap, duration);
        Ok(packet)
    }

    /// Sends a frame encrypted with your own DAVE `encryptor` instead of the
    /// runtime's DAVE session.
    pub async fn send_opus_frame_with_dave<E>(
        &self,
        opus_frame: &[u8],
        duration: Duration,
        encryptor: &mut E,
    ) -> Result<VoiceOutboundPacket, DiscordError>
    where
        E: VoiceDaveFrameEncryptor + ?Sized,
    {
        self.send_opus_frame_with_dave_after_gap(opus_frame, duration, Duration::ZERO, encryptor)
            .await
    }

    /// Like [`Self::send_opus_frame_with_dave`], for a frame that follows
    /// `gap` of media that was never transmitted; see
    /// [`Self::send_opus_frame_after_gap`].
    ///
    /// The DAVE encryptor runs before the RTP state is locked, so a frame
    /// whose send fails or is cancelled has still used up one DAVE frame
    /// counter; DAVE receivers tolerate such gaps like packet loss.
    pub async fn send_opus_frame_with_dave_after_gap<E>(
        &self,
        opus_frame: &[u8],
        duration: Duration,
        gap: Duration,
        encryptor: &mut E,
    ) -> Result<VoiceOutboundPacket, DiscordError>
    where
        E: VoiceDaveFrameEncryptor + ?Sized,
    {
        let encrypted = encryptor.encrypt_opus(opus_frame)?;
        self.send_media(OutboundMedia::DaveFrame(&encrypted), gap, duration)
            .await
    }
    #[cfg(feature = "voice-encode")]
    pub async fn play_audio_source<S>(
        &self,
        source: &mut S,
        encoder: &mut VoiceOpusEncoder,
    ) -> Result<usize, DiscordError>
    where
        S: AudioSource + ?Sized,
    {
        self.play_audio_source_limited(source, encoder, None).await
    }

    #[cfg(feature = "voice-encode")]
    pub async fn play_audio_source_limited<S>(
        &self,
        source: &mut S,
        encoder: &mut VoiceOpusEncoder,
        max_frames: Option<usize>,
    ) -> Result<usize, DiscordError>
    where
        S: AudioSource + ?Sized,
    {
        let frames = encoder.encode_source_frames(source, max_frames)?;
        self.play_opus_frames(frames).await
    }

    pub async fn play_opus_frames<I>(&self, frames: I) -> Result<usize, DiscordError>
    where
        I: IntoIterator<Item = VoiceOpusFrame>,
    {
        self.set_speaking(VoiceSpeakingFlags::MICROPHONE, 0)?;
        let mut sent = 0;
        let mut result = Ok(());
        for frame in frames {
            if let Err(error) = self.send_opus_frame(&frame.bytes, frame.duration).await {
                result = Err(error);
                break;
            }
            sent += 1;
            sleep(frame.duration).await;
        }
        let stop_result = self.set_speaking(VoiceSpeakingFlags::default(), 0);
        result?;
        stop_result?;
        Ok(sent)
    }

    pub async fn close(mut self) -> Result<(), DiscordError> {
        if let Some(close_tx) = self.close_tx.take() {
            // Fails only if the task already ended; awaited just below.
            close_tx.send(()).ok();
        }

        match self.task.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) if error.to_string().contains("ResetWithoutClosingHandshake") => Ok(()),
            Ok(result) => result,
            Err(error) => Err(format!("voice runtime task failed: {error}").into()),
        }
    }
}

/// Provides the `connect` helper.
///
/// Connects the voice websocket, performs UDP discovery and protocol
/// selection, and waits for SESSION_DESCRIPTION. When it negotiates DAVE,
/// the returned handle's runtime takes part in the call's MLS group on its
/// own (see [`VoiceRuntimeHandle`]).
///
/// # Errors
///
/// A handshake stage that times out or fails, or the voice gateway closing
/// the connection; a close with code 4017 (DAVE required) explains why the
/// connection advertised no DAVE support.
pub async fn connect(config: VoiceRuntimeConfig) -> Result<VoiceRuntimeHandle, DiscordError> {
    let advertised_dave = config.advertised_dave_version();
    if config.channel_id.is_none() && config.max_dave_protocol_version.is_some_and(|v| v > 0) {
        tracing::warn!(
            "VoiceRuntimeConfig::channel_id is not set, so this voice connection cannot use DAVE \
             end-to-end encryption; Discord refuses it outside Stage channels (close code 4017)"
        );
    }
    let websocket_url = config.websocket_url();
    let (ws_stream, _) = voice_stage_timeout("websocket connect", VOICE_CONNECT_TIMEOUT, async {
        Ok(connect_async(&websocket_url).await?)
    })
    .await?;
    let (mut write, mut read) = ws_stream.split();
    let mut handshake = Handshake {
        config: &config,
        advertised_dave,
        backlog: Vec::new(),
        last_sequence: None,
    };

    let hello =
        voice_stage_timeout("hello", VOICE_HELLO_TIMEOUT, handshake.next_text(&mut read)).await?;
    let heartbeat_interval_ms = read_hello_interval(&hello)?;
    if let Some(seq) = VoiceEnvelope::parse(&hello)?.seq.0 {
        handshake.last_sequence = Some(seq);
    }

    let identify = build_identify_payload(&config);
    write
        .send(WsMessage::Text(identify.to_string().into()))
        .await?;

    let ready_payload = voice_stage_timeout(
        "ready",
        VOICE_READY_TIMEOUT,
        handshake.wait_for(&mut read, VOICE_OP_READY),
    )
    .await?;
    let ready: VoiceGatewayReady = VoiceEnvelope::parse(&ready_payload)?.data("ready")?;

    let udp_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    udp_socket.connect((&*ready.ip, ready.port)).await?;

    let request = VoiceUdpDiscoveryPacket::request(ready.ssrc);
    udp_socket.send(&request).await?;

    let mut discovery_buffer = [0_u8; VoiceUdpDiscoveryPacket::LEN];
    let received = timeout(
        VOICE_UDP_DISCOVERY_TIMEOUT,
        udp_socket.recv(&mut discovery_buffer),
    )
    .await
    .map_err(|_| invalid_data_error("voice UDP discovery timed out"))??;
    let discovery = VoiceUdpDiscoveryPacket::decode(&discovery_buffer[..received])?;
    let selected_mode = select_encryption_mode(&config, &ready)?;

    let select_protocol = VoiceGatewayCommand::SelectProtocol(VoiceSelectProtocolCommand::udp(
        discovery.address.clone(),
        discovery.port,
        selected_mode.clone(),
    ));
    write
        .send(WsMessage::Text(
            select_protocol.payload().to_string().into(),
        ))
        .await?;

    let session_description_payload = voice_stage_timeout(
        "session description",
        VOICE_SESSION_DESCRIPTION_TIMEOUT,
        handshake.wait_for(&mut read, VOICE_OP_SESSION_DESCRIPTION),
    )
    .await?;
    let session_description: VoiceSessionDescription =
        VoiceEnvelope::parse(&session_description_payload)?.data("session description")?;
    let dave_protocol_version = session_description.dave_protocol_version;
    let Handshake {
        backlog,
        last_sequence,
        ..
    } = handshake;

    let mut controller = DaveController::new(
        config.user_id,
        config.channel_id,
        u16::from(advertised_dave.unwrap_or(0)),
    );
    let initial_commands =
        controller.on_session_description(u16::from(dave_protocol_version.unwrap_or(0)));
    let dave_status = controller.subscribe();
    let dave = Arc::new(StdMutex::new(controller));

    let initial_state = VoiceRuntimeState {
        config,
        heartbeat_interval_ms,
        last_sequence,
        ready,
        discovery,
        selected_mode,
        session_description: Some(session_description),
        ssrc_users: HashMap::new(),
        speaking: HashMap::new(),
        dave: VoiceDaveState {
            protocol_version: dave_protocol_version,
            passthrough: dave_protocol_version.unwrap_or(0) == 0,
            ..VoiceDaveState::default()
        },
        resumed: false,
    };
    let (state_tx, state_rx) = watch::channel(initial_state);
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<VoiceGatewayCommand>();
    let (close_tx, mut close_rx) = oneshot::channel();
    let (task_done_tx, task_done) = watch::channel(());
    let udp_socket_handle = Arc::clone(&udp_socket);
    let outbound_rtp = Mutex::new(VoiceOutboundRtpState::new(state_tx.borrow().ready.ssrc));
    let gateway_dave = Arc::clone(&dave);

    let task = tokio::spawn(async move {
        // Dropped when this task ends, however it ends, which wakes
        // `wait_until_playable`.
        let _task_done = task_done_tx;
        let mut gateway = GatewayLoop {
            seq_ack: last_sequence,
            state_tx,
            dave: gateway_dave,
            heartbeat_ack_pending: false,
            heartbeat_sent_at: None,
        };
        let mut heartbeat = interval(Duration::from_millis(heartbeat_interval_ms));
        let mut heartbeat_nonce = initial_voice_heartbeat_nonce();
        let heartbeat_ack_timeout = voice_heartbeat_ack_timeout(heartbeat_interval_ms);

        // The key package first (DAVE: sent after SESSION_DESCRIPTION), then
        // whatever arrived during the handshake, in order: CLIENTS_CONNECT
        // must be known before the first proposals are checked against it.
        send_voice_commands(&mut write, initial_commands).await?;
        for message in backlog {
            match gateway.handle(message)? {
                GatewayStep::Continue(commands) => {
                    send_voice_commands(&mut write, commands).await?
                }
                GatewayStep::Closed => return Ok(()),
            }
        }

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if gateway.heartbeat_ack_pending {
                        if gateway
                            .heartbeat_sent_at
                            .map(|sent_at| sent_at.elapsed() >= heartbeat_ack_timeout)
                            .unwrap_or(false)
                        {
                            return Err(invalid_data_error("voice heartbeat ACK timed out"));
                        }
                        continue;
                    }
                    let heartbeat_payload = build_heartbeat_payload(
                        next_voice_heartbeat_nonce(&mut heartbeat_nonce),
                        gateway.seq_ack,
                    );
                    write.send(WsMessage::Text(heartbeat_payload.to_string().into())).await?;
                    gateway.heartbeat_ack_pending = true;
                    gateway.heartbeat_sent_at = Some(Instant::now());
                }
                command = command_rx.recv() => {
                    match command {
                        Some(command) => send_voice_command(&mut write, &command).await?,
                        None => break,
                    }
                }
                _ = &mut close_rx => {
                    // Closing either way; the peer just misses the frame.
                    if let Err(error) = write.send(WsMessage::Close(None)).await {
                        tracing::debug!("voice gateway close frame not sent: {error}");
                    }
                    break;
                }
                message = read.next() => {
                    match message {
                        Some(Ok(message)) => match gateway.handle(message)? {
                            GatewayStep::Continue(commands) => {
                                send_voice_commands(&mut write, commands).await?;
                            }
                            GatewayStep::Closed => break,
                        },
                        Some(Err(error)) => return Err(error.into()),
                        None => break,
                    }
                }
            }
        }

        Ok(())
    });

    Ok(VoiceRuntimeHandle {
        state_rx,
        command_tx,
        close_tx: Some(close_tx),
        task,
        task_done,
        udp_socket: udp_socket_handle,
        outbound_rtp,
        dave,
        dave_status,
    })
}

type VoiceWsWrite = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    WsMessage,
>;

type VoiceWsRead = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

async fn send_voice_command(
    write: &mut VoiceWsWrite,
    command: &VoiceGatewayCommand,
) -> Result<(), DiscordError> {
    if let Some(bytes) = command.binary_payload() {
        write.send(WsMessage::Binary(bytes.into())).await?;
    } else {
        write
            .send(WsMessage::Text(command.payload().to_string().into()))
            .await?;
    }
    Ok(())
}

async fn send_voice_commands(
    write: &mut VoiceWsWrite,
    commands: Vec<VoiceGatewayCommand>,
) -> Result<(), DiscordError> {
    for command in commands {
        send_voice_command(write, &command).await?;
    }
    Ok(())
}

/// What the gateway loop does after a message.
enum GatewayStep {
    /// Keep going after sending these commands.
    Continue(Vec<VoiceGatewayCommand>),
    /// The voice gateway closed the connection.
    Closed,
}

/// The gateway task's message handling, shared by the handshake backlog and
/// the live loop.
struct GatewayLoop {
    state_tx: watch::Sender<VoiceRuntimeState>,
    dave: Arc<StdMutex<DaveController>>,
    seq_ack: Option<i64>,
    heartbeat_ack_pending: bool,
    heartbeat_sent_at: Option<Instant>,
}

impl GatewayLoop {
    fn handle(&mut self, message: WsMessage) -> Result<GatewayStep, DiscordError> {
        let commands = match message {
            WsMessage::Text(text) => self.handle_text(text.as_str())?,
            WsMessage::Binary(bytes) => self.handle_binary(&bytes)?,
            WsMessage::Close(frame) => {
                log_voice_close(frame.as_ref());
                return Ok(GatewayStep::Closed);
            }
            _ => Vec::new(),
        };
        Ok(GatewayStep::Continue(commands))
    }

    fn handle_text(&mut self, text: &str) -> Result<Vec<VoiceGatewayCommand>, DiscordError> {
        let payload = VoiceEnvelope::parse(text)?;
        if let Some(seq) = payload.seq.0 {
            self.seq_ack = Some(seq);
            update_state(&self.state_tx, |state| state.last_sequence = Some(seq))?;
        }

        let mut commands = Vec::new();
        match payload.op.0 {
            Some(VOICE_OP_SESSION_DESCRIPTION) => {
                let description: VoiceSessionDescription = payload.data("session description")?;
                update_state(&self.state_tx, |state| {
                    state.dave.protocol_version = description.dave_protocol_version;
                    state.dave.passthrough = description.dave_protocol_version.unwrap_or(0) == 0;
                    state.session_description = Some(description);
                })?;
            }
            Some(VOICE_OP_RESUMED) => {
                update_state(&self.state_tx, |state| state.resumed = true)?;
            }
            Some(VOICE_OP_DAVE_PREPARE_TRANSITION) => {
                if let Some(data) = payload.lenient_data::<DaveTransitionData>() {
                    update_state(&self.state_tx, |state| {
                        state.dave.transition_id = data.transition_id.0;
                        if data.protocol_version.0 == Some(0) {
                            state.dave.passthrough = true;
                        }
                    })?;
                    if let (Some(transition_id), Some(version)) =
                        (data.transition_id.0, data.dave_version())
                    {
                        commands =
                            lock_dave(&self.dave).on_prepare_transition(transition_id, version);
                    }
                }
            }
            Some(VOICE_OP_DAVE_EXECUTE_TRANSITION) => {
                update_state(&self.state_tx, |state| {
                    state.dave.transition_id = None;
                    state.dave.pending_commit = None;
                    state.dave.pending_welcome = None;
                    state.dave.proposals.clear();
                })?;
                if let Some(transition_id) = payload
                    .lenient_data::<DaveTransitionData>()
                    .and_then(|data| data.transition_id.0)
                {
                    lock_dave(&self.dave).on_execute_transition(transition_id);
                }
            }
            Some(VOICE_OP_DAVE_PREPARE_EPOCH) => {
                if let Some(data) = payload.lenient_data::<DaveTransitionData>() {
                    update_state(&self.state_tx, |state| {
                        state.dave.transition_id = data.transition_id.0;
                        state.dave.epoch = data.epoch.0;
                        if let Some(protocol_version) = data
                            .protocol_version
                            .0
                            .and_then(|version| u8::try_from(version).ok())
                        {
                            state.dave.protocol_version = Some(protocol_version);
                            state.dave.passthrough = protocol_version == 0;
                        }
                    })?;
                    if let (Some(epoch), Some(version)) = (data.epoch.0, data.dave_version()) {
                        commands = lock_dave(&self.dave).on_prepare_epoch(epoch, version);
                    }
                }
            }
            Some(code) if code == u64::from(VoiceGatewayOpcode::SPEAKING.code()) => {
                let update: VoiceSpeakingUpdate = payload.data("speaking")?;
                update_state(&self.state_tx, |state| {
                    if let Some(user_id) = update.user_id {
                        state.ssrc_users.insert(update.ssrc, user_id);
                    }
                    state.speaking.insert(update.ssrc, update);
                })?;
            }
            Some(VOICE_OP_CLIENTS_CONNECT) => {
                let pairs = payload.connected_clients();
                if !pairs.is_empty() {
                    update_state(&self.state_tx, |state| {
                        for (ssrc, user_id) in pairs {
                            state.ssrc_users.insert(ssrc, user_id);
                        }
                    })?;
                }
                lock_dave(&self.dave).on_clients_connect(payload.connected_user_ids());
            }
            Some(VOICE_OP_CLIENT_DISCONNECT) => {
                if let Some(user_id) = payload
                    .lenient_data::<ClientDisconnectData>()
                    .and_then(|data| data.user_id.0)
                {
                    update_state(&self.state_tx, |state| {
                        state
                            .ssrc_users
                            .retain(|_, stored_user_id| stored_user_id != &user_id);
                    })?;
                    lock_dave(&self.dave).on_client_disconnect(user_id);
                }
            }
            Some(VOICE_OP_HEARTBEAT_ACK) => {
                self.heartbeat_ack_pending = false;
                self.heartbeat_sent_at = None;
            }
            Some(VOICE_OP_HELLO | VOICE_OP_READY | VOICE_OP_HEARTBEAT | VOICE_OP_RESUME) => {}
            _ => {}
        }
        Ok(commands)
    }

    /// A binary message: `[sequence u16][opcode u8][payload]` (DAVE protocol,
    /// "Voice Gateway Opcodes").
    fn handle_binary(&mut self, bytes: &[u8]) -> Result<Vec<VoiceGatewayCommand>, DiscordError> {
        if let Some(seq) = binary_sequence(bytes) {
            self.seq_ack = Some(seq);
            update_state(&self.state_tx, |state| state.last_sequence = Some(seq))?;
        }
        let Some((&opcode, payload)) = bytes.get(2..).and_then(<[u8]>::split_first) else {
            return Ok(Vec::new());
        };
        let opcode = u64::from(opcode);
        let mut commands = Vec::new();
        match opcode {
            VOICE_OP_DAVE_MLS_EXTERNAL_SENDER => {
                update_state(&self.state_tx, |state| {
                    state.dave.external_sender = Some(payload.to_vec());
                })?;
                lock_dave(&self.dave).on_external_sender(payload);
            }
            VOICE_OP_DAVE_MLS_PROPOSALS => {
                update_state(&self.state_tx, |state| {
                    state.dave.proposals.push(payload.to_vec());
                })?;
                commands = lock_dave(&self.dave).on_proposals(payload);
            }
            VOICE_OP_DAVE_MLS_ANNOUNCE_COMMIT_TRANSITION => {
                if let Some((transition_id, commit)) = payload.split_first_chunk::<2>() {
                    update_state(&self.state_tx, |state| {
                        state.dave.transition_id =
                            Some(u64::from(u16::from_be_bytes(*transition_id)));
                        state.dave.pending_commit = Some(commit.to_vec());
                    })?;
                }
                commands = lock_dave(&self.dave).on_announce_commit(payload);
            }
            VOICE_OP_DAVE_MLS_WELCOME => {
                if let Some((transition_id, welcome)) = payload.split_first_chunk::<2>() {
                    update_state(&self.state_tx, |state| {
                        state.dave.transition_id =
                            Some(u64::from(u16::from_be_bytes(*transition_id)));
                        state.dave.pending_welcome = Some(welcome.to_vec());
                    })?;
                }
                commands = lock_dave(&self.dave).on_welcome(payload);
            }
            _ => {}
        }
        Ok(commands)
    }
}

/// The sequence number that starts every binary voice gateway message.
fn binary_sequence(bytes: &[u8]) -> Option<i64> {
    let (seq, _) = bytes.split_first_chunk::<2>()?;
    Some(i64::from(u16::from_be_bytes(*seq)))
}

/// What Discord's voice close codes mean (Discord docs: Opcodes and Status
/// Codes › Voice Close Event Codes).
fn voice_close_description(code: u16) -> Option<&'static str> {
    Some(match code {
        4001 => "unknown opcode",
        4002 => "failed to decode payload",
        4003 => "not authenticated",
        4004 => "authentication failed",
        4005 => "already authenticated",
        4006 => "session no longer valid",
        4009 => "session timeout",
        4011 => "server not found",
        4012 => "unknown protocol",
        4014 => "disconnected (kicked, channel deleted, or the main gateway session dropped)",
        4015 => "voice server crashed",
        4016 => "unknown encryption mode",
        VOICE_CLOSE_DAVE_REQUIRED => "E2EE/DAVE protocol required",
        4020 => "bad request",
        4021 => "disconnected: rate limited",
        4022 => "disconnected: call terminated",
        _ => return None,
    })
}

fn describe_voice_close(frame: Option<&CloseFrame>) -> String {
    let Some(frame) = frame else {
        return "without a close code".to_owned();
    };
    let code = u16::from(frame.code);
    let mut text = format!("with code {code}");
    if let Some(description) = voice_close_description(code) {
        text.push_str(&format!(" ({description})"));
    }
    if !frame.reason.is_empty() {
        text.push_str(&format!(": {}", frame.reason.as_str()));
    }
    text
}

fn log_voice_close(frame: Option<&CloseFrame>) {
    let closed = describe_voice_close(frame);
    if frame.is_some_and(|frame| u16::from(frame.code) >= 4000) {
        tracing::warn!("voice gateway closed the connection {closed}");
    } else {
        tracing::debug!("voice gateway closed the connection {closed}");
    }
}

/// The handshake's view of the voice websocket: messages that arrive before
/// the one a stage waits for are kept for the runtime loop instead of being
/// dropped (CLIENTS_CONNECT can precede SESSION_DESCRIPTION, and DAVE needs
/// it to accept proposals; Songbird keeps them the same way).
struct Handshake<'a> {
    config: &'a VoiceRuntimeConfig,
    advertised_dave: Option<u8>,
    backlog: Vec<WsMessage>,
    last_sequence: Option<i64>,
}

impl Handshake<'_> {
    /// The next text message; binary messages before it go to the backlog.
    async fn next_text(&mut self, read: &mut VoiceWsRead) -> Result<String, DiscordError> {
        loop {
            match read.next().await {
                Some(Ok(WsMessage::Text(text))) => return Ok(text.as_str().to_owned()),
                Some(Ok(WsMessage::Binary(bytes))) => {
                    if let Some(seq) = binary_sequence(&bytes) {
                        self.last_sequence = Some(seq);
                    }
                    self.backlog.push(WsMessage::Binary(bytes));
                }
                Some(Ok(WsMessage::Close(frame))) => {
                    return Err(self.close_error(frame.as_ref()));
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error.into()),
                None => return Err(invalid_data_error("voice websocket closed unexpectedly")),
            }
        }
    }

    /// The next message with `opcode`; other text messages go to the
    /// backlog.
    async fn wait_for(
        &mut self,
        read: &mut VoiceWsRead,
        opcode: u64,
    ) -> Result<String, DiscordError> {
        loop {
            let payload = self.next_text(read).await?;
            let envelope = VoiceEnvelope::parse(&payload)?;
            if let Some(seq) = envelope.seq.0 {
                self.last_sequence = Some(seq);
            }
            if envelope.op.0 == Some(opcode) {
                return Ok(payload);
            }
            self.backlog.push(WsMessage::Text(payload.into()));
        }
    }

    fn close_error(&self, frame: Option<&CloseFrame>) -> DiscordError {
        let mut message = format!(
            "voice gateway closed the connection during the handshake {}",
            describe_voice_close(frame)
        );
        if frame.is_some_and(|frame| u16::from(frame.code) == VOICE_CLOSE_DAVE_REQUIRED) {
            let advertised = self.advertised_dave.unwrap_or(0);
            message.push_str(&format!(
                ". Discord requires DAVE end-to-end encryption in this channel, and this \
                 connection advertised max_dave_protocol_version {advertised}"
            ));
            if self.config.channel_id.is_none() {
                message.push_str(
                    " because VoiceRuntimeConfig::channel_id is not set (DAVE uses the channel as \
                     its MLS group); set it with VoiceRuntimeConfig::channel_id(..), or use \
                     VoiceManager::runtime_config / Context::connect_voice_runtime, which do",
                );
            } else {
                message.push_str(
                    "; leave VoiceRuntimeConfig::max_dave_protocol_version at its default",
                );
            }
        }
        DiscordError::voice(message)
    }
}

fn build_identify_payload(config: &VoiceRuntimeConfig) -> Value {
    let mut payload = serde_json::json!({
        "op": VOICE_OP_IDENTIFY,
        "d": {
            "server_id": config.server_id,
            "user_id": config.user_id,
            "session_id": config.session_id,
            "token": config.token,
        }
    });

    if let Some(max_dave_protocol_version) = config.advertised_dave_version() {
        payload["d"]["max_dave_protocol_version"] = serde_json::json!(max_dave_protocol_version);
    }

    payload
}

fn build_heartbeat_payload(heartbeat_nonce: u64, seq_ack: Option<i64>) -> Value {
    serde_json::json!({
        "op": VOICE_OP_HEARTBEAT,
        "d": {
            "t": heartbeat_nonce,
            "seq_ack": seq_ack.unwrap_or(-1),
        }
    })
}

fn initial_voice_heartbeat_nonce() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        // Saturating only from the year 2554 on.
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The low 32 bits, wrapping like an `as` cast but explicit about it: the
/// RTP nonce suffix starts from a time-derived value, not a saturated one.
fn low_32_bits(value: u64) -> u32 {
    let [b0, b1, b2, b3, ..] = value.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

fn next_voice_heartbeat_nonce(current: &mut u64) -> u64 {
    *current = current.wrapping_add(1);
    *current
}

/// Expands the 4-byte packet counter of the rtpsize modes into the AEAD
/// nonce: the counter's big-endian bytes come FIRST, followed by zeros.
///
/// Discord's transport docs only say the counter is "appended to payload";
/// the nonce layout is what compatible clients use: @discordjs/voice writes
/// it with `nonceBuffer.writeUInt32BE(nonce, 0)` (`Networking.ts`
/// `encryptOpusPacket`) and copies it back to offset 0 on receive
/// (`VoiceReceiver.ts` `decrypt`), and Songbird copies it into
/// `nonce[..4]` (`driver/crypto.rs`). The "4 least significant bytes" rule in
/// Discord's docs belongs to DAVE's AES-128-GCM frame nonce, which `davey`
/// handles, not to transport encryption. Changing this breaks interop in
/// both directions; the known-answer tests pin it.
fn rtpsize_nonce<const LEN: usize>(counter: &[u8; RTPSIZE_NONCE_SUFFIX_LEN]) -> [u8; LEN] {
    let mut nonce = [0_u8; LEN];
    nonce[..RTPSIZE_NONCE_SUFFIX_LEN].copy_from_slice(counter);
    nonce
}

/// Length of the cleartext part of an rtpsize packet, which is also the
/// AEAD additional data: the fixed header, the CSRCs and, if the X bit is
/// set, only the 4-byte extension preamble.
///
/// Discord: "The RTP size variants determine the unencrypted size of the RTP
/// header in the same way as SRTP, which considers CSRCs and (optionally) the
/// extension preamble to be part of the unencrypted header" (Voice
/// Connections › Transport Encryption Modes). Unlike RFC 3711 SRTP, the
/// extension body is therefore encrypted with the payload, as in
/// @discordjs/voice (`VoiceReceiver.ts` `parsePacket`: `headerSize += 4`) and
/// Songbird (`decrypt_rtp_in_place`: "Discord begins encryption after the RTP
/// extension *header*").
fn rtpsize_cleartext_len(packet: &[u8]) -> Result<usize, DiscordError> {
    let first = packet
        .first()
        .ok_or_else(|| invalid_data_error("voice RTP packet is empty"))?;
    let csrc_len = usize::from(first & 0x0f) * RTP_WORD_LEN;
    let preamble_len = if first & 0x10 != 0 {
        RTP_EXTENSION_PREAMBLE_LEN
    } else {
        0
    };
    Ok(RTP_HEADER_LEN + csrc_len + preamble_len)
}

/// Authenticates and decrypts an rtpsize packet and returns its media
/// payload (an Opus or DAVE frame) without the header extension body or the
/// RTP padding.
///
/// The AEAD boundary ([`rtpsize_cleartext_len`]) and the media offset
/// (`rtp.header_len`) differ by the extension body, and the padding sits at
/// the end of the ciphertext; both are only readable after decryption, so
/// they are stripped from the plaintext afterwards.
fn decrypt_transport_payload(
    packet: &[u8],
    rtp: &VoiceRtpHeader,
    mode: &VoiceEncryptionMode,
    secret_key: &[u8],
) -> Result<Vec<u8>, DiscordError> {
    if secret_key.len() != VOICE_SECRET_KEY_LEN {
        return Err(invalid_data_error("voice secret_key must be 32 bytes"));
    }
    let cleartext_len = rtpsize_cleartext_len(packet)?;
    let extension_body_len = rtp
        .header_len
        .checked_sub(cleartext_len)
        .ok_or_else(|| invalid_data_error("voice RTP header does not match the packet"))?;
    if packet.len() < rtp.header_len + AEAD_TAG_LEN + RTPSIZE_NONCE_SUFFIX_LEN {
        return Err(invalid_data_error(
            "voice RTP packet is too short for its header, AEAD tag, and nonce counter",
        ));
    }

    let (sealed, nonce_suffix) = packet
        .split_last_chunk::<RTPSIZE_NONCE_SUFFIX_LEN>()
        .ok_or_else(|| invalid_data_error("voice RTP packet is missing its nonce counter"))?;
    let (aad, ciphertext) = sealed.split_at(cleartext_len);
    let mut plaintext = ciphertext.to_vec();

    if mode == &VoiceEncryptionMode::aead_aes256_gcm_rtpsize() {
        let cipher = Aes256Gcm::new_from_slice(secret_key)
            .map_err(|_| invalid_data_error("invalid AES-GCM voice secret key"))?;
        let nonce = rtpsize_nonce::<AES_GCM_NONCE_LEN>(nonce_suffix);
        cipher
            .decrypt_in_place(AesNonce::from_slice(&nonce), aad, &mut plaintext)
            .map_err(|_| invalid_data_error("failed to decrypt AES-GCM voice packet"))?;
    } else if mode == &VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize() {
        let cipher = XChaCha20Poly1305::new_from_slice(secret_key)
            .map_err(|_| invalid_data_error("invalid XChaCha20-Poly1305 voice secret key"))?;
        let nonce = rtpsize_nonce::<XCHACHA20_NONCE_LEN>(nonce_suffix);
        cipher
            .decrypt_in_place(XNonce::from_slice(&nonce), aad, &mut plaintext)
            .map_err(|_| invalid_data_error("failed to decrypt XChaCha20-Poly1305 voice packet"))?;
    } else {
        return Err(invalid_data_error(format!(
            "unsupported voice encryption mode for receive: {mode:?}"
        )));
    }

    // The length check above leaves at least the extension body in the
    // plaintext. Its elements (RFC 8285) are not media, so they never reach
    // the Opus decoder or the DAVE frame parser.
    plaintext.drain(..extension_body_len);
    if rtp.padding {
        strip_rtp_padding(&mut plaintext)?;
    }
    Ok(plaintext)
}

/// Removes RTP padding from a decrypted media payload (the plaintext after
/// the extension body).
///
/// With the P bit set, the payload ends with padding whose last octet counts
/// the padding octets, itself included (RFC 3550 §5.1). In the rtpsize modes
/// the padding is inside the encrypted region, so it can only be removed
/// after decryption, as @discordjs/voice does (`VoiceReceiver.ts`
/// `parsePacket`); left in place, it corrupts the Opus frame and hides the
/// DAVE magic marker at the end of a DAVE frame. A count of zero (the count
/// octet counts itself) or one longer than the payload (it would reach into
/// the extension body) is malformed. A count equal to the payload length is
/// a padding-only packet and leaves an empty payload.
fn strip_rtp_padding(payload: &mut Vec<u8>) -> Result<(), DiscordError> {
    let count = payload
        .last()
        .copied()
        .map(usize::from)
        .ok_or_else(|| invalid_data_error("padded voice RTP packet has no padding count"))?;
    let media_len = payload
        .len()
        .checked_sub(count)
        .filter(|_| count > 0)
        .ok_or_else(|| {
            invalid_data_error(format!(
                "voice RTP padding count {count} does not fit a {}-byte payload",
                payload.len()
            ))
        })?;
    payload.truncate(media_len);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VoiceOutboundEncryptParams {
    sequence: u16,
    timestamp: u32,
    ssrc: u32,
    payload_type: u8,
    nonce_suffix: [u8; 4],
}

fn encrypt_transport_payload(
    params: VoiceOutboundEncryptParams,
    opus_frame: &[u8],
    mode: &VoiceEncryptionMode,
    secret_key: &[u8],
) -> Result<Vec<u8>, DiscordError> {
    // Outbound packets carry no CSRCs, header extension, or padding, so the
    // whole 12-byte header is the cleartext part and the AAD
    // (`rtpsize_cleartext_len`). Emitting an extension or padding would mean
    // encrypting its body or padding together with the payload.
    let header = build_rtp_header(
        params.sequence,
        params.timestamp,
        params.ssrc,
        params.payload_type,
    );
    seal_rtpsize_packet(header, opus_frame, params.nonce_suffix, mode, secret_key)
}

/// Builds an rtpsize packet: `cleartext` (the RTP header up to the
/// extension preamble, also the AAD), then `plaintext` encrypted, the
/// 16-byte tag, and the 4-byte `counter` that [`rtpsize_nonce`] expands.
fn seal_rtpsize_packet(
    cleartext: Vec<u8>,
    plaintext: &[u8],
    counter: [u8; RTPSIZE_NONCE_SUFFIX_LEN],
    mode: &VoiceEncryptionMode,
    secret_key: &[u8],
) -> Result<Vec<u8>, DiscordError> {
    if secret_key.len() != VOICE_SECRET_KEY_LEN {
        return Err(invalid_data_error("voice secret_key must be 32 bytes"));
    }

    let mut packet = cleartext;
    let mut encrypted = plaintext.to_vec();
    if mode == &VoiceEncryptionMode::aead_aes256_gcm_rtpsize() {
        let cipher = Aes256Gcm::new_from_slice(secret_key)
            .map_err(|_| invalid_data_error("invalid AES-GCM voice secret key"))?;
        let nonce = rtpsize_nonce::<AES_GCM_NONCE_LEN>(&counter);
        cipher
            .encrypt_in_place(AesNonce::from_slice(&nonce), &packet, &mut encrypted)
            .map_err(|_| invalid_data_error("failed to encrypt AES-GCM voice packet"))?;
    } else if mode == &VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize() {
        let cipher = XChaCha20Poly1305::new_from_slice(secret_key)
            .map_err(|_| invalid_data_error("invalid XChaCha20-Poly1305 voice secret key"))?;
        let nonce = rtpsize_nonce::<XCHACHA20_NONCE_LEN>(&counter);
        cipher
            .encrypt_in_place(XNonce::from_slice(&nonce), &packet, &mut encrypted)
            .map_err(|_| invalid_data_error("failed to encrypt XChaCha20-Poly1305 voice packet"))?;
    } else {
        return Err(invalid_data_error(format!(
            "unsupported voice encryption mode for send: {mode:?}"
        )));
    }

    packet.extend_from_slice(&encrypted);
    packet.extend_from_slice(&counter);
    Ok(packet)
}

fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32, payload_type: u8) -> Vec<u8> {
    let mut packet = vec![RTP_VERSION << 6, payload_type & 0x7f];
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());
    packet
}

/// Like `timestamp_increment`, but a zero gap adds nothing (the frame
/// increment rounds up to at least one sample).
fn gap_increment(sample_rate: u32, gap: Duration) -> u32 {
    if gap.is_zero() {
        0
    } else {
        timestamp_increment(sample_rate, gap)
    }
}

fn timestamp_increment(sample_rate: u32, duration: Duration) -> u32 {
    let nanos = duration.as_nanos();
    let samples = (u128::from(sample_rate) * nanos) / 1_000_000_000;
    u32::try_from(samples.max(1)).unwrap_or(u32::MAX)
}

fn read_hello_interval(payload: &str) -> Result<u64, DiscordError> {
    #[derive(Deserialize)]
    struct HelloData {
        #[serde(default)]
        heartbeat_interval: Lenient<u64>,
    }
    VoiceEnvelope::parse(payload)?
        .lenient_data::<HelloData>()
        .and_then(|hello| hello.heartbeat_interval.0)
        .ok_or_else(|| invalid_data_error("missing voice hello heartbeat interval"))
}

/// A voice gateway message as the runtime reads it: `op` and `seq` read
/// leniently (a malformed one is `None`, as before), and `d` left as text
/// borrowed from the frame.
///
/// Each payload is then decoded straight from that text with the reader the
/// rest of the crate uses, instead of parsing the frame into a `Value` and
/// decoding again from it.
#[derive(Deserialize)]
struct VoiceEnvelope<'a> {
    #[serde(default)]
    op: Lenient<u64>,
    #[serde(default)]
    seq: Lenient<i64>,
    #[serde(default, borrow)]
    d: Option<&'a RawValue>,
}

impl<'a> VoiceEnvelope<'a> {
    fn parse(text: &'a str) -> Result<Self, DiscordError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Decodes the required `d` payload of a `what` message.
    fn data<T: DeserializeOwned>(&self, what: &str) -> Result<T, DiscordError> {
        let data = self
            .d
            .ok_or_else(|| invalid_data_error(format!("missing {what} data")))?;
        Ok(serde_json::from_str(data.get())?)
    }

    /// Decodes `d` when present and well formed; `None` otherwise.
    fn lenient_data<T: DeserializeOwned>(&self) -> Option<T> {
        self.d
            .and_then(|data| serde_json::from_str(data.get()).ok())
    }

    /// The users of a CLIENTS_CONNECT payload (`user_ids`, voice gateway v8;
    /// Discord docs: Voice › Opcodes, "Clients Connect"). Invalid IDs are
    /// skipped.
    fn connected_user_ids(&self) -> Vec<UserId> {
        #[derive(Deserialize)]
        struct ClientsConnect {
            #[serde(default)]
            user_ids: Vec<Lenient<UserId>>,
        }
        self.lenient_data::<ClientsConnect>()
            .map(|data| data.user_ids.into_iter().filter_map(|id| id.0).collect())
            .unwrap_or_default()
    }

    /// The `(ssrc, user_id)` pairs of a CLIENTS_CONNECT payload. Entries
    /// without a valid user ID or a 32-bit SSRC are skipped, and a payload
    /// without a `users` array yields nothing.
    fn connected_clients(&self) -> Vec<(u32, UserId)> {
        #[derive(Deserialize)]
        struct Users<'a> {
            #[serde(default, borrow)]
            users: Option<&'a RawValue>,
        }
        #[derive(Deserialize)]
        struct Client {
            #[serde(default)]
            user_id: Lenient<UserId>,
            #[serde(default)]
            ssrc: Lenient<u64>,
        }
        let Some(users) = self
            .d
            .and_then(|data| serde_json::from_str::<Users<'_>>(data.get()).ok())
            .and_then(|users| users.users)
        else {
            return Vec::new();
        };
        let entries: Vec<&RawValue> = serde_json::from_str(users.get()).unwrap_or_default();
        entries
            .into_iter()
            .filter_map(|entry| {
                let client: Client = serde_json::from_str(entry.get()).ok()?;
                let ssrc = u32::try_from(client.ssrc.0?).ok()?;
                Some((ssrc, client.user_id.0?))
            })
            .collect()
    }
}

/// DAVE prepare-transition / execute-transition / prepare-epoch fields,
/// each read leniently.
#[derive(Deserialize)]
struct DaveTransitionData {
    #[serde(default)]
    transition_id: Lenient<u64>,
    #[serde(default)]
    epoch: Lenient<u64>,
    #[serde(default)]
    protocol_version: Lenient<u64>,
}

impl DaveTransitionData {
    /// The protocol version, if present and within DAVE's 16 bits.
    fn dave_version(&self) -> Option<u16> {
        self.protocol_version
            .0
            .and_then(|version| u16::try_from(version).ok())
    }
}

/// CLIENT_DISCONNECT's user, read leniently.
#[derive(Deserialize)]
struct ClientDisconnectData {
    #[serde(default)]
    user_id: Lenient<UserId>,
}

fn select_encryption_mode(
    config: &VoiceRuntimeConfig,
    ready: &VoiceGatewayReady,
) -> Result<VoiceEncryptionMode, DiscordError> {
    if let Some(preferred_mode) = &config.preferred_mode {
        if ready.modes.contains(preferred_mode) {
            return Ok(preferred_mode.clone());
        }
    }

    ready
        .modes
        .first()
        .cloned()
        .ok_or_else(|| invalid_data_error("voice ready payload did not include encryption modes"))
}

fn update_state(
    state_tx: &watch::Sender<VoiceRuntimeState>,
    update: impl FnOnce(&mut VoiceRuntimeState),
) -> Result<(), DiscordError> {
    let mut state = state_tx.borrow().clone();
    update(&mut state);
    state_tx.send(state).map_err(|error| {
        invalid_data_error(format!("failed to publish voice runtime state: {error}"))
    })
}

async fn voice_stage_timeout<T>(
    stage: &'static str,
    duration: Duration,
    future: impl Future<Output = Result<T, DiscordError>>,
) -> Result<T, DiscordError> {
    timeout(duration, future)
        .await
        .map_err(|_| invalid_data_error(format!("voice {stage} timed out")))?
}

fn voice_heartbeat_ack_timeout(heartbeat_interval_ms: u64) -> Duration {
    let doubled_interval = Duration::from_millis(heartbeat_interval_ms.saturating_mul(2));
    doubled_interval.max(Duration::from_secs(1))
}

#[cfg(test)]
mod tests {
    use crate::model::test_id;
    use std::collections::HashMap;
    use std::sync::Arc;

    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use tokio::net::{TcpListener, UdpSocket};
    use tokio::sync::{oneshot, watch};
    use tokio::time::timeout;
    use tokio::time::Duration;
    use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};

    use super::VoiceDaveFrameEncryptor;
    use super::{
        build_heartbeat_payload, build_identify_payload, connect, decrypt_transport_payload,
        gap_increment, next_voice_heartbeat_nonce, parse_dave_frame, parse_rtp_header,
        parse_uleb128, read_hello_interval, seal_rtpsize_packet, select_encryption_mode,
        update_state, voice_heartbeat_ack_timeout, voice_stage_timeout, VoiceDaveState,
        VoiceOpusDecoder, VoiceOutboundRtpState, VoiceRuntimeConfig, VoiceRuntimeState,
        VoiceSessionDescription,
    };
    #[cfg(feature = "voice-encode")]
    use super::{AudioMixer, AudioSource, PcmFrame, VoiceOpusEncoder};
    use crate::error::DiscordError;
    use crate::voice::VoiceGatewayCommand;
    use crate::voice::{
        VoiceEncryptionMode, VoiceGatewayReady, VoiceSpeakingFlags, VoiceUdpDiscoveryPacket,
    };

    // Known-answer vectors for the rtpsize transport modes, generated by
    // `ci/voice_transport_kat.py` outside this crate's AEAD code (Python
    // `cryptography` for AES-256-GCM and ChaCha20-Poly1305, plus an HChaCha20
    // checked against draft-irtf-cfrg-xchacha-03) and cross-checked byte for
    // byte with Node's AES-256-GCM and libsodium's XChaCha20-Poly1305 called
    // the way @discordjs/voice `encryptOpusPacket` calls them. The 32-bit
    // counter has four distinct nonzero bytes, so placing it anywhere but the
    // first four nonce bytes, or in the wrong byte order, changes every vector.
    const KAT_KEY: [u8; 32] = [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d,
        0x3e, 0x3f,
    ];
    /// Discord's Opus silence frame, the plaintext of every vector.
    const KAT_OPUS: [u8; 3] = [0xf8, 0xff, 0xfe];
    /// Outbound shape (no CSRCs, no extension): sequence 0x1234, timestamp
    /// 0x56789abc, SSRC 0xdeadbeef, counter 0x01020304.
    const KAT_AES_PLAIN: [u8; 35] = [
        0x80, 0x78, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xad, 0xbe, 0xef, 0x0a, 0x04, 0xd3,
        0x9b, 0x3c, 0xe7, 0xe7, 0xb4, 0xb6, 0xac, 0x42, 0x39, 0xef, 0x6a, 0x8a, 0x81, 0xdf, 0xae,
        0x10, 0x01, 0x02, 0x03, 0x04,
    ];
    const KAT_XCHACHA_PLAIN: [u8; 35] = [
        0x80, 0x78, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xad, 0xbe, 0xef, 0xc8, 0x9f, 0xa7,
        0x47, 0x7a, 0xc4, 0x82, 0xbb, 0x91, 0x87, 0x1c, 0x27, 0xaf, 0x5e, 0xec, 0xca, 0xc9, 0x5a,
        0x2a, 0x01, 0x02, 0x03, 0x04,
    ];
    /// Inbound shape: two CSRCs (0xcafebabe, 0x0badf00d) and a one-word
    /// RFC 8285 extension (preamble `be de 00 01`, body `10 ab 00 00`);
    /// sequence 0x0102, timestamp 0x0a0b0c0d, SSRC 0x11223344, counter
    /// 0xa1b2c3d4. The first 24 bytes are the cleartext AAD; the extension
    /// body is encrypted together with the Opus frame.
    const KAT_AES_EXTENDED: [u8; 51] = [
        0x92, 0x78, 0x01, 0x02, 0x0a, 0x0b, 0x0c, 0x0d, 0x11, 0x22, 0x33, 0x44, 0xca, 0xfe, 0xba,
        0xbe, 0x0b, 0xad, 0xf0, 0x0d, 0xbe, 0xde, 0x00, 0x01, 0x9b, 0xdd, 0x6d, 0xab, 0xe7, 0xda,
        0x95, 0xc2, 0x70, 0xa0, 0x8a, 0x60, 0x87, 0x0c, 0xfe, 0x15, 0x52, 0xa6, 0x49, 0x49, 0xd1,
        0x1a, 0x7c, 0xa1, 0xb2, 0xc3, 0xd4,
    ];
    const KAT_XCHACHA_EXTENDED: [u8; 51] = [
        0x92, 0x78, 0x01, 0x02, 0x0a, 0x0b, 0x0c, 0x0d, 0x11, 0x22, 0x33, 0x44, 0xca, 0xfe, 0xba,
        0xbe, 0x0b, 0xad, 0xf0, 0x0d, 0xbe, 0xde, 0x00, 0x01, 0x93, 0x72, 0x58, 0x88, 0x1a, 0xd7,
        0x7e, 0xec, 0x3c, 0x20, 0x5d, 0xa8, 0x4c, 0xb6, 0xb1, 0x6e, 0x39, 0x12, 0x1a, 0x80, 0xde,
        0x17, 0xfd, 0xa1, 0xb2, 0xc3, 0xd4,
    ];

    /// Padded shape: P and X bits, one CSRC (0xfeedface), a one-word
    /// extension (body `10 cd 00 00`), and the Opus frame followed by four
    /// octets of padding (`00 00 00 04`), all encrypted; sequence 0x0304,
    /// timestamp 0x0e0f1011, SSRC 0x55667788, counter 0x5a6b7c8d.
    const KAT_AES_PADDED: [u8; 51] = [
        0xb1, 0x78, 0x03, 0x04, 0x0e, 0x0f, 0x10, 0x11, 0x55, 0x66, 0x77, 0x88, 0xfe, 0xed, 0xfa,
        0xce, 0xbe, 0xde, 0x00, 0x01, 0x14, 0xcc, 0x08, 0xf3, 0x6a, 0xc2, 0x83, 0x16, 0xc1, 0xca,
        0x29, 0x02, 0xda, 0xd2, 0x88, 0x81, 0x49, 0x75, 0x87, 0xc1, 0xa6, 0x20, 0xf2, 0x41, 0x73,
        0x9d, 0xe2, 0x5a, 0x6b, 0x7c, 0x8d,
    ];
    const KAT_XCHACHA_PADDED: [u8; 51] = [
        0xb1, 0x78, 0x03, 0x04, 0x0e, 0x0f, 0x10, 0x11, 0x55, 0x66, 0x77, 0x88, 0xfe, 0xed, 0xfa,
        0xce, 0xbe, 0xde, 0x00, 0x01, 0xf5, 0x76, 0x70, 0xcb, 0x31, 0xe6, 0x5a, 0xe0, 0x17, 0xe6,
        0xbc, 0x5a, 0x16, 0x0e, 0x59, 0xff, 0x91, 0x57, 0xb5, 0xf0, 0x66, 0xdd, 0x8f, 0xf9, 0xd5,
        0x11, 0xa8, 0x5a, 0x6b, 0x7c, 0x8d,
    ];
    /// The DAVE frame from `voice_dave_frame_parser_reads_trailer_and_ranges`:
    /// ciphertext `01 02 03`, an 8-byte tag, nonce 300, one unencrypted
    /// range, the supplemental size, and the magic marker at the very end.
    const DAVE_FRAME: [u8; 18] = [
        1, 2, 3, 9, 9, 9, 9, 9, 9, 9, 9, 0xac, 0x02, 5, 6, 15, 0xfa, 0xfa,
    ];

    fn kat_padded() -> [(VoiceEncryptionMode, &'static [u8]); 2] {
        [
            (
                VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
                &KAT_AES_PADDED,
            ),
            (
                VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
                &KAT_XCHACHA_PADDED,
            ),
        ]
    }

    /// A packet with the P bit set: `cleartext` is the RTP header up to the
    /// extension preamble, `plaintext` everything encrypted after it. Uses
    /// the production sealing path, which the known-answer tests pin.
    fn seal_padded(mode: &VoiceEncryptionMode, extension: bool, plaintext: &[u8]) -> Vec<u8> {
        let mut cleartext = vec![0xa0, 0x78, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0, 0, 0, 3];
        if extension {
            cleartext[0] |= 0x10;
            cleartext.extend_from_slice(&[0xbe, 0xde, 0x00, 0x01]);
        }
        seal_rtpsize_packet(
            cleartext,
            plaintext,
            [0x0a, 0x0b, 0x0c, 0x0d],
            mode,
            &KAT_KEY,
        )
        .unwrap()
    }

    fn kat_modes() -> [(VoiceEncryptionMode, &'static [u8], &'static [u8]); 2] {
        [
            (
                VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
                &KAT_AES_PLAIN,
                &KAT_AES_EXTENDED,
            ),
            (
                VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
                &KAT_XCHACHA_PLAIN,
                &KAT_XCHACHA_EXTENDED,
            ),
        ]
    }

    #[test]
    fn voice_runtime_config_normalizes_voice_gateway_url() {
        let config = VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            "voice.discord.media:443",
        );
        assert_eq!(config.websocket_url(), "wss://voice.discord.media:443/?v=8");
    }

    #[test]
    fn voice_runtime_config_clamps_version_and_extends_existing_query() {
        let config = VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            "ws://127.0.0.1/socket?encoding=json",
        )
        .gateway_version(2)
        .preferred_mode(VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize())
        .max_dave_protocol_version(3);

        assert_eq!(
            config.websocket_url(),
            "ws://127.0.0.1/socket?encoding=json&v=4"
        );
        assert_eq!(
            config.preferred_mode,
            Some(VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize())
        );
        assert_eq!(config.max_dave_protocol_version, Some(3));
    }

    #[test]
    fn voice_runtime_debug_redacts_live_secret_material() {
        let config = VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "voice-token-value",
            "voice.discord.media",
        );
        let config_debug = format!("{config:?}");
        assert!(config_debug.contains("<redacted>"));
        assert!(!config_debug.contains("voice-token-value"));

        let description = VoiceSessionDescription {
            mode: VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
            secret_key: vec![7; 32],
            audio_codec: Some("opus".to_string()),
            dave_protocol_version: Some(1),
        };
        let description_debug = format!("{description:?}");
        assert!(description_debug.contains("<redacted:32 bytes>"));
        assert!(!description_debug.contains("[7, 7"));

        let dave = VoiceDaveState {
            protocol_version: Some(1),
            transition_id: Some(9),
            epoch: Some(1),
            passthrough: false,
            external_sender: Some(vec![8; 70]),
            proposals: vec![vec![9; 11]],
            pending_commit: Some(vec![10; 12]),
            pending_welcome: Some(vec![11; 13]),
        };
        let dave_debug = format!("{dave:?}");
        assert!(dave_debug.contains("<redacted:70 bytes>"));
        assert!(dave_debug.contains("proposal_lengths"));
        assert!(dave_debug.contains("pending_commit_len"));
        assert!(!dave_debug.contains("[8, 8"));
        assert!(!dave_debug.contains("[9, 9"));
        assert!(!dave_debug.contains("[10, 10"));
        assert!(!dave_debug.contains("[11, 11"));
    }

    #[test]
    fn voice_runtime_helper_builders_cover_optional_and_fallback_fields() {
        let config = VoiceRuntimeConfig {
            max_dave_protocol_version: None,
            ..VoiceRuntimeConfig::new(
                test_id(1),
                test_id(2),
                "session",
                "token",
                "voice.discord.media",
            )
        };
        assert_eq!(
            build_identify_payload(&config),
            serde_json::json!({
                "op": 0,
                "d": {
                    "server_id": "1",
                    "user_id": "2",
                    "session_id": "session",
                    "token": "token",
                }
            })
        );
        assert_eq!(
            build_heartbeat_payload(55, None),
            serde_json::json!({
                "op": 3,
                "d": {
                    "t": 55,
                    "seq_ack": -1,
                }
            })
        );
        assert_eq!(
            build_heartbeat_payload(55, Some(7)),
            serde_json::json!({
                "op": 3,
                "d": {
                    "t": 55,
                    "seq_ack": 7,
                }
            })
        );
    }

    #[test]
    fn voice_runtime_heartbeat_nonce_does_not_reuse_interval_value() {
        let heartbeat_interval_ms = 5_000;
        let mut nonce = heartbeat_interval_ms;

        let first = next_voice_heartbeat_nonce(&mut nonce);
        let second = next_voice_heartbeat_nonce(&mut nonce);

        assert_ne!(first, heartbeat_interval_ms);
        assert_ne!(first, second);
        assert_eq!(
            build_heartbeat_payload(first, None)["d"]["t"],
            serde_json::json!(first)
        );
    }

    #[test]
    fn voice_heartbeat_ack_timeout_scales_with_heartbeat_interval() {
        assert_eq!(voice_heartbeat_ack_timeout(20), Duration::from_secs(1));
        assert_eq!(voice_heartbeat_ack_timeout(5_000), Duration::from_secs(10));
    }

    #[tokio::test]
    async fn voice_stage_timeout_reports_the_timed_out_stage() {
        let error = voice_stage_timeout(
            "ready",
            Duration::from_millis(1),
            std::future::pending::<Result<(), crate::error::DiscordError>>(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("voice ready timed out"));
    }

    #[test]
    fn voice_envelope_reads_op_seq_and_payloads_from_text() {
        use super::{ClientDisconnectData, DaveTransitionData, VoiceEnvelope};

        let message = r#"{"op":5,"seq":"x","d":{"ssrc":7,"user_id":"9","speaking":1}}"#;
        let envelope = VoiceEnvelope::parse(message).unwrap();
        assert_eq!(envelope.op.0, Some(5));
        assert_eq!(
            envelope.seq.0, None,
            "a malformed seq is ignored, as before"
        );
        let update: super::VoiceSpeakingUpdate = envelope.data("speaking").unwrap();
        assert_eq!(update.ssrc, 7);

        let missing = VoiceEnvelope::parse(r#"{"op":4,"seq":3,"d":null}"#).unwrap();
        assert_eq!(missing.seq.0, Some(3));
        let error = missing
            .data::<super::VoiceSessionDescription>("session description")
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing session description data"));
        assert!(missing.lenient_data::<DaveTransitionData>().is_none());

        let transition =
            VoiceEnvelope::parse(r#"{"op":21,"d":{"transition_id":"bad","protocol_version":0}}"#)
                .unwrap()
                .lenient_data::<DaveTransitionData>()
                .unwrap();
        assert_eq!(transition.transition_id.0, None);
        assert_eq!(transition.protocol_version.0, Some(0));

        let connect = VoiceEnvelope::parse(
            r#"{"op":11,"d":{"users":[{"user_id":"5","ssrc":10},{"user_id":"bad","ssrc":11},{"user_id":"6","ssrc":5000000000},7]}}"#,
        )
        .unwrap();
        assert_eq!(connect.connected_clients(), vec![(10, test_id(5))]);
        assert!(VoiceEnvelope::parse(r#"{"op":11,"d":{"users":{}}}"#)
            .unwrap()
            .connected_clients()
            .is_empty());

        let disconnect = VoiceEnvelope::parse(r#"{"op":13,"d":{"user_id":"8"}}"#)
            .unwrap()
            .lenient_data::<ClientDisconnectData>()
            .and_then(|data| data.user_id.0);
        assert_eq!(disconnect, Some(test_id(8)));
        assert!(VoiceEnvelope::parse("[").is_err());
    }

    #[test]
    fn voice_runtime_read_hello_interval_and_mode_selection_handle_edge_cases() {
        assert_eq!(
            read_hello_interval(r#"{"op":8,"d":{"heartbeat_interval":250}}"#).unwrap(),
            250
        );
        assert!(read_hello_interval(r#"{"d":{"heartbeat_interval":250.5}}"#).is_err());
        assert!(read_hello_interval(r#"{"op":8}"#).is_err());
        assert!(read_hello_interval("not json").is_err());
        assert!(read_hello_interval(r#"{ "d": {} }"#)
            .unwrap_err()
            .to_string()
            .contains("missing voice hello heartbeat interval"));

        let preferred = VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize();
        let fallback = VoiceEncryptionMode::aead_aes256_gcm_rtpsize();
        let ready = VoiceGatewayReady::new(42, "127.0.0.1", 5000)
            .mode(fallback.clone())
            .mode(preferred.clone());
        let config = VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            "voice.discord.media",
        )
        .preferred_mode(preferred.clone());
        assert_eq!(select_encryption_mode(&config, &ready).unwrap(), preferred);

        let fallback_only = VoiceGatewayReady::new(42, "127.0.0.1", 5000).mode(fallback.clone());
        let config_without_match = VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            "voice.discord.media",
        )
        .preferred_mode(VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize());
        assert_eq!(
            select_encryption_mode(&config_without_match, &fallback_only).unwrap(),
            fallback
        );

        let empty_ready = VoiceGatewayReady::new(42, "127.0.0.1", 5000);
        assert!(select_encryption_mode(
            &VoiceRuntimeConfig::new(
                test_id(1),
                test_id(2),
                "session",
                "token",
                "voice.discord.media"
            ),
            &empty_ready
        )
        .unwrap_err()
        .to_string()
        .contains("did not include encryption modes"));
    }

    #[test]
    fn voice_receive_decrypts_rtp_size_known_answer_vectors() {
        for (mode, plain, _) in kat_modes() {
            let rtp = parse_rtp_header(plain).unwrap();
            assert_eq!(rtp.version, 2);
            assert_eq!(rtp.payload_type, 120);
            assert_eq!(rtp.sequence, 0x1234);
            assert_eq!(rtp.timestamp, 0x5678_9abc);
            assert_eq!(rtp.ssrc, 0xdead_beef);
            assert_eq!(rtp.header_len, 12);

            let decrypted = decrypt_transport_payload(plain, &rtp, &mode, &KAT_KEY).unwrap();
            assert_eq!(decrypted, KAT_OPUS, "{mode:?}");
        }
    }

    #[test]
    fn voice_receive_authenticates_extension_preamble_and_decrypts_extension_body() {
        for (mode, _, extended) in kat_modes() {
            let rtp = parse_rtp_header(extended).unwrap();
            assert!(rtp.extension);
            assert_eq!(
                (rtp.sequence, rtp.timestamp, rtp.ssrc),
                (0x0102, 0x0a0b_0c0d, 0x1122_3344)
            );
            // Fixed header, two CSRCs, preamble, and the one-word body: the
            // media payload offset once the packet is decrypted.
            assert_eq!(rtp.header_len, 12 + 8 + 4 + 4);

            // The extension body is not part of the media payload.
            let decrypted = decrypt_transport_payload(extended, &rtp, &mode, &KAT_KEY).unwrap();
            assert_eq!(decrypted, KAT_OPUS, "{mode:?}");

            // CSRCs and the preamble are authenticated (AAD), the extension
            // body is ciphertext: flipping a bit in either must fail.
            for index in [12, 20, 23, 24, 27] {
                let mut tampered = extended.to_vec();
                tampered[index] ^= 0x01;
                let rtp = parse_rtp_header(&tampered).unwrap();
                assert!(
                    decrypt_transport_payload(&tampered, &rtp, &mode, &KAT_KEY).is_err(),
                    "{mode:?}: byte {index} is not authenticated"
                );
            }
        }
    }

    #[test]
    fn voice_receive_strips_padding_after_decrypting_known_answer_vectors() {
        for (mode, padded) in kat_padded() {
            let rtp = parse_rtp_header(padded).unwrap();
            assert!(rtp.padding && rtp.extension);
            assert_eq!(
                (rtp.sequence, rtp.timestamp, rtp.ssrc),
                (0x0304, 0x0e0f_1011, 0x5566_7788)
            );
            assert_eq!(rtp.header_len, 12 + 4 + 4 + 4);

            // Neither the extension body nor the four padding octets
            // (including the count) reach the Opus decoder.
            let decrypted = decrypt_transport_payload(padded, &rtp, &mode, &KAT_KEY).unwrap();
            assert_eq!(decrypted, KAT_OPUS, "{mode:?}");
        }
    }

    #[test]
    fn voice_receive_strips_padding_after_a_dave_frame() {
        for (mode, _) in kat_padded() {
            let packet = seal_padded(
                &mode,
                true,
                &[&[0x10, 0xab, 0, 0], &DAVE_FRAME[..], &[0, 0, 0, 4]].concat(),
            );
            let rtp = parse_rtp_header(&packet).unwrap();
            let frame = decrypt_transport_payload(&packet, &rtp, &mode, &KAT_KEY).unwrap();
            assert_eq!(frame, DAVE_FRAME);
            // The magic marker is the last two bytes again, so the frame
            // parses; with the padding left on it failed with "missing magic
            // marker".
            let parsed = parse_dave_frame(&frame).unwrap();
            assert_eq!(parsed.ciphertext, vec![1, 2, 3]);
            assert_eq!(parsed.nonce, 300);
        }
    }

    #[test]
    fn voice_receive_rejects_malformed_padding_and_accepts_padding_only_packets() {
        for (mode, _) in kat_padded() {
            let cases: [(bool, &[u8], &str); 4] = [
                // The count octet counts itself, so zero is impossible.
                (false, &[0xf8, 0xff, 0xfe, 0x00], "padding count 0"),
                // A count of 7 would reach into the 4-byte extension body.
                (true, &[0x10, 0xab, 0, 0, 0xf8, 0x07], "padding count 7"),
                // Five octets of padding in a 4-byte payload.
                (false, &[0xf8, 0x00, 0x00, 0x05], "padding count 5"),
                // P bit set but nothing encrypted at all.
                (false, &[], "no padding count"),
            ];
            for (extension, plaintext, expected) in cases {
                let packet = seal_padded(&mode, extension, plaintext);
                let rtp = parse_rtp_header(&packet).unwrap();
                let error = decrypt_transport_payload(&packet, &rtp, &mode, &KAT_KEY)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(expected), "{mode:?}: {error}");
            }

            // A count equal to the payload length is padding only (RFC 3550
            // allows it) and leaves no media.
            let packet = seal_padded(&mode, true, &[0x10, 0xab, 0, 0, 0, 0, 3]);
            let rtp = parse_rtp_header(&packet).unwrap();
            assert_eq!(
                decrypt_transport_payload(&packet, &rtp, &mode, &KAT_KEY).unwrap(),
                Vec::<u8>::new()
            );
        }
    }

    #[test]
    fn voice_receive_rejects_packets_too_short_for_tag_and_nonce() {
        for (mode, plain, extended) in kat_modes() {
            for packet in [plain, extended] {
                let rtp = parse_rtp_header(packet).unwrap();
                // Long enough for the parsed header, too short for the
                // 16-byte tag plus the 4-byte counter.
                let truncated = &packet[..rtp.header_len + 19];
                let rtp = parse_rtp_header(truncated).unwrap();
                assert!(decrypt_transport_payload(truncated, &rtp, &mode, &KAT_KEY)
                    .unwrap_err()
                    .to_string()
                    .contains("too short"));
            }
        }
    }

    #[test]
    fn voice_outbound_rtp_state_matches_known_answer_vectors_and_advances_counters() {
        for (mode, plain, _) in kat_modes() {
            let mut state =
                VoiceOutboundRtpState::with_counters(0xdead_beef, 0x1234, 0x5678_9abc, 0x0102_0304);

            let packet = state
                .build_packet(&KAT_OPUS, Duration::from_millis(20), &mode, &KAT_KEY)
                .unwrap();

            assert_eq!(packet.bytes, plain, "{mode:?}");
            assert_eq!(packet.rtp.version, 2);
            assert_eq!(packet.rtp.payload_type, 120);
            assert_eq!(packet.rtp.sequence, 0x1234);
            assert_eq!(packet.rtp.timestamp, 0x5678_9abc);
            assert_eq!(packet.rtp.ssrc, 0xdead_beef);
            assert_eq!(packet.nonce_suffix, [0x01, 0x02, 0x03, 0x04]);
            assert_eq!(state.sequence, 0x1235);
            assert_eq!(state.timestamp, 0x5678_9abc + 960);
            assert_eq!(state.nonce_suffix, 0x0102_0305);
        }
    }

    #[test]
    fn voice_outbound_rtp_state_commits_only_sent_packets_and_moves_across_gaps() {
        let secret_key = [7_u8; 32];
        let mode = VoiceEncryptionMode::aead_aes256_gcm_rtpsize();
        let frame = Duration::from_millis(20);
        let mut state = VoiceOutboundRtpState::with_counters(42, 10, 1_000, 5);

        // A sealed packet that is never sent uses up its nonce only.
        let unsent = state
            .seal_packet(&[1, 2, 3], Duration::ZERO, &mode, &secret_key)
            .unwrap();
        assert_eq!(
            (
                unsent.rtp.sequence,
                unsent.rtp.timestamp,
                unsent.nonce_suffix
            ),
            (10, 1_000, [0, 0, 0, 5])
        );
        assert_eq!(
            (state.sequence, state.timestamp, state.nonce_suffix),
            (10, 1_000, 6)
        );

        // The next packet follows 40 ms of untransmitted media: its timestamp
        // jumps over the gap while the sequence stays contiguous, and its
        // nonce is fresh.
        let after_gap = state
            .seal_packet(&[1, 2, 3], frame * 2, &mode, &secret_key)
            .unwrap();
        assert_eq!(
            (
                after_gap.rtp.sequence,
                after_gap.rtp.timestamp,
                after_gap.nonce_suffix
            ),
            (10, 1_000 + 1_920, [0, 0, 0, 6])
        );
        state.commit_packet(frame * 2, frame);
        assert_eq!(
            (state.sequence, state.timestamp, state.nonce_suffix),
            (11, 1_000 + 1_920 + 960, 7)
        );
        assert_eq!(gap_increment(48_000, Duration::ZERO), 0);
    }

    #[test]
    fn voice_outbound_rtp_state_round_trips_xchacha_and_rejects_empty_frames() {
        let secret_key = [9_u8; 32];
        let opus_frame = [0x11, 0x22, 0x33, 0x44];
        let mut state = VoiceOutboundRtpState::with_counters(99, 0x2233, 96_000, 10);

        let packet = state
            .build_packet(
                &opus_frame,
                Duration::from_millis(40),
                &VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
                &secret_key,
            )
            .unwrap();
        let decrypted = decrypt_transport_payload(
            &packet.bytes,
            &packet.rtp,
            &VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
            &secret_key,
        )
        .unwrap();
        assert_eq!(decrypted, opus_frame);
        assert_eq!(state.timestamp, 97_920);

        assert!(state
            .build_packet(
                &[],
                Duration::from_millis(20),
                &VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
                &secret_key,
            )
            .unwrap_err()
            .to_string()
            .contains("opus frame must not be empty"));
    }

    #[test]
    fn voice_opus_decoder_decodes_discord_silence_frame() {
        let mut decoder = VoiceOpusDecoder::discord_default().unwrap();
        let (samples_per_channel, pcm) = decoder
            .decode_opus_frame(&[0xf8, 0xff, 0xfe], false)
            .unwrap();

        assert_eq!(decoder.sample_rate(), 48_000);
        assert_eq!(decoder.channels(), 2);
        assert!(samples_per_channel > 0);
        assert_eq!(pcm.len(), samples_per_channel * 2);
    }

    #[cfg(feature = "voice-encode")]
    #[derive(Clone)]
    struct OneFrameSource {
        frame: Option<PcmFrame>,
    }

    #[cfg(feature = "voice-encode")]
    impl AudioSource for OneFrameSource {
        fn next_pcm_frame(&mut self) -> Result<Option<PcmFrame>, crate::error::DiscordError> {
            Ok(self.frame.take())
        }
    }

    #[cfg(feature = "voice-encode")]
    #[test]
    fn pcm_frame_accepts_only_discord_stereo_20ms_shape() {
        let frame = PcmFrame::discord_stereo_20ms(vec![0.0; 1_920]).unwrap();

        assert_eq!(frame.sample_rate(), 48_000);
        assert_eq!(frame.channels(), 2);
        assert_eq!(frame.samples_per_channel(), 960);
        assert_eq!(frame.duration(), Duration::from_millis(20));

        assert!(PcmFrame::discord_stereo_20ms(vec![0.0; 1_919])
            .unwrap_err()
            .to_string()
            .contains("1920"));
    }

    #[cfg(feature = "voice-encode")]
    #[test]
    fn audio_mixer_sums_sources_clamps_and_removes_finished_sources() {
        let mut mixer = AudioMixer::new();
        mixer.push_source(OneFrameSource {
            frame: Some(PcmFrame::discord_stereo_20ms(vec![0.75; 1_920]).unwrap()),
        });
        mixer.push_source(OneFrameSource {
            frame: Some(PcmFrame::discord_stereo_20ms(vec![0.5; 1_920]).unwrap()),
        });

        let mixed = mixer.next_pcm_frame().unwrap().unwrap();
        assert_eq!(mixed.samples()[0], 1.0);
        assert_eq!(mixed.samples()[1], 1.0);
        assert_eq!(mixer.active_sources(), 2);

        assert!(mixer.next_pcm_frame().unwrap().is_none());
        assert_eq!(mixer.active_sources(), 0);
    }

    #[cfg(feature = "voice-encode")]
    #[test]
    fn voice_opus_encoder_encodes_discord_pcm_with_audio_cbr_settings() {
        let mut encoder = VoiceOpusEncoder::discord_music()
            .unwrap()
            .with_bitrate_bps(96_000)
            .with_cbr(true);
        let frame = PcmFrame::discord_stereo_20ms(vec![0.0; 1_920]).unwrap();

        assert_eq!(encoder.sample_rate(), 48_000);
        assert_eq!(encoder.channels(), 2);
        assert_eq!(encoder.samples_per_channel(), 960);
        assert_eq!(encoder.bitrate_bps(), 96_000);
        assert!(encoder.use_cbr());

        let encoded = encoder.encode_pcm_frame(&frame).unwrap();
        assert!(!encoded.bytes.is_empty());
        assert_eq!(encoded.duration, Duration::from_millis(20));
    }

    #[test]
    fn voice_dave_frame_parser_reads_trailer_and_ranges() {
        assert_eq!(parse_uleb128(&[0xac, 0x02]).unwrap(), (300, 2));

        let mut frame = vec![1, 2, 3];
        frame.extend_from_slice(&[9_u8; 8]);
        frame.extend_from_slice(&[0xac, 0x02]);
        frame.extend_from_slice(&[5, 6]);
        frame.push(15);
        frame.extend_from_slice(&[0xfa, 0xfa]);

        let parsed = parse_dave_frame(&frame).unwrap();
        assert_eq!(parsed.ciphertext, vec![1, 2, 3]);
        assert_eq!(parsed.auth_tag, [9_u8; 8]);
        assert_eq!(parsed.nonce, 300);
        assert_eq!(parsed.supplemental_size, 15);
        assert_eq!(
            parsed.unencrypted_ranges,
            vec![super::VoiceDaveUnencryptedRange { offset: 5, len: 6 }]
        );
    }

    #[test]
    fn voice_davey_decryptor_wraps_session_lifecycle_entrypoints() {
        let mut decryptor = super::VoiceDaveyDecryptor::new(
            std::num::NonZeroU16::new(davey::DAVE_PROTOCOL_VERSION).unwrap(),
            42,
            100,
        )
        .unwrap();

        assert!(!decryptor.is_ready());
        assert_eq!(decryptor.session().user_id(), 42);
        assert_eq!(decryptor.session().channel_id(), 100);
        assert!(decryptor.encrypt_opus(&[0xf8, 0xff, 0xfe]).is_err());
        decryptor.set_passthrough_mode(true, Some(1));
    }

    #[test]
    fn voice_runtime_update_state_publishes_changes_and_reports_closed_receivers() {
        let initial_state = VoiceRuntimeState {
            config: VoiceRuntimeConfig::new(
                test_id(1),
                test_id(2),
                "session",
                "token",
                "voice.discord.media",
            ),
            heartbeat_interval_ms: 250,
            last_sequence: Some(3),
            ready: VoiceGatewayReady::new(42, "127.0.0.1", 5000),
            discovery: VoiceUdpDiscoveryPacket {
                ssrc: 42,
                address: "203.0.113.7".to_string(),
                port: 5000,
            },
            selected_mode: VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
            session_description: None,
            ssrc_users: HashMap::new(),
            speaking: HashMap::new(),
            dave: VoiceDaveState::default(),
            resumed: false,
        };
        let (state_tx, state_rx) = watch::channel(initial_state.clone());
        update_state(&state_tx, |state| {
            state.last_sequence = Some(9);
            state.resumed = true;
        })
        .unwrap();

        let updated = state_rx.borrow().clone();
        assert_eq!(updated.last_sequence, Some(9));
        assert!(updated.resumed);

        let (closed_tx, closed_rx) = watch::channel(initial_state);
        drop(closed_rx);
        assert!(update_state(&closed_tx, |state| state.resumed = true)
            .unwrap_err()
            .to_string()
            .contains("failed to publish voice runtime state"));
    }

    #[tokio::test]
    async fn voice_runtime_connects_and_completes_handshake() {
        let udp_listener = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let udp_port = udp_listener.local_addr().unwrap().port();
        let media_listener = Arc::clone(&udp_listener);

        let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = tcp_listener.local_addr().unwrap().port();
        let (speaking_tx, speaking_rx) = oneshot::channel();

        let server = tokio::spawn(async move {
            let mut speaking_tx = Some(speaking_tx);
            let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
            let mut ws = accept_async(tcp_stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 8,
                    "d": { "heartbeat_interval": 20 }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let identify = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let identify_payload: Value = serde_json::from_str(&identify).unwrap();
            assert_eq!(identify_payload["op"], serde_json::json!(0));

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 2,
                    "d": {
                        "ssrc": 42,
                        "ip": "127.0.0.1",
                        "port": udp_port,
                        "modes": ["aead_aes256_gcm_rtpsize"]
                    },
                    "seq": 7
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let mut discovery_buffer = [0_u8; VoiceUdpDiscoveryPacket::LEN];
            let (received, remote_addr) =
                udp_listener.recv_from(&mut discovery_buffer).await.unwrap();
            assert_eq!(received, VoiceUdpDiscoveryPacket::LEN);

            let mut response = VoiceUdpDiscoveryPacket::request(42);
            response[..2].copy_from_slice(&VoiceUdpDiscoveryPacket::RESPONSE_TYPE.to_be_bytes());
            response[2..4].copy_from_slice(&VoiceUdpDiscoveryPacket::BODY_LEN.to_be_bytes());
            let address = b"203.0.113.7";
            response[8..8 + address.len()].copy_from_slice(address);
            response[72..74].copy_from_slice(&remote_addr.port().to_be_bytes());
            udp_listener.send_to(&response, remote_addr).await.unwrap();

            let select_protocol = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let select_protocol_payload: Value = serde_json::from_str(&select_protocol).unwrap();
            assert_eq!(select_protocol_payload["op"], serde_json::json!(1));

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 4,
                    "d": {
                        "mode": "aead_aes256_gcm_rtpsize",
                        "secret_key": KAT_KEY.to_vec(),
                        "dave_protocol_version": 0
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            // A packet with CSRCs and a header extension, as real voice
            // servers send: the runtime must hand over only the Opus frame.
            let raw_udp_packet = KAT_AES_EXTENDED;
            udp_listener
                .send_to(&raw_udp_packet, remote_addr)
                .await
                .unwrap();
            udp_listener
                .send_to(&raw_udp_packet, remote_addr)
                .await
                .unwrap();
            udp_listener
                .send_to(&raw_udp_packet, remote_addr)
                .await
                .unwrap();

            loop {
                let message = match ws.next().await {
                    Some(Ok(message)) => message,
                    Some(Err(_)) | None => break,
                };
                match message {
                    WsMessage::Text(text) => {
                        let payload: Value = serde_json::from_str(&text).unwrap();
                        if payload["op"] == serde_json::json!(3) {
                            ws.send(WsMessage::Text(
                                serde_json::json!({
                                    "op": 6,
                                    "d": payload["d"].clone()
                                })
                                .to_string()
                                .into(),
                            ))
                            .await
                            .unwrap();
                        }
                        if payload["op"] == serde_json::json!(5) {
                            if let Some(speaking_tx) = speaking_tx.take() {
                                let _ = speaking_tx.send(());
                            }
                        }
                    }
                    WsMessage::Close(frame) => {
                        let _ = ws.send(WsMessage::Close(frame)).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        let handle = connect(VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            format!("ws://127.0.0.1:{ws_port}"),
        ))
        .await
        .unwrap();

        assert_eq!(handle.state().ready.ssrc, 42);
        assert_eq!(handle.state().discovery.address, "203.0.113.7");
        let raw_packet = handle.recv_raw_udp_packet(64).await.unwrap();
        assert_eq!(raw_packet.version, Some(2));
        assert_eq!(raw_packet.payload_type, Some(120));
        assert_eq!(raw_packet.sequence, Some(0x0102));
        assert_eq!(raw_packet.timestamp, Some(0x0a0b_0c0d));
        assert_eq!(raw_packet.ssrc, Some(0x1122_3344));
        assert_eq!(raw_packet.bytes, KAT_AES_EXTENDED);
        let received = handle.recv_voice_packet(64).await.unwrap();
        assert_eq!(received.rtp.ssrc, 0x1122_3344);
        assert_eq!(received.opus_frame, KAT_OPUS);
        let mut decoder = VoiceOpusDecoder::discord_default().unwrap();
        let decoded = handle
            .recv_decoded_voice_packet(&mut decoder, 64)
            .await
            .unwrap();
        assert_eq!(decoded.packet.rtp.ssrc, 0x1122_3344);
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.pcm.len(), decoded.samples_per_channel * 2);
        handle
            .set_speaking(VoiceSpeakingFlags::MICROPHONE, 0)
            .unwrap();
        speaking_rx.await.unwrap();

        // Sent frames advance the RTP timeline by their duration, and a
        // frame sent after a gap moves the timestamp across the gap while
        // the sequence stays contiguous.
        let frame = Duration::from_millis(20);
        let first = handle.send_opus_frame(&[1, 2, 3], frame).await.unwrap();
        let second = handle
            .send_opus_frame_after_gap(&[4, 5, 6], frame, frame * 3)
            .await
            .unwrap();
        assert_eq!(
            (second.rtp.sequence, second.rtp.timestamp),
            (
                first.rtp.sequence.wrapping_add(1),
                first.rtp.timestamp.wrapping_add(960 * 4)
            )
        );
        let mut datagram = [0_u8; 128];
        for expected in [&first, &second] {
            let (received, _) = media_listener.recv_from(&mut datagram).await.unwrap();
            assert_eq!(&datagram[..received], expected.bytes.as_slice());
        }

        handle.close().await.unwrap();
        server.await.unwrap();
    }

    /// Runs a minimal voice gateway (HELLO, READY, UDP discovery, then
    /// SESSION_DESCRIPTION with `dave_protocol_version`) that answers
    /// heartbeats until the client closes, and returns the connected handle
    /// with the UDP socket that receives its media.
    async fn connect_mock_voice_session(
        dave_protocol_version: u8,
    ) -> (
        super::VoiceRuntimeHandle,
        Arc<UdpSocket>,
        tokio::task::JoinHandle<()>,
    ) {
        let udp_listener = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let udp_port = udp_listener.local_addr().unwrap().port();
        let media = Arc::clone(&udp_listener);
        let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = tcp_listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
            let mut ws = accept_async(tcp_stream).await.unwrap();
            let send_json = |payload: Value| WsMessage::Text(payload.to_string().into());

            ws.send(send_json(serde_json::json!({
                "op": 8,
                "d": { "heartbeat_interval": 20 }
            })))
            .await
            .unwrap();
            let _identify = ws.next().await.unwrap().unwrap();
            ws.send(send_json(serde_json::json!({
                "op": 2,
                "d": {
                    "ssrc": 42,
                    "ip": "127.0.0.1",
                    "port": udp_port,
                    "modes": ["aead_aes256_gcm_rtpsize"]
                }
            })))
            .await
            .unwrap();

            let mut discovery = [0_u8; VoiceUdpDiscoveryPacket::LEN];
            let (_, remote_addr) = udp_listener.recv_from(&mut discovery).await.unwrap();
            let mut response = VoiceUdpDiscoveryPacket::request(42);
            response[..2].copy_from_slice(&VoiceUdpDiscoveryPacket::RESPONSE_TYPE.to_be_bytes());
            response[2..4].copy_from_slice(&VoiceUdpDiscoveryPacket::BODY_LEN.to_be_bytes());
            response[8..17].copy_from_slice(b"127.0.0.1");
            response[72..74].copy_from_slice(&remote_addr.port().to_be_bytes());
            udp_listener.send_to(&response, remote_addr).await.unwrap();

            let _select_protocol = ws.next().await.unwrap().unwrap();
            ws.send(send_json(serde_json::json!({
                "op": 4,
                "d": {
                    "mode": "aead_aes256_gcm_rtpsize",
                    "secret_key": vec![7_u8; 32],
                    "dave_protocol_version": dave_protocol_version
                }
            })))
            .await
            .unwrap();

            while let Some(Ok(message)) = ws.next().await {
                match message {
                    WsMessage::Text(text) => {
                        let payload: Value = serde_json::from_str(&text).unwrap();
                        if payload["op"] == serde_json::json!(3) {
                            let ack = serde_json::json!({ "op": 6, "d": payload["d"].clone() });
                            ws.send(send_json(ack)).await.unwrap();
                        }
                    }
                    WsMessage::Close(frame) => {
                        let _ = ws.send(WsMessage::Close(frame)).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        let handle = connect(VoiceRuntimeConfig::new(
            test_id(1),
            test_id(2),
            "session",
            "token",
            format!("ws://127.0.0.1:{ws_port}"),
        ))
        .await
        .unwrap();
        (handle, media, server)
    }

    #[tokio::test]
    async fn voice_runtime_receives_padded_dave_frames_over_udp() {
        /// Records the parsed frame and returns its ciphertext as the media.
        struct RecordingDecryptor(Option<super::VoiceDaveFrame>);

        impl super::VoiceDaveFrameDecryptor for RecordingDecryptor {
            fn decrypt_frame(
                &mut self,
                _rtp: &super::VoiceRtpHeader,
                _user_id: Option<crate::model::UserId>,
                frame: &super::VoiceDaveFrame,
            ) -> Result<Vec<u8>, crate::error::DiscordError> {
                self.0 = Some(frame.clone());
                Ok(frame.ciphertext.clone())
            }
        }

        let (handle, media, server) = connect_mock_voice_session(1).await;
        let mut client = handle.udp_socket().local_addr().unwrap();
        client.set_ip(std::net::Ipv4Addr::LOCALHOST.into());

        // P and X set: extension body, DAVE frame, and four padding octets
        // are all encrypted with the session key of the mock gateway.
        let cleartext = vec![
            0xb0, 0x78, 0x00, 0x07, 0x00, 0x00, 0x03, 0xc0, 0x00, 0x00, 0x00, 0x63, 0xbe, 0xde,
            0x00, 0x01,
        ];
        let plaintext = [&[0x10, 0xab, 0, 0], &DAVE_FRAME[..], &[0, 0, 0, 4]].concat();
        let packet = seal_rtpsize_packet(
            cleartext,
            &plaintext,
            [0, 0, 0, 1],
            &VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
            &[7_u8; 32],
        )
        .unwrap();
        media.send_to(&packet, client).await.unwrap();

        let mut decryptor = RecordingDecryptor(None);
        let received = handle
            .recv_voice_packet_with_dave(128, &mut decryptor)
            .await
            .unwrap();
        assert_eq!(received.rtp.ssrc, 0x63);
        assert_eq!(received.opus_frame, vec![1, 2, 3]);
        assert_eq!(decryptor.0.unwrap().bytes, DAVE_FRAME);

        handle.close().await.unwrap();
        server.await.unwrap();
    }

    /// Regression: the SFU's unencrypted silence packet for a muted user
    /// went through DAVE frame parsing, failed with "DAVE frame is too
    /// short", and ended `recv_decoded_voice_packet_with_dave(..).await?`
    /// loops. It must pass through, and only the exact 3-byte frame may.
    #[tokio::test]
    async fn voice_runtime_passes_dave_silence_packets_through() {
        /// Counts DAVE decrypt calls and returns the ciphertext.
        struct CountingDecryptor(usize);

        impl super::VoiceDaveFrameDecryptor for CountingDecryptor {
            fn decrypt_frame(
                &mut self,
                _rtp: &super::VoiceRtpHeader,
                _user_id: Option<crate::model::UserId>,
                frame: &super::VoiceDaveFrame,
            ) -> Result<Vec<u8>, crate::error::DiscordError> {
                self.0 += 1;
                Ok(frame.ciphertext.clone())
            }
        }

        let (handle, media, server) = connect_mock_voice_session(1).await;
        let mut client = handle.udp_socket().local_addr().unwrap();
        client.set_ip(std::net::Ipv4Addr::LOCALHOST.into());
        let mode = VoiceEncryptionMode::aead_aes256_gcm_rtpsize();
        let seal = |sequence: u8, media: &[u8]| {
            let cleartext = vec![
                0x80, 0x78, 0x00, sequence, 0x00, 0x00, 0x03, 0xc0, 0x00, 0x00, 0x00, 0x63,
            ];
            seal_rtpsize_packet(cleartext, media, [0, 0, 0, sequence], &mode, &[7_u8; 32]).unwrap()
        };

        let mut decoder = VoiceOpusDecoder::discord_default().unwrap();
        let mut decryptor = CountingDecryptor(0);
        media
            .send_to(&seal(1, &[0xf8, 0xff, 0xfe]), client)
            .await
            .unwrap();
        let silence = handle
            .recv_decoded_voice_packet_with_dave(&mut decoder, 128, &mut decryptor)
            .await
            .expect("a silence packet is not a DAVE error");
        assert!(!silence.pcm.is_empty());
        assert_eq!(decryptor.0, 0, "silence must bypass the DAVE decryptor");

        // The receive loop goes on: the next DAVE frame is decrypted.
        media.send_to(&seal(2, &DAVE_FRAME), client).await.unwrap();
        let received = handle
            .recv_voice_packet_with_dave(128, &mut decryptor)
            .await
            .unwrap();
        assert_eq!(received.opus_frame, vec![1, 2, 3]);
        assert_eq!(decryptor.0, 1);

        // Anything but the exact silence frame is still parsed as DAVE.
        for not_silence in [&[0xf8, 0xff][..], &[0xf8, 0xff, 0xfe, 0x00][..]] {
            media.send_to(&seal(3, not_silence), client).await.unwrap();
            let error = handle
                .recv_voice_packet_with_dave(128, &mut decryptor)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("DAVE frame"), "{error}");
        }
        assert_eq!(decryptor.0, 1);

        handle.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn voice_runtime_dave_sends_move_the_timestamp_across_gaps() {
        /// Stands in for DAVE by tagging the frame.
        struct TaggingEncryptor;

        impl VoiceDaveFrameEncryptor for TaggingEncryptor {
            fn encrypt_opus(&mut self, opus_frame: &[u8]) -> Result<Vec<u8>, DiscordError> {
                Ok([&[0xDA], opus_frame].concat())
            }
        }

        let (handle, media, server) = connect_mock_voice_session(1).await;
        let frame = Duration::from_millis(20);
        let mut encryptor = TaggingEncryptor;

        // Plain sends are refused on a DAVE session and use up nothing.
        assert!(handle.send_opus_frame(&[1], frame).await.is_err());
        let first = handle
            .send_opus_frame_with_dave(&[1, 2, 3], frame, &mut encryptor)
            .await
            .unwrap();
        let second = handle
            .send_opus_frame_with_dave_after_gap(&[4, 5, 6], frame, frame * 2, &mut encryptor)
            .await
            .unwrap();
        assert_eq!((first.rtp.sequence, first.rtp.timestamp), (0, 0));
        assert_eq!((second.rtp.sequence, second.rtp.timestamp), (1, 960 * 3));
        assert_eq!(first.opus_frame, vec![0xDA, 1, 2, 3]);

        let mut datagram = [0_u8; 128];
        for expected in [&first, &second] {
            let (received, _) = media.recv_from(&mut datagram).await.unwrap();
            assert_eq!(&datagram[..received], expected.bytes.as_slice());
        }
        handle.close().await.unwrap();
        server.await.unwrap();
    }

    // Exercises the raw DAVE command helpers and the raw `VoiceDaveState`
    // mirror; the runtime-managed session sees no DAVE version in this
    // session description, so it adds no commands of its own.
    #[tokio::test]
    async fn voice_runtime_loop_updates_state_and_sends_custom_commands() {
        let udp_listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let udp_port = udp_listener.local_addr().unwrap().port();

        let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = tcp_listener.local_addr().unwrap().port();
        let (heartbeat_tx, heartbeat_rx) = oneshot::channel();
        let (dave_commands_tx, dave_commands_rx) = oneshot::channel();

        let server = tokio::spawn(async move {
            let mut heartbeat_tx = Some(heartbeat_tx);
            let mut dave_commands_tx = Some(dave_commands_tx);
            let mut saw_transition_ready = false;
            let mut saw_key_package = false;
            let mut saw_commit_welcome = false;
            let mut saw_invalid_commit_welcome = false;
            let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
            let mut ws = accept_async(tcp_stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 8,
                    "d": { "heartbeat_interval": 5_000 },
                    "seq": 3
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let identify = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let identify_payload: Value = serde_json::from_str(&identify).unwrap();
            assert_eq!(identify_payload["d"]["max_dave_protocol_version"], 1);

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 2,
                    "d": {
                        "ssrc": 42,
                        "ip": "127.0.0.1",
                        "port": udp_port,
                        "modes": [
                            "aead_aes256_gcm_rtpsize",
                            "aead_xchacha20_poly1305_rtpsize"
                        ]
                    },
                    "seq": 7
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let mut discovery_buffer = [0_u8; VoiceUdpDiscoveryPacket::LEN];
            let (received, remote_addr) =
                udp_listener.recv_from(&mut discovery_buffer).await.unwrap();
            assert_eq!(received, VoiceUdpDiscoveryPacket::LEN);

            let mut response = VoiceUdpDiscoveryPacket::request(42);
            response[..2].copy_from_slice(&VoiceUdpDiscoveryPacket::RESPONSE_TYPE.to_be_bytes());
            response[2..4].copy_from_slice(&VoiceUdpDiscoveryPacket::BODY_LEN.to_be_bytes());
            let address = b"203.0.113.7";
            response[8..8 + address.len()].copy_from_slice(address);
            response[72..74].copy_from_slice(&remote_addr.port().to_be_bytes());
            udp_listener.send_to(&response, remote_addr).await.unwrap();

            let select_protocol = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let select_protocol_payload: Value = serde_json::from_str(&select_protocol).unwrap();
            assert_eq!(
                select_protocol_payload["d"]["data"]["mode"],
                serde_json::json!("aead_xchacha20_poly1305_rtpsize")
            );

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 4,
                    "d": {
                        "mode": "aead_xchacha20_poly1305_rtpsize",
                        "secret_key": [1, 2, 3, 4]
                    },
                    "seq": 9
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 4,
                    "d": {
                        "mode": "aead_xchacha20_poly1305_rtpsize",
                        "secret_key": [9, 9],
                        "audio_codec": "opus"
                    },
                    "seq": 11
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 9,
                    "seq": 12
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 24,
                    "d": {
                        "transition_id": 99,
                        "epoch": 2,
                        "protocol_version": 1
                    },
                    "seq": 13
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
            ws.send(WsMessage::Binary(vec![0, 14, 25, 1, 2, 3].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 15, 27, 4, 5].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 16, 29, 1].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 17, 30].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 18, 29, 0, 99, 6].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 19, 30, 0, 99, 7].into()))
                .await
                .unwrap();
            ws.send(WsMessage::Binary(vec![0, 25].into()))
                .await
                .unwrap();

            loop {
                let message = match ws.next().await {
                    Some(Ok(message)) => message,
                    Some(Err(_)) | None => break,
                };
                match message {
                    WsMessage::Text(text) => {
                        let payload: Value = serde_json::from_str(&text).unwrap();
                        if payload["op"] == serde_json::json!(3) {
                            ws.send(WsMessage::Text(
                                serde_json::json!({
                                    "op": 6,
                                    "d": payload["d"].clone()
                                })
                                .to_string()
                                .into(),
                            ))
                            .await
                            .unwrap();
                        }
                        if payload["op"] == serde_json::json!(3)
                            && payload["d"] == serde_json::json!(55)
                        {
                            if let Some(heartbeat_tx) = heartbeat_tx.take() {
                                let _ = heartbeat_tx.send(());
                            }
                        }
                        if payload["op"] == serde_json::json!(23)
                            && payload["d"]["transition_id"] == serde_json::json!(99)
                        {
                            saw_transition_ready = true;
                        }
                        if payload["op"] == serde_json::json!(31)
                            && payload["d"]["transition_id"] == serde_json::json!(99)
                        {
                            saw_invalid_commit_welcome = true;
                        }
                    }
                    WsMessage::Binary(bytes) => {
                        if bytes.as_ref() == [26, 1, 2, 3] {
                            saw_key_package = true;
                        }
                        if bytes.as_ref() == [28, 4, 5, 6, 7] {
                            saw_commit_welcome = true;
                        }
                    }
                    WsMessage::Close(frame) => {
                        let _ = ws.send(WsMessage::Close(frame)).await;
                        break;
                    }
                    _ => {}
                }
                if saw_transition_ready
                    && saw_key_package
                    && saw_commit_welcome
                    && saw_invalid_commit_welcome
                {
                    if let Some(dave_commands_tx) = dave_commands_tx.take() {
                        let _ = dave_commands_tx.send(());
                    }
                }
            }
        });

        let handle = connect(
            VoiceRuntimeConfig::new(
                test_id(1),
                test_id(2),
                "session",
                "token",
                format!("ws://127.0.0.1:{ws_port}"),
            )
            .channel_id(test_id(3))
            .preferred_mode(VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize()),
        )
        .await
        .unwrap();

        let mut state_rx = handle.subscribe();
        handle
            .send(VoiceGatewayCommand::Heartbeat { nonce: 55 })
            .unwrap();
        heartbeat_rx.await.unwrap();
        handle.send_dave_transition_ready(99).unwrap();
        handle.send_dave_mls_key_package(vec![1, 2, 3]).unwrap();
        handle
            .send_dave_mls_commit_welcome(vec![4, 5], Some(vec![6, 7]))
            .unwrap();
        handle.send_dave_mls_invalid_commit_welcome(99).unwrap();
        dave_commands_rx.await.unwrap();

        timeout(Duration::from_secs(1), async {
            loop {
                let state = state_rx.borrow().clone();
                if state.resumed
                    && state.last_sequence == Some(25)
                    && state
                        .session_description
                        .as_ref()
                        .map(|description| description.secret_key.as_slice() == [9, 9])
                        == Some(true)
                    && state.dave.epoch == Some(2)
                    && state.dave.external_sender.as_deref() == Some([1, 2, 3].as_slice())
                {
                    break;
                }

                state_rx.changed().await.unwrap();
            }
        })
        .await
        .unwrap();

        let state = handle.state();
        assert_eq!(
            state.selected_mode,
            VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize()
        );
        assert_eq!(state.last_sequence, Some(25));
        assert!(state.resumed);
        assert_eq!(state.dave.protocol_version, Some(1));
        assert_eq!(state.dave.transition_id, Some(99));
        assert_eq!(state.dave.proposals, vec![vec![4, 5]]);
        assert_eq!(state.dave.pending_commit, Some(vec![6]));
        assert_eq!(state.dave.pending_welcome, Some(vec![7]));
        assert_eq!(
            state.session_description,
            Some(VoiceSessionDescription {
                mode: VoiceEncryptionMode::aead_xchacha20_poly1305_rtpsize(),
                secret_key: vec![9, 9],
                audio_codec: Some("opus".to_string()),
                dave_protocol_version: None,
            })
        );

        handle.close().await.unwrap();
        server.await.unwrap();
    }
}

#[cfg(test)]
pub(crate) mod test_call {
    //! A mock voice gateway and UDP media server hosting a DAVE call with
    //! one other member (a second, independent `davey` session), for
    //! end-to-end tests of the runtime's own DAVE handling.

    use std::net::SocketAddr;
    use std::sync::Arc;

    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tokio::net::{TcpListener, UdpSocket};
    use tokio::sync::{mpsc, oneshot};
    use tokio::task::JoinHandle;
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};

    use super::dave::test_gateway::{commit_welcome, transition, MockDaveGateway};
    use super::dave::DaveController;
    use super::{
        build_rtp_header, connect, decrypt_transport_payload, parse_rtp_header,
        seal_rtpsize_packet, VoiceRuntimeConfig, VoiceRuntimeHandle,
    };
    use crate::model::test_id;
    use crate::voice::{VoiceEncryptionMode, VoiceUdpDiscoveryPacket};

    pub(crate) const BOT: u64 = 10;
    pub(crate) const PEER: u64 = 20;
    pub(crate) const CHANNEL: u64 = 500;
    pub(crate) const PEER_SSRC: u32 = 77;
    const KEY: [u8; 32] = [7; 32];

    /// A connected runtime in a DAVE call.
    pub(crate) struct DaveCall {
        pub(crate) handle: Arc<VoiceRuntimeHandle>,
        /// The other member of the call.
        pub(crate) peer: DaveController,
        pub(crate) gateway: MockDaveGateway,
        /// The IDENTIFY payload the runtime sent.
        pub(crate) identify: Value,
        media: Arc<UdpSocket>,
        client: SocketAddr,
        /// Messages the runtime sent after the handshake (heartbeats are
        /// answered by the server and not forwarded).
        from_client: mpsc::UnboundedReceiver<WsMessage>,
        pub(crate) to_client: mpsc::UnboundedSender<WsMessage>,
        server: JoinHandle<()>,
        peer_sequence: u16,
        server_sequence: u16,
    }

    impl DaveCall {
        /// Connects a runtime (`config` gets the mock endpoint) to a DAVE
        /// call: CLIENTS_CONNECT and the external sender arrive during the
        /// handshake, before SESSION_DESCRIPTION.
        pub(crate) async fn connect(
            configure: impl FnOnce(VoiceRuntimeConfig) -> VoiceRuntimeConfig,
        ) -> Self {
            let gateway = MockDaveGateway::new(CHANNEL);
            let external_sender = gateway.external_sender().to_vec();
            let media = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            let udp_port = media.local_addr().unwrap().port();
            let udp = Arc::clone(&media);
            let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let ws_port = tcp.local_addr().unwrap().port();
            let (identify_tx, identify_rx) = oneshot::channel();
            let (from_client_tx, from_client) = mpsc::unbounded_channel();
            let (to_client, mut to_client_rx) = mpsc::unbounded_channel::<WsMessage>();

            let server = tokio::spawn(async move {
                let (stream, _) = tcp.accept().await.unwrap();
                let mut ws = accept_async(stream).await.unwrap();
                let text = |payload: Value| WsMessage::Text(payload.to_string().into());
                ws.send(text(
                    json!({ "op": 8, "d": { "heartbeat_interval": 5_000 } }),
                ))
                .await
                .unwrap();
                let identify = ws.next().await.unwrap().unwrap().into_text().unwrap();
                identify_tx
                    .send(serde_json::from_str::<Value>(&identify).unwrap())
                    .unwrap();
                ws.send(text(json!({
                    "op": 2,
                    "d": { "ssrc": 42, "ip": "127.0.0.1", "port": udp_port, "modes": ["aead_aes256_gcm_rtpsize"] },
                    "seq": 1
                })))
                .await
                .unwrap();
                // Before SESSION_DESCRIPTION, as Songbird observed: the
                // runtime must keep these for its DAVE session.
                ws.send(text(
                    json!({ "op": 11, "d": { "user_ids": [PEER.to_string()] }, "seq": 2 }),
                ))
                .await
                .unwrap();
                ws.send(WsMessage::Binary(
                    [&[0, 3, 25][..], &external_sender].concat().into(),
                ))
                .await
                .unwrap();

                let mut discovery = [0_u8; VoiceUdpDiscoveryPacket::LEN];
                let (_, remote) = udp.recv_from(&mut discovery).await.unwrap();
                let mut response = VoiceUdpDiscoveryPacket::request(42);
                response[..2]
                    .copy_from_slice(&VoiceUdpDiscoveryPacket::RESPONSE_TYPE.to_be_bytes());
                response[2..4].copy_from_slice(&VoiceUdpDiscoveryPacket::BODY_LEN.to_be_bytes());
                response[8..17].copy_from_slice(b"127.0.0.1");
                response[72..74].copy_from_slice(&remote.port().to_be_bytes());
                udp.send_to(&response, remote).await.unwrap();

                let _select_protocol = ws.next().await.unwrap().unwrap();
                ws.send(text(json!({
                    "op": 4,
                    "d": { "mode": "aead_aes256_gcm_rtpsize", "secret_key": KEY.to_vec(), "dave_protocol_version": 1 },
                    "seq": 4
                })))
                .await
                .unwrap();

                loop {
                    tokio::select! {
                        message = ws.next() => match message {
                            Some(Ok(WsMessage::Text(text))) => {
                                let payload: Value = serde_json::from_str(&text).unwrap();
                                if payload["op"] == 3 {
                                    let ack = json!({ "op": 6, "d": payload["d"].clone() });
                                    ws.send(WsMessage::Text(ack.to_string().into())).await.unwrap();
                                } else {
                                    from_client_tx.send(WsMessage::Text(text)).ok();
                                }
                            }
                            Some(Ok(WsMessage::Close(frame))) => {
                                ws.send(WsMessage::Close(frame)).await.ok();
                                break;
                            }
                            Some(Ok(message)) => {
                                from_client_tx.send(message).ok();
                            }
                            Some(Err(_)) | None => break,
                        },
                        message = to_client_rx.recv() => match message {
                            Some(message) => ws.send(message).await.unwrap(),
                            None => break,
                        },
                    }
                }
            });

            let config = configure(
                VoiceRuntimeConfig::new(
                    test_id(1),
                    test_id(BOT),
                    "session",
                    "token",
                    format!("ws://127.0.0.1:{ws_port}"),
                )
                .channel_id(test_id(CHANNEL)),
            );
            let handle = connect(config).await.unwrap();
            let mut client = handle.udp_socket().local_addr().unwrap();
            client.set_ip(std::net::Ipv4Addr::LOCALHOST.into());

            let mut peer = DaveController::new(test_id(PEER), Some(test_id(CHANNEL)), 1);
            peer.on_session_description(1);
            peer.on_external_sender(gateway.external_sender());
            peer.on_clients_connect([test_id(BOT)]);

            Self {
                handle: Arc::new(handle),
                peer,
                gateway,
                identify: identify_rx.await.unwrap(),
                media,
                client,
                from_client,
                to_client,
                server,
                peer_sequence: 0,
                server_sequence: 10,
            }
        }

        /// The next message the runtime sent.
        pub(crate) async fn next_from_client(&mut self) -> WsMessage {
            timeout(Duration::from_secs(10), self.from_client.recv())
                .await
                .expect("the runtime sent nothing")
                .expect("the mock gateway stopped")
        }

        /// The next binary message, as `(opcode, payload)`.
        pub(crate) async fn next_binary(&mut self) -> (u8, Vec<u8>) {
            match self.next_from_client().await {
                WsMessage::Binary(bytes) => (bytes[0], bytes[1..].to_vec()),
                other => panic!("expected a binary message, got {other:?}"),
            }
        }

        /// The next text message as JSON.
        pub(crate) async fn next_json(&mut self) -> Value {
            match self.next_from_client().await {
                WsMessage::Text(text) => serde_json::from_str(&text).unwrap(),
                other => panic!("expected a text message, got {other:?}"),
            }
        }

        pub(crate) fn send_json(&self, payload: Value) {
            self.to_client
                .send(WsMessage::Text(payload.to_string().into()))
                .unwrap();
        }

        /// A server → client binary message.
        pub(crate) fn send_binary(&mut self, opcode: u8, payload: &[u8]) {
            self.server_sequence = self.server_sequence.wrapping_add(1);
            let message = [&self.server_sequence.to_be_bytes()[..], &[opcode], payload].concat();
            self.to_client
                .send(WsMessage::Binary(message.into()))
                .unwrap();
        }

        /// Adds the runtime's key package to the peer's group and welcomes
        /// the runtime in `transition_id`, as the voice gateway does when a
        /// client joins a call, up to the executed transition.
        pub(crate) async fn welcome_runtime(&mut self, key_package: &[u8], transition_id: u16) {
            let commands = self
                .peer
                .on_proposals(&self.gateway.add_proposal(key_package, self.peer_epoch()));
            let (commit, welcome) = commit_welcome(&commands);
            let ready = self
                .peer
                .on_announce_commit(&transition(transition_id, &commit));
            assert_eq!(ready.len(), 1, "{ready:?}");
            self.send_binary(30, &transition(transition_id, &welcome.unwrap()));
            let ready = self.next_json().await;
            assert_eq!(
                ready,
                json!({ "op": 23, "d": { "transition_id": transition_id } })
            );
            self.send_json(json!({ "op": 22, "d": { "transition_id": transition_id } }));
            self.peer.on_execute_transition(u64::from(transition_id));
            timeout(Duration::from_secs(10), self.handle.wait_until_playable())
                .await
                .expect("the runtime never became playable")
                .unwrap();
            // Playable as soon as the welcome gave it keys; wait for the
            // execute as well so callers see a settled status.
            let mut status = self.handle.subscribe_dave_status();
            timeout(
                Duration::from_secs(10),
                status
                    .wait_for(|status| status.last_transition_id == Some(u64::from(transition_id))),
            )
            .await
            .expect("the transition never executed")
            .unwrap();
        }

        fn peer_epoch(&self) -> u64 {
            self.peer.status().epoch.unwrap_or(0)
        }

        /// The next media frame the runtime sent, without transport
        /// encryption.
        pub(crate) async fn recv_media(&self) -> (u16, Vec<u8>) {
            let mut datagram = [0_u8; 2048];
            let (len, _) = timeout(Duration::from_secs(10), self.media.recv_from(&mut datagram))
                .await
                .expect("no media from the runtime")
                .unwrap();
            let packet = &datagram[..len];
            let rtp = parse_rtp_header(packet).unwrap();
            let frame = decrypt_transport_payload(
                packet,
                &rtp,
                &VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
                &KEY,
            )
            .unwrap();
            (rtp.sequence, frame)
        }

        /// Sends `frame` (already DAVE-encrypted or not) as the peer's media.
        pub(crate) async fn send_peer_frame(&mut self, frame: &[u8]) {
            self.peer_sequence = self.peer_sequence.wrapping_add(1);
            let header = build_rtp_header(
                self.peer_sequence,
                960 * u32::from(self.peer_sequence),
                PEER_SSRC,
                120,
            );
            let packet = seal_rtpsize_packet(
                header,
                frame,
                u32::from(self.peer_sequence).to_be_bytes(),
                &VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
                &KEY,
            )
            .unwrap();
            self.media.send_to(&packet, self.client).await.unwrap();
        }

        /// Maps the peer's SSRC to its user, as SPEAKING does.
        pub(crate) fn announce_peer_speaking(&self) {
            self.send_json(json!({
                "op": 5,
                "d": { "speaking": 1, "ssrc": PEER_SSRC, "user_id": PEER.to_string() }
            }));
        }

        pub(crate) async fn close(self) {
            let Ok(handle) = Arc::try_unwrap(self.handle) else {
                panic!("the runtime handle is still shared");
            };
            handle.close().await.unwrap();
            self.server.await.unwrap();
        }
    }

    /// The key package a binary message carries.
    pub(crate) fn expect_key_package((opcode, payload): (u8, Vec<u8>)) -> Vec<u8> {
        assert_eq!(opcode, 26, "expected a key package");
        assert!(!payload.is_empty());
        payload
    }
}

#[cfg(test)]
mod dave_call_tests {
    use serde_json::json;
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use super::dave::test_gateway::{key_package, split_commit_welcome, transition};
    use super::dave::DaveController;
    use super::test_call::{expect_key_package, DaveCall, BOT, CHANNEL, PEER, PEER_SSRC};
    use crate::model::test_id;

    const OPUS: [u8; 4] = [0x78, 9, 8, 7];
    const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];

    #[tokio::test]
    async fn voice_runtime_joins_a_dave_call_and_exchanges_audio_both_ways() {
        let mut call = DaveCall::connect(|config| config).await;
        assert_eq!(call.identify["d"]["max_dave_protocol_version"], 1);

        // SESSION_DESCRIPTION negotiated DAVE: the runtime sends its key
        // package and has no keys yet, so it refuses to send audio and uses
        // up nothing doing so.
        let key_package = expect_key_package(call.next_binary().await);
        assert!(!call.handle.is_playable());
        let error = call
            .handle
            .send_opus_frame(&OPUS, Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("end-to-end encrypted"),
            "{error}"
        );

        // CLIENTS_CONNECT arrived before SESSION_DESCRIPTION; had it been
        // dropped, the peer's own proposals would be refused later. The
        // welcome makes the runtime a member.
        call.welcome_runtime(&key_package, 5).await;
        let status = call.handle.dave_status();
        assert!(status.ready && status.can_send_audio(), "{status:?}");
        assert_eq!(status.epoch, Some(1));
        assert_eq!(status.last_transition_id, Some(5));
        assert_eq!(
            status.voice_privacy_code,
            call.peer.status().voice_privacy_code
        );
        assert_eq!(
            call.handle.dave_verification_code(test_id(PEER)).unwrap(),
            call.peer.verification_code(test_id(BOT)).unwrap()
        );

        // Plain send → DAVE → transport; the failed attempt above left no
        // hole in the sequence.
        call.handle
            .send_opus_frame(&OPUS, Duration::from_millis(20))
            .await
            .unwrap();
        let (sequence, frame) = call.recv_media().await;
        assert_eq!(sequence, 0);
        let inbound = call.peer.decrypt_inbound(Some(test_id(BOT)), frame);
        assert_eq!(inbound.media, Some(OPUS.to_vec()));

        // The peer's audio, attributed through SPEAKING, decrypts on receive.
        call.announce_peer_speaking();
        // SPEAKING and the datagram race; wait until the mapping is there.
        let mut state = call.handle.subscribe();
        timeout(
            Duration::from_secs(5),
            state.wait_for(|state| state.ssrc_users.contains_key(&PEER_SSRC)),
        )
        .await
        .unwrap()
        .unwrap();
        let frame = call.peer.encrypt_outbound(&OPUS).unwrap().into_owned();
        call.send_peer_frame(&frame).await;
        let received = call.handle.recv_voice_packet(2048).await.unwrap();
        assert_eq!(received.opus_frame, OPUS);
        assert_eq!(received.user_id, Some(test_id(PEER)));

        // A frame that does not decrypt is skipped, not returned as an
        // error; silence passes untouched.
        let mut tampered = call.peer.encrypt_outbound(&OPUS).unwrap().into_owned();
        tampered[0] ^= 0x01;
        call.send_peer_frame(&tampered).await;
        call.send_peer_frame(&SILENCE).await;
        let received = call.handle.recv_voice_packet(2048).await.unwrap();
        assert_eq!(received.opus_frame, SILENCE);
        assert_eq!(call.handle.dave_status().undecryptable_packets, 1);

        call.close().await;
    }

    #[tokio::test]
    async fn voice_runtime_follows_a_dave_downgrade_to_plaintext() {
        let mut call = DaveCall::connect(|config| config).await;
        let key_package = expect_key_package(call.next_binary().await);
        call.welcome_runtime(&key_package, 5).await;

        call.send_json(json!({ "op": 21, "d": { "transition_id": 8, "protocol_version": 0 } }));
        assert_eq!(
            call.next_json().await,
            json!({ "op": 23, "d": { "transition_id": 8 } })
        );
        call.send_json(json!({ "op": 22, "d": { "transition_id": 8 } }));
        let mut status = call.handle.subscribe_dave_status();
        timeout(
            Duration::from_secs(5),
            status.wait_for(|status| status.protocol_version == 0),
        )
        .await
        .unwrap()
        .unwrap();

        // Transport encryption only from here: the Opus frame as it is.
        call.handle
            .send_opus_frame(&OPUS, Duration::from_millis(20))
            .await
            .unwrap();
        assert_eq!(call.recv_media().await.1, OPUS);
        assert!(call.handle.state().dave.passthrough);
        call.close().await;
    }

    #[tokio::test]
    async fn voice_runtime_reports_an_invalid_welcome_and_rejoins() {
        let mut call = DaveCall::connect(|config| config).await;
        expect_key_package(call.next_binary().await);

        call.send_binary(30, &[0, 6, 1, 2, 3]);
        assert_eq!(
            call.next_json().await,
            json!({ "op": 31, "d": { "transition_id": 6 } })
        );
        // A fresh key package follows; joining with it works.
        let key_package = expect_key_package(call.next_binary().await);
        call.welcome_runtime(&key_package, 7).await;
        assert_eq!(call.handle.dave_status().recoveries, 1);
        call.close().await;
    }

    #[tokio::test]
    async fn voice_runtime_commits_proposals_when_it_is_the_groups_only_member() {
        let mut call = DaveCall::connect(|config| config).await;
        expect_key_package(call.next_binary().await);
        // Alone in the call: a pending group of one, not playable.
        assert!(!call.handle.is_playable());

        // Someone joins: the gateway asks the runtime to add their key
        // package, and the runtime answers with a commit and a welcome.
        let mut joiner = DaveController::new(test_id(PEER), Some(test_id(CHANNEL)), 1);
        let joiner_key_package = key_package(&joiner.on_session_description(1));
        joiner.on_external_sender(call.gateway.external_sender());
        let proposals = call.gateway.add_proposal(&joiner_key_package, 0);
        call.send_binary(27, &proposals);
        let (opcode, payload) = call.next_binary().await;
        assert_eq!(opcode, 28);
        let (commit, welcome) = split_commit_welcome(&payload);

        // The gateway announces the commit to the runtime and welcomes the
        // joiner; both report ready and the transition executes.
        call.send_binary(29, &transition(9, &commit));
        assert_eq!(
            call.next_json().await,
            json!({ "op": 23, "d": { "transition_id": 9 } })
        );
        joiner.on_welcome(&transition(9, &welcome));
        call.send_json(json!({ "op": 22, "d": { "transition_id": 9 } }));
        joiner.on_execute_transition(9);
        timeout(Duration::from_secs(10), call.handle.wait_until_playable())
            .await
            .unwrap()
            .unwrap();

        call.handle
            .send_opus_frame(&OPUS, Duration::from_millis(20))
            .await
            .unwrap();
        let (_, frame) = call.recv_media().await;
        assert_eq!(
            joiner.decrypt_inbound(Some(test_id(BOT)), frame).media,
            Some(OPUS.to_vec())
        );
        call.close().await;
    }

    /// The default playback path in a DAVE call: `AudioPlayer` →
    /// `subscribe_runtime` → Opus → DAVE → transport encryption, waiting in
    /// `AutoPaused` until the runtime holds the group's keys.
    #[cfg(feature = "voice-encode")]
    #[tokio::test]
    async fn audio_player_plays_a_tone_into_a_dave_call() {
        use std::sync::Arc;

        use crate::voice::player::{
            AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioPlayerState,
            AudioResource, TrackEndReason, PLAYER_FRAME_SAMPLES,
        };
        use crate::voice_runtime::VoiceOpusDecoder;

        let mut call = DaveCall::connect(|config| config).await;
        let key_package = expect_key_package(call.next_binary().await);

        // 0.3 s of a 440 Hz tone, 48 kHz stereo s16le.
        let frames = 15;
        let pcm = (0..PLAYER_FRAME_SAMPLES / 2 * frames)
            .flat_map(|index| {
                let phase = f64::from(u32::try_from(index).unwrap()) * 440.0 / 48_000.0;
                let sample = (phase * std::f64::consts::TAU).sin() * 12_000.0;
                let sample = i16::try_from(sample.round() as i64).unwrap().to_le_bytes();
                [sample, sample].concat()
            })
            .collect::<Vec<u8>>();
        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let mut events = player.events();
        let mut state = player.state_watch();
        let subscription = player.subscribe_runtime(Arc::clone(&call.handle)).unwrap();
        player
            .play(AudioResource::new(AudioInput::raw_pcm(
                std::io::Cursor::new(pcm),
            )))
            .unwrap();

        // No keys yet: the track waits.
        timeout(
            Duration::from_secs(5),
            state.wait_for(|state| *state == AudioPlayerState::AutoPaused),
        )
        .await
        .unwrap()
        .unwrap();

        call.welcome_runtime(&key_package, 5).await;

        // Every frame the peer receives decrypts; the tone decodes to audio.
        let mut decoder = VoiceOpusDecoder::discord_default().unwrap();
        let mut tone_frames = 0;
        let mut loudest = 0_i32;
        let mut silence = 0;
        while silence < 5 {
            let (_, frame) = call.recv_media().await;
            if frame == SILENCE {
                silence += 1;
                continue;
            }
            let opus = call
                .peer
                .decrypt_inbound(Some(test_id(BOT)), frame)
                .media
                .expect("the peer decrypts every frame");
            let (_, pcm) = decoder.decode_opus_frame(&opus, false).unwrap();
            loudest = loudest.max(
                pcm.iter()
                    .map(|sample| i32::from(*sample).abs())
                    .max()
                    .unwrap_or(0),
            );
            tone_frames += 1;
        }
        assert_eq!(
            tone_frames, frames,
            "the whole track, nothing lost while waiting for keys"
        );
        assert!(loudest > 4_000, "the tone survived: peak {loudest}");

        loop {
            if let AudioPlayerEvent::TrackEnd { reason } =
                timeout(Duration::from_secs(5), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
            {
                assert_eq!(reason, TrackEndReason::Finished);
                break;
            }
        }
        player.shutdown().await;
        drop(subscription);
        call.close().await;
    }

    #[tokio::test]
    async fn voice_runtime_stops_being_playable_when_the_gateway_closes() {
        let mut call = DaveCall::connect(|config| config).await;
        let key_package = expect_key_package(call.next_binary().await);
        call.welcome_runtime(&key_package, 5).await;
        assert!(call.handle.is_playable());

        call.to_client
            .send(WsMessage::Close(Some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: 4014.into(),
                    reason: "".into(),
                },
            )))
            .unwrap();
        // The gateway task ends on the close frame.
        timeout(Duration::from_secs(5), async {
            while call.handle.is_playable() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let error = call.handle.wait_until_playable().await.unwrap_err();
        assert!(error.to_string().contains("closed"), "{error}");
        call.close().await;
    }

    #[test]
    fn voice_close_codes_are_described() {
        use tokio_tungstenite::tungstenite::protocol::CloseFrame;

        assert_eq!(super::describe_voice_close(None), "without a close code");
        let frame = CloseFrame {
            code: 4017.into(),
            reason: "E2EE/DAVE protocol required".into(),
        };
        assert_eq!(
            super::describe_voice_close(Some(&frame)),
            "with code 4017 (E2EE/DAVE protocol required): E2EE/DAVE protocol required"
        );
        let unknown = CloseFrame {
            code: 4999.into(),
            reason: "".into(),
        };
        assert_eq!(
            super::describe_voice_close(Some(&unknown)),
            "with code 4999"
        );
        for code in [
            4001, 4002, 4003, 4004, 4005, 4006, 4009, 4011, 4012, 4014, 4015, 4016, 4020, 4021,
            4022,
        ] {
            assert!(super::voice_close_description(code).is_some(), "{code}");
        }
        super::log_voice_close(Some(&unknown));
        super::log_voice_close(None);
    }

    #[tokio::test]
    async fn voice_runtime_explains_a_dave_required_close() {
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = tcp.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            use futures_util::{SinkExt, StreamExt};
            let (stream, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(WsMessage::Text(
                json!({ "op": 8, "d": { "heartbeat_interval": 5_000 } })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let identify = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let identify: serde_json::Value = serde_json::from_str(&identify).unwrap();
            ws.send(WsMessage::Close(Some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: 4017.into(),
                    reason: "E2EE/DAVE protocol required".into(),
                },
            )))
            .await
            .unwrap();
            identify
        });

        // No channel ID: nothing to build the MLS group from, so 0 is
        // advertised and Discord refuses the connection.
        let error = super::connect(super::VoiceRuntimeConfig::new(
            test_id(1),
            test_id(BOT),
            "session",
            "token",
            format!("ws://127.0.0.1:{port}"),
        ))
        .await
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains("4017"), "{error}");
        assert!(
            error.contains("advertised max_dave_protocol_version 0"),
            "{error}"
        );
        assert!(error.contains("VoiceRuntimeConfig::channel_id"), "{error}");
        let identify = server.await.unwrap();
        assert_eq!(identify["d"]["max_dave_protocol_version"], 0);
    }
}
