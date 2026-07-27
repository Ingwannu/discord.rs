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
//! Opus encoding requires the `voice-encode` feature. Without it the player
//! still drives the full state machine (useful for tests and dry runs) and
//! still transmits the Opus silence frames Discord recommends, but PCM frames
//! are not encoded or transmitted.

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
use tokio::sync::{broadcast, mpsc, watch};
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

fn voice_error(message: impl Into<String>) -> DiscordError {
    DiscordError::voice(message)
}

/// Converts interleaved s16le PCM bytes to `f32` samples, applying `volume`.
///
/// Samples are scaled to `[-1.0, 1.0]` (dividing by 32768), multiplied by
/// `volume`, and clamped back into `[-1.0, 1.0]`. A trailing odd byte, if
/// any, is ignored.
pub fn pcm_s16le_to_f32(bytes: &[u8], volume: f32) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
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
        self.bits.store(volume.max(0.0).to_bits(), Ordering::Relaxed);
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
    pub async fn next_pcm_frame(&mut self) -> Result<Option<Vec<f32>>, DiscordError> {
        if self.ended {
            return Ok(None);
        }

        let mut buffer = vec![0_u8; PLAYER_FRAME_BYTES];
        let mut filled = 0_usize;
        while filled < PLAYER_FRAME_BYTES {
            let read = self
                .input
                .reader
                .read(&mut buffer[filled..])
                .await
                .map_err(|error| voice_error(format!("failed to read PCM audio input: {error}")))?;
            if read == 0 {
                self.ended = true;
                break;
            }
            filled += read;
        }

        if filled == 0 {
            return Ok(None);
        }
        if filled < PLAYER_FRAME_BYTES {
            buffer[filled..].fill(0);
        }

        Ok(Some(pcm_s16le_to_f32(&buffer, self.volume.get())))
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
pub trait VoiceFrameSink: Send + Sync {
    /// Sends one raw Opus payload of the given duration.
    fn send_opus_packet<'a>(&'a self, opus_frame: &'a [u8], duration: Duration)
        -> VoiceSinkFuture<'a>;

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

enum PlayerCommand {
    Play(Box<AudioResource>),
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
/// The player runs a background task ticking every 20 ms
/// ([`MissedTickBehavior::Delay`]): it reads PCM frames from the current
/// [`AudioResource`], encodes them to Opus (with the `voice-encode` feature),
/// and transmits them to every subscribed voice connection. Five Opus silence
/// frames are transmitted whenever playback pauses or ends, per Discord's
/// recommendation, and speaking flags are toggled automatically.
///
/// Handles are cheap to clone; all clones control the same player. The
/// background task stops once every handle and subscription is dropped.
#[derive(Clone)]
pub struct AudioPlayer {
    command_tx: mpsc::UnboundedSender<PlayerCommand>,
    state_rx: watch::Receiver<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    next_subscriber_id: Arc<AtomicU64>,
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
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (state_tx, state_rx) = watch::channel(AudioPlayerState::Idle);
        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let task = PlayerTask {
            options,
            subscribers: HashMap::new(),
            state_tx,
            events_tx: events_tx.clone(),
            resource: None,
            #[cfg(feature = "voice-encode")]
            encoder: None,
            track_started: false,
            silence_remaining: 0,
        };
        tokio::spawn(task.run(command_rx));
        tracing::debug!("audio player task started");

        Self {
            command_tx,
            state_rx,
            events_tx,
            next_subscriber_id: Arc::new(AtomicU64::new(0)),
        }
    }

    fn send_command(&self, command: PlayerCommand) -> Result<(), DiscordError> {
        self.command_tx
            .send(command)
            .map_err(|_| voice_error("audio player task has stopped"))
    }

    /// Starts playing `resource`, replacing (and stopping) any current one.
    pub fn play(&self, resource: AudioResource) -> Result<(), DiscordError> {
        self.send_command(PlayerCommand::Play(Box::new(resource)))
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

struct PlayerTask {
    options: AudioPlayerOptions,
    subscribers: HashMap<u64, Arc<dyn VoiceFrameSink>>,
    state_tx: watch::Sender<AudioPlayerState>,
    events_tx: broadcast::Sender<AudioPlayerEvent>,
    resource: Option<AudioResource>,
    #[cfg(feature = "voice-encode")]
    encoder: Option<VoiceOpusEncoder>,
    track_started: bool,
    silence_remaining: u8,
}

impl PlayerTask {
    async fn run(mut self, mut command_rx: mpsc::UnboundedReceiver<PlayerCommand>) {
        let mut ticker = interval(PLAYER_FRAME_DURATION);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = command_rx.recv() => match command {
                    Some(command) => self.handle_command(command).await,
                    None => break,
                },
                _ = ticker.tick() => self.handle_tick().await,
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

    fn set_speaking_all(&self, speaking: bool) {
        for sink in self.subscribers.values() {
            if let Err(error) = sink.update_speaking(speaking) {
                tracing::warn!(%error, "failed to update speaking state on subscribed connection");
            }
        }
    }

    /// Associated function (not a method) so the future only borrows the
    /// subscriber map, keeping the task future `Send` even though audio
    /// readers are not required to be `Sync`.
    async fn send_to_all(subscribers: &HashMap<u64, Arc<dyn VoiceFrameSink>>, opus_frame: &[u8]) {
        for sink in subscribers.values() {
            if let Err(error) = sink
                .send_opus_packet(opus_frame, PLAYER_FRAME_DURATION)
                .await
            {
                tracing::warn!(%error, "failed to send opus frame to subscribed connection");
            }
        }
    }

    fn begin_silence(&mut self) {
        self.silence_remaining = SILENCE_FRAME_COUNT;
        self.set_speaking_all(false);
    }

    fn drop_resource(&mut self) {
        self.resource = None;
        self.track_started = false;
        #[cfg(feature = "voice-encode")]
        {
            self.encoder = None;
        }
    }

    async fn finish_playback(&mut self, reason: TrackEndReason) {
        tracing::info!(?reason, "audio player track ended");
        self.drop_resource();
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
        if self.track_started {
            self.set_state(AudioPlayerState::Playing {
                since: Instant::now(),
            });
            self.set_speaking_all(true);
        } else {
            self.set_state(AudioPlayerState::Buffering);
        }
    }

    async fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::Play(resource) => {
                if self.resource.is_some() {
                    self.drop_resource();
                    self.emit(AudioPlayerEvent::TrackEnd {
                        reason: TrackEndReason::Stopped,
                    });
                }
                tracing::info!(title = ?resource.title(), "audio player playing new resource");
                self.resource = Some(*resource);
                self.track_started = false;
                self.silence_remaining = 0;
                self.set_state(AudioPlayerState::Buffering);
            }
            PlayerCommand::Pause => {
                if self.resource.is_some()
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
                if self.resource.is_some() && self.current_state() == AudioPlayerState::Paused {
                    self.resume_playback();
                }
            }
            PlayerCommand::Stop => {
                if self.resource.is_some() {
                    self.finish_playback(TrackEndReason::Stopped).await;
                }
            }
            PlayerCommand::Subscribe { id, sink } => {
                if matches!(self.current_state(), AudioPlayerState::Playing { .. }) {
                    if let Err(error) = sink.update_speaking(true) {
                        tracing::warn!(%error, "failed to set speaking on new subscription");
                    }
                }
                self.subscribers.insert(id, sink);
                tracing::debug!(subscribers = self.subscribers.len(), "connection subscribed");
                if self.current_state() == AudioPlayerState::AutoPaused {
                    self.resume_playback();
                }
            }
            PlayerCommand::Unsubscribe { id } => {
                if self.subscribers.remove(&id).is_some() {
                    tracing::debug!(
                        subscribers = self.subscribers.len(),
                        "connection unsubscribed"
                    );
                }
            }
        }
    }

    async fn handle_tick(&mut self) {
        match self.current_state() {
            AudioPlayerState::Buffering | AudioPlayerState::Playing { .. } => {
                if self.resource.is_none() {
                    self.set_state(AudioPlayerState::Idle);
                    return;
                }
                if self.subscribers.is_empty() {
                    match self.options.no_subscriber_behavior {
                        NoSubscriberBehavior::Pause => {
                            self.begin_silence();
                            self.set_state(AudioPlayerState::AutoPaused);
                            return;
                        }
                        NoSubscriberBehavior::Stop => {
                            self.finish_playback(TrackEndReason::Stopped).await;
                            return;
                        }
                        NoSubscriberBehavior::Play => {}
                    }
                }
                self.play_next_frame().await;
            }
            AudioPlayerState::AutoPaused => {
                if self.subscribers.is_empty() {
                    self.send_silence_step().await;
                } else {
                    self.resume_playback();
                }
            }
            AudioPlayerState::Paused | AudioPlayerState::Idle => {
                self.send_silence_step().await;
            }
        }
    }

    async fn send_silence_step(&mut self) {
        if self.silence_remaining == 0 {
            return;
        }
        self.silence_remaining -= 1;
        Self::send_to_all(&self.subscribers, &OPUS_SILENCE_FRAME).await;
    }

    async fn play_next_frame(&mut self) {
        let frame = match self.resource.as_mut() {
            Some(resource) => resource.next_pcm_frame().await,
            None => return,
        };

        match frame {
            Ok(Some(pcm)) => {
                if !self.track_started {
                    self.track_started = true;
                    self.set_state(AudioPlayerState::Playing {
                        since: Instant::now(),
                    });
                    self.set_speaking_all(true);
                    self.emit(AudioPlayerEvent::TrackStart);
                }
                self.transmit_pcm_frame(&pcm).await;
            }
            Ok(None) => self.finish_playback(TrackEndReason::Finished).await,
            Err(error) => {
                tracing::warn!(%error, "audio resource failed while reading");
                self.finish_playback(TrackEndReason::Error(error.to_string()))
                    .await;
            }
        }
    }

    #[cfg(feature = "voice-encode")]
    async fn transmit_pcm_frame(&mut self, pcm: &[f32]) {
        match self.encode_frame(pcm) {
            Ok(opus) => Self::send_to_all(&self.subscribers, &opus).await,
            Err(error) => {
                tracing::warn!(%error, "failed to encode PCM frame to Opus");
                self.finish_playback(TrackEndReason::Error(error.to_string()))
                    .await;
            }
        }
    }

    #[cfg(not(feature = "voice-encode"))]
    async fn transmit_pcm_frame(&mut self, _pcm: &[f32]) {
        tracing::trace!("voice-encode feature disabled; PCM frame not transmitted");
    }

    #[cfg(feature = "voice-encode")]
    fn encode_frame(&mut self, pcm: &[f32]) -> Result<Vec<u8>, DiscordError> {
        let encoder = match self.encoder.as_mut() {
            Some(encoder) => encoder,
            None => self.encoder.insert(VoiceOpusEncoder::discord_music()?),
        };
        let frame = PcmFrame::discord_stereo_20ms(pcm.to_vec())?;
        Ok(encoder.encode_pcm_frame(&frame)?.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    struct FailingReader;

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            _buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("synthetic read failure")))
        }
    }

    fn silent_pcm_resource(frames: usize) -> AudioResource {
        let bytes = vec![0_u8; PLAYER_FRAME_BYTES * frames];
        AudioResource::new(AudioInput::raw_pcm(std::io::Cursor::new(bytes)))
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
        let mut resource = AudioResource::new(AudioInput::raw_pcm(std::io::Cursor::new(bytes)));

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

    #[tokio::test]
    async fn audio_resource_volume_control_applies_at_read_time() {
        let bytes: Vec<u8> = std::iter::repeat_n(16_384_i16.to_le_bytes(), PLAYER_FRAME_SAMPLES * 2)
            .flatten()
            .collect();
        let mut resource =
            AudioResource::new(AudioInput::raw_pcm(std::io::Cursor::new(bytes))).with_volume(0.5);
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
        let path = std::env::temp_dir().join(format!(
            "discordrs-player-test-{}.pcm",
            std::process::id()
        ));
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

    #[tokio::test]
    async fn player_play_emits_track_start_then_finished_and_flushes_silence() {
        let player = AudioPlayer::new(AudioPlayerOptions::default());
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
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(sink.silence_frames(), 5);

        // Speaking toggled on at TrackStart and off at the end.
        let speaking = sink.speaking();
        assert_eq!(speaking.first(), Some(&true));
        assert_eq!(speaking.last(), Some(&false));

        #[cfg(feature = "voice-encode")]
        {
            let encoded: Vec<_> = sink
                .frames()
                .into_iter()
                .filter(|frame| frame.as_slice() != OPUS_SILENCE_FRAME)
                .collect();
            assert_eq!(encoded.len(), 3);
            assert!(encoded.iter().all(|frame| !frame.is_empty()));
        }

        drop(subscription);
    }

    #[tokio::test]
    async fn player_pause_unpause_and_stop_transitions() {
        let player = AudioPlayer::new(AudioPlayerOptions::default());
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
        tokio::time::sleep(Duration::from_millis(200)).await;
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

    #[tokio::test]
    async fn player_auto_pauses_without_subscribers_and_resumes_on_subscribe() {
        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let mut events = player.events();
        let mut state = player.state_watch();

        player.play(silent_pcm_resource(3)).expect("play");
        while *state.borrow() != AudioPlayerState::AutoPaused {
            state.changed().await.expect("state watch closed");
        }

        // Subscribing resumes playback and the track then finishes.
        let sink = Arc::new(RecordingSink::default());
        let _subscription = player
            .subscribe(sink as Arc<dyn VoiceFrameSink>)
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
    }

    #[tokio::test]
    async fn player_no_subscriber_play_behavior_keeps_consuming() {
        let player = AudioPlayer::new(AudioPlayerOptions {
            no_subscriber_behavior: NoSubscriberBehavior::Play,
        });
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

    #[tokio::test]
    async fn player_reports_read_errors_as_track_end_error() {
        let player = AudioPlayer::new(AudioPlayerOptions {
            no_subscriber_behavior: NoSubscriberBehavior::Play,
        });
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
    #[tokio::test]
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
        let resource = AudioResource::new(AudioInput::raw_pcm(std::io::Cursor::new(bytes)));

        player.play(resource).expect("play");
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
