# 음성 런타임의 DAVE(E2EE) 세션 관리

작성: 2026-10-18. 기준: main `84e6721`(3.0.0 배포본). 브랜치 `fix-dave-voice`.
구현 위치: `src/voice_runtime/dave.rs`(상태 기계), `src/voice_runtime.rs`(게이트웨이 루프·송수신),
`src/voice/player.rs`(재생기 연동). 이 문서를 바꾸지 않고 계약을 바꾸지 않는다.

## 1. 문제

Discord는 2026-03-01부터 Stage가 아닌 모든 음성 채널에 DAVE(MLS 기반 종단 간 암호화)를 요구한다.
지원하지 않는 클라이언트는 닫힘 코드 4017로 거절된다
([지원 문서](https://support.discord.com/hc/en-us/articles/38749827197591),
[Voice Connections › Indicating DAVE Protocol Support](https://docs.discord.com/developers/topics/voice-connections#indicating-dave-protocol-support),
[DAVE 명세](https://daveprotocol.com/)).

3.0.0은 IDENTIFY에서 `max_dave_protocol_version = 1`을 광고하지만 MLS 세션을 아무도 운영하지 않는다.

- `AudioPlayer::subscribe_runtime`의 기본 싱크는 평문 `send_opus_frame_after_gap`을 부르고, DAVE가
  협상된 세션에서 이 함수는 UDP를 보내기 전에 오류를 낸다. 재생기는 경고만 남기고 트랙을 계속
  진행하므로 사용자는 아무것도 듣지 못한다.
- `recv_voice_packet`은 DAVE 세션을 거절한다.
- `dave` 기능은 `davey` 래퍼와 `*_with_dave` 함수만 더할 뿐, 외부 발신자·키 패키지·제안·커밋/웰컴·
  에포크·전환 준비/실행·패스스루를 처리하지 않는다. 사용자가 직접 해야 했다.

@discordjs/voice(`networking/Networking.ts`, `networking/DAVESession.ts`)와 Songbird
(`driver/tasks/ws.rs`, `driver/tasks/mixer`)는 런타임이 MLS를 운영하고 Opus → DAVE 프레임 암호화 →
전송 암호화 순서로 보낸다. 이 변경은 같은 구조를 따른다.

## 2. 소유 구조

```text
VoiceRuntimeHandle ─┬─ outbound_rtp: tokio::Mutex<VoiceOutboundRtpState>   (송신 순서)
                    ├─ dave: Arc<std::Mutex<DaveController>>              (MLS·암복호화)
                    └─ dave_status: watch::Receiver<VoiceDaveStatus>       (재생기·사용자 관찰용)
runtime task ───────── 같은 Arc<Mutex<DaveController>>를 잠가 게이트웨이 메시지를 처리하고,
                       돌려받은 VoiceGatewayCommand를 웹소켓에 쓴다
```

- `DaveController`는 네트워크를 모르는 동기 상태 기계다. 게이트웨이 이벤트를 받아 보낼 명령
  목록을 돌려주고, 송신 프레임을 암호화하고, 수신 프레임을 사용자별로 복호화한다. 그래서 MLS
  전환 시나리오를 소켓 없이 단위 테스트할 수 있다.
- 잠금은 std `Mutex`다. MLS 처리와 프레임 암복호화는 모두 동기 계산이고 `.await`를 가로지르지
  않는다(`await_holding_lock` 린트로 강제). 송신 경로는 RTP tokio 잠금을 잡은 채 DAVE 잠금을
  잠깐 잡는다(순서: RTP → DAVE, 다른 곳은 DAVE만 잡으므로 교착 없음). DAVE 프레임 nonce가
  RTP 순서와 같은 순서로 나간다.
- 재생기의 제어 태스크는 매 틱 `is_playable()`을 부른다. 이것이 MLS 처리 중(수 ms)인 잠금을
  기다리면 안 되므로 상태는 `watch`로 발행하고 `is_playable()`은 `watch`만 읽는다.

## 3. 게이트웨이 opcode 처리

이진 메시지는 서버 → 클라이언트가 `[seq u16][op u8][payload]`, 클라이언트 → 서버가
`[op u8][payload]`다(명세 "Voice Gateway Opcodes").

| op | 방향 | 처리 |
|---|---|---|
| 4 SESSION_DESCRIPTION | S→C | `dave_protocol_version`으로 세션 버전 확정. >0이면 `davey::DaveSession` 생성 후 키 패키지(26) 전송 |
| 11 CLIENTS_CONNECT | S→C | `user_ids`를 연결된 사용자 집합에 추가(제안 검증용 "인식된 사용자") |
| 13 CLIENT_DISCONNECT | S→C | 집합에서 제거 |
| 21 PREPARE_TRANSITION | S→C | 대기 전환에 `(id, version)` 기록. id 0이면 즉시 실행. 아니면 버전 0(다운그레이드)일 때 복호화기 패스스루 허용(24초) 후 READY(23) |
| 22 EXECUTE_TRANSITION | S→C | 대기 중인 id면 버전을 바꾸고 마지막 전환 id 기록. 0 → >0 업그레이드면 패스스루를 10초 뒤 닫음 |
| 24 PREPARE_EPOCH | S→C | epoch 1이면 버전을 갱신하고 세션 재초기화(새 그룹) + 새 키 패키지 |
| 25 MLS_EXTERNAL_SENDER | S→C | 외부 발신자 저장·적용(로컬 대기 그룹 생성). 세션이 없으면 저장만 하고 나중에 적용 |
| 27 MLS_PROPOSALS | S→C | `[optype][proposals]`. 인식된 사용자로 검증해 처리, 커밋이 생기면 COMMIT_WELCOME(28) |
| 29 ANNOUNCE_COMMIT_TRANSITION | S→C | `[transition_id u16][commit]`. 성공: id≠0이면 대기 전환 기록 + READY, id 0이면 즉시 실행. 실패: 복구 |
| 30 MLS_WELCOME | S→C | `[transition_id u16][welcome]`. 29와 같은 규칙 |
| 31 INVALID_COMMIT_WELCOME | C→S | 복구: 31 전송, 세션 재초기화, 새 키 패키지(26). 이미 복구 중이면 반복하지 않음 |

복구 규칙은 discord.js `DAVESession.recoverFromInvalidTransition`과 명세 "Invalid commit/welcome"을
따른다. 수신 복호화가 대기 전환도 복구도 없는 상태에서 연속 36회(discord.js 기본값) 실패해도
마지막 전환 id로 같은 복구를 한다.

**핸드셰이크 중 메시지.** HELLO·READY·SESSION_DESCRIPTION을 기다리는 동안 들어온 다른 메시지
(CLIENTS_CONNECT, 이진 DAVE 메시지)는 버리지 않고 쌓아 두었다가 런타임 루프가 시작될 때 순서대로
처리한다(Songbird가 biased select로 같은 문제를 푼다). 버리면 인식된 사용자 집합이 비어 첫
제안을 거절한다.

## 4. 버전 협상과 기능 게이트

- 광고하는 버전 = `min(config.max_dave_protocol_version, davey::DAVE_PROTOCOL_VERSION)`. 단
  `VoiceRuntimeConfig::channel_id`가 없으면 0을 광고하고 경고를 남긴다. MLS 그룹 id가 채널 id라
  채널 없이는 세션을 운영할 수 없기 때문이다(`davey::DaveSession::new(.., channel_id, ..)`).
  `None`은 이전처럼 필드를 보내지 않는다.
- 핸드셰이크 중 닫힘 프레임은 코드와 설명이 담긴 오류가 된다. 4017이면 "이 채널은 DAVE가 필요하고
  런타임은 0을 광고했다(이유: channel_id 없음 / 설정이 0)"를 알려 준다.
- `voice` 기능이 `davey`를 포함한다. `dave`는 호환용 별칭(`dave = ["voice"]`)으로 남는다.
  `#[cfg(feature = "dave")]`였던 API는 `voice`만으로 쓸 수 있다. 결정 근거는 7절.

## 5. 송신(재생기 포함)

버전 두 개를 구분한다. `protocol_version`은 discord.js처럼 PREPARE_EPOCH(epoch 1)와 EXECUTE에서
바뀌는 목표 버전이고, `active_version`은 마지막으로 실행된 전환(또는 SESSION_DESCRIPTION)의
버전이다.

| 상태 | 평문 `send_opus_frame*` / 기본 싱크 |
|---|---|
| Opus 무음 프레임 `F8 FF FE` | 그대로 보냄(명세 "Silence packets", discord.js `encrypt`) |
| `protocol_version == 0` | 그대로 보냄(전송 암호화만, Stage·다운그레이드) |
| 세션 준비됨(키 있음) | `davey` `encrypt_opus` 후 전송 암호화 |
| 준비 안 됨, `active_version == 0`(0 → 1 업그레이드 진행 중) | 그대로 보냄. 명세: 송신자는 전환이 실행될 때까지 이전 에포크(여기서는 패스스루)를 쓴다 |
| 준비 안 됨, `active_version > 0`(E2EE 통화에 새로 들어옴, 재초기화 중, 혼자 남음) | **보내지 않고 오류**. 아무것도 소비하지 않음(RTP 순번·타임스탬프·전송 nonce) |

마지막 줄이 discord.js·Songbird와 다르다. 둘은 키가 없으면 평문을 보내지만 상대는 그 평문을
버리고(패스스루 창이 닫혀 있음) SFU는 E2EE 통화의 평문을 보게 된다. libdave(공식 클라이언트)의
암호화기도 키가 없으면 실패한다.

**재생기.** `VoiceFrameSink`에 기본 구현이 있는 `is_playable()`을 더한다(기본 `true`).
`VoiceRuntimeHandle`은 "런타임 태스크가 살아 있고 지금 오디오를 보낼 수 있다"를 돌려준다.
`AudioPlayer`는 discord.js의 `playable`처럼 재생 가능한 구독만 구독으로 센다.

- 재생 가능한 구독이 하나도 없으면 `no_subscriber_behavior`를 적용한다(기본 `Pause` →
  `AutoPaused`). DAVE 그룹이 생기면(`ready`) 다시 `Playing`이 되어 트랙 처음부터 들린다. 혼자 있는
  채널에서는 그룹이 생기지 않으므로 누군가 들어올 때까지 기다린다.
- 프레임은 재생 가능한 구독에만 넣는다. 재생 불가로 바뀐 구독은 대기 오디오를 버리고 무음 꼬리와
  speaking off를 받는다(무음은 DAVE 없이 보낼 수 있다).
- 그래도 실패한 전송(경쟁, 암호화 실패, UDP 오류)은 RTP 간격으로 계산되고(기존 계약), 실패
  구간마다 한 번 `PlayerConnectionEvent::SendFailed`(새 `AudioPlayer::connection_events()`)와
  경고 로그, 회복 시 `Recovered { failed }`와 info 로그를 낸다. 프레임마다 경고를 찍지 않는다.
- W4 계약(60 ms 시작 기한, 60 ms 정체 취소, 최대 120 ms 지연, 간격 보고, nonce 재사용 금지)은
  그대로다. DAVE 암호화는 RTP 잠금 안에서 동기로 끝나므로 취소 지점이 늘지 않는다. 취소된 전송은
  DAVE 프레임 nonce를 하나 쓰는데, 수신자는 이를 패킷 손실처럼 견딘다(명세의 missing nonce 창).

## 6. 수신

`recv_voice_packet`/`recv_decoded_voice_packet`이 DAVE를 자동으로 푼다. SSRC → 사용자는 SPEAKING(5)
매핑을 쓴다.

| 상태 | 결과 |
|---|---|
| 무음 프레임 | 그대로 반환 |
| 세션 없음 또는 (`protocol_version == 0`이고 사용자 패스스루 가능) 또는 준비 안 됨 | DAVE 표식(`FA FA`)이 없으면 평문으로 반환, 있으면 버림 |
| 사용자를 모름(SPEAKING 전) | DAVE 프레임이면 버림, 아니면 반환 |
| 복호화 성공 | 반환, 연속 실패 수 0 |
| 복호화 실패 | 버림. 대기 전환·복구 중이 아니면 실패 수 +1, 36 초과면 복구(3절) |

버린 패킷은 오류로 돌려주지 않고 다음 패킷을 기다린다. 전환 중 몇 개가 풀리지 않는 것은 정상이고
(discord.js도 `null`로 무시), 오류로 돌려주면 `?`를 쓰는 수신 루프가 끝난다(무음 프레임 수정과 같은
이유). 버린 수는 `VoiceDaveStatus::undecryptable_packets`로 보이고 debug 로그를 남긴다.
`recv_voice_packet_with_dave`(사용자 복호화기)는 동작을 바꾸지 않는다.

## 7. 공개 API와 SemVer

추가: `VoiceRuntimeConfig::channel_id(ChannelId)` 빌더, `VoiceDaveStatus`(non_exhaustive),
`VoiceRuntimeHandle::{dave_status, subscribe_dave_status, wait_until_playable,
dave_verification_code}`, `VoiceFrameSink::is_playable`(기본 구현), `AudioPlayer::connection_events`,
`PlayerConnectionEvent`(non_exhaustive), `PlayerSubscription::id`. `VoiceManager::runtime_config`와
`Context::connect_voice_runtime`은 채널 id를 채운다.

호환성 주의: `VoiceRuntimeConfig`에 공개 필드 `channel_id: Option<ChannelId>`가 생긴다. 모든 필드가
공개인 구조체라 **필드를 전부 나열한 구조체 리터럴은 컴파일되지 않는다**(`..VoiceRuntimeConfig::new(..)`
갱신 문법과 `new()`는 그대로 동작). 채널 id 없이는 DAVE를 운영할 수 없고, 다른 저장 위치(비공개
필드, 별도 함수)는 같은 정도로 깨지거나 기본 경로를 고치지 못한다. 릴리스 버전 판단은 관리자에게
남긴다(3.1.0과 변경 기록 안내 권장).

동작 변화: 평문 `send_opus_frame*`/`play_opus_frames`는 DAVE 세션에서 오류 대신 자동 암호화하고,
`recv_voice_packet*`는 오류 대신 자동 복호화한다. `AudioPlayer`는 DAVE 그룹이 준비될 때까지
`AutoPaused`에 머문다. `VoiceDaveState`(원시 메시지 미러)는 이전처럼 채워진다. 런타임과 별도로
`VoiceDaveySession`과 `send_dave_*` 명령으로 MLS를 직접 운영하는 것은 더 이상 지원하지 않는다(같은
연결에서 키 패키지가 둘이 된다). 저수준 API 자체는 남는다.

## 8. 테스트

- 상태 기계 단위 테스트(`src/voice_runtime/dave.rs`): 테스트 전용 모의 게이트웨이(openmls로 외부
  발신자 서명, 외부 Add 제안 생성)와 독립된 두 `davey` 세션으로 실제 MLS 그룹을 만든다. 혼자인
  봇이 커밋하는 경로, 웰컴으로 합류하는 경로, 양방향 암복호화, 커밋/웰컴 실패 복구(중복 방지 포함),
  id 0 전환, 다운그레이드 → 평문 → 업그레이드, 복호화 실패 허용치와 복구, 무음·알 수 없는 사용자,
  상태 발행.
- 런타임 통합 테스트: 모의 음성 게이트웨이 + UDP로 핸드셰이크 버퍼링, 키 패키지/READY/EXECUTE
  왕복, 자동 송신을 모의 상대가 DAVE·전송 복호화, 상대의 DAVE 프레임을 `recv_voice_packet`이
  복호화, 4017 닫힘 오류, IDENTIFY 광고 버전.
- 재생기 테스트: 재생 불가 → `AutoPaused` → 준비 → `Playing`, 재생 불가 구독 제외, 실패 구간 이벤트
  한 번과 회복 이벤트.
- 실제 Discord: `tests/live_voice_dave.rs`(무시됨)가 자동 경로로 톤을 재생하고 다른 참가자의
  음성을 복호화한다. 실행하지 않았다.

openmls·openmls_basic_credential·openmls_rust_crypto는 dev-dependency다. 이미 `davey`가 같은 버전을
쓰므로 잠금 파일에 새 패키지가 생기지 않는다. 대신 테스트 대상을 빌드할 때는 `voice` 없이도
이 크레이트들을 컴파일한다(한 번 캐시되면 기능 루프에서 재사용).

## 9. 실제 통화 없이 확인하지 못한 것

- Discord SFU가 우리 커밋/키 패키지를 받아들이는지(모의 게이트웨이는 openmls로 만들었고 Discord
  게이트웨이의 검증 규칙을 재현하지 않는다).
- 실제 전환 순서(특히 업그레이드에 EXECUTE가 오는지, CLIENTS_CONNECT가 SESSION_DESCRIPTION보다 먼저
  오는지)와 시간 간격.
- 다른 클라이언트(공식 앱)가 우리 DAVE 프레임을 복호화하는지. 테스트의 양쪽은 모두 `davey`다.

```text
[Decision Log]
- 목적과 의도 (Purpose): 기본 설정의 음성 런타임과 AudioPlayer가 Stage가 아닌 채널(DAVE 필수)에서
  실제로 들리게 하고, 실패는 조용히 진행하지 않고 드러낸다.
- 기존 구현 및 제약 조건 (Existing implementation and constraints): 3.0.0은 DAVE 버전 1을 광고하지만
  MLS를 운영하지 않고, 기본 싱크·수신은 DAVE 세션에서 오류를 낸다. SemVer를 지키는 3.x 릴리스여야
  하므로 망라형 공개 열거형(AudioPlayerEvent, DiscordError)에 변형을 더할 수 없다. W4 실시간 계약과
  코드 규칙(unwrap/as/잠금 보유 await 금지)을 지켜야 한다. MLS 그룹 id는 채널 id다.
- 검토한 주요 대안 (Alternatives considered):
  (1) 사용자에게 MLS를 맡기고 문서만 보강 — 지금의 실패를 그대로 둔다.
  (2) 런타임이 MLS를 운영하되 `dave` 기능을 선택으로 두고, 없으면 0을 광고하고 4017을 설명.
  (3) 런타임이 MLS를 운영하고 `voice`가 `davey`를 포함.
  키가 없을 때 송신: (a) discord.js·Songbird처럼 평문 전송, (b) 보내지 않고 재생기를 자동 일시정지.
  오류 노출: (i) AudioPlayerEvent 변형 추가, (ii) 별도 연결 이벤트 스트림, (iii) 로그만.
- 선택한 방식 (Chosen approach): (3) + (b) + (ii). 런타임 소유 `DaveController`(std Mutex, 동기
  상태 기계), 핸드셰이크 메시지 버퍼링, 채널 id 없으면 0 광고, 4017 설명 오류, 재생 가능성 기반
  자동 일시정지, 실패 구간 이벤트.
- 다른 대안 대신 이 방식을 선택한 이유 (Why this over the alternatives): 비-Stage 음성은 DAVE 없이 쓸 수
  없으므로 `voice`만 켠 사용자도 동작해야 한다. `davey`는 순수 Rust(시스템 라이브러리 없음)이고
  깨끗한 빌드 비용은 `voice` 41 s → 47 s(+6 s, 이 머신 기준), 의존 크레이트 +103개다. (a)는 상대가
  버리는 평문을 SFU에 노출하고 트랙 앞부분이 사라진다. (b)는 discord.js의 `playable` 의미와 같고
  혼자 있는 채널에서는 아무도 듣지 않으므로 기다리는 편이 맞다. (i)은 망라형 열거형을 깨므로 3.x에서
  불가하다.
- 장점, 단점 및 영향 (Pros, cons, and impact): 기본 경로가 DAVE 채널에서 동작하고 E2EE가 유지된다.
  단점: `voice` 빌드가 무거워지고, `VoiceRuntimeConfig` 구조체 리터럴 사용자는 필드를 더해야 하며,
  `NoSubscriberBehavior::Stop`은 DAVE 준비 전 구독을 "없음"으로 보아 트랙을 멈춘다(문서화,
  `wait_until_playable`). 실제 Discord와의 상호 운용은 무시된 라이브 테스트로만 확인할 수 있다.
```
