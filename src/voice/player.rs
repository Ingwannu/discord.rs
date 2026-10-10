//! discord.js-style audio playback pipeline for Discord voice connections.
//!
//! This module provides a `@discordjs/voice`-inspired playback stack:
//!
//! - [`AudioInput`] — a source of interleaved signed 16-bit little-endian PCM
//!   (48 kHz stereo), backed by FFmpeg, a raw PCM file, or any async reader.
//! - [`AudioResource`] — wraps an [`AudioInput`] with metadata and a live
//!   volume control, and slices it into Discord-sized 20 ms frames.
//! - [`AudioPlayer`] — a state machine (`Idle` / `Buffering` / `Playing` /
//!   `Paused` / `AutoPaused`) driven on a 20 ms tick, with a
//!   [`watch`](tokio::sync::watch) state channel and a
//!   [`broadcast`](tokio::sync::broadcast) event stream.
//! - [`PlayerSubscription`] — connects a player to a
//!   [`VoiceRuntimeHandle`](crate::voice_runtime::VoiceRuntimeHandle) so
//!   encoded Opus frames are transmitted; unsubscribes on drop.
//!
//! Commands such as stop, pause, and track replacement, and dropping the
//! player, take effect immediately even while an FFmpeg stream or a voice
//! connection is stalled: the player reads each resource and feeds each
//! subscribed connection from separate tasks, and its control loop never
//! waits on either. See [`AudioPlayer`] for the buffering and drop policies.
//!
//! Opus encoding requires the `voice-encode` feature. Without it the whole
//! API still compiles, but [`AudioPlayer::play`] returns a
//! [`DiscordError::Voice`] error instead of accepting a resource: a player
//! without an encoder could only advance through the track while
//! transmitting nothing.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration, MissedTickBehavior};

use crate::error::DiscordError;
use crate::voice::{AudioTrack, VoiceSpeakingFlags};
use crate::voice_runtime::VoiceRuntimeHandle;
#[cfg(feature = "voice-encode")]
use crate::voice_runtime::{PcmFrame, VoiceOpusEncoder};

/// Sample rate required by Discord voice (48 kHz).
pub const PLAYER_SAMPLE_RATE: u32 = 48_000;
/// Channel count required by Discord voice (stereo).
pub const PLAYER_CHANNELS: usize = 2;
/// Samples per channel in one 20 ms Discord audio frame.
pub const PLAYER_SAMPLES_PER_CHANNEL: usize = 960;
/// Interleaved samples in one 20 ms stereo frame.
pub const PLAYER_FRAME_SAMPLES: usize = PLAYER_SAMPLES_PER_CHANNEL * PLAYER_CHANNELS;
/// Bytes of s16le PCM in one 20 ms stereo frame.
pub const PLAYER_FRAME_BYTES: usize = PLAYER_FRAME_SAMPLES * 2;
/// Duration of one Discord audio frame.
pub const PLAYER_FRAME_DURATION: Duration = Duration::from_millis(20);
/// The raw Opus payload Discord recommends sending as silence.
pub const OPUS_SILENCE_FRAME: [u8; 3] = [0xF8, 0xFF, 0xFE];
/// Number of silence frames sent after playback pauses or ends.
const SILENCE_FRAME_COUNT: u8 = 5;
/// Capacity of the player event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 64;
/// Frames a track's reader task may decode ahead of playback (100 ms).
///
/// Enough to ride out FFmpeg and pipe scheduling jitter, small enough to keep
/// read-ahead memory bounded. When the buffer is full the reader waits, which
/// backpressures FFmpeg through its stdout pipe while the player is paused.
const SOURCE_BUFFER_FRAMES: usize = 5;
/// Frames queued for one subscribed sink before new frames for that sink
/// are dropped (100 ms).
///
/// Bounds the memory a stalled connection can pin and the extra latency a
/// briefly slow one can build up.
const SINK_QUEUE_FRAMES: usize = 5;

fn voice_error(message: impl Into<String>) -> DiscordError {
    DiscordError::voice(message)
}

/// Converts interleaved s16le PCM bytes to `f32` samples, applying `volume`.
///
/// Samples are scaled to `[-1.0, 1.0]` (dividing by 32768), multiplied by
/// `volume`, and clamped back into `[-1.0, 1.0]`. A trailing odd byte, if
/// any, is ignored.
pub fn pcm_s16le_to_f32(bytes: &[u8], volume: f32) -> Vec<f32> {
    // `as_chunks` yields `[u8; 2]` arrays, so the sample conversion needs no
    // bounds-checked indexing; the remainder (an odd trailing byte) is the
    // ignored `.1` half.
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let sample = i16::from_le_bytes(*pair);
            (f32::from(sample) / 32_768.0 * volume).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A source of interleaved s16le 48 kHz stereo PCM audio.
pub struct AudioInput {
    reader: Box<dyn AsyncRead + Send + Unpin>,
    /// Keeps a spawned FFmpeg process alive (and killed on drop) while the
    /// input is being read.
    _child: Option<tokio::process::Child>,
}

impl fmt::Debug for AudioInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioInput")
            .field("ffmpeg", &self._child.is_some())
            .finish()
    }
}

impl AudioInput {
    /// Creates an input from any async reader of raw interleaved s16le
    /// 48 kHz stereo PCM (a `Box<dyn AsyncRead + Send + Unpin>` works too).
    pub fn raw_pcm(reader: impl AsyncRead + Send + Unpin + 'static) -> Self {
        Self {
            reader: Box::new(reader),
            _child: None,
        }
    }

    /// Creates an input from a file containing raw interleaved s16le 48 kHz
    /// stereo PCM.
    ///
    /// The file is read fully into memory off the async runtime; for large or
    /// non-PCM media prefer [`AudioInput::ffmpeg`].
    pub async fn file(path: impl AsRef<Path>) -> Result<Self, DiscordError> {
        let path = path.as_ref().to_path_buf();
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
            .await
            .map_err(|error| voice_error(format!("failed to read PCM file: {error}")))?
            .map_err(|error| voice_error(format!("failed to read PCM file: {error}")))?;
        Ok(Self::raw_pcm(std::io::Cursor::new(bytes)))
    }

    /// Spawns `ffmpeg` to decode `input` (a URL or file path) into s16le
    /// 48 kHz stereo PCM read from its stdout.
    ///
    /// Equivalent to
    /// `ffmpeg -i <input> -f s16le -ar 48000 -ac 2 -loglevel error pipe:1`.
    pub fn ffmpeg(input: impl AsRef<str>) -> Result<Self, DiscordError> {
        Self::ffmpeg_with_binary("ffmpeg", input)
    }

    /// Like [`AudioInput::ffmpeg`], but with a custom FFmpeg binary name or
    /// path.
    pub fn ffmpeg_with_binary(
        binary: impl AsRef<OsStr>,
        input: impl AsRef<str>,
    ) -> Result<Self, DiscordError> {
        Self::ffmpeg_with_args(
            binary,
            [
                "-i",
                input.as_ref(),
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-loglevel",
                "error",
                "pipe:1",
            ],
        )
    }

    /// Spawns an FFmpeg binary with fully custom arguments.
    ///
    /// The command must write interleaved s16le 48 kHz stereo PCM to stdout
    /// (`-f s16le -ar 48000 -ac 2 ... pipe:1`). The process is killed when
    /// the input is dropped.
    pub fn ffmpeg_with_args<B, I, A>(binary: B, args: I) -> Result<Self, DiscordError>
    where
        B: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        let mut child = Command::new(binary.as_ref())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                voice_error(format!(
                    "failed to spawn {}: {error}",
                    binary.as_ref().to_string_lossy()
                ))
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| voice_error("ffmpeg process has no captured stdout"))?;
        tracing::debug!("spawned ffmpeg process for audio input");
        Ok(Self {
            reader: Box::new(stdout),
            _child: Some(child),
        })
    }
}

/// A live volume control shared with a playing [`AudioResource`].
///
/// The multiplier is applied to PCM samples before Opus encoding, so changes
/// take effect on the next 20 ms frame.
#[derive(Clone, Debug)]
pub struct VolumeControl {
    bits: Arc<AtomicU32>,
}

impl VolumeControl {
    fn new(volume: f32) -> Self {
        Self {
            bits: Arc::new(AtomicU32::new(volume.max(0.0).to_bits())),
        }
    }

    /// Returns the current volume multiplier.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.bits.load(Ordering::Relaxed))
    }

    /// Sets the volume multiplier (clamped to be non-negative).
    pub fn set(&self, volume: f32) {
        self.bits
            .store(volume.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

/// An audio track ready to be played by an [`AudioPlayer`].
///
/// Wraps an [`AudioInput`] together with optional metadata and a volume
/// control, and yields 20 ms `f32` PCM frames sized for Discord.
pub struct AudioResource {
    input: AudioInput,
    title: Option<String>,
    volume: VolumeControl,
    /// The frame being assembled, always `PLAYER_FRAME_BYTES` long.
    ///
    /// It lives on the resource instead of inside the read future so that a
    /// cancelled read keeps the bytes it already consumed from the input; the
    /// next read completes the same frame. A future-local buffer would lose
    /// them and shift every later sample by a partial frame.
    partial: Vec<u8>,
    /// How many bytes of `partial` hold PCM read from the input.
    filled: usize,
    ended: bool,
}

impl fmt::Debug for AudioResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioResource")
            .field("title", &self.title)
            .field("volume", &self.volume.get())
            .field("ended", &self.ended)
            .finish()
    }
}

