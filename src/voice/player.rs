//! discord.js-style audio playback pipeline for Discord voice connections.
//!
//! This module provides a `@discordjs/voice`-inspired playback stack:
//!
//! - [`AudioInput`] — a source of interleaved signed 16-bit little-endian PCM
//!   (48 kHz stereo), backed by FFmpeg, a raw PCM file, or any async reader.
//! - [`AudioResource`] — wraps an [`AudioInput`] with metadata and a live
//!   volume control, and slices it into Discord-sized 20 ms frames.
//! - [`AudioPlayer`] — a state machine (`Idle` / `Buffering` / `Playing` /
//!   `Paused` / `AutoPaused`) driven on a 20 ms tick, with a [`watch`] state
//!   channel and a [`broadcast`] event stream.
//! - [`PlayerSubscription`] — connects a player to a [`VoiceRuntimeHandle`]
//!   so encoded Opus frames are transmitted; unsubscribes on drop.
//!
//! The player reads each resource and feeds each subscribed connection from
//! separate tasks, and its control loop never waits on either, so commands
//! are handled promptly even while an FFmpeg stream or a voice connection is
//! stalled. [`AudioPlayer`] documents the buffering, real-time delivery, and
//! shutdown rules; [`VoiceFrameSink`] documents what a connection sees.
//!
//! Opus encoding requires the `voice-encode` feature. Without it the whole
//! API still compiles, but [`AudioPlayer::play`] returns a
//! [`DiscordError::Voice`] error instead of accepting a resource: a player
//! without an encoder could only advance through the track while
//! transmitting nothing.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Poll;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{broadcast, mpsc, watch, Notify};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

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
/// Silence frames a connection receives when its audio stops, before its
/// speaking flag is cleared, so the receiver does not interpolate artifacts.
const SILENCE_FRAME_COUNT: u8 = 5;
/// Capacity of the player event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 64;
/// Decoded frames waiting between a track's reader task and the control task
/// (100 ms).
///
/// When it is full the reader waits, which backpressures FFmpeg through its
/// stdout pipe while the player is paused.
const SOURCE_BUFFER_FRAMES: usize = 5;
/// Frames that must be buffered before a track starts, or resumes after an
/// underrun, unless the input has already ended (60 ms).
///
/// Resuming on the first frame that trickles in would toggle the connection
/// between audio and its silence tail on every hiccup of a slow stream.
const REBUFFER_FRAMES: usize = 3;
/// Consecutive ticks without a frame after which a playing track rebuffers
/// (60 ms). Shorter gaps are left to the receiver's loss concealment.
const UNDERRUN_GRACE_FRAMES: u8 = 3;
/// Frames held for one connection; when it is full the oldest is dropped.
const SINK_QUEUE_FRAMES: usize = 3;
/// How long after it was produced a queued frame may still start sending
/// (jitter tolerance). Later frames expire: a receiver would play them out
/// of time or discard them.
const SINK_FRAME_TTL: Duration = Duration::from_millis(60);
/// How long one send may take before the connection counts as stalled and
/// the send is cancelled. A separate bound from `SINK_FRAME_TTL`, so that
/// after one cancelled send the next frame still has its full window; with
/// both, no frame reaches a connection more than 120 ms after it was
/// produced.
const SINK_SEND_TIMEOUT: Duration = Duration::from_millis(60);
/// Minimum time between the end of one send to a connection and the start
/// of the next, so a connection recovering from a stall catches up at about
/// twice real time instead of bursting its backlog.
const SINK_CATCH_UP_SPACING: Duration = Duration::from_millis(10);
/// How long a closing connection may take to send its silence tail and
/// clear speaking before its task is cancelled.
const SINK_CLOSE_GRACE: Duration = Duration::from_millis(200);

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
/// The multiplier is applied to PCM samples when the player encodes a frame,
/// so a change takes effect on the next frame encoded. Frames already queued
/// for a lagging connection (at most three) keep the volume they were
/// encoded with.
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
    /// The player decodes ahead with this and applies the volume when it
    /// encodes a frame, so volume changes are not delayed by read-ahead.
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

/// Where an Opus frame sits on a connection's media timeline.
///
/// RTP timestamps count 48 kHz sampling time (RFC 7587 §4.1), so a sender
/// must move its timestamp across media it never transmitted, while the
/// sequence number counts only the packets actually sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VoiceFrameTiming {
    /// Audio carried by this frame (20 ms for frames from an [`AudioPlayer`]).
    pub duration: Duration,
    /// Media between the previous frame this sink transmitted and this one
    /// that was never transmitted: frames dropped or expired because the
    /// connection lagged, frames whose send failed or was cancelled, and
    /// ticks on which the source had no audio. Zero for contiguous frames and
    /// for the first frame after a silence tail.
    pub gap: Duration,
}

impl VoiceFrameTiming {
    /// A frame that directly follows the previously transmitted one.
    pub const fn contiguous(duration: Duration) -> Self {
        Self {
            duration,
            gap: Duration::ZERO,
        }
    }

    /// A frame that follows `gap` of untransmitted media.
    pub const fn after_gap(duration: Duration, gap: Duration) -> Self {
        Self { duration, gap }
    }
}

/// A destination for encoded Opus frames produced by an [`AudioPlayer`].
///
/// Implemented for [`VoiceRuntimeHandle`]; custom implementations are useful
/// for testing or fan-out.
///
/// # Contract
///
/// The player drives each subscription from its own task, so calls for one
/// subscription never overlap, and a slow sink only delays itself. A sink
/// subscribed again to the same player (the same `Arc`) is handed over: the
/// earlier subscription finishes first, so two tasks never feed one sink.
///
/// - **Transmit only on completion.** A send must transmit nothing before
///   the poll in which its future completes, and must return `Ok` only if
///   the frame was transmitted. The player counts an error or a dropped
///   future as "not transmitted" and reports the frame's media time in the
///   next frame's [`VoiceFrameTiming::gap`]; that accounting is exact only
///   if a dropped send really sent nothing. [`VoiceRuntimeHandle`] meets
///   this: its datagram goes out in the poll that completes the send.
/// - **Cancellation.** The future may be dropped before it completes: when
///   it has run for 60 ms (the connection counts as stalled), when playback
///   is stopped, paused, or replaced, when new audio supersedes a pending
///   silence frame, and when the 200 ms close grace period of an
///   unsubscribed or shut-down player runs out. A dropped send must leave
///   the sink usable.
/// - **Polls must not block.** The player polls the future while holding
///   the subscription's internal lock, which is what guarantees that a send
///   superseded by stop, pause, or replacement is never polled again (see
///   [`AudioPlayer`]). Polls that block delay the player's control task.
/// - `update_speaking` receives coalesced state changes: `true` before the
///   first frame of a stretch of audio, and `false` after its five-frame
///   silence tail or when the subscription closes. On forced cancellation
///   the `false` is sent from a destructor, so it must not block or panic.
pub trait VoiceFrameSink: Send + Sync {
    /// Sends one raw Opus payload at the given position on the timeline.
    fn send_opus_packet<'a>(
        &'a self,
        opus_frame: &'a [u8],
        timing: VoiceFrameTiming,
    ) -> VoiceSinkFuture<'a>;

    /// Updates the speaking indicator for this connection.
    fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError>;
}

