# 음성(Voice)

음성은 선택 기능입니다. `voice`를 켜면 음성 게이트웨이·UDP 런타임, `AudioPlayer` 재생 파이프라인,
전송 암호화, Opus PCM 디코딩, 그리고 DAVE 종단 간 암호화가 함께 들어옵니다. 영문 원문은
[Voice](../../docs/api/voice.md)이고, 이 페이지는 연결·재생·수신과 DAVE를 다룹니다.

## 기능 켜기

```toml
[dependencies]
# 음성 런타임 + DAVE 종단 간 암호화
discordrs = { version = "3.0.0", features = ["voice"] }

# PCM -> Opus 인코딩(AudioPlayer::play에 필요)
discordrs = { version = "3.0.0", features = ["voice", "voice-encode"] }
```

`dave` 기능은 `voice`의 별칭으로 남아 있어서 `features = ["voice", "dave"]`도 그대로 빌드됩니다.
DAVE가 선택 사항이 아니게 된 이유는 아래 [DAVE](#dave-종단-간-암호화)에 있습니다.

## 연결 흐름

1. `ctx.join_voice(guild_id, channel_id, self_mute, self_deaf)`로 음성 상태를 보냅니다.
2. 봇 자신의 VOICE_STATE_UPDATE(`session_id`)와 VOICE_SERVER_UPDATE(`token`, `endpoint`)를 기다립니다.
   순서는 정해져 있지 않습니다.
3. `connect_voice_runtime(config)`로 연결합니다. **`VoiceRuntimeConfig::channel_id(..)`로 음성 채널을
   꼭 넣습니다.** DAVE가 채널을 MLS 그룹 id로 쓰기 때문입니다. `Context::connect_voice_runtime`과
   `VoiceManager::runtime_config`는 자동으로 넣습니다.
4. `AudioPlayer`로 재생하거나 Opus 프레임을 직접 보내고, `recv_voice_packet*`로 받습니다.

```rust
use std::sync::Arc;

use discordrs::voice::player::{
    AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
};
use discordrs::{connect_voice_runtime, ChannelId, VoiceRuntimeConfig};

async fn play(
    config: VoiceRuntimeConfig,
    channel_id: ChannelId,
    input: &str,
) -> Result<(), discordrs::DiscordError> {
    // 채널 id가 있어야 DAVE를 광고하고 MLS 그룹에 들어갑니다.
    let handle = Arc::new(connect_voice_runtime(config.channel_id(channel_id)).await?);

    let player = AudioPlayer::new(AudioPlayerOptions::default());
    let subscription = player.subscribe_runtime(Arc::clone(&handle))?;
    let mut events = player.events();
    // DAVE 키가 오기 전에는 AutoPaused로 기다렸다가 트랙 처음부터 재생합니다.
    player.play(AudioResource::new(AudioInput::ffmpeg(input)?))?;

    while let Ok(event) = events.recv().await {
        if let AudioPlayerEvent::TrackEnd { reason } = event {
            println!("트랙 종료: {reason:?}");
            break;
        }
    }
    player.shutdown().await;
    drop(subscription);
    match Arc::try_unwrap(handle) {
        Ok(handle) => handle.close().await,
        Err(_) => Ok(()),
    }
}
```

전체 예제는 `examples/music_bot.rs`입니다
(`cargo run --example music_bot --features gateway,voice,voice-encode`, `ffmpeg` 필요).

## AudioPlayer와 재생 가능성

- 구독한 연결은 *재생 가능(playable)* 할 때만 구독으로 셉니다(`VoiceFrameSink::is_playable`,
  discord.js `AudioPlayer.playable`과 같은 의미). DAVE 통화의 `VoiceRuntimeHandle`은 MLS 그룹 키를
  받은 뒤부터 재생 가능합니다. 재생 가능한 연결이 없으면 `NoSubscriberBehavior`를 적용합니다. 기본값
  `Pause`는 `AutoPaused`로 기다렸다가 트랙의 첫 프레임부터 재생합니다. DAVE 채널에 혼자 있으면 누군가
  들어올 때 그룹이 생깁니다.
- `NoSubscriberBehavior::Stop`을 쓰면 `play` 전에 `handle.wait_until_playable()`을 기다립니다.
  그렇지 않으면 키가 오기 전에 트랙이 멈춥니다.
- `player.connection_events()`는 연결로 보내기가 실패하기 시작할 때
  `PlayerConnectionEvent::SendFailed { subscription, error }`를, 다시 될 때
  `Recovered { subscription, failed }`를 한 번씩 알립니다(프레임마다가 아님, 로그도 남김).
  `PlayerSubscription::id()`가 `subscription`과 같습니다. 보내지 못한 프레임은 여전히 RTP 타임스탬프
  간격이 됩니다.
- 실시간 규칙(60 ms 안에 전송 시작, 60 ms 정체 시 취소, 최대 120 ms 지연, 큐가 차면 가장 오래된
  프레임 버림, 멈출 때 무음 5프레임과 speaking off)은 영문 문서의 "Real-Time Delivery"와 같습니다.
- 직접 만든 `VoiceFrameSink`의 `is_playable()`(기본 `true`)은 재생기 제어 태스크가 20 ms마다 부르므로
  가볍고 막히지 않아야 합니다.

## 수신

```rust
use discordrs::{VoiceOpusDecoder, VoiceRuntimeHandle};

async fn receive(handle: &VoiceRuntimeHandle) -> Result<(), discordrs::DiscordError> {
    let mut decoder = VoiceOpusDecoder::discord_default()?;
    loop {
        // 전송 암호화와 (DAVE 통화라면) 화자별 DAVE 암호화를 풀어 줍니다.
        let decoded = handle.recv_decoded_voice_packet(&mut decoder, 2048).await?;
        println!(
            "SSRC {} 사용자 {:?}: 채널당 {} 샘플",
            decoded.packet.rtp.ssrc, decoded.packet.user_id, decoded.samples_per_channel
        );
    }
}
```

화자는 SPEAKING이 알려 준 SSRC → 사용자 매핑으로 찾습니다. 여러 화자를 받을 때는 SSRC마다 디코더를
따로 두는 편이 맞습니다.

## DAVE 종단 간 암호화

Discord는 2026-03-01부터 Stage가 아닌 모든 음성 채널에 [DAVE](https://daveprotocol.com/)(MLS 기반
종단 간 암호화)를 요구하고, 지원하지 않는 클라이언트를 닫힘 코드 4017로 거절합니다. 음성 런타임은
@discordjs/voice·Songbird처럼 DAVE를 직접 운영합니다.

- **참여.** 채널 id가 있으면 IDENTIFY에 `max_dave_protocol_version` 1을 광고합니다. SESSION_DESCRIPTION이
  DAVE를 정하면 MLS 키 패키지를 보내고, 게이트웨이의 외부 발신자를 적용하고, 새로 들어온 멤버의 제안을
  `CLIENTS_CONNECT`와 대조해 커밋하고, 커밋과 웰컴을 처리하고, `transition_ready`를 알린 뒤
  `execute_transition`에서 키를 바꿉니다. 핸드셰이크 중에 온 메시지(`CLIENTS_CONNECT`, 외부 발신자)는
  버리지 않고 세션이 시작될 때 처리합니다.
- **송신.** `send_opus_frame(...)`, `send_opus_frame_after_gap(...)`, `play_opus_frames(...)`, 그리고
  `subscribe_runtime`으로 구독한 `AudioPlayer`는 Opus 프레임을 DAVE로, 이어서 전송 암호화로 감쌉니다.
  Opus 무음 프레임은 프로토콜이 허용하는 대로 DAVE 없이 나갑니다. 그룹 키가 없을 때는 오디오를 평문으로
  내보내는 대신 이유를 설명하는 `DiscordError::Voice`를 돌려주고 아무것도 소비하지 않습니다(순번,
  타임스탬프, nonce). 그동안 `is_playable()`은 `false`입니다.
- **수신.** `recv_voice_packet(...)`과 `recv_decoded_voice_packet(...)`이 화자의 키로 복호화합니다. MLS
  전환 전후에는 몇 개가 늘 풀리지 않으므로, 그런 패킷과 아직 사용자를 모르는 SSRC의 DAVE 프레임은
  오류로 돌려주지 않고 건너뜁니다(`?`를 쓰는 수신 루프가 끝나지 않도록).
  `VoiceDaveStatus::undecryptable_packets`가 그 수를 셉니다. 음소거 사용자에 대해 SFU가 DAVE 없이 보내는
  무음 프레임(`F8 FF FE`)은 그대로 돌려줍니다.
- **전환.** 버전 0으로 내려가는 전환(DAVE를 지원하지 않는 클라이언트가 들어옴)은 준비되자마자 평문을
  받아들이고 실행된 뒤 평문을 보냅니다. 다시 올라가는 전환(epoch 1의 `prepare_epoch`)은 새 그룹을 만들고,
  암호화할 수 있을 때까지 평문을 계속 보냅니다. 잘못된 커밋·웰컴은 `dave_mls_invalid_commit_welcome`으로
  알리고 새 키 패키지로 다시 들어갑니다. 전환이 없는데 복호화가 36번 넘게 연속 실패해도 같은 복구를
  합니다.
- **채널 id가 없으면** MLS 그룹을 만들 수 없어 버전 0을 광고합니다(경고 로그). Stage 채널은 동작하고,
  그 밖에서는 Discord가 4017로 닫으며 `connect_voice_runtime`이 빠진 설정을 알려 주는 오류를 돌려줍니다.

```rust
use discordrs::VoiceRuntimeHandle;

fn describe(handle: &VoiceRuntimeHandle) {
    let dave = handle.dave_status();
    if dave.protocol_version == 0 {
        println!("전송 암호화만 사용(Stage 채널 또는 다운그레이드)");
    } else if dave.can_send_audio() {
        println!("종단 간 암호화, epoch {:?}, 개인정보 코드 {:?}", dave.epoch, dave.voice_privacy_code);
    } else {
        println!("MLS 그룹 대기 중, 마지막 오류: {:?}", dave.last_error);
    }
}
```

`subscribe_dave_status()`는 같은 상태의 watch 수신기를, `dave_verification_code(user_id)`는 Discord
클라이언트가 멤버를 확인할 때 보여 주는 쌍별 코드를 돌려줍니다.

**직접 DAVE를 다룰 때.** `send_opus_frame_with_dave(...)`/`recv_voice_packet_with_dave(...)`에
`VoiceDaveFrameEncryptor`/`VoiceDaveFrameDecryptor`를 넘기는 방식은 그대로 동작합니다. 다만 런타임이 이미
세션을 운영하는 연결에서 `VoiceDaveySession`과 `send_dave_*` 명령으로 MLS를 손수 진행하는 것은 지원하지
않습니다. 게이트웨이가 한 연결에서 키 패키지를 둘 받게 됩니다. `VoiceRuntimeState::dave`
(`VoiceDaveState`)는 진단용으로 원시 DAVE 메시지를 계속 담습니다.

**3.0.0에서 올릴 때.** 3.0.0은 DAVE를 광고만 하고 운영하지 않아서, DAVE 채널에서 `AudioPlayer`는 아무
소리도 내지 못했고(프레임마다 경고만 남김) 일반 송수신 메서드는 오류를 냈습니다. 설정을 직접 만든다면
`VoiceRuntimeConfig::channel_id(..)`를 넣습니다. `VoiceRuntimeConfig`의 모든 필드를 구조체 리터럴로 적은
코드는 `channel_id`를 더해야 합니다. `VoiceRuntimeConfig::new(..)`와 `..VoiceRuntimeConfig::new(..)`는
영향이 없습니다.

**검증 상태.** 독립된 두 `davey` 세션과 테스트용 음성 게이트웨이(OpenMLS 외부 발신자)로 웰컴 참여, 혼자인
멤버의 커밋, 양방향 오디오(실제 Opus를 쓰는 `AudioPlayer` 포함), 다운그레이드·업그레이드, 잘못된
커밋·웰컴 복구, 4017 닫힘을 검사합니다. 런타임이 운영하는 이 경로는 **아직 실제 Discord에서 실행하지
않았습니다**. 무시된 라이브 테스트 `tests/live_voice_dave.rs`가 기준입니다. 절차는
[Live DAVE Validation](../../docs/api/live-dave-validation.md)(영문)에 있습니다.
