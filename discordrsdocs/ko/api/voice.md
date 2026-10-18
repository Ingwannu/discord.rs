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

1. `ctx.connect_voice(VoiceConnectionConfig::new(guild_id, channel_id))`로 연결을 추적하게 한 뒤
   `ctx.join_voice(guild_id, channel_id, self_mute, self_deaf)`로 음성 상태를 보냅니다(먼저 추적해야
   VOICE_SERVER_UPDATE가 봇의 VOICE_STATE_UPDATE보다 먼저 와도 남습니다).
2. 봇 자신의 VOICE_STATE_UPDATE(`session_id`)와 VOICE_SERVER_UPDATE(`token`, `endpoint`)를 기다립니다.
   순서는 정해져 있지 않으며, 둘 다 오면 `ctx.voice_runtime_config(..)`가 `Some`이 됩니다. 컨텍스트는
   READY의 봇 사용자 것만 추적하므로 다른 멤버가 들어오거나 옮기거나 나가도 바뀌지 않습니다.
3. **`connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))`로 연결합니다.**
   그러면 런타임이 DAVE를 직접 운영합니다. DAVE가 채널을 MLS 그룹 id로 쓰기 때문에 채널이 필요합니다.
   게이트웨이 봇에서는 `Context::connect_voice_runtime_with_dave(guild_id, user_id)`가 추적한 음성 상태로 이렇게 연결하고,
   `VoiceManager::runtime_options(guild_id)`가 옵션을 만들어 줍니다. 3.0.0의
   `Context::connect_voice_runtime`은 그대로입니다. 옵션 없는 `connect_voice_runtime(config)`은 3.0.0과 같이 DAVE를 호출자에게
   맡깁니다(아래 참고).
4. `AudioPlayer`로 재생하거나 Opus 프레임을 직접 보내고, `recv_voice_packet*`로 받습니다.