impl VoiceFrameSink for VoiceRuntimeHandle {
    fn send_opus_packet<'a>(
        &'a self,
        opus_frame: &'a [u8],
        timing: VoiceFrameTiming,
    ) -> VoiceSinkFuture<'a> {
        Box::pin(async move {
            self.send_opus_frame_after_gap(opus_frame, timing.duration, timing.gap)
                .await?;
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
    /// A resource is loaded and the player is waiting for enough audio (60 ms)
    /// to start it, or to resume it after the source fell behind real time.
    Buffering,
    /// A resource is actively playing.
    Playing {
        /// When playback (or the latest resume) started.
        since: std::time::Instant,
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
    Shutdown,
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
    /// When the player handles the request, audio queued for the connection
    /// is discarded. The connection then gets its silence tail and speaking
    /// off in the background, within a 200 ms grace period; a connection
    /// still stalled after that is cancelled and its speaking flag cleared
    /// anyway. Use [`AudioPlayer::shutdown`] to wait for that cleanup.
    pub fn unsubscribe(self) {}
}

impl Drop for PlayerSubscription {
    fn drop(&mut self) {
        // Fails only if the player already stopped, which unsubscribed
        // every connection.
        self.command_tx
            .send(PlayerCommand::Unsubscribe { id: self.id })
            .ok();
    }
}

/// A discord.js-style audio player.
///
/// The player runs a control task ticking every 20 ms
/// ([`MissedTickBehavior::Delay`]): each tick takes one PCM frame of the
/// current [`AudioResource`], applies its volume, encodes it to Opus (with
/// the `voice-encode` feature), and queues it for every subscribed
/// connection. Commands such as [`play`](Self::play) and
/// [`stop`](Self::stop) return once the command is queued for the control
/// task, which handles it as soon as it arrives, independent of the tick
/// and of any stalled read or send. Observe the outcome through
/// [`state_watch`](Self::state_watch) and [`events`](Self::events).
///
/// # Sources and underruns
///
/// Each resource is read by its own task, at most 5 frames (100 ms) ahead
/// plus the frame being read. A track starts, resumes after an underrun,
/// and resumes after [`unpause`](Self::unpause), once 3 frames (60 ms) are
/// buffered or the input has ended; until then the player is
/// [`AudioPlayerState::Buffering`]. A tick without a frame sends nothing;
/// after 3 such ticks in a row the player goes back to `Buffering` and every
/// connection gets its silence tail. Audio is delayed by an underrun, never
/// skipped.
///
/// # Connections
///
/// Each connection has its own task and a 3-frame queue:
///
/// - **Real time first.** A queued frame must start sending within 60 ms
///   of being produced or it expires, and a send that takes longer than
///   60 ms is cancelled, so no frame reaches a connection more than 120 ms
///   late. When the queue is full the *oldest* frame is dropped. Frames that are not transmitted are reported
///   to the sink as a timeline gap ([`VoiceFrameTiming::gap`]) so RTP
///   timestamps keep counting sampling time. After a send completes, the next
///   one waits at least 10 ms, so a connection recovering from a stall
///   catches up at about twice real time instead of bursting its backlog.
///   A warning is logged once while a connection lags.
/// - **Stop, pause, and replacement flush.** Before it publishes
///   [`AudioPlayerEvent::TrackEnd`] (stop, replacement) or the `Paused`
///   state, the player discards the audio queued for every connection and
///   supersedes any send of it in progress. A superseded send is never
///   polled again, so with a sink that keeps the [`VoiceFrameSink`]
///   contract no audio of the old track is transmitted after the event;
///   only a send that already completed went out before it. Pausing keeps
///   the resource's own read-ahead, so unpausing continues from the next
///   unread frame.
/// - **Silence tail.** When a connection's audio stops (stop, pause, end of
///   track, underrun, unsubscribe), it gets five Opus silence frames at
///   20 ms intervals and then speaking off, as Discord recommends. The tail
///   is generated by the connection's task rather than queued with the
///   audio, so a lagging connection still gets it once it recovers; new
///   audio supersedes a tail that has not been sent yet. The first silence
///   frame accounts for media that passed untransmitted; once silence is
///   on the wire, the next audio starts a new segment without a gap. A
///   failed silence frame is retried, not counted; after five failures in a
///   row the tail is given up and only speaking off is sent. Speaking states
///   are coalesced: the connection eventually sees the latest one.
/// - **One task per connection.** Subscribing a sink that is already
///   subscribed, or still closing after an unsubscribe, hands it over: the
///   earlier subscription is detached, and the new one waits until its tail
///   and speaking off are done (at most 200 ms) before sending.
///
/// # Shutdown
///
/// [`shutdown`](Self::shutdown) stops playback, gives every connection up to
/// 200 ms for its silence tail and speaking off, cancels whatever is still
/// stalled, and returns once every task of the player has finished. Dropping
/// every handle and subscription performs the same cleanup in the
/// background, without a way to wait for it.
///
/// Handles are cheap to clone; all clones control the same player.
#[derive(Clone)]
pub struct AudioPlayer {
    command_tx: mpsc::UnboundedSender<PlayerCommand>,
    state_rx: watch::Receiver<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    // Shared by every clone so `subscribe()` ids stay unique.
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
            position: 0,
            retiring: Vec::new(),
        };
        crate::task::spawn_erased(task.run(command_rx));
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
    /// Returns [`DiscordError::Voice`] if the player has shut down, or if the
    /// crate was built without the `voice-encode` feature. Without an Opus
    /// encoder the player would run through the track while transmitting
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
    ///
    /// The player returns to `Playing` right away if 3 frames are buffered
    /// (or the input has ended), and otherwise to `Buffering` until they
    /// are, the same rule as for a start or an underrun.
    pub fn unpause(&self) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Unpause)
    }

    /// Stops the current resource, emitting [`TrackEndReason::Stopped`].
    ///
    /// Takes effect even while the resource's input is stalled: the input's
    /// reader is cancelled and the resource dropped shortly after (use
    /// [`shutdown`](Self::shutdown) to wait for that).
    pub fn stop(&self) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Stop)
    }

    /// Stops playback, detaches every subscription, and waits until all of
    /// the player's tasks have finished.
    ///
    /// The current track ends with [`TrackEndReason::Stopped`] and its input
    /// is dropped (an FFmpeg process is killed and reaped in the background).
    /// Each connection gets up to 200 ms to send its silence tail and clear
    /// speaking; one still stalled after that is cancelled and its speaking
    /// flag cleared anyway. This returns within about 240 ms even if the
    /// input and every connection are stalled. Only a source or sink blocked
    /// in synchronous code, which cannot be cancelled, is abandoned after
    /// that bound (and logged).
    ///
    /// Afterwards every command on any handle fails. Calling it again, or
    /// after the player has stopped, returns immediately.
    pub async fn shutdown(&self) {
        // Fails only if the player already stopped; the wait below then
        // returns at once.
        self.command_tx.send(PlayerCommand::Shutdown).ok();
        // The control task owns the state sender, so this channel closes
        // only once the task has finished its cleanup.
        let mut state = self.state_rx.clone();
        while state.changed().await.is_ok() {}
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
    ///
    /// Sinks are identified by their `Arc`. Subscribing a sink that is
    /// already subscribed replaces the earlier subscription (dropping the
    /// earlier [`PlayerSubscription`] then has no effect), and a sink still
    /// closing after an unsubscribe is handed over only once its silence
    /// tail and speaking off are done.
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

/// Owns a task the player spawned and aborts it when dropped.
///
/// The control task holds every task it spawns (one reader per track, one
/// worker per connection) through this guard, so every path that lets go of
/// one, including the control task itself being dropped, at least requests
/// cancellation; a bare `JoinHandle` would detach the task instead. Abort
/// only requests cancellation, so shutdown also waits for the tasks with
/// [`TaskGuard::join_until`].
struct TaskGuard(Option<JoinHandle<()>>);

impl TaskGuard {
    fn spawn(future: impl Future<Output = ()> + Send + 'static) -> Self {
        Self(Some(crate::task::spawn_erased(future)))
    }

    fn abort(&self) {
        if let Some(handle) = &self.0 {
            handle.abort();
        }
    }

    fn is_finished(&self) -> bool {
        self.0.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Waits for the task until `deadline` and reports whether it finished.
    /// The guard stays armed if the wait is cut short or cancelled.
    async fn join_until(&mut self, deadline: Instant) -> bool {
        let Some(handle) = self.0.as_mut() else {
            return true;
        };
        if tokio::time::timeout_at(deadline, handle).await.is_err() {
            return false;
        }
        self.0 = None;
        true
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.abort();
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
/// the receiver, and is aborted when the track goes away, which cancels a
/// pending read. The resource is discarded at that point, so the
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

fn read_failure(error: &DiscordError) -> TrackEndReason {
    tracing::warn!(%error, "audio resource failed while reading");
    TrackEndReason::Error(error.to_string())
}

/// The control task's state for the loaded resource.
struct ActiveTrack {
    frames: mpsc::Receiver<SourceEvent>,
    /// Frames taken from `frames` while (re)buffering, played first.
    ready: VecDeque<Vec<u8>>,
    /// How the input ended, once seen while buffering; reported after
    /// `ready` drains.
    ended: Option<TrackEndReason>,
    volume: VolumeControl,
    make_encoder: EncoderFactory,
    encoder: Option<Box<dyn FrameEncoder>>,
    started: bool,
    /// Consecutive playing ticks without a frame.
    missed: u8,
    reader: TaskGuard,
}

impl ActiveTrack {
    fn spawn(resource: AudioResource, make_encoder: EncoderFactory) -> Self {
        let (frames_tx, frames) = mpsc::channel(SOURCE_BUFFER_FRAMES);
        let volume = resource.volume();
        Self {
            frames,
            ready: VecDeque::new(),
            ended: None,
            volume,
            make_encoder,
            encoder: None,
            started: false,
            missed: 0,
            reader: TaskGuard::spawn(read_source(resource, frames_tx)),
        }
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>, TrackEndReason> {
        match self.frames.try_recv() {
            Ok(SourceEvent::Frame(bytes)) => Ok(Some(bytes)),
            Ok(SourceEvent::End) => Err(TrackEndReason::Finished),
            Ok(SourceEvent::Failed(error)) => Err(read_failure(&error)),
            Err(TryRecvError::Empty) => Ok(None),
            // The reader drops its sender without reporting End or Failed
            // only if the input panicked.
            Err(TryRecvError::Disconnected) => Err(read_failure(&voice_error(
                "audio source reader stopped unexpectedly",
            ))),
        }
    }

    /// Buffers frames until playback can start or resume, which it can once
    /// `REBUFFER_FRAMES` are ready or the input has ended.
    fn fill(&mut self) -> bool {
        while self.ended.is_none() && self.ready.len() < REBUFFER_FRAMES {
            match self.receive() {
                Ok(Some(bytes)) => self.ready.push_back(bytes),
                Ok(None) => break,
                Err(reason) => self.ended = Some(reason),
            }
        }
        self.ended.is_some() || self.ready.len() >= REBUFFER_FRAMES
    }

    /// The next frame to play: `Ok(None)` on an underrun, `Err` once the
    /// input is exhausted.
    fn next_frame(&mut self) -> Result<Option<Vec<u8>>, TrackEndReason> {
        if let Some(bytes) = self.ready.pop_front() {
            return Ok(Some(bytes));
        }
        match self.ended.take() {
            Some(reason) => Err(reason),
            None => self.receive(),
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

/// A frame queued for one connection.
struct QueuedFrame {
    /// The connection's playback generation when the frame was queued; a
    /// stop, pause, or replacement moves the generation on, which cancels an
    /// in-flight send of the frame.
    generation: u64,
    /// The frame's tick on the player's media clock, which advances once per
    /// playing tick, so consecutive positions are 20 ms apart.
    position: u64,
    deadline: Instant,
    payload: Arc<[u8]>,
}

/// Where a connection is between stretches of audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Talkspurt {
    /// Nothing to send.
    #[default]
    Idle,
    /// Audio is flowing.
    Active,
    /// The audio stopped; the connection still owes this many silence frames,
    /// then speaking off.
    Ending { silence_left: u8 },
}

/// What one connection still has to do, shared by the control task (which
/// changes it without waiting) and the connection's task (which acts on it).
struct SinkState {
    id: u64,
    generation: u64,
    frames: VecDeque<QueuedFrame>,
    talkspurt: Talkspurt,
    /// The media clock when the last stretch of audio ended, so the first
    /// silence frame can account for media that passed untransmitted.
    end_position: u64,
    /// When the connection must be finished; set when it is detached.
    closing: Option<Instant>,
    lagging: bool,
    dropped: u64,
}

impl SinkState {
    fn push(&mut self, position: u64, payload: &Arc<[u8]>, now: Instant) {
        self.talkspurt = Talkspurt::Active;
        self.expire(now);
        if self.frames.len() >= SINK_QUEUE_FRAMES {
            self.frames.pop_front();
            self.note_dropped();
        }
        self.frames.push_back(QueuedFrame {
            generation: self.generation,
            position,
            deadline: now + SINK_FRAME_TTL,
            payload: Arc::clone(payload),
        });
    }

    fn expire(&mut self, now: Instant) {
        while self
            .frames
            .front()
            .is_some_and(|frame| frame.deadline <= now)
        {
            self.frames.pop_front();
            self.note_dropped();
        }
    }

    /// Ends the current stretch of audio at media clock `position`. With
    /// `flush`, audio still queued is discarded and an in-flight send of it
    /// will not be polled again; without, it plays out first. Either way the
    /// silence tail follows.
    fn end_talkspurt(&mut self, flush: bool, position: u64) {
        if flush {
            self.generation = self.generation.wrapping_add(1);
            self.frames.clear();
        }
        if self.talkspurt == Talkspurt::Active {
            self.talkspurt = Talkspurt::Ending {
                silence_left: SILENCE_FRAME_COUNT,
            };
            self.end_position = position;
        }
    }

    fn note_dropped(&mut self) {
        if !self.lagging {
            self.lagging = true;
            tracing::warn!(
                subscriber = self.id,
                "subscribed connection is not keeping up; dropping its oldest audio frames"
            );
        }
        self.dropped += 1;
    }

    fn note_delivered(&mut self) {
        if self.lagging {
            tracing::info!(
                subscriber = self.id,
                dropped = self.dropped,
                "subscribed connection caught up"
            );
            self.lagging = false;
            self.dropped = 0;
        }
    }
}

struct SinkShared {
    // Mailbox between the player's control task and this connection's sink
    // task; a std lock, never held across an await.
    state: Mutex<SinkState>,
    /// Wakes the connection's task after any change, including while it is
    /// inside a send, so stop and close can cancel that send.
    wake: Notify,
}

impl SinkShared {
    fn lock(&self) -> MutexGuard<'_, SinkState> {
        // The only user code run under this lock is the poll of a sink's send
        // future (see `SinkWorker::unless_superseded`), which changes nothing
        // here, so a lock poisoned by a panicking sink still holds consistent
        // state.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, change: impl FnOnce(&mut SinkState)) {
        change(&mut self.lock());
        self.wake.notify_one();
    }
}

/// Identifies a connection by the address of the sink behind its `Arc`, so
/// two subscriptions of the same sink (or clones of its `Arc`) are known to
/// share one connection.
fn connection_key(sink: &Arc<dyn VoiceFrameSink>) -> usize {
    Arc::as_ptr(sink).cast::<()>().addr()
}

/// The control task's side of one subscribed connection.
struct Subscriber {
    key: usize,
    shared: Arc<SinkShared>,
    worker: TaskGuard,
    /// Closes when the connection's task has finished with the sink.
    done: watch::Receiver<()>,
}

impl Subscriber {
    /// Starts the connection's task. With a `predecessor` (an earlier task
    /// for the same connection that is still closing) the new task waits for
    /// it to finish before it touches the sink, so the two never interleave
    /// frames or speaking updates.
    fn spawn(
        id: u64,
        sink: Arc<dyn VoiceFrameSink>,
        predecessor: Option<watch::Receiver<()>>,
    ) -> Self {
        let key = connection_key(&sink);
        let shared = Arc::new(SinkShared {
            state: Mutex::new(SinkState {
                id,
                generation: 0,
                frames: VecDeque::new(),
                talkspurt: Talkspurt::Idle,
                end_position: 0,
                closing: None,
                lagging: false,
                dropped: 0,
            }),
            wake: Notify::new(),
        });
        let (done_tx, done) = watch::channel(());
        let worker = TaskGuard::spawn(drive_sink(sink, Arc::clone(&shared), predecessor, done_tx));
        Self {
            key,
            shared,
            worker,
            done,
        }
    }

    /// Detaches the connection at media clock `position`: queued audio is
    /// discarded, and the task sends the silence tail and exits by
    /// `deadline`.
    fn close(self, deadline: Instant, position: u64) -> Retiring {
        self.shared.update(|state| {
            state.end_talkspurt(true, position);
            state.closing = Some(deadline);
        });
        Retiring {
            task: self.worker,
            connection: Some((self.key, self.done)),
        }
    }
}

/// A task that is winding down: the reader of a finished track, or the task
/// of a detached connection. Shutdown waits for these.
struct Retiring {
    task: TaskGuard,
    /// For a connection's task: its connection key and completion signal, so
    /// a new subscription of the same connection can wait for it.
    connection: Option<(usize, watch::Receiver<()>)>,
}

/// What a connection's task does next.
enum SinkStep {
    Frame(QueuedFrame),
    /// A silence frame of the tail, after this much untransmitted media.
    Silence(Duration),
    SpeakingOff,
    Wait(Option<Instant>),
    Exit,
}

/// One connection's task state. Its destructor clears speaking, so every way
/// the task can end (normal close, the close deadline, abort) leaves a
/// still-live connection not speaking.
struct SinkWorker {
    sink: Arc<dyn VoiceFrameSink>,
    shared: Arc<SinkShared>,
    speaking: bool,
    /// Position of the last audio frame transmitted in the current media
    /// segment. A transmitted silence frame ends the segment.
    last_position: Option<u64>,
    /// When the last completed send finished, for pacing.
    last_sent_at: Option<Instant>,
    /// Consecutive silence frames the sink failed to send.
    silence_failures: u8,
    /// Released after `Drop` has cleared speaking (fields drop after the
    /// destructor runs), which lets a successor on the same connection start.
    _done: watch::Sender<()>,
}

/// Converts a number of 20 ms frames into a timeline gap.
fn frames_gap(frames: u64) -> Duration {
    PLAYER_FRAME_DURATION.saturating_mul(u32::try_from(frames).unwrap_or(u32::MAX))
}

fn earliest(at: Instant, other: Option<Instant>) -> Instant {
    other.map_or(at, |other| at.min(other))
}

async fn sleep_until_opt(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Delivers one connection's audio, silence tail, and speaking state.
///
/// One task per connection keeps a slow or stalled connection from delaying
/// the control loop or the other connections. It ends after its close
/// deadline at the latest, or is aborted with the player. `done` is dropped
/// when the task ends, however it ends, which releases a successor waiting
/// on the same connection.
async fn drive_sink(
    sink: Arc<dyn VoiceFrameSink>,
    shared: Arc<SinkShared>,
    predecessor: Option<watch::Receiver<()>>,
    done: watch::Sender<()>,
) {
    if let Some(mut predecessor) = predecessor {
        // The predecessor's close deadline bounds this wait.
        while predecessor.changed().await.is_ok() {}
    }
    let mut worker = SinkWorker {
        sink,
        shared,
        speaking: false,
        last_position: None,
        last_sent_at: None,
        silence_failures: 0,
        _done: done,
    };
    loop {
        match worker.next_step(Instant::now()) {
            SinkStep::Frame(frame) => worker.send_frame(frame).await,
            SinkStep::Silence(gap) => worker.send_silence(gap).await,
            SinkStep::SpeakingOff => worker.set_speaking(false),
            SinkStep::Wait(until) => worker.wait(until).await,
            SinkStep::Exit => return,
        }
    }
}

impl SinkWorker {
    fn next_step(&mut self, now: Instant) -> SinkStep {
        let mut state = self.shared.lock();
        if state.closing.is_some_and(|deadline| now >= deadline) {
            return SinkStep::Exit;
        }
        state.expire(now);
        if let Some(front_deadline) = state.frames.front().map(|frame| frame.deadline) {
            let slot = self
                .last_sent_at
                .map(|at| at + SINK_CATCH_UP_SPACING)
                .filter(|slot| *slot > now);
            if let Some(slot) = slot {
                return SinkStep::Wait(Some(earliest(slot.min(front_deadline), state.closing)));
            }
            if let Some(frame) = state.frames.pop_front() {
                return SinkStep::Frame(frame);
            }
        }
        match state.talkspurt {
            Talkspurt::Ending { silence_left } if self.speaking && silence_left > 0 => {
                let slot = self
                    .last_sent_at
                    .map_or(now, |at| at + PLAYER_FRAME_DURATION);
                if slot > now {
                    SinkStep::Wait(Some(earliest(slot, state.closing)))
                } else {
                    // Until a silence frame is on the wire, the segment is
                    // still the audio's, so the first one accounts for the
                    // media that passed since the last transmitted frame.
                    let skipped = self
                        .last_position
                        .map_or(0, |last| state.end_position.saturating_sub(last));
                    SinkStep::Silence(frames_gap(skipped))
                }
            }
            Talkspurt::Ending { .. } => {
                state.talkspurt = Talkspurt::Idle;
                if self.speaking {
                    SinkStep::SpeakingOff
                } else if state.closing.is_some() {
                    SinkStep::Exit
                } else {
                    SinkStep::Wait(None)
                }
            }
            Talkspurt::Idle if state.closing.is_some() => SinkStep::Exit,
            Talkspurt::Idle | Talkspurt::Active => SinkStep::Wait(state.closing),
        }
    }

    fn set_speaking(&mut self, speaking: bool) {
        self.speaking = speaking;
        if !speaking {
            self.last_position = None;
        }
        if let Err(error) = self.sink.update_speaking(speaking) {
            tracing::warn!(%error, "failed to update speaking state on subscribed connection");
        }
    }

    async fn wait(&self, until: Option<Instant>) {
        tokio::select! {
            () = self.shared.wake.notified() => {}
            () = sleep_until_opt(until) => {}
        }
    }

    /// Runs `future` unless the player supersedes it first: when
    /// `superseded` holds, once `deadline` has passed, or once the close
    /// deadline has passed. A superseded future is dropped (cancelled).
    ///
    /// The checks run before every poll of `future`, and `future` is polled
    /// while the mailbox lock is held, the lock the control task takes to
    /// stop, pause, replace, or detach. So once the control task has made
    /// such a change, `future` is never polled again: whatever the send
    /// transmits, it transmitted before the change, and so before the event
    /// that announces it.
    async fn unless_superseded<F: Future>(
        &self,
        future: F,
        deadline: Option<Instant>,
        superseded: impl Fn(&SinkState) -> bool,
    ) -> Option<F::Output> {
        let mut future = std::pin::pin!(future);
        let mut supervised = std::future::poll_fn(|cx| {
            let state = self.shared.lock();
            let now = Instant::now();
            let passed = |at: Option<Instant>| at.is_some_and(|at| now >= at);
            if superseded(&state) || passed(deadline) || passed(state.closing) {
                return Poll::Ready(None);
            }
            future.as_mut().poll(cx).map(Some)
        });
        loop {
            let wake_at = match (deadline, self.shared.lock().closing) {
                (Some(deadline), closing) => Some(earliest(deadline, closing)),
                (None, closing) => closing,
            };
            // Changes and deadlines only wake the task; `supervised` decides,
            // so a send that became ready together with a change is still
            // checked against the change first. It is polled before the timer
            // because a passed deadline keeps the timer ready.
            tokio::select! {
                biased;
                () = self.shared.wake.notified() => {}
                output = &mut supervised => return output,
                () = sleep_until_opt(wake_at) => {}
            }
        }
    }

    async fn send_frame(&mut self, frame: QueuedFrame) {
        if !self.speaking {
            self.set_speaking(true);
        }
        let skipped = self.last_position.map_or(0, |last| {
            frame.position.saturating_sub(last.saturating_add(1))
        });
        let timing = VoiceFrameTiming::after_gap(PLAYER_FRAME_DURATION, frames_gap(skipped));
        let generation = frame.generation;
        let stalled_at = Instant::now() + SINK_SEND_TIMEOUT;
        let send = self.sink.send_opus_packet(&frame.payload, timing);
        let outcome = self
            .unless_superseded(send, Some(stalled_at), |state| {
                state.generation != generation
            })
            .await;
        match outcome {
            Some(Ok(())) => {
                self.last_position = Some(frame.position);
                self.last_sent_at = Some(Instant::now());
                self.shared.lock().note_delivered();
            }
            Some(Err(error)) => {
                self.last_sent_at = Some(Instant::now());
                tracing::warn!(%error, "failed to send opus frame to subscribed connection");
            }
            None => tracing::trace!("cancelled a stalled or superseded opus frame send"),
        }
    }

    async fn send_silence(&mut self, gap: Duration) {
        let send = self.sink.send_opus_packet(
            &OPUS_SILENCE_FRAME,
            VoiceFrameTiming::after_gap(PLAYER_FRAME_DURATION, gap),
        );
        let outcome = self
            .unless_superseded(send, None, |state| state.talkspurt == Talkspurt::Active)
            .await;
        // A superseded silence frame was not sent: the segment, and with it
        // the gap reference for the next audio frame, stays as it was.
        let Some(result) = outcome else {
            return;
        };
        self.last_sent_at = Some(Instant::now());
        let delivered = match result {
            Ok(()) => {
                // Silence is on the wire: the tail now starts a new segment,
                // which the next audio frame follows without a gap.
                self.last_position = None;
                self.silence_failures = 0;
                1
            }
            Err(error) => {
                tracing::warn!(%error, "failed to send silence frame to subscribed connection");
                // Not sent, so not counted. A connection that keeps failing
                // gets no tail rather than retries forever; speaking off is
                // still sent.
                self.silence_failures += 1;
                if self.silence_failures < SILENCE_FRAME_COUNT {
                    0
                } else {
                    self.silence_failures = 0;
                    SILENCE_FRAME_COUNT
                }
            }
        };
        if let Talkspurt::Ending { silence_left } = &mut self.shared.lock().talkspurt {
            *silence_left = silence_left.saturating_sub(delivered);
        }
    }
}

impl Drop for SinkWorker {
    fn drop(&mut self) {
        // Skipped while unwinding from a sink panic: calling back into the
        // panicking sink could abort the process.
        if self.speaking && !std::thread::panicking() {
            self.set_speaking(false);
        }
    }
}

// [Decision Log] Cancellable source reads and real-time connection delivery
// (3.0 phase 6)
// - Purpose: stop, pause, replacement, unsubscribe, and shutdown must take
//   effect promptly while FFmpeg or a voice connection is stalled; no audio
//   of an old track may reach a connection after its end is published; every
//   connection must get its silence tail and speaking off; RTP timestamps
//   must keep counting sampling time when frames are not transmitted; and
//   shutdown must be bounded and observable.
// - Previous implementation and constraints: one task awaited the PCM read
//   and then each sink's send inside the tick branch of its `select!`, so a
//   stalled read or send blocked every command, including shutdown. The
//   first rework used per-sink mpsc queues: old frames survived a stop,
//   silence frames were lost with the lossy queue, dropped frames vanished
//   from the RTP timeline, and abort-only teardown was not observable.
//   `VoiceFrameSink` futures may never complete, and a frame must stay
//   intact if its read is cancelled.
// - Alternatives considered: (1) racing reads and sends against the command
//   channel in one task: cancelling the read loses its partial frame, and
//   sinks are still served one after another; (2) per-sink mpsc queues of
//   frames and speaking updates: a queued item cannot be revoked or
//   expired, and the tail shares the queue's loss; (3) one task per source
//   and per connection, with each connection's pending work in a small
//   mutex-protected mailbox that the control task rewrites without waiting:
//   a generation for flushes, expiring frames, a talkspurt state that owes
//   the silence tail, and a close deadline.
// - Chosen: (3), dropping the oldest frame when a connection's queue is
//   full, with frames positioned on a player media clock so skipped media
//   becomes an RTP gap.
// - Why: the control loop never awaits I/O, so command latency does not
//   depend on any source or connection. The connection's task polls a
//   pending send only under the mailbox lock and after re-checking the
//   mailbox, so once the control task has flushed, the old send is never
//   polled again, and the flush is complete before the event that
//   announces it (merely waking the task first would let a send that became
//   ready at the same moment complete). The tail survives lag because it is
//   state, not a queued item. Connections are identified by their sink's
//   `Arc`, and a new task for a connection waits for the previous one, so a
//   retiring task can never touch a connection that was handed over.
// - Trade-offs: more moving parts than a channel; a lagging connection loses
//   audio instead of delaying everyone; a frame can still arrive up to
//   120 ms late (60 ms queued plus 60 ms in flight), and a recovering
//   connection catches up at about twice real time; a send in progress when
//   the network takes the packet cannot be recalled; a sink poll that blocks
//   also blocks the control task's next update of that connection; a
//   re-subscribed connection resumes only after the old tail (up to 200 ms).
struct PlayerTask {
    options: AudioPlayerOptions,
    subscribers: HashMap<u64, Subscriber>,
    state_tx: watch::Sender<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    track: Option<ActiveTrack>,
    /// The media clock: advances once per playing tick.
    position: u64,
    /// Readers of finished tracks and tasks of detached connections that may
    /// still be running; shutdown waits for them.
    retiring: Vec<Retiring>,
}

impl PlayerTask {
    /// Neither branch awaits anything but the command channel and the ticker,
    /// so a command is handled as soon as it arrives.
    async fn run(mut self, mut command_rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        let mut ticker = interval(PLAYER_FRAME_DURATION);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = command_rx.recv() => match command {
                    Some(PlayerCommand::Shutdown) | None => break,
                    Some(command) => self.handle_command(command),
                },
                _ = ticker.tick() => self.handle_tick(),
            }
        }
        // Refuse further commands while cleaning up.
        drop(command_rx);
        self.shut_down().await;
    }

    async fn shut_down(mut self) {
        if self.track.is_some() {
            self.finish_playback(TrackEndReason::Stopped, true);
        }
        let close_by = Instant::now() + SINK_CLOSE_GRACE;
        for (_, subscriber) in std::mem::take(&mut self.subscribers) {
            self.retiring
                .push(subscriber.close(close_by, self.position));
        }
        // Connection tasks exit by their close deadline on their own (a task
        // waiting for a predecessor on the same connection starts after it,
        // but its own deadline is the same); the extra frame covers
        // scheduling. Whatever is left is aborted and given one more frame to
        // unwind.
        let graceful_until = close_by + PLAYER_FRAME_DURATION;
        for retiring in &mut self.retiring {
            retiring.task.join_until(graceful_until).await;
        }
        let forced_until = Instant::now() + PLAYER_FRAME_DURATION;
        for Retiring { task, .. } in &mut self.retiring {
            task.abort();
            if !task.join_until(forced_until).await {
                tracing::warn!(
                    "an audio player task ignored cancellation; it is blocked outside an await"
                );
            }
        }
        self.retiring.clear();
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
        self.emit(AudioPlayerEvent::StateChange { old, new });
    }

    fn emit(&self, event: AudioPlayerEvent) {
        // A broadcast send fails only when nobody is subscribed.
        self.events_tx.send(event).ok();
    }

    /// Ends every connection's stretch of audio. When this returns, a flushed
    /// connection will not poll a send of the old audio again (see
    /// `SinkWorker::unless_superseded`), so callers publish the matching
    /// event only afterwards.
    fn end_talkspurts(&self, flush: bool) {
        for subscriber in self.subscribers.values() {
            subscriber
                .shared
                .update(|state| state.end_talkspurt(flush, self.position));
        }
    }

    fn send_to_all(&self, position: u64, frame: &Arc<[u8]>) {
        let now = Instant::now();
        for subscriber in self.subscribers.values() {
            subscriber
                .shared
                .update(|state| state.push(position, frame, now));
        }
    }

    /// Cancels the current track's reader and keeps its guard for shutdown.
    fn retire_track(&mut self) -> bool {
        let Some(track) = self.track.take() else {
            return false;
        };
        track.reader.abort();
        self.retiring.push(Retiring {
            task: track.reader,
            connection: None,
        });
        true
    }

    /// Detaches a connection, keeping its task for shutdown and for a later
    /// subscription of the same connection to wait on.
    fn retire_subscriber(&mut self, subscriber: Subscriber) {
        let retiring = subscriber.close(Instant::now() + SINK_CLOSE_GRACE, self.position);
        self.retiring.push(retiring);
    }

    /// Hands a connection over to a new subscription: an earlier active
    /// subscription of the same connection is detached, and the most recent
    /// task still closing on it is returned so the new task can wait for it.
    fn take_over(&mut self, key: usize) -> Option<watch::Receiver<()>> {
        let active = self
            .subscribers
            .iter()
            .find(|(_, subscriber)| subscriber.key == key)
            .map(|(id, _)| *id);
        if let Some(subscriber) = active.and_then(|id| self.subscribers.remove(&id)) {
            tracing::debug!("connection subscribed again; replacing its earlier subscription");
            self.retire_subscriber(subscriber);
        }
        self.retiring.iter().rev().find_map(|retiring| {
            retiring
                .connection
                .as_ref()
                .filter(|(retiring_key, _)| *retiring_key == key)
                .map(|(_, done)| done.clone())
        })
    }

    fn finish_playback(&mut self, reason: TrackEndReason, flush: bool) {
        tracing::info!(?reason, "audio player track ended");
        self.retire_track();
        // Connections learn first: with `flush`, audio still queued for them
        // is invalidated before TrackEnd is published.
        self.end_talkspurts(flush);
        self.set_state(AudioPlayerState::Idle);
        self.emit(AudioPlayerEvent::TrackEnd { reason });
    }

    /// Transitions out of Paused/AutoPaused back into playback, under the
    /// same rule as a start or rebuffer: `Playing` only once
    /// `REBUFFER_FRAMES` are buffered (or the input has ended), otherwise
    /// `Buffering` until they are.
    fn resume_playback(&mut self) {
        if self.subscribers.is_empty()
            && self.options.no_subscriber_behavior == NoSubscriberBehavior::Pause
        {
            self.set_state(AudioPlayerState::AutoPaused);
            return;
        }
        let ready = match self.track.as_mut() {
            Some(track) if track.started => {
                track.missed = 0;
                track.fill()
            }
            _ => false,
        };
        if ready {
            self.set_state(AudioPlayerState::Playing {
                since: std::time::Instant::now(),
            });
        } else {
            self.set_state(AudioPlayerState::Buffering);
        }
    }

    fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Play { resource, encoder } => {
                if self.retire_track() {
                    // Invalidate the old track's queued audio before
                    // announcing its end.
                    self.end_talkspurts(true);
                    self.emit(AudioPlayerEvent::TrackEnd {
                        reason: TrackEndReason::Stopped,
                    });
                }
                tracing::info!(title = ?resource.title(), "audio player playing new resource");
                self.track = Some(ActiveTrack::spawn(*resource, encoder));
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
                    self.end_talkspurts(true);
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
                    self.finish_playback(TrackEndReason::Stopped, true);
                }
            }
            PlayerCommand::Subscribe { id, sink } => {
                let predecessor = self.take_over(connection_key(&sink));
                self.subscribers
                    .insert(id, Subscriber::spawn(id, sink, predecessor));
                tracing::debug!(
                    subscribers = self.subscribers.len(),
                    "connection subscribed"
                );
                if self.current_state() == AudioPlayerState::AutoPaused {
                    self.resume_playback();
                }
            }
            PlayerCommand::Unsubscribe { id } => {
                if let Some(subscriber) = self.subscribers.remove(&id) {
                    self.retire_subscriber(subscriber);
                    tracing::debug!(
                        subscribers = self.subscribers.len(),
                        "connection unsubscribed"
                    );
                }
            }
            // `run` handles shutdown before dispatching here.
            PlayerCommand::Shutdown => {}
        }
    }

    /// Forgets finished retiring tasks and detaches connections whose task
    /// died, which happens only if their sink panicked.
    fn reap(&mut self) {
        self.retiring
            .retain(|retiring| !retiring.task.is_finished());
        self.subscribers.retain(|id, subscriber| {
            let alive = !subscriber.worker.is_finished();
            if !alive {
                tracing::error!(
                    subscriber = id,
                    "subscribed sink task stopped (did the sink panic?); detaching it"
                );
            }
            alive
        });
    }

    fn handle_tick(&mut self) {
        self.reap();
        match self.current_state() {
            state @ (AudioPlayerState::Buffering | AudioPlayerState::Playing { .. }) => {
                if self.subscribers.is_empty() {
                    match self.options.no_subscriber_behavior {
                        NoSubscriberBehavior::Pause => {
                            self.set_state(AudioPlayerState::AutoPaused);
                            return;
                        }
                        NoSubscriberBehavior::Stop => {
                            self.finish_playback(TrackEndReason::Stopped, true);
                            return;
                        }
                        NoSubscriberBehavior::Play => {}
                    }
                }
                if state == AudioPlayerState::Buffering {
                    self.try_start();
                } else {
                    self.play_tick();
                }
            }
            AudioPlayerState::AutoPaused => {
                if !self.subscribers.is_empty() {
                    self.resume_playback();
                }
            }
            AudioPlayerState::Paused | AudioPlayerState::Idle => {}
        }
    }

    /// Starts or resumes playback once enough audio is buffered.
    fn try_start(&mut self) {
        let Some(track) = self.track.as_mut() else {
            self.set_state(AudioPlayerState::Idle);
            return;
        };
        if !track.fill() {
            return;
        }
        if track.ready.is_empty() {
            // The input ended (or failed) with nothing left to play.
            let reason = track.ended.take().unwrap_or(TrackEndReason::Finished);
            self.finish_playback(reason, false);
            return;
        }
        let first_start = !track.started;
        track.started = true;
        track.missed = 0;
        self.set_state(AudioPlayerState::Playing {
            since: std::time::Instant::now(),
        });
        if first_start {
            self.emit(AudioPlayerEvent::TrackStart);
        }
        self.play_tick();
    }

    fn play_tick(&mut self) {
        let Some(track) = self.track.as_mut() else {
            self.set_state(AudioPlayerState::Idle);
            return;
        };
        self.position += 1;
        let bytes = match track.next_frame() {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                track.missed += 1;
                if track.missed >= UNDERRUN_GRACE_FRAMES {
                    track.missed = 0;
                    tracing::debug!("audio source underrun; rebuffering");
                    self.end_talkspurts(false);
                    self.set_state(AudioPlayerState::Buffering);
                }
                return;
            }
            Err(reason) => {
                self.finish_playback(reason, false);
                return;
            }
        };
        track.missed = 0;
        // With nobody subscribed (NoSubscriberBehavior::Play) the frame is
        // consumed in real time but not encoded.
        let encoded = (!self.subscribers.is_empty())
            .then(|| track.encode(pcm_s16le_to_f32(&bytes, track.volume.get())));
        match encoded {
            Some(Ok(opus)) => self.send_to_all(self.position, &opus.into()),
            Some(Err(error)) => {
                tracing::warn!(%error, "failed to encode PCM frame to Opus");
                self.finish_playback(TrackEndReason::Error(error.to_string()), false);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::task::Context;

    use tokio::io::{AsyncWriteExt, ReadBuf};
    use tokio::sync::{oneshot, Semaphore};
    use tokio::time::{sleep, timeout};

    /// One frame period: the latency bound for commands.
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

    /// One frame as a sink saw it.
    #[derive(Clone, Debug)]
    struct SentFrame {
        payload: Vec<u8>,
        timing: VoiceFrameTiming,
        at: Instant,
    }

    impl SentFrame {
        /// The frame index of an audio payload, `None` for silence.
        fn index(&self) -> Option<i16> {
            <[u8; 2]>::try_from(self.payload.as_slice())
                .ok()
                .map(i16::from_le_bytes)
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        frames: Mutex<Vec<SentFrame>>,
        speaking: Mutex<Vec<bool>>,
    }

    impl RecordingSink {
        fn record(&self, opus_frame: &[u8], timing: VoiceFrameTiming) {
            self.frames
                .lock()
                .expect("frames mutex poisoned")
                .push(SentFrame {
                    payload: opus_frame.to_vec(),
                    timing,
                    at: Instant::now(),
                });
        }

        fn frames(&self) -> Vec<SentFrame> {
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
                .filter(|frame| frame.payload == OPUS_SILENCE_FRAME)
                .count()
        }

        /// Audio (non-silence) frames, in send order.
        fn audio(&self) -> Vec<SentFrame> {
            self.frames()
                .into_iter()
                .filter(|frame| frame.index().is_some())
                .collect()
        }

        fn audio_frames(&self) -> Vec<i16> {
            self.audio().iter().filter_map(SentFrame::index).collect()
        }
    }

    impl VoiceFrameSink for RecordingSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                self.record(opus_frame, timing);
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
    /// permits it is stalled. A frame is recorded when its permit arrives.
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

        fn release(&self) {
            self.gate.add_permits(10_000);
        }
    }

    impl VoiceFrameSink for GatedSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                self.gate
                    .acquire()
                    .await
                    .map_err(|_| voice_error("gate closed"))?
                    .forget();
                self.inner.record(opus_frame, timing);
                Ok(())
            })
        }

        fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
            self.inner.update_speaking(speaking)
        }
    }

    /// A connection that needs `delay` per send, slower than real time.
    struct SlowSink {
        delay: Duration,
        inner: RecordingSink,
    }

    impl VoiceFrameSink for SlowSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                sleep(self.delay).await;
                self.inner.record(opus_frame, timing);
                Ok(())
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
            _timing: VoiceFrameTiming,
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
            _timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async { Err(voice_error("synthetic send failure")) })
        }

        fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
            Err(voice_error("synthetic speaking failure"))
        }
    }

    /// A stalled connection that records, for each audio frame it
    /// transmits, the player state at that moment.
    struct WitnessSink {
        gate: Semaphore,
        attempts: AtomicUsize,
        player_state: watch::Receiver<AudioPlayerState>,
        transmitted: Mutex<Vec<(i16, AudioPlayerState)>>,
    }

    impl VoiceFrameSink for WitnessSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            _timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                self.attempts.fetch_add(1, Ordering::SeqCst);
                self.gate
                    .acquire()
                    .await
                    .map_err(|_| voice_error("gate closed"))?
                    .forget();
                if let Ok(index) = <[u8; 2]>::try_from(opus_frame) {
                    let state = self.player_state.borrow().clone();
                    self.transmitted
                        .lock()
                        .expect("witness mutex poisoned")
                        .push((i16::from_le_bytes(index), state));
                }
                Ok(())
            })
        }

        fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    /// Passes audio through but holds every silence frame forever.
    #[derive(Default)]
    struct SilenceStallingSink {
        inner: RecordingSink,
    }

    impl VoiceFrameSink for SilenceStallingSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                if opus_frame == OPUS_SILENCE_FRAME {
                    std::future::pending::<()>().await;
                }
                self.inner.record(opus_frame, timing);
                Ok(())
            })
        }

        fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
            self.inner.update_speaking(speaking)
        }
    }

    /// Fails its first `failures` silence sends.
    struct FlakySilenceSink {
        failures: AtomicUsize,
        silence_attempts: AtomicUsize,
        inner: RecordingSink,
    }

    impl FlakySilenceSink {
        fn failing(failures: usize) -> Self {
            Self {
                failures: AtomicUsize::new(failures),
                silence_attempts: AtomicUsize::new(0),
                inner: RecordingSink::default(),
            }
        }
    }

    impl VoiceFrameSink for FlakySilenceSink {
        fn send_opus_packet<'a>(
            &'a self,
            opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                if opus_frame == OPUS_SILENCE_FRAME {
                    self.silence_attempts.fetch_add(1, Ordering::SeqCst);
                    let left = self.failures.load(Ordering::SeqCst);
                    if left > 0 {
                        self.failures.store(left - 1, Ordering::SeqCst);
                        return Err(voice_error("synthetic silence failure"));
                    }
                }
                self.inner.record(opus_frame, timing);
                Ok(())
            })
        }

        fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> {
            self.inner.update_speaking(speaking)
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

    fn subscribe(player: &AudioPlayer, sink: Arc<dyn VoiceFrameSink>) -> PlayerSubscription {
        player.subscribe(sink).expect("subscribe")
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

    /// Every event up to and including the next `TrackEnd`.
    async fn events_until_track_end(
        events: &mut broadcast::Receiver<AudioPlayerEvent>,
    ) -> Vec<AudioPlayerEvent> {
        let mut seen = Vec::new();
        loop {
            let event = next_event(events).await;
            let end = matches!(event, AudioPlayerEvent::TrackEnd { .. });
            seen.push(event);
            if end {
                return seen;
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
        limit: Duration,
        what: &str,
        predicate: impl FnMut(&AudioPlayerState) -> bool,
    ) {
        within(limit, what, state.wait_for(predicate))
            .await
            .expect("state watch closed");
    }

    fn is_playing(state: &AudioPlayerState) -> bool {
        matches!(state, AudioPlayerState::Playing { .. })
    }

    /// Asserts each audio frame reports exactly the frames skipped since
    /// the previous one as its timeline gap.
    fn assert_gaps_match_skipped_frames(audio: &[SentFrame]) {
        for pair in audio.windows(2) {
            let (Some(previous), Some(next)) = (pair[0].index(), pair[1].index()) else {
                continue;
            };
            assert!(next > previous, "frames out of order: {audio:?}");
            let skipped = u32::try_from(next - previous - 1).expect("positive skip");
            assert_eq!(
                pair[1].timing.gap,
                FRAME * skipped,
                "gap before frame {next} (after {previous})"
            );
        }
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

    #[test]
    fn voice_frame_timing_constructors_set_gap() {
        assert_eq!(VoiceFrameTiming::contiguous(FRAME).gap, Duration::ZERO);
        let timing = VoiceFrameTiming::after_gap(FRAME, FRAME * 3);
        assert_eq!((timing.duration, timing.gap), (FRAME, FRAME * 3));
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
        let _subscription = subscribe(&player, sink.clone());
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
        // Buffering -> Playing once enough audio is buffered.
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

        // Let the post-track silence tail flush.
        sleep(Duration::from_millis(200)).await;
        assert_eq!(sink.audio_frames(), vec![0, 0, 0]);
        assert_eq!(sink.silence_frames(), 5);
        let frames = sink.frames();
        assert!(frames[3..]
            .iter()
            .all(|frame| frame.payload == OPUS_SILENCE_FRAME));
        // The tail is paced like audio.
        for pair in frames[3..].windows(2) {
            assert_eq!(pair[1].at - pair[0].at, FRAME);
        }

        // Speaking toggled on before the first frame and off after the tail.
        assert_eq!(sink.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_pause_unpause_and_stop_transitions() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = subscribe(&player, sink.clone());
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

        // The silence tail and speaking off follow a pause.
        sleep(Duration::from_millis(200)).await;
        assert_eq!(sink.silence_frames(), 5);
        assert_eq!(sink.speaking(), vec![true, false]);

        player.unpause().expect("unpause");
        assert!(matches!(
            next_event(&mut events).await,
            AudioPlayerEvent::StateChange {
                old: AudioPlayerState::Paused,
                new: AudioPlayerState::Playing { .. },
            }
        ));
        sleep(FRAME * 3).await;
        assert_eq!(sink.speaking(), vec![true, false, true]);

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
        wait_for_state(&mut state, FRAME * 5, "auto-pause", |state| {
            *state == AudioPlayerState::AutoPaused
        })
        .await;

        // Subscribing resumes playback and the track then finishes.
        let sink = Arc::new(RecordingSink::default());
        let _subscription = subscribe(&player, sink.clone());

        let seen = within(
            FRAME * 10,
            "the track finishing",
            events_until_track_end(&mut events),
        )
        .await;
        assert!(seen.contains(&AudioPlayerEvent::TrackStart));
        assert_eq!(
            seen.last(),
            Some(&AudioPlayerEvent::TrackEnd {
                reason: TrackEndReason::Finished
            })
        );
        assert_eq!(sink.audio_frames().len(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn player_no_subscriber_play_behavior_keeps_consuming() {
        let player = test_player(NoSubscriberBehavior::Play);
        let mut events = player.events();

        player.play(silent_pcm_resource(2)).expect("play");
        let seen = within(
            FRAME * 10,
            "the track finishing",
            events_until_track_end(&mut events),
        )
        .await;
        assert!(seen.contains(&AudioPlayerEvent::TrackStart));
        assert_eq!(
            seen.last(),
            Some(&AudioPlayerEvent::TrackEnd {
                reason: TrackEndReason::Finished
            })
        );
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
        let seen = within(
            FRAME * 5,
            "the track ending",
            events_until_track_end(&mut events),
        )
        .await;
        // A track that never produced audio does not start.
        assert!(!seen.contains(&AudioPlayerEvent::TrackStart), "{seen:?}");
        match seen.last() {
            Some(AudioPlayerEvent::TrackEnd {
                reason: TrackEndReason::Error(message),
            }) => assert!(message.contains("synthetic read failure"), "{message}"),
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
        let _subscription = subscribe(&player, sink.clone());
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
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();
        let mut state = player.state_watch();

        // Three and a half frames, then the stream hangs in the middle of a frame.
        let mut data = numbered_pcm(0..3);
        data.extend(vec![0_u8; PLAYER_FRAME_BYTES / 2]);
        let (resource, mut dropped) = stalling_resource(data);
        player.play(resource).expect("play");
        wait_until(FRAME * 10, "three frames playing", || {
            sink.audio_frames().len() == 3
        })
        .await;
        // The underrun sends the player back to Buffering.
        wait_for_state(&mut state, FRAME * 5, "rebuffering", |state| {
            *state == AudioPlayerState::Buffering
        })
        .await;
        assert_eq!(sink.audio_frames(), vec![0, 1, 2]);

        player.pause().expect("pause");
        wait_for_state(&mut state, FRAME, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        // Nothing is buffered, so unpausing waits in Buffering like a rebuffer.
        player.unpause().expect("unpause");
        wait_for_state(&mut state, FRAME, "unpause", |state| {
            *state == AudioPlayerState::Buffering
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
        let _subscription = subscribe(&player, sink.clone());
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
    async fn player_rebuffers_after_an_underrun_without_skipping_audio() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();
        let mut state = player.state_watch();
        let (mut writer, reader) = tokio::io::duplex(PLAYER_FRAME_BYTES * 8);

        writer
            .write_all(&numbered_pcm(0..4))
            .await
            .expect("write first frames");
        player
            .play(AudioResource::new(AudioInput::raw_pcm(reader)))
            .expect("play");

        // Four frames play, then the source stalls: after three empty ticks
        // the player rebuffers and the connection gets its silence tail.
        wait_until(FRAME * 20, "the first four frames", || {
            sink.audio_frames().len() == 4
        })
        .await;
        wait_for_state(&mut state, FRAME * 5, "rebuffering", |state| {
            *state == AudioPlayerState::Buffering
        })
        .await;
        sleep(FRAME * 10).await;
        assert_eq!(player.state(), AudioPlayerState::Buffering);
        assert_eq!(sink.silence_frames(), 5);
        assert_eq!(sink.speaking(), vec![true, false]);

        // The source catches up; playback resumes where it left off.
        writer
            .write_all(&numbered_pcm(4..8))
            .await
            .expect("write more frames");
        drop(writer);
        let seen = within(
            FRAME * 20,
            "the track finishing",
            events_until_track_end(&mut events),
        )
        .await;
        sleep(FRAME * 10).await;

        assert_eq!(sink.audio_frames(), (0..8).collect::<Vec<i16>>());
        // The first silence frame accounts for the three empty ticks; the
        // rest of the tail and the resumed audio follow without a gap.
        let silence: Vec<SentFrame> = sink
            .frames()
            .into_iter()
            .filter(|frame| frame.payload == OPUS_SILENCE_FRAME)
            .collect();
        assert_eq!(silence[0].timing.gap, FRAME * 3);
        assert!(silence[1..5].iter().all(|frame| frame.timing.gap.is_zero()));
        let resumed = sink.audio()[4].clone();
        assert_eq!(
            resumed.timing.gap,
            Duration::ZERO,
            "a new segment after the tail"
        );
        assert_eq!(sink.silence_frames(), 10);
        assert_eq!(sink.speaking(), vec![true, false, true, false]);
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
                        new: AudioPlayerState::Buffering,
                        ..
                    },
                    AudioPlayerEvent::StateChange {
                        new: AudioPlayerState::Playing { .. },
                        ..
                    },
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
    }

    #[tokio::test(start_paused = true)]
    async fn player_keeps_serving_other_sinks_and_commands_while_one_sink_is_stalled() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let stalled = Arc::new(GatedSink::stalled());
        let healthy = Arc::new(RecordingSink::default());
        let stalled_subscription = subscribe(&player, stalled.clone());
        let _healthy_subscription = subscribe(&player, healthy.clone());
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
        assert!(healthy
            .audio()
            .iter()
            .all(|frame| frame.timing.gap.is_zero()));

        player.pause().expect("pause");
        wait_for_state(&mut state, FRAME, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        player.stop().expect("stop");
        assert_eq!(
            within(FRAME, "stop", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );

        sleep(FRAME * 10).await;
        assert_eq!(healthy.silence_frames(), 5);
        assert_eq!(healthy.speaking(), vec![true, false]);

        // Unsubscribing a connection that stays stalled cancels it after the
        // close grace period and still clears its speaking flag.
        drop(stalled_subscription);
        wait_until(
            SINK_CLOSE_GRACE + FRAME * 2,
            "releasing the stalled sink",
            || Arc::strong_count(&stalled) == 1,
        )
        .await;
        assert!(stalled.inner.frames().is_empty());
        assert_eq!(stalled.inner.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_stop_discards_audio_queued_for_a_stalled_sink() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(GatedSink::stalled());
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        sleep(FRAME * 10).await;
        player.stop().expect("stop");
        assert_eq!(
            within(FRAME, "stop", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );

        // The connection recovers long after the stop.
        sleep(Duration::from_secs(10)).await;
        sink.release();
        sleep(FRAME * 10).await;

        // No audio of the stopped track, but the full silence tail.
        let frames = sink.inner.frames();
        assert!(sink.inner.audio_frames().is_empty(), "{frames:?}");
        assert_eq!(sink.inner.silence_frames(), 5);
        assert!(frames.iter().all(|frame| frame.timing.gap.is_zero()));
        assert_eq!(sink.inner.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_replacement_never_delivers_the_previous_tracks_audio() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(GatedSink::stalled());
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();

        // The first track is numbered from 1000, the second from 0.
        player
            .play(pcm_resource(numbered_pcm(1000..1500)))
            .expect("play first");
        sleep(FRAME * 10).await;
        player
            .play(pcm_resource(numbered_pcm(0..100)))
            .expect("play second");
        assert_eq!(
            within(FRAME, "replacement", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );
        sleep(FRAME * 5).await;
        sink.release();

        assert_eq!(
            within(
                FRAME * 120,
                "the second track",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        let delivered = sink.inner.audio_frames();
        assert!(!delivered.is_empty());
        assert!(delivered.iter().all(|index| *index < 1000), "{delivered:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn player_delivers_the_silence_tail_after_a_lagging_sink_recovers() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(GatedSink::stalled());
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..10)))
            .expect("play");
        assert_eq!(
            within(
                FRAME * 20,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        sleep(Duration::from_secs(1)).await;
        sink.release();
        sleep(FRAME * 10).await;

        // The audio expired while the connection was stalled, but the tail is
        // owed state, so it arrives in full, followed by speaking off.
        assert!(sink.inner.audio_frames().is_empty());
        assert_eq!(sink.inner.silence_frames(), 5);
        assert_eq!(sink.inner.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_expires_stale_frames_and_paces_catch_up_after_a_stall() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let lagging = Arc::new(GatedSink::stalled());
        let healthy = Arc::new(RecordingSink::default());
        let _lagging_subscription = subscribe(&player, lagging.clone());
        let _healthy_subscription = subscribe(&player, healthy.clone());

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        sleep(FRAME * 20).await;
        let current = *healthy.audio_frames().last().expect("healthy frames");
        let released_at = Instant::now();
        lagging.release();
        sleep(FRAME * 10).await;

        let audio = lagging.inner.audio();
        let first = audio
            .first()
            .and_then(SentFrame::index)
            .expect("frames after recovery");
        // Only frames within the lateness bound survived the stall.
        let tolerance =
            ((SINK_FRAME_TTL + SINK_SEND_TIMEOUT).as_millis() / FRAME.as_millis()) as i16;
        assert!(
            first >= current - tolerance,
            "first {first}, current {current}"
        );
        assert_eq!(audio[0].at, released_at);
        // The backlog is paced instead of burst out.
        for pair in audio.windows(2) {
            assert!(
                pair[1].at - pair[0].at >= SINK_CATCH_UP_SPACING,
                "{:?} then {:?}",
                pair[0],
                pair[1]
            );
        }
        assert_gaps_match_skipped_frames(&audio);
        assert_eq!(lagging.inner.speaking(), vec![true]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_reports_dropped_frames_as_rtp_gaps() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let slow = Arc::new(SlowSink {
            delay: Duration::from_millis(35),
            inner: RecordingSink::default(),
        });
        let _subscription = subscribe(&player, slow.clone());
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..50)))
            .expect("play");
        assert_eq!(
            within(
                FRAME * 70,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );

        let audio = slow.inner.audio();
        assert!(
            audio.len() > 10 && audio.len() < 50,
            "a sink slower than real time loses some frames: {}",
            audio.len()
        );
        assert_eq!(audio[0].timing.gap, Duration::ZERO);
        assert!(audio.iter().any(|frame| !frame.timing.gap.is_zero()));
        assert_gaps_match_skipped_frames(&audio);
    }

    #[tokio::test(start_paused = true)]
    async fn player_detaches_a_panicking_sink_and_keeps_playing() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let panicking = Arc::new(PanickingSink);
        let healthy = Arc::new(RecordingSink::default());
        let _panicking_subscription = subscribe(&player, panicking.clone());
        let _healthy_subscription = subscribe(&player, healthy.clone());
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
    async fn player_shutdown_waits_for_cleanup_of_a_stalled_source_and_sink() {
        let metrics = tokio::runtime::Handle::current().metrics();
        assert_eq!(metrics.num_alive_tasks(), 0);

        let player = test_player(NoSubscriberBehavior::Pause);
        let stalled_sink = Arc::new(GatedSink::stalled());
        let subscription = subscribe(&player, stalled_sink.clone());
        let mut state = player.state_watch();
        let mut events = player.events();

        // Three frames, then the source hangs; the sink never finishes a send.
        let (resource, mut dropped) = stalling_resource(numbered_pcm(0..3));
        player.play(resource).expect("play");
        wait_for_state(&mut state, FRAME * 5, "playing", is_playing).await;
        sleep(FRAME * 2).await;
        assert_eq!(
            metrics.num_alive_tasks(),
            3,
            "control task, source reader, and sink worker"
        );

        let started = Instant::now();
        within(SINK_CLOSE_GRACE + FRAME * 3, "shutdown", player.shutdown()).await;
        assert!(started.elapsed() <= SINK_CLOSE_GRACE + FRAME * 2);

        // Completion is observable: nothing is left running.
        assert_eq!(metrics.num_alive_tasks(), 0);
        assert_eq!(
            dropped.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        );
        assert_eq!(stalled_sink.inner.speaking(), vec![true, false]);
        assert_eq!(Arc::strong_count(&stalled_sink), 1);
        assert_eq!(
            wait_for_track_end(&mut events).await,
            TrackEndReason::Stopped
        );

        assert!(player.play(silent_pcm_resource(1)).is_err());
        within(FRAME, "a second shutdown", player.shutdown()).await;
        drop(subscription);
    }

    #[tokio::test(start_paused = true)]
    async fn player_shutdown_delivers_the_silence_tail_to_healthy_sinks() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let _subscription = subscribe(&player, sink.clone());
        let mut state = player.state_watch();

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        wait_for_state(&mut state, FRAME * 5, "playing", is_playing).await;
        sleep(FRAME * 5).await;

        within(SINK_CLOSE_GRACE, "shutdown", player.shutdown()).await;
        assert_eq!(sink.silence_frames(), 5);
        assert_eq!(sink.speaking(), vec![true, false]);
        assert_eq!(player.state(), AudioPlayerState::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn player_dropped_handles_clean_up_without_leaking_tasks() {
        let metrics = tokio::runtime::Handle::current().metrics();
        let player = test_player(NoSubscriberBehavior::Pause);
        let stalled_sink = Arc::new(GatedSink::stalled());
        let subscription = subscribe(&player, stalled_sink.clone());
        let mut state = player.state_watch();

        let (resource, mut dropped) = stalling_resource(numbered_pcm(0..3));
        player.play(resource).expect("play");
        wait_for_state(&mut state, FRAME * 5, "playing", is_playing).await;

        drop(player);
        drop(subscription);
        let _ = within(FRAME, "dropping the stalled input", &mut dropped).await;
        wait_until(
            SINK_CLOSE_GRACE + FRAME * 3,
            "every player task exiting",
            || metrics.num_alive_tasks() == 0,
        )
        .await;
        assert_eq!(Arc::strong_count(&stalled_sink), 1);
        assert_eq!(stalled_sink.inner.speaking(), vec![true, false]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_late_subscriber_gets_speaking_before_its_first_frame() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let early = Arc::new(RecordingSink::default());
        let late = Arc::new(RecordingSink::default());
        let _early_subscription = subscribe(&player, early.clone());
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..10)))
            .expect("play");
        wait_until(FRAME * 10, "three frames playing", || {
            early.audio_frames().len() >= 3
        })
        .await;
        let _late_subscription = subscribe(&player, late.clone());

        assert_eq!(
            within(
                FRAME * 20,
                "the track finishing",
                wait_for_track_end(&mut events)
            )
            .await,
            TrackEndReason::Finished
        );
        sleep(FRAME * 10).await;
        assert_eq!(early.audio_frames(), (0..10).collect::<Vec<i16>>());
        let joined = late.audio_frames();
        assert!(!joined.is_empty() && joined[0] >= 3, "{joined:?}");
        assert_eq!(late.audio()[0].timing.gap, Duration::ZERO);
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
        let subscription = subscribe(&player, sink.clone());
        let resource = silent_pcm_resource(500).with_title("long silence");
        assert!(format!("{resource:?}").contains("long silence"));
        player.play(resource).expect("play");
        wait_for_state(&mut state, FRAME * 5, "playing", is_playing).await;

        player.pause().expect("pause");
        wait_for_state(&mut state, FRAME, "pause", |state| {
            *state == AudioPlayerState::Paused
        })
        .await;
        subscription.unsubscribe();
        player.unpause().expect("unpause");
        wait_for_state(&mut state, FRAME, "auto-pause", |state| {
            *state == AudioPlayerState::AutoPaused
        })
        .await;
        assert!(format!("{player:?}").contains("AutoPaused"));
    }

    #[tokio::test(start_paused = true)]
    async fn player_keeps_sinks_that_return_errors_attached() {
        // With `Stop`, detaching the only sink would end the track as Stopped.
        let player = test_player(NoSubscriberBehavior::Stop);
        let _subscription = subscribe(&player, Arc::new(ErroringSink));
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
        within(SINK_CLOSE_GRACE, "shutdown", player.shutdown()).await;
    }

    #[tokio::test(start_paused = true)]
    async fn player_never_transmits_a_send_superseded_by_stop() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(WitnessSink {
            gate: Semaphore::new(0),
            attempts: AtomicUsize::new(0),
            player_state: player.state_watch(),
            transmitted: Mutex::new(Vec::new()),
        });
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        wait_until(FRAME * 10, "a send in progress", || {
            sink.attempts.load(Ordering::SeqCst) > 0
        })
        .await;
        // The pending send becomes ready in the same instant the stop lands:
        // the control task runs first and supersedes it, then the
        // connection's task wakes with the send ready to complete.
        player.stop().expect("stop");
        sink.gate.add_permits(1);
        assert_eq!(
            within(FRAME, "stop", wait_for_track_end(&mut events)).await,
            TrackEndReason::Stopped
        );
        sink.gate.add_permits(100);
        sleep(FRAME * 10).await;

        let transmitted = sink
            .transmitted
            .lock()
            .expect("witness mutex poisoned")
            .clone();
        assert!(
            transmitted
                .iter()
                .all(|(_, state)| *state != AudioPlayerState::Idle),
            "audio transmitted after the track ended: {transmitted:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn player_keeps_the_gap_reference_when_unsent_silence_is_superseded() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(SilenceStallingSink::default());
        let _subscription = subscribe(&player, sink.clone());
        let mut events = player.events();
        let mut state = player.state_watch();
        let (mut writer, reader) = tokio::io::duplex(PLAYER_FRAME_BYTES * 8);

        writer
            .write_all(&numbered_pcm(0..4))
            .await
            .expect("write first frames");
        player
            .play(AudioResource::new(AudioInput::raw_pcm(reader)))
            .expect("play");
        wait_until(FRAME * 20, "the first four frames", || {
            sink.inner.audio_frames().len() == 4
        })
        .await;
        // Three empty ticks: the player rebuffers and the first silence
        // frame of the tail is pending, never transmitted.
        wait_for_state(&mut state, FRAME * 5, "rebuffering", |state| {
            *state == AudioPlayerState::Buffering
        })
        .await;
        sleep(FRAME * 3).await;

        // New audio supersedes the pending silence frame.
        writer
            .write_all(&numbered_pcm(4..8))
            .await
            .expect("write more frames");
        drop(writer);
        within(
            FRAME * 20,
            "the track finishing",
            wait_for_track_end(&mut events),
        )
        .await;

        assert_eq!(sink.inner.silence_frames(), 0);
        let audio = sink.inner.audio();
        assert_eq!(sink.inner.audio_frames(), (0..8).collect::<Vec<i16>>());
        // Nothing went out between frames 3 and 4, so frame 4 still counts
        // from frame 3: the three empty ticks are its gap.
        assert_eq!(audio[4].timing.gap, FRAME * 3);
        assert_eq!(sink.inner.speaking(), vec![true]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_retries_failed_silence_and_gives_up_on_a_failing_connection() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let flaky = Arc::new(FlakySilenceSink::failing(2));
        let broken = Arc::new(FlakySilenceSink::failing(usize::MAX));
        let _flaky_subscription = subscribe(&player, flaky.clone());
        let _broken_subscription = subscribe(&player, broken.clone());
        let mut events = player.events();

        player.play(silent_pcm_resource(3)).expect("play");
        within(
            FRAME * 10,
            "the track finishing",
            wait_for_track_end(&mut events),
        )
        .await;
        sleep(FRAME * 20).await;

        // Failed silence frames are not counted: the full tail still arrives.
        assert_eq!(flaky.silence_attempts.load(Ordering::SeqCst), 7);
        assert_eq!(flaky.inner.silence_frames(), 5);
        assert_eq!(flaky.inner.speaking(), vec![true, false]);
        // A connection that keeps failing gives up after five attempts and
        // still gets speaking off.
        assert_eq!(broken.silence_attempts.load(Ordering::SeqCst), 5);
        assert_eq!(broken.inner.silence_frames(), 0);
        assert_eq!(broken.inner.speaking(), vec![true, false]);
    }

    /// Asserts a connection saw: audio, then exactly one silence tail, then
    /// audio again, with strictly increasing frame indices throughout.
    fn assert_single_handover(frames: &[SentFrame]) {
        let indices: Vec<i16> = frames.iter().filter_map(SentFrame::index).collect();
        assert!(
            indices.windows(2).all(|pair| pair[1] > pair[0]),
            "duplicated or reordered audio: {indices:?}"
        );
        let first_silence = frames
            .iter()
            .position(|frame| frame.payload == OPUS_SILENCE_FRAME)
            .expect("a silence tail");
        let tail = &frames[first_silence..first_silence + 5];
        assert!(tail.iter().all(|frame| frame.payload == OPUS_SILENCE_FRAME));
        assert!(
            frames[first_silence + 5..]
                .iter()
                .all(|frame| frame.index().is_some()),
            "silence interleaved with the new subscription's audio"
        );
        assert!(frames.len() > first_silence + 5, "audio resumed");
    }

    #[tokio::test(start_paused = true)]
    async fn player_resubscribing_a_connection_hands_it_over_without_interleaving() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let first = subscribe(&player, sink.clone());

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        wait_until(FRAME * 10, "audio playing", || {
            sink.audio_frames().len() >= 5
        })
        .await;
        first.unsubscribe();
        let _second = subscribe(&player, sink.clone());
        sleep(FRAME * 20).await;

        assert_single_handover(&sink.frames());
        // The retiring task finished (speaking off) before the new one began.
        assert_eq!(sink.speaking(), vec![true, false, true]);
    }

    #[tokio::test(start_paused = true)]
    async fn player_subscribing_a_connection_twice_replaces_the_earlier_subscription() {
        let player = test_player(NoSubscriberBehavior::Pause);
        let sink = Arc::new(RecordingSink::default());
        let first = subscribe(&player, sink.clone());

        player
            .play(pcm_resource(numbered_pcm(0..500)))
            .expect("play");
        wait_until(FRAME * 10, "audio playing", || {
            sink.audio_frames().len() >= 5
        })
        .await;
        let second = subscribe(&player, sink.clone());
        sleep(FRAME * 20).await;
        assert_single_handover(&sink.frames());

        // The replaced subscription no longer controls the connection.
        let before = sink.audio_frames().len();
        drop(first);
        sleep(FRAME * 5).await;
        assert!(sink.audio_frames().len() > before);
        assert_eq!(sink.speaking(), vec![true, false, true]);

        drop(second);
        sleep(FRAME * 10).await;
        assert_eq!(sink.speaking(), vec![true, false, true, false]);
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
        let _subscription = subscribe(&player, sink.clone());
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
        sleep(FRAME).await;

        let encoded: Vec<_> = sink
            .frames()
            .into_iter()
            .filter(|frame| frame.payload != OPUS_SILENCE_FRAME)
            .collect();
        assert_eq!(encoded.len(), 2);
        assert!(encoded.iter().all(|frame| !frame.payload.is_empty()));
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