impl AudioResource {
    /// Creates a resource from an input with volume `1.0` and no title.
    pub fn new(input: AudioInput) -> Self {
        Self {
            input,
            title: None,
            volume: VolumeControl::new(1.0),
            partial: vec![0_u8; PLAYER_FRAME_BYTES],
            filled: 0,
            ended: false,
        }
    }

    /// Sets a human-readable title for this resource.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the initial volume multiplier.
    pub fn with_volume(self, volume: f32) -> Self {
        self.volume.set(volume);
        self
    }

    /// Returns the resource title, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns a live volume control that stays valid while the resource
    /// plays inside an [`AudioPlayer`].
    pub fn volume(&self) -> VolumeControl {
        self.volume.clone()
    }

    /// Reads the next 20 ms frame as interleaved `f32` samples with the
    /// current volume applied.
    ///
    /// Returns `Ok(None)` once the input is exhausted. A trailing partial
    /// frame is padded with silence to a full 20 ms frame (rather than being
    /// dropped) so the very end of a track is not cut off.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe: if the future is dropped before it
    /// completes (for example as a losing `tokio::select!` branch), the PCM
    /// already read for the current frame stays buffered in the resource and
    /// the next call completes that frame. No samples are lost or shifted.
    pub async fn next_pcm_frame(&mut self) -> Result<Option<Vec<f32>>, DiscordError> {
        let frame = self.next_frame_bytes().await?;
        Ok(frame.map(|bytes| pcm_s16le_to_f32(&bytes, self.volume.get())))
    }

    /// Reads the next 20 ms frame as raw s16le bytes, padding a trailing
    /// partial frame with silence.
    ///
    /// The player decodes ahead with this and applies the volume only when it
    /// transmits a frame, so volume changes are not delayed by read-ahead.
    /// Cancel safe because the only await is `AsyncReadExt::read`, which is
    /// itself cancel safe, and progress is recorded in `self` right after it.
    async fn next_frame_bytes(&mut self) -> Result<Option<Vec<u8>>, DiscordError> {
        while !self.ended && self.filled < PLAYER_FRAME_BYTES {
            let read = self
                .input
                .reader
                .read(&mut self.partial[self.filled..])
                .await
                .map_err(|error| voice_error(format!("failed to read PCM audio input: {error}")))?;
            if read == 0 {
                self.ended = true;
            } else {
                self.filled += read;
            }
        }

        if self.filled == 0 {
            return Ok(None);
        }
        // A reader may scribble on the unfilled tail without reporting it, so
        // pad explicitly instead of trusting the zeroed allocation.
        self.partial[self.filled..].fill(0);
        self.filled = 0;
        Ok(Some(std::mem::replace(
            &mut self.partial,
            vec![0_u8; PLAYER_FRAME_BYTES],
        )))
    }
}

impl From<AudioInput> for AudioResource {
    fn from(input: AudioInput) -> Self {
        Self::new(input)
    }
}

impl AudioTrack {
    /// Bridges the legacy queue-based [`AudioTrack`] into the new playback
    /// pipeline by decoding its `source` (URL or path) through FFmpeg.
    pub fn to_ffmpeg_resource(&self) -> Result<AudioResource, DiscordError> {
        let mut resource = AudioResource::new(AudioInput::ffmpeg(&self.source)?);
        if let Some(title) = &self.title {
            resource = resource.with_title(title.clone());
        }
        Ok(resource)
    }
}

/// Boxed future returned by [`VoiceFrameSink`] methods.
pub type VoiceSinkFuture<'a> = Pin<Box<dyn Future<Output = Result<(), DiscordError>> + Send + 'a>>;

/// A destination for encoded Opus frames produced by an [`AudioPlayer`].
///
/// Implemented for [`VoiceRuntimeHandle`]; custom implementations are useful
/// for testing or fan-out.
///
/// The player drives every subscribed sink from its own task and calls both
/// methods there, in playback order, so a slow `send_opus_packet` only
/// delays that sink. A send that never completes is cancelled (its future is
/// dropped) when the sink is unsubscribed or the player shuts down.
pub trait VoiceFrameSink: Send + Sync {
    /// Sends one raw Opus payload of the given duration.
    fn send_opus_packet<'a>(
        &'a self,
        opus_frame: &'a [u8],
        duration: Duration,
    ) -> VoiceSinkFuture<'a>;

    /// Updates the speaking indicator for this connection.
    fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError>;
}

impl VoiceFrameSink for VoiceRuntimeHandle {
    fn send_opus_packet<'a>(
        &'a self,
        opus_frame: &'a [u8],
        duration: Duration,
    ) -> VoiceSinkFuture<'a> {
        Box::pin(async move {
            self.send_opus_frame(opus_frame, duration).await?;
            Ok(())
        })
    }

    fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
        let flags = if speaking {
            VoiceSpeakingFlags::MICROPHONE
        } else {
            VoiceSpeakingFlags::default()
        };
        VoiceRuntimeHandle::set_speaking(self, flags, 0)
    }
}

/// The state of an [`AudioPlayer`], mirroring `@discordjs/voice`.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioPlayerState {
    /// No resource is loaded.
    Idle,
    /// A resource is loaded but its first frame has not been read yet.
    Buffering,
    /// A resource is actively playing.
    Playing {
        /// When playback (or the latest resume) started.
        since: Instant,
    },
    /// Playback was paused by [`AudioPlayer::pause`].
    Paused,
    /// Playback paused automatically because no connection is subscribed.
    AutoPaused,
}

/// Why a track stopped playing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackEndReason {
    /// The input was fully consumed.
    Finished,
    /// Playback was stopped (or the resource was replaced).
    Stopped,
    /// Reading or encoding the resource failed.
    Error(String),
}

/// Events emitted by an [`AudioPlayer`] on its broadcast stream.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioPlayerEvent {
    /// The player transitioned between states.
    StateChange {
        /// State before the transition.
        old: AudioPlayerState,
        /// State after the transition.
        new: AudioPlayerState,
    },
    /// The current resource produced its first frame.
    TrackStart,
    /// The current resource stopped playing.
    TrackEnd {
        /// Why the track ended.
        reason: TrackEndReason,
    },
}

/// What the player does while it has no subscribed connections,
/// mirroring `@discordjs/voice`'s `NoSubscriberBehavior`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoSubscriberBehavior {
    /// Transition to [`AudioPlayerState::AutoPaused`] until a connection
    /// subscribes (the default).
    #[default]
    Pause,
    /// Keep consuming the resource in real time, discarding frames.
    Play,
    /// Stop the current resource with [`TrackEndReason::Stopped`].
    Stop,
}

/// Configuration for an [`AudioPlayer`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioPlayerOptions {
    /// Behavior while no connection is subscribed.
    pub no_subscriber_behavior: NoSubscriberBehavior,
}

/// Turns one 20 ms PCM frame into the payload handed to sinks.
///
/// A seam instead of calling [`VoiceOpusEncoder`] directly so the player's
/// scheduling, stall, and cancellation behaviour is tested in every feature
/// set, including builds without `voice-encode` that have no Opus encoder.
trait FrameEncoder: Send {
    fn encode(&mut self, pcm: Vec<f32>) -> Result<Vec<u8>, DiscordError>;
}

/// Creates the encoder for a track. Called on the track's first frame.
type EncoderFactory = fn() -> Result<Box<dyn FrameEncoder>, DiscordError>;

#[cfg(feature = "voice-encode")]
impl FrameEncoder for VoiceOpusEncoder {
    fn encode(&mut self, pcm: Vec<f32>) -> Result<Vec<u8>, DiscordError> {
        let frame = PcmFrame::discord_stereo_20ms(pcm)?;
        Ok(self.encode_pcm_frame(&frame)?.bytes)
    }
}

#[cfg(feature = "voice-encode")]
fn opus_encoder() -> Result<Box<dyn FrameEncoder>, DiscordError> {
    Ok(Box::new(VoiceOpusEncoder::discord_music()?))
}

/// The encoder [`AudioPlayer::new`] uses.
#[cfg(feature = "voice-encode")]
const DEFAULT_ENCODER: Option<EncoderFactory> = Some(opus_encoder);
/// No Opus encoder is compiled in, so [`AudioPlayer::play`] refuses resources.
#[cfg(not(feature = "voice-encode"))]
const DEFAULT_ENCODER: Option<EncoderFactory> = None;

fn encoding_unsupported() -> DiscordError {
    voice_error(
        "AudioPlayer::play requires the `voice-encode` feature: without an Opus encoder \
         the player cannot transmit PCM audio",
    )
}

enum PlayerCommand {
    Play {
        resource: Box<AudioResource>,
        encoder: EncoderFactory,
    },
    Pause,
    Unpause,
    Stop,
    Subscribe {
        id: u64,
        sink: Arc<dyn VoiceFrameSink>,
    },
    Unsubscribe {
        id: u64,
    },
}