```rust
use std::sync::Arc;

use discordrs::voice::player::{
    AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
};
use discordrs::{
    connect_voice_runtime_with_options, ChannelId, VoiceConnectOptions, VoiceRuntimeConfig,
};

async fn play(
    config: VoiceRuntimeConfig,
    channel_id: ChannelId,
    input: &str,
) -> Result<(), discordrs::DiscordError> {
    // 채널을 넘겨야 런타임이 DAVE를 운영하고 MLS 그룹에 들어갑니다.
    let options = VoiceConnectOptions::new(channel_id);
    let handle = Arc::new(connect_voice_runtime_with_options(config, options).await?);

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
종단 간 암호화)를 요구하고, 지원하지 않는 클라이언트를 닫힘 코드 4017로 거절합니다.
`connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))`로 연결한 런타임은
(`Context::connect_voice_runtime_with_dave`가 이렇게 연결) @discordjs/voice·Songbird처럼 DAVE를 직접 운영합니다.
음성 게이트웨이는 세션이 어느 채널인지 알려 주지 않으므로 채널(MLS 그룹)을 옵션으로 받습니다. 옵션 없는
`connect_voice_runtime(config)`과 `Context::connect_voice_runtime`은 이 절 끝에 적은 3.0.0 동작을 그대로
유지합니다.

> **보안: `davey` trace 로그를 끄십시오.** 런타임이 MLS와 복호화에 쓰는 DAVE 라이브러리 `davey`는
> **trace** 수준에서 미디어 키 비밀값(모든 멤버의 MLS 내보내기 송신 비밀, 래칫 키)을 `davey::session`,
> `davey::cryptor::hash_ratchet`, `davey::cryptor::mlspp_crypto` 대상으로 `tracing`에 씁니다. 그 키가
> 남은 로그로는 녹화한 통화 음성을 복호화할 수 있습니다. `davey` 대상은 `info` 이하로 두십시오(예:
> `RUST_LOG=info,davey=info`, 나머지를 trace로 둘 때도 `davey=info`를 더함). 3.0.0의 `dave`
> 기능(`VoiceDaveySession`) 때부터 그랬고 최신판 davey 0.1.5도 같습니다. **런타임은 이를 완화할 뿐
> 고치지 못합니다.** 자신의 davey 호출을 `tracing` 필터 아래에서 실행해 davey의 trace 이벤트(키 재료
> 포함)가 구독자에 닿기 전에 버리고, 그 모듈 중 하나라도 trace인 상태로 DAVE 세션을 시작하면 경고를
> 남깁니다. 다루지 못하는 것: 런타임 밖의 davey 호출(`VoiceDaveySession`, davey 직접 호출), 그리고
> `tracing` 구독자가 없을 때 `tracing`이 `log` 크레이트로 넘기는 davey 이벤트(이 경로는 경고도 보지
> 못합니다). 근본 수정은 davey에서 해야 하며, 상류 이슈는 초안만 있고 제출하지 않았습니다.

- **참여.** IDENTIFY에 `max_dave_protocol_version`을 광고합니다(기본 1, 최대 1). SESSION_DESCRIPTION이
  DAVE를 정하면 MLS 키 패키지를 보내고, 게이트웨이의 외부 발신자를 적용하고, 새로 들어온 멤버의 제안을
  `CLIENTS_CONNECT`와 대조해 커밋하고, 커밋과 웰컴을 처리하고, `transition_ready`를 알린 뒤
  `execute_transition`에서 키를 바꿉니다. 핸드셰이크 중에 온 메시지(`CLIENTS_CONNECT`, 외부 발신자)는
  버리지 않고 세션이 시작될 때 처리합니다.
- **송신.** `send_opus_frame(...)`, `send_opus_frame_after_gap(...)`, `play_opus_frames(...)`, 그리고
  `subscribe_runtime`으로 구독한 `AudioPlayer`는 Opus 프레임을 DAVE로, 이어서 전송 암호화로 감쌉니다.
  Opus 무음 프레임은 프로토콜이 허용하는 대로 DAVE 없이 나갑니다. 프로토콜대로 키는 전환이 실행될
  때만 바뀝니다. 커밋을 처리한 뒤에도 아직 그 커밋을 처리하지 않은 멤버가 가진 이전 에포크 키로 계속
  보냅니다(이를 위해 런타임이 송신 키를 `davey`와 같은 방식으로 직접 유도해 가지고, MLS와 복호화는
  `davey`가 합니다). 쓸 키가 없을 때는 오디오를 평문으로
  내보내는 대신 이유를 설명하는 `DiscordError::Voice`를 돌려주고 아무것도 소비하지 않습니다(순번,
  타임스탬프, nonce). 그동안 `is_playable()`은 `false`입니다.
- **수신.** `recv_voice_packet(...)`과 `recv_decoded_voice_packet(...)`이 화자의 키로 복호화합니다. MLS
  전환 전후에는 몇 개가 늘 풀리지 않으므로, 그런 패킷과 아직 사용자를 모르는 SSRC의 DAVE 프레임은
  오류로 돌려주지 않고 건너뜁니다(`?`를 쓰는 수신 루프가 끝나지 않도록).
  `VoiceDaveStatus::undecryptable_packets`가 그 수를 셉니다. 음소거 사용자에 대해 SFU가 DAVE 없이 보내는
  무음 프레임(`F8 FF FE`)은 그대로 돌려줍니다.
- **전환.** 버전 0으로 내려가는 전환(DAVE를 지원하지 않는 클라이언트가 들어옴)은 준비되자마자 평문을
  받아들이고 실행된 뒤 평문을 보냅니다. 다시 올라가는 전환(epoch 1의 `prepare_epoch`)은 새 그룹을 만들고,
  그 전환이 실행될 때까지 평문을 계속 보냅니다. 잘못된 커밋·웰컴은 `dave_mls_invalid_commit_welcome`으로
  알리고 새 키 패키지로 다시 들어갑니다. 전환이 없는데 복호화가 36번 넘게 연속 실패해도 같은 복구를
  합니다.
- **핸드셰이크 중 4017 닫힘**(Discord가 DAVE를 요구)은 연결이 무엇을 광고했는지 알려 주는 오류가 되고,
  `connect_voice_runtime`이었다면 `connect_voice_runtime_with_options`를 안내합니다.
- **재개(RESUME) 없음.** 음성 런타임에는 RESUME 루프가 없습니다. 음성 웹소켓이 닫히면 핸들의 게이트웨이
  태스크가 끝나고(`is_playable()`이 `false`, `wait_until_playable()`은 실패), 다시 연결하면 새 IDENTIFY와
  함께 DAVE 세션도 새로 만들어 새 키 패키지로 MLS 그룹에 다시 들어갑니다.

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

**`connect_voice_runtime`(3.0.0 동작).** 설정한 `max_dave_protocol_version`을 광고하지만 DAVE 세션은
운영하지 않습니다. DAVE 통화에서 일반 송수신은 오류를 내고, `dave_status()`는 협상된 버전만 보여 주며,
`is_playable()`은 늘 `true`라서 `AudioPlayer`는 3.0.0처럼 동작합니다. DAVE는 직접 다룹니다:
`VoiceDaveySession`, 원시 `send_dave_*` 명령, 원시 DAVE 메시지를 담는 `VoiceRuntimeState::dave`
(`VoiceDaveState`), 그리고 `VoiceDaveFrameEncryptor`/`VoiceDaveFrameDecryptor`를 넘기는
`send_opus_frame_with_dave(...)`/`recv_voice_packet_with_dave(...)`. 이 `*_with_dave` 메서드는
`connect_voice_runtime_with_options` 런타임에서도 직접 만든 프레임 암호화에 쓸 수 있지만, 그 연결에서 MLS를
손수 진행하지는 마십시오. 런타임이 자기 키 패키지를 보내므로 게이트웨이가 키 패키지를 둘 받게 됩니다.

**3.0.0에서 올릴 때.** 3.0.0의 런타임은 DAVE를 광고만 하고 운영하지 않아서, DAVE 채널에서
`AudioPlayer`는 아무 소리도 내지 못했습니다(프레임마다 경고만 남김). 깨지는 변경은 없습니다.
`connect_voice_runtime(config)`을 쓰는 코드는 전과 같이 동작합니다. DAVE 채널에서 소리를 내려면
`connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))`로 연결합니다.
게이트웨이 봇이라면 `Context::connect_voice_runtime` 대신 `Context::connect_voice_runtime_with_dave`를 씁니다.

**한계.** `davey` 0.1.x는 웰컴에 담긴 사용자 목록을 통화 멤버와 아직 대조하지 않습니다(상류의 TODO).
커밋과 제안은 대조합니다. 런타임의 프레임 카운터는 MLS 에포크 하나에 2^32 - 1프레임(연속 오디오로 약
2.7년)까지이고, 그 뒤에는 nonce를 다시 쓰는 대신 다음 에포크까지 송신이 실패합니다.

**검증 상태.** 독립된 두 `davey` 세션과 테스트용 음성 게이트웨이(OpenMLS 외부 발신자)로 웰컴 참여, 혼자인
멤버의 커밋, 양방향 오디오(실제 Opus를 쓰는 `AudioPlayer` 포함), 다운그레이드·업그레이드, 잘못된
커밋·웰컴 복구, 4017 닫힘을 검사합니다. 런타임이 운영하는 이 경로는 **아직 실제 Discord에서 실행하지
않았습니다**. 무시된 라이브 테스트 `tests/live_voice_dave.rs`가 기준입니다. 절차는
[Live DAVE Validation](../../docs/api/live-dave-validation.md)(영문)에 있습니다.