/// A handle tying an [`AudioPlayer`] to one voice connection.
///
/// Dropping the subscription (or calling [`PlayerSubscription::unsubscribe`])
/// detaches the connection from the player.
pub struct PlayerSubscription {
    id: u64,
    command_tx: mpsc::UnboundedSender<PlayerCommand>,
}

impl fmt::Debug for PlayerSubscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlayerSubscription")
            .field("id", &self.id)
            .finish()
    }
}

impl PlayerSubscription {
    /// Detaches this connection from the player.
    ///
    /// Frames still queued for the connection are discarded and a send in
    /// progress is cancelled, so a stalled connection is released at once.
    pub fn unsubscribe(self) {}
}

impl Drop for PlayerSubscription {
    fn drop(&mut self) {
        let _ = self
            .command_tx
            .send(PlayerCommand::Unsubscribe { id: self.id });
    }
}

/// A discord.js-style audio player.
///
/// The player runs a control task ticking every 20 ms
/// ([`MissedTickBehavior::Delay`]): each tick takes one PCM frame of the
/// current [`AudioResource`], applies its volume, encodes it to Opus (with
/// the `voice-encode` feature), and hands it to every subscribed voice
/// connection. Five Opus silence frames are transmitted whenever playback
/// pauses or ends, per Discord's recommendation, and speaking flags are
/// toggled automatically.
///
/// # Stalls and backpressure
///
/// - **Commands are never blocked.** The control task handles commands as
///   soon as they arrive and never waits on audio input or on a connection,
///   so [`stop`](Self::stop), [`pause`](Self::pause), replacing the resource
///   with [`play`](Self::play), unsubscribing, and dropping the player take
///   effect immediately, even while FFmpeg or a connection is stalled.
/// - **Sources are read ahead by their own task**, at most 5 frames
///   (100 ms). When no frame is ready on a tick (a slow network stream,
///   FFmpeg still starting) the tick sends nothing and playback continues
///   once frames arrive: an underrun delays audio but never skips it, and the
///   player stays `Buffering` or `Playing`. Stopping or replacing a resource
///   cancels its reader and drops the resource, which kills an FFmpeg
///   process.
/// - **Each connection gets its own task and a 5-frame queue.** When a
///   connection falls further behind, the player drops the *newest* frames
///   for that connection only and logs a warning; the queued frames stay in
///   order and other connections are unaffected. Dropping the newest frame
///   keeps the hot path a lock-free `try_send`; it means a connection that
///   recovers first plays up to 100 ms of older audio. Speaking updates are
///   not dropped: the latest one waits until the queue has room, and frames
///   for that connection are dropped meanwhile so none overtakes it.
///
/// Handles are cheap to clone; all clones control the same player. Once
/// every handle and subscription is dropped, the control task exits and
/// cancels the reader and connection tasks, even if they are stalled.
#[derive(Clone)]
pub struct AudioPlayer {
    command_tx: mpsc::UnboundedSender<PlayerCommand>,
    state_rx: watch::Receiver<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    next_subscriber_id: Arc<AtomicU64>,
    encoder: Option<EncoderFactory>,
}

impl fmt::Debug for AudioPlayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioPlayer")
            .field("state", &*self.state_rx.borrow())
            .finish()
    }
}

impl AudioPlayer {
    /// Creates a player and spawns its background task.
    ///
    /// Must be called from within a Tokio runtime.
    pub fn new(options: AudioPlayerOptions) -> Self {
        Self::with_encoder(options, DEFAULT_ENCODER)
    }

    /// `encoder` is `None` when no Opus encoder is compiled in; tests pass a
    /// stand-in so the scheduling logic runs in every feature set.
    fn with_encoder(options: AudioPlayerOptions, encoder: Option<EncoderFactory>) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (state_tx, state_rx) = watch::channel(AudioPlayerState::Idle);
        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let task = PlayerTask {
            options,
            subscribers: HashMap::new(),
            state_tx,
            events_tx: events_tx.clone(),
            track: None,
            silence_remaining: 0,
            silence_frame: Arc::from(&OPUS_SILENCE_FRAME[..]),
        };
        tokio::spawn(task.run(command_rx));
        tracing::debug!("audio player task started");

        Self {
            command_tx,
            state_rx,
            events_tx,
            next_subscriber_id: Arc::new(AtomicU64::new(0)),
            encoder,
        }
    }

    fn send_command(&self, command: PlayerCommand) -> Result<(), DiscordError> {
        self.command_tx
            .send(command)
            .map_err(|_| voice_error("audio player task has stopped"))
    }

    /// Starts playing `resource`, replacing (and stopping) any current one.
    ///
    /// # Errors
    ///
    /// Returns [`DiscordError::Voice`] if the player task has stopped, or if
    /// the crate was built without the `voice-encode` feature. Without an
    /// Opus encoder the player would run through the track while transmitting
    /// nothing, so it refuses the resource instead and drops it, which kills
    /// an FFmpeg process.
    pub fn play(&self, resource: AudioResource) -> Result<(), DiscordError> {
        let Some(encoder) = self.encoder else {
            return Err(encoding_unsupported());
        };
        self.send_command(PlayerCommand::Play {
            resource: Box::new(resource),
            encoder,
        })
    }

    /// Pauses playback. No-op unless a resource is loaded.
    pub fn pause(&self) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Pause)
    }

    /// Resumes playback after [`AudioPlayer::pause`].
    pub fn unpause(&self) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Unpause)
    }

    /// Stops the current resource, emitting [`TrackEndReason::Stopped`].
    ///
    /// Takes effect even while the resource's input is stalled; the resource
    /// is dropped right away.
    pub fn stop(&self) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Stop)
    }

    /// Returns the current player state.
    pub fn state(&self) -> AudioPlayerState {
        self.state_rx.borrow().clone()
    }

    /// Returns a watch receiver that tracks every state transition.
    pub fn state_watch(&self) -> watch::Receiver<AudioPlayerState> {
        self.state_rx.clone()
    }

    /// Returns a new receiver on the player's event stream.
    pub fn events(&self) -> broadcast::Receiver<AudioPlayerEvent> {
        self.events_tx.subscribe()
    }

    /// Subscribes a frame sink (e.g. a voice connection) to this player.
    ///
    /// The player transmits encoded audio to every subscribed sink and
    /// auto-pauses/resumes according to
    /// [`AudioPlayerOptions::no_subscriber_behavior`].
    pub fn subscribe(
        &self,
        sink: Arc<dyn VoiceFrameSink>,
    ) -> Result<PlayerSubscription, DiscordError> {
        let id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        self.send_command(PlayerCommand::Subscribe { id, sink })?;
        Ok(PlayerSubscription {
            id,
            command_tx: self.command_tx.clone(),
        })
    }

    /// Subscribes a [`VoiceRuntimeHandle`] to this player.
    pub fn subscribe_runtime(
        &self,
        handle: Arc<VoiceRuntimeHandle>,
    ) -> Result<PlayerSubscription, DiscordError> {
        self.subscribe(handle)
    }
}

/// Aborts a task spawned by the player when dropped.
///
/// The control task owns every task it spawns (one reader per track, one
/// worker per subscribed sink) through this guard, so dropping a track
/// (stop, replacement, end), a subscriber (unsubscribe), or the whole
/// control task (every handle dropped) cancels the task even while it is
/// parked in a read or send that will never complete. A bare `JoinHandle`
/// would detach the task instead and leak it together with its FFmpeg
/// process or sink.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What a track's reader task reports to the control task.
enum SourceEvent {
    Frame(Vec<u8>),
    End,
    Failed(DiscordError),
}

/// Decodes `resource` ahead of playback into the bounded `frames` buffer.
///
/// Runs as its own task so a stalled input (FFmpeg waiting on the network,
/// a pipe nobody writes to) never blocks the control loop. It ends after
/// reporting the end of input or an error, or when the control task drops
/// the receiver; it is also aborted when the track goes away, which cancels
/// a pending read. The resource is discarded at that point, so the
/// interrupted frame does not matter.
async fn read_source(mut resource: AudioResource, frames: mpsc::Sender<SourceEvent>) {
    loop {
        let (event, last) = match resource.next_frame_bytes().await {
            Ok(Some(frame)) => (SourceEvent::Frame(frame), false),
            Ok(None) => (SourceEvent::End, true),
            Err(error) => (SourceEvent::Failed(error), true),
        };
        // Waiting for buffer space is the backpressure point: a paused player
        // stops taking frames, and FFmpeg then blocks on its full stdout pipe.
        if frames.send(event).await.is_err() || last {
            return;
        }
    }
}

/// The control task's state for the loaded resource.
struct ActiveTrack {
    frames: mpsc::Receiver<SourceEvent>,
    volume: VolumeControl,
    make_encoder: EncoderFactory,
    encoder: Option<Box<dyn FrameEncoder>>,
    started: bool,
    _reader: AbortOnDrop,
}

impl ActiveTrack {
    fn spawn(resource: AudioResource, make_encoder: EncoderFactory) -> Self {
        let (frames_tx, frames) = mpsc::channel(SOURCE_BUFFER_FRAMES);
        let volume = resource.volume();
        let reader = AbortOnDrop(tokio::spawn(read_source(resource, frames_tx)));
        Self {
            frames,
            volume,
            make_encoder,
            encoder: None,
            started: false,
            _reader: reader,
        }
    }

    fn encode(&mut self, pcm: Vec<f32>) -> Result<Vec<u8>, DiscordError> {
        let encoder = match self.encoder.as_mut() {
            Some(encoder) => encoder,
            None => self.encoder.insert((self.make_encoder)()?),
        };
        encoder.encode(pcm)
    }
}

/// One item on a subscribed sink's queue, delivered in order.
enum SinkItem {
    Frame(Arc<[u8]>),
    Speaking(bool),
}

/// Delivers queued frames and speaking updates to one sink, in order.
///
/// One task per sink keeps a slow or stalled connection from delaying the
/// control loop or the other subscribers. It ends when the control task
/// drops the queue and is aborted on unsubscribe or shutdown, which also
/// cancels a send that never completes.
async fn drive_sink(sink: Arc<dyn VoiceFrameSink>, mut items: mpsc::Receiver<SinkItem>) {
    while let Some(item) = items.recv().await {
        match item {
            SinkItem::Frame(frame) => {
                if let Err(error) = sink.send_opus_packet(&frame, PLAYER_FRAME_DURATION).await {
                    tracing::warn!(%error, "failed to send opus frame to subscribed connection");
                }
            }
            SinkItem::Speaking(speaking) => {
                if let Err(error) = sink.update_speaking(speaking) {
                    tracing::warn!(%error, "failed to update speaking state on subscribed connection");
                }
            }
        }
    }
}

/// The control task's side of one subscribed sink.
///
/// The `push_*` methods never wait. They return `false` once the sink's task
/// has died, which only happens if the sink panicked.
struct Subscriber {
    id: u64,
    queue: mpsc::Sender<SinkItem>,
    /// The latest speaking state that did not fit into the full queue yet.
    pending_speaking: Option<bool>,
    /// Frames dropped for this sink since it last accepted one.
    dropped_frames: u64,
    _worker: AbortOnDrop,
}

impl Subscriber {
    fn spawn(id: u64, sink: Arc<dyn VoiceFrameSink>) -> Self {
        let (queue, items) = mpsc::channel(SINK_QUEUE_FRAMES);
        Self {
            id,
            queue,
            pending_speaking: None,
            dropped_frames: 0,
            _worker: AbortOnDrop(tokio::spawn(drive_sink(sink, items))),
        }
    }

    fn push_speaking(&mut self, speaking: bool) -> bool {
        self.pending_speaking = Some(speaking);
        self.flush_speaking()
    }

    fn flush_speaking(&mut self) -> bool {
        let Some(speaking) = self.pending_speaking else {
            return true;
        };
        match self.queue.try_send(SinkItem::Speaking(speaking)) {
            Ok(()) => {
                self.pending_speaking = None;
                true
            }
            Err(TrySendError::Full(_)) => true,
            Err(TrySendError::Closed(_)) => false,
        }
    }

    fn push_frame(&mut self, frame: &Arc<[u8]>) -> bool {
        if self.pending_speaking.is_some() {
            // No frame may overtake a speaking update the sink has not seen.
            self.note_dropped_frame();
            return true;
        }
        match self.queue.try_send(SinkItem::Frame(Arc::clone(frame))) {
            Ok(()) => {
                if self.dropped_frames > 0 {
                    tracing::info!(
                        subscriber = self.id,
                        dropped = self.dropped_frames,
                        "subscribed connection caught up"
                    );
                    self.dropped_frames = 0;
                }
                true
            }
            Err(TrySendError::Full(_)) => {
                self.note_dropped_frame();
                true
            }
            Err(TrySendError::Closed(_)) => false,
        }
    }

    fn note_dropped_frame(&mut self) {
        if self.dropped_frames == 0 {
            tracing::warn!(
                subscriber = self.id,
                "subscribed connection is not keeping up; dropping audio frames for it"
            );
        }
        self.dropped_frames += 1;
    }
}

/// Logs and reports whether a subscriber's task is still alive.
fn subscriber_alive(id: u64, alive: bool) -> bool {
    if !alive {
        tracing::error!(
            subscriber = id,
            "subscribed sink task stopped (did the sink panic?); detaching it"
        );
    }
    alive
}

// [Decision Log] Cancellable source reads and isolated sinks (3.0 phase 6)
// - Purpose: stop, pause, replacement, unsubscribe, and dropping every
//   handle must take effect within about one frame while FFmpeg or a voice
//   connection is stalled, and stalled work must not outlive the player.
// - Previous implementation and constraints: one task awaited the PCM read
//   and then each sink's send, in turn, inside the tick branch of its
//   `select!`. A stalled read or send therefore blocked every command,
//   including the shutdown signal, and one slow sink delayed all others.
//   `VoiceFrameSink` is public, so its futures may never complete, and a
//   frame must stay intact if its read is cancelled.
// - Alternatives considered: (1) race the read and sends against the
//   command channel in another `select!`: cancelling the read future drops
//   its partial frame, and sinks are still served one after another;
//   (2) per-tick timeouts on reads and sends: latency is bounded only by the
//   timeout, partial frames are still lost, and one slow sink still uses up
//   the others' time; (3) separate tasks: a reader per track feeding a
//   bounded frame buffer, a worker per sink behind a bounded queue, and a
//   control loop that only uses `try_recv`/`try_send`.
// - Chosen: (3), dropping the newest frames for a sink whose queue is full.
// - Why: the control loop never awaits I/O, so command latency no longer
//   depends on any source or sink. All spawned tasks are owned through
//   `AbortOnDrop`, so every exit path cancels them. Frames are assembled on
//   the resource, so even a cancelled read keeps its bytes.
// - Trade-offs: three kinds of task instead of one; up to 100 ms of
//   read-ahead per track and up to 100 ms of queued audio per sink; a sink
//   that falls behind loses frames instead of delaying everyone; frames
//   still queued for a sink are discarded when it unsubscribes.
struct PlayerTask {
    options: AudioPlayerOptions,
    subscribers: HashMap<u64, Subscriber>,
    state_tx: watch::Sender<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    track: Option<ActiveTrack>,
    silence_remaining: u8,
    silence_frame: Arc<[u8]>,
}

impl PlayerTask {
    /// Neither branch awaits anything but the command channel and the ticker,
    /// so a command is handled as soon as it arrives. When the last handle is
    /// dropped the loop ends, and dropping `self` aborts every spawned task.
    async fn run(mut self, mut command_rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        let mut ticker = interval(PLAYER_FRAME_DURATION);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = command_rx.recv() => match command {
                    Some(command) => self.handle_command(command),
                    None => break,
                },
                _ = ticker.tick() => self.handle_tick(),
            }
        }
        tracing::debug!("audio player task stopped");
    }

    fn current_state(&self) -> AudioPlayerState {
        self.state_tx.borrow().clone()
    }

    fn set_state(&mut self, new: AudioPlayerState) {
        let old = self.current_state();
        if old == new {
            return;
        }
        tracing::debug!(?old, ?new, "audio player state change");
        self.state_tx.send_replace(new.clone());
        let _ = self
            .events_tx
            .send(AudioPlayerEvent::StateChange { old, new });
    }

    fn emit(&self, event: AudioPlayerEvent) {
        let _ = self.events_tx.send(event);
    }

    fn set_speaking_all(&mut self, speaking: bool) {
        self.subscribers
            .retain(|id, subscriber| subscriber_alive(*id, subscriber.push_speaking(speaking)));
    }

    fn flush_pending_speaking(&mut self) {
        self.subscribers
            .retain(|id, subscriber| subscriber_alive(*id, subscriber.flush_speaking()));
    }

    fn send_to_all(&mut self, frame: &Arc<[u8]>) {
        self.subscribers
            .retain(|id, subscriber| subscriber_alive(*id, subscriber.push_frame(frame)));
    }

    fn begin_silence(&mut self) {
        self.silence_remaining = SILENCE_FRAME_COUNT;
        self.set_speaking_all(false);
    }

    fn finish_playback(&mut self, reason: TrackEndReason) {
        tracing::info!(?reason, "audio player track ended");
        // Dropping the track aborts its reader, even mid-read.
        self.track = None;
        self.begin_silence();
        self.set_state(AudioPlayerState::Idle);
        self.emit(AudioPlayerEvent::TrackEnd { reason });
    }

    /// Transitions out of Paused/AutoPaused back into playback.
    fn resume_playback(&mut self) {
        if self.subscribers.is_empty()
            && self.options.no_subscriber_behavior == NoSubscriberBehavior::Pause
        {
            self.set_state(AudioPlayerState::AutoPaused);
            return;
        }
        if self.track.as_ref().is_some_and(|track| track.started) {
            self.set_state(AudioPlayerState::Playing {
                since: Instant::now(),
            });
            self.set_speaking_all(true);
        } else {
            self.set_state(AudioPlayerState::Buffering);
        }
    }

    fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Play { resource, encoder } => {
                if self.track.take().is_some() {
                    self.emit(AudioPlayerEvent::TrackEnd {
                        reason: TrackEndReason::Stopped,
                    });
                }
                tracing::info!(title = ?resource.title(), "audio player playing new resource");
                self.track = Some(ActiveTrack::spawn(*resource, encoder));
                self.silence_remaining = 0;
                self.set_state(AudioPlayerState::Buffering);
            }
            PlayerCommand::Pause => {
                if self.track.is_some()
                    && matches!(
                        self.current_state(),
                        AudioPlayerState::Playing { .. }
                            | AudioPlayerState::Buffering
                            | AudioPlayerState::AutoPaused
                    )
                {
                    self.begin_silence();
                    self.set_state(AudioPlayerState::Paused);
                }
            }
            PlayerCommand::Unpause => {
                if self.track.is_some() && self.current_state() == AudioPlayerState::Paused {
                    self.resume_playback();
                }
            }
            PlayerCommand::Stop => {
                if self.track.is_some() {
                    self.finish_playback(TrackEndReason::Stopped);
                }
            }
            PlayerCommand::Subscribe { id, sink } => {
                let mut subscriber = Subscriber::spawn(id, sink);
                if matches!(self.current_state(), AudioPlayerState::Playing { .. }) {
                    // A fresh queue always has room, so this cannot be dropped.
                    subscriber.push_speaking(true);
                }
                self.subscribers.insert(id, subscriber);
                tracing::debug!(
                    subscribers = self.subscribers.len(),
                    "connection subscribed"
                );
                if self.current_state() == AudioPlayerState::AutoPaused {
                    self.resume_playback();
                }
            }
            PlayerCommand::Unsubscribe { id } => {
                // Dropping the subscriber aborts its task and any stalled send.
                if self.subscribers.remove(&id).is_some() {
                    tracing::debug!(
                        subscribers = self.subscribers.len(),
                        "connection unsubscribed"
                    );
                }
            }
        }
    }

    fn handle_tick(&mut self) {
        self.flush_pending_speaking();
        match self.current_state() {
            AudioPlayerState::Buffering | AudioPlayerState::Playing { .. } => {
                if self.subscribers.is_empty() {
                    match self.options.no_subscriber_behavior {
                        NoSubscriberBehavior::Pause => {
                            self.begin_silence();
                            self.set_state(AudioPlayerState::AutoPaused);
                            return;
                        }
                        NoSubscriberBehavior::Stop => {
                            self.finish_playback(TrackEndReason::Stopped);
                            return;
                        }
                        NoSubscriberBehavior::Play => {}
                    }
                }
                self.play_next_frame();
            }
            AudioPlayerState::AutoPaused => {
                if self.subscribers.is_empty() {
                    self.send_silence_step();
                } else {
                    self.resume_playback();
                }
            }
            AudioPlayerState::Paused | AudioPlayerState::Idle => {
                self.send_silence_step();
            }
        }
    }

    fn send_silence_step(&mut self) {
        if self.silence_remaining == 0 {
            return;
        }
        self.silence_remaining -= 1;
        let frame = Arc::clone(&self.silence_frame);
        self.send_to_all(&frame);
    }

    fn play_next_frame(&mut self) {
        let Some(track) = self.track.as_mut() else {
            self.set_state(AudioPlayerState::Idle);
            return;
        };
        let next = match track.frames.try_recv() {
            Ok(SourceEvent::Frame(bytes)) => Ok(bytes),
            Ok(SourceEvent::End) => Err(TrackEndReason::Finished),
            Ok(SourceEvent::Failed(error)) => Err(read_failure(error)),
            // Underrun: the next frame is not decoded yet. Send nothing this
            // tick instead of waiting for it; the audio plays late, not never.
            Err(TryRecvError::Empty) => {
                tracing::trace!("audio source underrun; waiting for the next frame");
                return;
            }
            // The reader drops its sender without reporting End or Failed
            // only if the input panicked.
            Err(TryRecvError::Disconnected) => Err(read_failure(voice_error(
                "audio source reader stopped unexpectedly",
            ))),
        };
        let bytes = match next {
            Ok(bytes) => bytes,
            Err(reason) => {
                self.finish_playback(reason);
                return;
            }
        };

        let first_frame = !track.started;
        track.started = true;
        // With nobody subscribed (NoSubscriberBehavior::Play) the frame is
        // consumed in real time but not encoded.
        let encoded = (!self.subscribers.is_empty())
            .then(|| track.encode(pcm_s16le_to_f32(&bytes, track.volume.get())));

        if first_frame {
            self.set_state(AudioPlayerState::Playing {
                since: Instant::now(),
            });
            self.set_speaking_all(true);
            self.emit(AudioPlayerEvent::TrackStart);
        }
        match encoded {
            Some(Ok(opus)) => {
                let frame: Arc<[u8]> = opus.into();
                self.send_to_all(&frame);
            }
            Some(Err(error)) => {
                tracing::warn!(%error, "failed to encode PCM frame to Opus");
                self.finish_playback(TrackEndReason::Error(error.to_string()));
            }
            None => {}
        }
    }
}

fn read_failure(error: DiscordError) -> TrackEndReason {
    tracing::warn!(%error, "audio resource failed while reading");
    TrackEndReason::Error(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncWriteExt, ReadBuf};
    use tokio::sync::{oneshot, Semaphore};
    use tokio::time::{sleep, timeout};

    /// One frame period: the latency bound for commands and shutdown.
    const FRAME: Duration = PLAYER_FRAME_DURATION;

    /// Stand-in for Opus so the scheduling tests run in every feature set.
    /// The payload is the frame's first sample, which `numbered_pcm` sets to
    /// the frame index.
    struct FirstSampleEncoder;

    impl FrameEncoder for FirstSampleEncoder {
        fn encode(&mut self, pcm: Vec<f32>) -> Result<Vec<u8>, DiscordError> {
            let first = pcm.first().copied().unwrap_or_default();
            Ok(((first * 32_768.0) as i16).to_le_bytes().to_vec())
        }
    }

    fn first_sample_encoder() -> Result<Box<dyn FrameEncoder>, DiscordError> {
        Ok(Box::new(FirstSampleEncoder))
    }

    fn failing_encoder() -> Result<Box<dyn FrameEncoder>, DiscordError> {
        Err(voice_error("synthetic encoder failure"))
    }

    fn test_player(no_subscriber_behavior: NoSubscriberBehavior) -> AudioPlayer {
        AudioPlayer::with_encoder(
            AudioPlayerOptions {
                no_subscriber_behavior,
            },
            Some(first_sample_encoder),
        )
    }

    #[derive(Default)]
    struct RecordingSink {
        frames: Mutex<Vec<Vec<u8>>>,
        speaking: Mutex<Vec<bool>>,
    }

    impl RecordingSink {
        fn frames(&self) -> Vec<Vec<u8>> {
            self.frames.lock().expect("frames mutex poisoned").clone()
        }

        fn speaking(&self) -> Vec<bool> {
            self.speaking
                .lock()
                .expect("speaking mutex poisoned")
                .clone()
        }

        fn silence_frames(&self) -> usize {
            self.frames()
                .iter()
                .filter(|frame| frame.as_slice() == OPUS_SILENCE_FRAME)
                .count()
        }

        /// Frame indices of the audio (non-silence) payloads, in send order.
        fn audio_frames(&self) -> Vec<i16> {
            self.frames()
                .iter()
                .filter_map(|frame| <[u8; 2]>::try_from(frame.as_slice()).ok())
                .map(i16::from_le_bytes)
                .collect()
        }
    }

    impl VoiceFrameSink for RecordingSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            _duration: Duration,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                self.frames
                    .lock()
                    .expect("frames mutex poisoned")
                    .push(opus_frame.to_vec());
                Ok(())
            })
        }

        fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
            self.speaking
                .lock()
                .expect("speaking mutex poisoned")
                .push(speaking);
            Ok(())
        }
    }

    /// A connection whose sends wait for permits the test hands out; with no
    /// permits it is stalled forever.
    struct GatedSink {
        gate: Semaphore,
        inner: RecordingSink,
    }

    impl GatedSink {
        fn stalled() -> Self {
            Self {
                gate: Semaphore::new(0),
                inner: RecordingSink::default(),
            }
        }
    }

    impl VoiceFrameSink for GatedSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            duration: Duration,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                self.gate
                    .acquire()
                    .await
                    .map_err(|_| voice_error("gate closed"))?
                    .forget();
                self.inner.send_opus_packet(opus_frame, duration).await
            })
        }

        fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
            self.inner.update_speaking(speaking)
        }
    }

    struct PanickingSink;

    impl VoiceFrameSink for PanickingSink {
        fn send_opus_packet<'a>(
            &'a self,
            _opus_frame: &'a [u8],
            _duration: Duration,
        ) -> VoiceSinkFuture<'a> {
            panic!("synthetic sink panic")
        }

        fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    /// A connection whose every call fails, like a runtime that disconnected.
    struct ErroringSink;

    impl VoiceFrameSink for ErroringSink {
        fn send_opus_packet<'a>(
            &'a self,
            _opus_frame: &'a [u8],
            _duration: Duration,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async { Err(voice_error("synthetic send failure")) })
        }

        fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
            Err(voice_error("synthetic speaking failure"))
        }
    }

    struct FailingReader;

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Err(std::io::Error::other("synthetic read failure")))
        }
    }

    struct PanickingReader;

    impl AsyncRead for PanickingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            panic!("synthetic reader panic")
        }
    }

    /// Yields `data`, then never produces another byte, like an FFmpeg pipe
    /// whose upstream hung. Dropping it closes the paired receiver, which is
    /// how tests observe that the player released the input.
    struct StallingReader {
        data: std::io::Cursor<Vec<u8>>,
        _dropped: oneshot::Sender<()>,
    }

    impl AsyncRead for StallingReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let total = self.data.get_ref().len() as u64;
            if self.data.position() < total {
                return Pin::new(&mut self.data).poll_read(cx, buf);
            }
            Poll::Pending
        }
    }

    fn stalling_resource(data: Vec<u8>) -> (AudioResource, oneshot::Receiver<()>) {
        let (dropped_tx, dropped_rx) = oneshot::channel();
        let reader = StallingReader {
            data: std::io::Cursor::new(data),
            _dropped: dropped_tx,
        };
        (AudioResource::new(AudioInput::raw_pcm(reader)), dropped_rx)
    }

    /// PCM in which every sample of frame `i` equals `i`.
    fn numbered_pcm(frames: std::ops::Range<i16>) -> Vec<u8> {
        frames
            .flat_map(|index| std::iter::repeat_n(index.to_le_bytes(), PLAYER_FRAME_SAMPLES))
            .flatten()
            .collect()
    }

    fn pcm_resource(bytes: Vec<u8>) -> AudioResource {
        AudioResource::new(AudioInput::raw_pcm(std::io::Cursor::new(bytes)))
    }

    fn silent_pcm_resource(frames: usize) -> AudioResource {
        pcm_resource(vec![0_u8; PLAYER_FRAME_BYTES * frames])
    }

    async fn next_event(events: &mut broadcast::Receiver<AudioPlayerEvent>) -> AudioPlayerEvent {
        events.recv().await.expect("event stream closed")
    }

    async fn wait_for_track_end(
        events: &mut broadcast::Receiver<AudioPlayerEvent>,
    ) -> TrackEndReason {
        loop {
            if let AudioPlayerEvent::TrackEnd { reason } = next_event(events).await {
                return reason;
            }
        }
    }

    /// Fails the test, instead of hanging it, if `future` takes longer than
    /// `limit` of (paused) time.
    async fn within<F: Future>(limit: Duration, what: &str, future: F) -> F::Output {
        match timeout(limit, future).await {
            Ok(output) => output,
            Err(_) => panic!("{what} did not happen within {limit:?}"),
        }
    }

    async fn wait_until(limit: Duration, what: &str, condition: impl Fn() -> bool) {
        within(limit, what, async {
            while !condition() {
                sleep(Duration::from_millis(1)).await;
            }
        })
        .await;
    }

    async fn wait_for_state(
        state: &mut watch::Receiver<AudioPlayerState>,
        what: &str,
        predicate: impl FnMut(&AudioPlayerState) -> bool,
    ) {
        within(FRAME, what, state.wait_for(predicate))
            .await
            .expect("state watch closed");
    }

    #[test]
    fn pcm_s16le_to_f32_converts_and_applies_volume() {
        let bytes: Vec<u8> = [0_i16, 16_384, -16_384, 32_767, -32_768]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();

        let unity = pcm_s16le_to_f32(&bytes, 1.0);
        assert_eq!(unity[0], 0.0);
        assert_eq!(unity[1], 0.5);
        assert_eq!(unity[2], -0.5);
        assert!((unity[3] - 32_767.0 / 32_768.0).abs() < f32::EPSILON);
        assert_eq!(unity[4], -1.0);

        let half = pcm_s16le_to_f32(&bytes, 0.5);
        assert_eq!(half[1], 0.25);
        assert_eq!(half[4], -0.5);

        // Amplification clamps to [-1.0, 1.0].
        let doubled = pcm_s16le_to_f32(&bytes, 2.0);
        assert_eq!(doubled[1], 1.0);
        assert_eq!(doubled[4], -1.0);

        // A trailing odd byte is ignored.
        assert_eq!(pcm_s16le_to_f32(&[0x01], 1.0), Vec::<f32>::new());
    }

    #[tokio::test]
    async fn audio_resource_chunks_frames_and_pads_trailing_partial_frame() {
        // Two full frames of sample 0x0101 followed by half a frame.
        let mut bytes = vec![0x01_u8; PLAYER_FRAME_BYTES * 2];
        bytes.extend(vec![0x01_u8; PLAYER_FRAME_BYTES / 2]);
        let mut resource = pcm_resource(bytes);

        let expected_sample = 257.0 / 32_768.0;
        for _ in 0..2 {
            let frame = resource
                .next_pcm_frame()
                .await
                .expect("read should succeed")
                .expect("frame should exist");
            assert_eq!(frame.len(), PLAYER_FRAME_SAMPLES);
            assert!(frame
                .iter()
                .all(|sample| (sample - expected_sample).abs() < f32::EPSILON));
        }

        // Trailing partial frame is padded with silence to full length.
        let last = resource
            .next_pcm_frame()
            .await
            .expect("read should succeed")
            .expect("padded trailing frame should exist");
        assert_eq!(last.len(), PLAYER_FRAME_SAMPLES);
        assert!(last[..PLAYER_FRAME_SAMPLES / 2]
            .iter()
            .all(|sample| (sample - expected_sample).abs() < f32::EPSILON));
        assert!(last[PLAYER_FRAME_SAMPLES / 2..]
            .iter()
            .all(|sample| *sample == 0.0));

        assert_eq!(
            resource.next_pcm_frame().await.expect("read after end"),
            None
        );
    }

    #[tokio::test(start_paused = true)]
    async fn audio_resource_read_is_cancel_safe_and_keeps_partial_frame() {
        let half = PLAYER_FRAME_SAMPLES / 2;
        let (mut writer, reader) = tokio::io::duplex(PLAYER_FRAME_BYTES * 2);
        let mut resource = AudioResource::new(AudioInput::raw_pcm(reader));

        let first_half: Vec<u8> = std::iter::repeat_n(1_i16.to_le_bytes(), half)
            .flatten()
            .collect();
        writer
            .write_all(&first_half)
            .await
            .expect("write first half");
        // The frame cannot complete yet, so the read is cancelled half-way,
        // exactly as a losing `select!` branch would cancel it.
        assert!(timeout(FRAME, resource.next_pcm_frame()).await.is_err());

        let second_half: Vec<u8> = std::iter::repeat_n(2_i16.to_le_bytes(), half)
            .flatten()
            .collect();
        writer
            .write_all(&second_half)
            .await
            .expect("write second half");
        let frame = within(
            FRAME,
            "completing the interrupted frame",
            resource.next_pcm_frame(),
        )
        .await
        .expect("read should succeed")
        .expect("frame should exist");

        assert_eq!(frame.len(), PLAYER_FRAME_SAMPLES);
        assert!(frame[..half].iter().all(|sample| *sample == 1.0 / 32_768.0));
        assert!(frame[half..].iter().all(|sample| *sample == 2.0 / 32_768.0));

        drop(writer);
        assert_eq!(
            resource.next_pcm_frame().await.expect("read after end"),
            None
        );
    }

    #[tokio::test]
    async fn audio_resource_volume_control_applies_at_read_time() {
        let bytes: Vec<u8> =
            std::iter::repeat_n(16_384_i16.to_le_bytes(), PLAYER_FRAME_SAMPLES * 2)
                .flatten()
                .collect();
        let mut resource = pcm_resource(bytes).with_volume(0.5);
        let volume = resource.volume();

        let first = resource
            .next_pcm_frame()
            .await
            .expect("read should succeed")
            .expect("first frame");
        assert_eq!(first[0], 0.25);

        volume.set(1.0);
        let second = resource
            .next_pcm_frame()
            .await
            .expect("read should succeed")
            .expect("second frame");
        assert_eq!(second[0], 0.5);
    }

    #[tokio::test]
    async fn audio_input_file_reads_raw_pcm() {
        let path =
            std::env::temp_dir().join(format!("discordrs-player-test-{}.pcm", std::process::id()));
        std::fs::write(&path, vec![0_u8; PLAYER_FRAME_BYTES]).expect("write temp pcm file");

        let input = AudioInput::file(&path).await.expect("file input");
        let mut resource = AudioResource::new(input);
        assert!(resource
            .next_pcm_frame()
            .await
            .expect("read should succeed")
            .is_some());
        assert_eq!(
            resource.next_pcm_frame().await.expect("read after end"),
            None
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(start_paused = true)]
    async fn player_play_emits_track_start_then_finished_and_flushes_silence() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        player.play(silent_pcm_resource(3)).expect("play");

        // Idle -> Buffering
        assert_eq!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Idle,
                new: AudioPlayerState::Buffering,
            }
        );
        // Buffering -> Playing on first frame.
        assert!(matches!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Buffering,
                new: AudioPlayerState::Playing { .. },
            }
        ));
        assert_eq!(next_event(&mut events).await, AudioPlayerEvent::TrackStart);

        // Playing -> Idle once the input is exhausted.
        assert!(matches!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Playing { .. },
                new: AudioPlayerState::Idle,
            }
        ));
        assert_eq!(
            next_event(&mut events).await,
            AudioPlayerEvent::TrackEnd {
                reason: TrackEndReason::Finished,
            }
        );
        assert_eq!(player.state(), AudioPlayerState::Idle);

        // Let the post-track silence frames flush.
        sleep(Duration::from_millis(200)).await;
        assert_eq!(sink.silence_frames(), 5);
        assert_eq!(sink.audio_frames(), vec![0, 0, 0]);

        // Speaking toggled on at TrackStart and off at the end.
        assert_eq!(sink.speaking(), vec![true, false]);

        drop(subscription);
    }

    #[tokio::test(start_paused = true)]
    async fn player_pause_unpause_and_stop_transitions() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        player.play(silent_pcm_resource(500)).expect("play");
        loop {
            if next_event(&mut events).await == AudioPlayerEvent::TrackStart {
                break;
            }
        }

        player.pause().expect("pause");
        assert!(matches!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Playing { .. },
                new: AudioPlayerState::Paused,
            }
        ));

        // Silence frames are transmitted while paused.
        sleep(Duration::from_millis(200)).await;
        assert_eq!(sink.silence_frames(), 5);

        player.unpause().expect("unpause");
        assert!(matches!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Paused,
                new: AudioPlayerState::Playing { .. },
            }
        ));

        player.stop().expect("stop");
        assert_eq!(
            wait_for_track_end(&mut events).await,
            TrackEndReason::Stopped
        );
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn player_auto_pauses_without_subscribers_and_resumes_on_subscribe() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let mut events = player.events();
        let mut state = player.state_watch();

        player.play(silent_pcm_resource(3)).expect("play");
        while *state.borrow() != AudioPlayerState::AutoPaused {
            state.changed().await.expect("state watch closed");
        }

        // Subscribing resumes playback and the track then finishes.
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");

        let mut saw_track_start = false;
        loop {
            match next_event(&mut events).await {
                AudioPlayerEvent::TrackStart => saw_track_start = true,
                AudioPlayerEvent::TrackEnd { reason } => {
                    assert_eq!(reason, TrackEndReason::Finished);
                    break;
                }
                AudioPlayerEvent::StateChange { .. } => {}
            }
        }
        assert!(saw_track_start);
        assert_eq!(sink.audio_frames().len(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn player_no_subscriber_play_behavior_keeps_consuming() {
        let player = test_player(NoSubscriberBehavior::Play);
        let mut events = player.events();

        player.play(silent_pcm_resource(2)).expect("play");
        let mut saw_track_start = false;
        loop {
            match next_event(&mut events).await {
                AudioPlayerEvent::TrackStart => saw_track_start = true,
                AudioPlayerEvent::TrackEnd { reason } => {
                    assert_eq!(reason, TrackEndReason::Finished);
                    break;
                }
                AudioPlayerEvent::StateChange { .. } => {}
            }
        }
        assert!(saw_track_start);
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn player_no_subscriber_stop_behavior_ends_the_track() {
        let player = test_player(NoSubscriberBehavior::Stop);
        let mut events = player.events();

        player.play(silent_pcm_resource(10)).expect("play");
        assert_eq!(
            within(
                FRAME * 2,
                "the track ending",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Stopped
        );
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn player_reports_read_errors_as_track_end_error() {
        let player = test_player(NoSubscriberBehavior::Play);
        let mut events = player.events();

        player
            .play(AudioResource::new(AudioInput::raw_pcm(FailingReader)))
            .expect("play");
        match wait_for_track_end(&mut events).await {
            TrackEndReason::Error(message) => {
                assert!(message.contains("synthetic read failure"), "{message}");
            }
            other => panic!("expected TrackEnd error, got {other:?}"),
        }
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn player_reports_a_panicking_source_as_track_end_error() {
        let player = test_player(NoSubscriberBehavior::Play);
        let mut events = player.events();

        player
            .play(AudioResource::new(AudioInput::raw_pcm(PanickingReader)))
            .expect("play");
        match within(
            FRAME * 5,
            "the track ending",
            wait_for_track_end(&mut events),
        )
        .await
        {
            TrackEndReason::Error(message) => {
                assert!(message.contains("stopped unexpectedly"), "{message}");
            }
            other => panic!("expected TrackEnd error, got {other:?}"),
        }

        // The panic was confined to the reader; the player still works.
        player.play(silent_pcm_resource(1)).expect("play");
        assert_eq!(
            within(FRAME * 5, "the next track", wait_for_track_end(&mut events)).await,
            TrackEndReason::Finished
        );
    }

    #[tokio::test(start_paused = true)]
    async fn player_reports_encoder_failures_as_track_end_error() {
        let player =
            AudioPlayer::with_encoder(AudioPlayerOptions::default(), Some(failing_encoder));
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        player.play(silent_pcm_resource(2)).expect("play");
        match within(
            FRAME * 5,
            "the track ending",
            wait_for_track_end(&mut events),
        )
        .await
        {
            TrackEndReason::Error(message) => {
                assert!(message.contains("synthetic encoder failure"), "{message}");
            }
            other => panic!("expected TrackEnd error, got {other:?}"),
        }
        assert!(sink.audio_frames().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn player_without_an_encoder_rejects_play_instead_of_playing_silently() {
        let player = AudioPlayer::with_encoder(AudioPlayerOptions::default(), None);
        let (resource, mut dropped) = stalling_resource(Vec::new());

        let error = player
            .play(resource)
            .expect_err("play must fail without an encoder");
        assert!(
            matches!(&error, DiscordError::Voice { message } if message.contains("voice-encode")),
            "{error}"
        );
        // The rejected resource is dropped at once, which kills an FFmpeg process.
        assert_eq!(
            dropped.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        );

        sleep(FRAME * 3).await;
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[cfg(not(feature = "voice-encode"))]
    #[tokio::test]
    async fn player_new_rejects_play_without_voice_encode_feature() {
        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let error = player
            .play(silent_pcm_resource(1))
            .expect_err("play must fail without voice-encode");
        assert!(error.to_string().contains("voice-encode"), "{error}");
    }

    #[tokio::test(start_paused = true)]
    async fn player_pause_and_stop_take_effect_within_a_frame_while_source_is_stalled() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();
        let mut state = player.state_watch();

        // Two and a half frames, then the stream hangs in the middle of a frame.
        let mut data = numbered_pcm(0..2);
        data.extend(vec![0_u8; PLAYER_FRAME_BYTES / 2]);
        let (resource, mut dropped) = stalling_resource(data);
        player.play(resource).expect("play");
        wait_until(FRAME * 10, "two frames playing", || {
            sink.audio_frames().len() == 2
        })
        .await;
        sleep(FRAME * 5).await;
        assert!(matches!(player.state(), AudioPlayerState::Playing { .. }));
        assert_eq!(sink.audio_frames(), vec![0, 1]);

        player.pause().expect("pause");
        wait_for_state(&mut state, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        player.unpause().expect("unpause");
        wait_for_state(&mut state, "unpause", |state| {
            matches!(state, AudioPlayerState::Playing { .. })
        })
        .await;

        player.stop().expect("stop");
        assert_eq!(
            within(FRAME, "stop", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );
        assert_eq!(player.state(), AudioPlayerState::Idle);
        let _ = within(FRAME, "dropping the stalled input", &mut dropped).await;
    }

    #[tokio::test(start_paused = true)]
    async fn player_replacing_a_stalled_resource_releases_it_within_a_frame() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        let (stalled, mut dropped) = stalling_resource(Vec::new());
        player.play(stalled).expect("play stalled");
        sleep(FRAME * 5).await;
        assert_eq!(player.state(), AudioPlayerState::Buffering);

        player
            .play(pcm_resource(numbered_pcm(0..3)))
            .expect("play replacement");
        assert_eq!(
            within(FRAME, "replacement", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );
        let _ = within(FRAME, "dropping the replaced input", &mut dropped).await;

        assert_eq!(
            within(
                FRAME * 10,
                "the replacement finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        assert_eq!(sink.audio_frames(), vec![0, 1, 2]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_waits_out_a_slow_source_without_skipping_frames() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();
        let (mut writer, reader) = tokio::io::duplex(PLAYER_FRAME_BYTES * 4);

        player
            .play(AudioResource::new(AudioInput::raw_pcm(reader)))
            .expect("play");
        // Each frame arrives three frame periods late.
        for index in 0..4_i16 {
            sleep(FRAME * 3).await;
            writer
                .write_all(&numbered_pcm(index..index + 1))
                .await
                .expect("write frame");
        }
        drop(writer);

        let mut seen = Vec::new();
        within(FRAME * 5, "the track finishing", async {
            loop {
                let event = next_event(&mut events).await;
                let end = matches!(event, AudioPlayerEvent::TrackEnd { .. });
                seen.push(event);
                if end {
                    break;
                }
            }
        })
        .await;
        assert_eq!(sink.audio_frames(), vec![0, 1, 2, 3]);
        // Underruns neither left Buffering/Playing nor toggled speaking.
        assert!(
            matches!(
                seen.as_slice(),
                [
                    AudioPlayerEvent::StateChange {
                        new: AudioPlayerState::Buffering,
                        ..
                    },
                    AudioPlayerEvent::StateChange {
                        new: AudioPlayerState::Playing { .. },
                        ..
                    },
                    AudioPlayerEvent::TrackStart,
                    AudioPlayerEvent::StateChange {
                        new: AudioPlayerState::Idle,
                        ..
                    },
                    AudioPlayerEvent::TrackEnd {
                        reason: TrackEndReason::Finished
                    },
                ]
            ),
            "{seen:?}"
        );
        assert_eq!(sink.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_keeps_serving_other_sinks_and_commands_while_one_sink_is_stalled() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let stalled = Arc::new(GatedSink::stalled());
        let healthy = Arc::new(RecordingSink::default());
        let stalled_subscription = player
            .subscribe(stalled.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe stalled");
        let _healthy_subscription = player
            .subscribe(healthy.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe healthy");
        let mut events = player.events();
        let mut state = player.state_watch();

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        sleep(FRAME * 20).await;

        // One frame per tick still reaches the healthy sink, in order, no gaps.
        let delivered = healthy.audio_frames();
        assert!(delivered.len() >= 18, "{delivered:?}");
        let expected: Vec<i16> = (0..).take(delivered.len()).collect();
        assert_eq!(delivered, expected);

        player.pause().expect("pause");
        wait_for_state(&mut state, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        player.stop().expect("stop");
        assert_eq!(
            within(FRAME, "stop", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );

        sleep(FRAME * 10).await;
        // Pause and stop each start a five-frame silence run.
        assert!(healthy.silence_frames() >= 5);
        assert_eq!(healthy.speaking().last(), Some(&false));

        // Unsubscribing cancels the stalled send and releases the sink.
        drop(stalled_subscription);
        wait_until(FRAME, "releasing the stalled sink", || {
            Arc::strong_count(&stalled) == 1
        })
        .await;
        assert!(stalled.inner.frames().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn player_drops_newest_frames_for_a_lagging_sink_and_keeps_speaking_order() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let lagging = Arc::new(GatedSink::stalled());
        let _subscription = player
            .subscribe(lagging.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut state = player.state_watch();

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        sleep(FRAME * 20).await;
        // Pausing while the queue is full leaves speaking(false) pending.
        player.pause().expect("pause");
        wait_for_state(&mut state, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        sleep(FRAME * 10).await;

        lagging.gate.add_permits(1_000);
        sleep(FRAME * 3).await;
        // The in-flight frame and a full queue were kept in order; every
        // newer frame was dropped, and speaking(false) came after them.
        let kept: Vec<i16> = (0..=SINK_QUEUE_FRAMES as i16).collect();
        assert_eq!(lagging.inner.audio_frames(), kept);
        assert_eq!(lagging.inner.speaking(), vec![true, false]);

        player.unpause().expect("unpause");
        sleep(FRAME * 5).await;
        // Playback resumes from the player's position, not the sink's.
        let frames = lagging.inner.audio_frames();
        assert!(frames.len() > kept.len(), "{frames:?}");
        assert!(frames[kept.len()] > 15, "{frames:?}");
        assert_eq!(lagging.inner.speaking(), vec![true, false, true]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_detaches_a_panicking_sink_and_keeps_playing() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let panicking = Arc::new(PanickingSink);
        let healthy = Arc::new(RecordingSink::default());
        let _panicking_subscription = player
            .subscribe(panicking.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe panicking");
        let _healthy_subscription = player
            .subscribe(healthy.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe healthy");
        let mut events = player.events();

        player.play(pcm_resource(numbered_pcm(0..3))).expect("play");
        assert_eq!(
            within(
                FRAME * 10,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        assert_eq!(healthy.audio_frames(), vec![0, 1, 2]);
        assert_eq!(Arc::strong_count(&panicking), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn player_shutdown_cancels_stalled_source_and_sink_without_leaking_tasks() {
        let metrics = tokio::runtime::Handle::current().metrics();
        assert_eq!(metrics.num_alive_tasks(), 0);

        let player = test_player(NoSubscriberBehavior::Pause);
        let stalled_sink = Arc::new(GatedSink::stalled());
        let subscription = player
            .subscribe(stalled_sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut state = player.state_watch();

        // One frame, then the source hangs; the sink never finishes a send.
        let (resource, mut dropped) = stalling_resource(numbered_pcm(0..1));
        player.play(resource).expect("play");
        within(
            FRAME * 5,
            "playing",
            state.wait_for(|state| matches!(state, AudioPlayerState::Playing { .. })),
        )
        .await
        .expect("state watch closed");
        sleep(FRAME * 3).await;
        assert_eq!(
            metrics.num_alive_tasks(),
            3,
            "control task, source reader, and sink worker"
        );

        drop(player);
        drop(subscription);
        let _ = within(FRAME, "dropping the stalled input", &mut dropped).await;
        within(FRAME, "the control task exiting", async {
            while state.changed().await.is_ok() {}
        })
        .await;
        wait_until(FRAME, "every player task exiting", || {
            metrics.num_alive_tasks() == 0
        })
        .await;
        assert_eq!(Arc::strong_count(&stalled_sink), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn player_late_subscriber_gets_speaking_before_its_first_frame() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let early = Arc::new(RecordingSink::default());
        let late = Arc::new(RecordingSink::default());
        let _early_subscription = player
            .subscribe(early.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe early");
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..10)))
            .expect("play");
        wait_until(FRAME * 10, "three frames playing", || {
            early.audio_frames().len() >= 3
        })
        .await;
        let _late_subscription = player
            .subscribe(late.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe late");

        assert_eq!(
            within(
                FRAME * 20,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        assert_eq!(early.audio_frames(), (0..10).collect::<Vec<i16>>());
        let joined = late.audio_frames();
        assert!(!joined.is_empty() && joined[0] >= 3, "{joined:?}");
        assert_eq!(late.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_unpause_without_subscribers_auto_pauses() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let mut state = player.state_watch();
        // Pausing with nothing loaded is a no-op.
        player.pause().expect("pause");
        sleep(FRAME * 2).await;
        assert_eq!(player.state(), AudioPlayerState::Idle);

        let sink = Arc::new(RecordingSink::default());
        let subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let resource = silent_pcm_resource(500).with_title("long silence");
        assert!(format!("{resource:?}").contains("long silence"));
        player.play(resource).expect("play");
        within(
            FRAME * 5,
            "playing",
            state.wait_for(|state| matches!(state, AudioPlayerState::Playing { .. })),
        )
        .await
        .expect("state watch closed");

        player.pause().expect("pause");
        wait_for_state(&mut state, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        subscription.unsubscribe();
        player.unpause().expect("unpause");
        wait_for_state(&mut state, "auto-pause", |state| {
            *state == AudioPlayerState::AutoPaused
        })
        .await;
        assert!(format!("{player:?}").contains("AutoPaused"));
    }

    #[tokio::test(start_paused = true)]
    async fn player_keeps_sinks_that_return_errors_attached() {
        // With `Stop`, detaching the only sink would end the track as Stopped.
        let player = test_player(NoSubscriberBehavior::Stop);
        let _subscription = player
            .subscribe(Arc::new(ErroringSink) as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        player.play(silent_pcm_resource(3)).expect("play");
        assert_eq!(
            within(
                FRAME * 10,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
    }

    #[tokio::test]
    #[ignore = "requires an ffmpeg binary on PATH"]
    async fn ffmpeg_input_produces_pcm_frames() {
        let input = AudioInput::ffmpeg_with_args(
            "ffmpeg",
            [
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-loglevel",
                "error",
                "pipe:1",
            ],
        )
        .expect("spawn ffmpeg");
        let mut resource = AudioResource::new(input);

        let mut frames = 0_usize;
        while resource
            .next_pcm_frame()
            .await
            .expect("ffmpeg read should succeed")
            .is_some()
        {
            frames += 1;
        }
        // 0.2s of audio is ~10 twenty-millisecond frames.
        assert!(frames >= 8, "expected at least 8 frames, got {frames}");
    }

    #[cfg(feature = "voice-encode")]
    #[tokio::test(start_paused = true)]
    async fn player_encodes_non_silent_pcm_to_opus_frames() {
        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink.clone() as Arc<dyn VoiceFrameSink>)
            .expect("subscribe");
        let mut events = player.events();

        // Two frames of a square-ish wave so the encoder has real signal.
        let bytes: Vec<u8> = (0..PLAYER_FRAME_SAMPLES * 2)
            .flat_map(|index| {
                let sample: i16 = if index % 96 < 48 { 12_000 } else { -12_000 };
                sample.to_le_bytes()
            })
            .collect();

        player.play(pcm_resource(bytes)).expect("play");
        assert_eq!(
            wait_for_track_end(&mut events).await,
            TrackEndReason::Finished
        );

        let encoded: Vec<_> = sink
            .frames()
            .into_iter()
            .filter(|frame| frame.as_slice() != OPUS_SILENCE_FRAME)
            .collect();
        assert_eq!(encoded.len(), 2);
        assert!(encoded.iter().all(|frame| !frame.is_empty()));
    }

    #[tokio::test]
    async fn audio_track_bridges_into_ffmpeg_resource_metadata() {
        // Only exercises metadata mapping; ffmpeg spawn may fail without the
        // binary, which is fine to skip here.
        let track = AudioTrack::new("id", "https://example.invalid/audio.mp3").title("Title");
        match track.to_ffmpeg_resource() {
            Ok(resource) => assert_eq!(resource.title(), Some("Title")),
            Err(error) => assert!(error.to_string().contains("failed to spawn")),
        }
    }
}
