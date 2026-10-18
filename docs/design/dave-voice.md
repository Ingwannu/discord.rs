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
                    ├─ dave: Option<Arc<std::Mutex<DaveController>>>      (MLS·암복호화, connect_with_options만)
                    └─ dave_status: watch::Receiver<VoiceDaveStatus>       (재생기·사용자 관찰용)
runtime task ───────── 같은 Arc<Mutex<DaveController>>를 잠가 게이트웨이 메시지를 처리하고,
                       돌려받은 VoiceGatewayCommand를 웹소켓에 쓴다
```

- `DaveController`는 네트워크를 모르는 동기 상태 기계다. 게이트웨이 이벤트를 받아 보낼 명령
  목록을 돌려주고, 송신 프레임을 암호화하고, 수신 프레임을 사용자별로 복호화한다. 그래서 MLS
  전환 시나리오를 소켓 없이 단위 테스트할 수 있다.
- MLS와 수신 복호화는 `davey::DaveSession`이 하고, **송신 키는 런타임이 따로 가진다**(5.1절).
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

- 진입점이 둘이다(7절). `connect_with_options(config, VoiceConnectOptions)`
  (`connect_voice_runtime_with_options`)는 런타임이 DAVE를 운영하고, 광고하는 버전은
  `min(config.max_dave_protocol_version, davey::DAVE_PROTOCOL_VERSION)`이다. MLS 그룹 id가 채널
  id이고(`davey::DaveSession::new(.., channel_id, ..)`) 음성 게이트웨이는 세션의 채널을 알려 주지
  않으므로 채널은 옵션으로 받는다. `connect(config)`(`connect_voice_runtime`)는 3.0.0 그대로
  설정값을 광고하고 세션을 운영하지 않는다. `None`은 두 경우 모두 필드를 보내지 않는다.
- 핸드셰이크 중 닫힘 프레임은 코드와 설명이 담긴 오류가 된다. 4017이면 "이 채널은 DAVE가 필요하고
  이 연결은 N을 광고했다"를 알려 주고, `connect`였다면 `connect_voice_runtime_with_options`를 안내한다.
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
| 실행된 전환의 송신 키가 있음 | 그 키로 DAVE 암호화(5.1절) 후 전송 암호화 |
| 송신 키 없음, `active_version == 0`(0 → 1 업그레이드가 아직 실행 전) | 그대로 보냄. 명세: 송신자는 전환이 실행될 때까지 이전 에포크(여기서는 패스스루)를 쓴다 |
| 송신 키 없음, `active_version > 0`(E2EE 통화에 새로 들어와 실행 전, 재초기화 중, 혼자 남음) | **보내지 않고 오류**. 아무것도 소비하지 않음(RTP 순번·타임스탬프·전송 nonce) |

마지막 줄이 discord.js·Songbird와 다르다. 둘은 키가 없으면 평문을 보내지만 상대는 그 평문을
버리고(패스스루 창이 닫혀 있음) SFU는 E2EE 통화의 평문을 보게 된다. libdave(공식 클라이언트)의
암호화기도 키가 없으면 실패한다.

### 5.1 송신 키는 EXECUTE에서 바뀐다

명세("Group Transitions")는 송신자가 전환이 실행될 때까지 이전 에포크의 키로 보내라고 한다. 커밋을
아직 처리하지 않은 멤버는 이전 키만 갖고 있기 때문이다. 그런데 `davey`의 `process_commit`/
`process_welcome`은 내부 `update_ratchets()`에서 세션 안의 암호화기 키를 즉시 새 에포크로 바꾸고,
이를 미루거나 이전 키를 꺼낼 API가 없다(0.1.5 기준). libdave는 암호화기를 세션과 분리해
(`Encryptor::SetKeyRatchet`) 전환 시점을 호출자에게 맡긴다.

그래서 런타임은 송신 키를 직접 가진다(`SenderKey`). 커밋·웰컴을 처리한 직후 `davey`의 MLS 그룹에서
그 에포크의 송신 비밀을 `export_secret("Discord Secure Frames v0", user_id(LE), 16)`으로 꺼내
(`davey`가 자기 암호화기에 쓰는 것과 같은 호출) 대기 전환 id에 묶어 두고, EXECUTE가 오면 그것으로
바꾼다. 전환 0은 즉시 바꾼다. 키 래칫(MLS++식 `DeriveTreeSecret`, HKDF-SHA256)과 Opus 프레임
암호화(전체 암호화, AES-128-GCM 8바이트 태그, ULEB128 nonce, 꼬리 표식)는 `davey`·libdave와 같은
방식으로 직접 쓴다. 수신은 계속 `davey` 복호화기가 하며, 이전 에포크 키를 10초 동안 유지한다.

키 재료(내보낸 송신 비밀, 대기 중인 전환별 비밀, 래칫 비밀, 유도한 키, HKDF 중간값)는
`zeroize::Zeroizing`에 두어 교체·해제 때 지우고, `SenderKey`에는 `Debug`가 없으며 컨트롤러의
`Debug`는 개수만 보인다. 남는 사본은 둘이다: 암호기 안의 AES 키 스케줄(aes 0.8은 aarch64 전용 선택
기능으로만 지운다)과 유도 중 HMAC 내부 상태(hmac 0.12는 지우지 않는다). `zeroize`는 이미 `voice`
빌드에 들어 있는 크레이트다.

검증: 같은 에포크의 첫 프레임이 `davey` 자체 암호화기 출력과 바이트 단위로 같고, 래칫이 `davey`의
테스트 벡터를 재현하며, 세 멤버 테스트에서 커밋을 먼저 처리한 멤버의 프레임을 아직 처리하지 않은
멤버가 복호화한다. 프레임 카운터는 검사하므로 한 에포크에 2^32 - 1프레임(약 2.7년)을 넘기면 nonce를
재사용하는 대신 오류를 낸다(`davey` 자체 암호화기는 여기서 `u32 += 1`로 넘쳐 디버그 빌드에서
패닉하지만 런타임은 그 경로를 쓰지 않는다).

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

**정정(2026-10-18 리뷰).** 이 절의 첫 판은 `VoiceRuntimeConfig`에 공개 필드 `channel_id`를 더하고
3.1.0을 권했다. 그 구조체는 모든 필드가 공개이고 `#[non_exhaustive]`가 아니라서 필드 추가는 구조체
리터럴을 깨는 **주 버전 변경**이다. 3.1.0으로는 낼 수 없었으므로 다음과 같이 바꿨다.

- `VoiceRuntimeConfig`는 3.0.0 그대로다(필드 추가 없음).
- 새 진입점 `connect_with_options(config, VoiceConnectOptions)`(`connect_voice_runtime_with_options`)와
  `VoiceConnectOptions`(`#[non_exhaustive]`, `VoiceConnectOptions::new(channel_id)`)가 런타임 DAVE를
  켠다. `VoiceManager::runtime_options(guild_id)`가 추적한 음성 상태로 옵션을 만든다.
- `connect(config)`는 3.0.0 동작을 그대로 지킨다: 설정한 버전을 광고하고, DAVE 세션을 운영하지 않고,
  일반 송수신은 DAVE 세션에서 3.0.0과 같은 오류를 내고, `VoiceDaveySession`·`send_dave_*`로 MLS를
  손수 진행할 수 있다. `is_playable()`은 늘 `true`라서 `AudioPlayer`도 3.0.0처럼 동작한다.
- 채널을 이미 아는 고수준 경로에는 새 메서드 `Context::connect_voice_runtime_with_dave`를 더한다. 첫 판은
  `Context::connect_voice_runtime` 자체를 새 진입점으로 바꿨으나, 그 핸들에서 MLS를 손수 진행하던
  3.0.0 사용자는 한 연결에 키가 다른 세션 둘을 갖게 되어(웰컴 실패, 재합류 반복) 깨진다(2차 리뷰).
  그래서 `Context::connect_voice_runtime`은 3.0.0 그대로 두고, 예제·안내서·라이브 테스트는 새 메서드를
  가리킨다.

나머지 추가는 모두 하위 호환이다: `VoiceDaveStatus`(non_exhaustive), `VoiceRuntimeHandle::{dave_status,
subscribe_dave_status, is_playable, wait_until_playable, dave_verification_code}`, 기본 구현이 있는
`VoiceFrameSink::is_playable`, `AudioPlayer::connection_events`, `PlayerConnectionEvent`(non_exhaustive),
`PlayerSubscription::id`. `AudioPlayer`의 재생 가능성 판단은 `is_playable()`이 `false`인 싱크에만
영향을 주므로, 기존 싱크와 `connect` 핸들에서는 동작이 같다(`NoSubscriberBehavior::Stop`이 DAVE 키
전 구독을 "없음"으로 보는 것도 새 경로에서만 일어난다). 실패 로그가 구간마다 한 번으로 줄어든 것은
로그 변화다. `dave` 기능이 `voice`의 별칭이 되어 `voice`만 켠 사용자도 `davey`를 빌드하지만 API
변화는 아니다. 핸드셰이크 중 메시지 보존과 4017 설명 오류는 두 진입점 모두에 적용되는 버그 수정이다.

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
- 송신 키(5.1절): 세 멤버 중 커밋을 먼저 처리한 멤버의 프레임을 아직 처리하지 않은 멤버가 복호화,
  업그레이드는 EXECUTE까지 평문, `davey` 자체 암호화기와 바이트 단위 일치, `davey` 래칫 테스트 벡터,
  카운터 소진.
- 진입점(7절): `connect`가 3.0.0처럼 키 패키지를 보내지 않고 일반 송수신에서 같은 오류를 내며 늘
  재생 가능한지, 4017 오류 문구가 진입점에 맞는지.
- 실제 Discord: `tests/live_voice_dave.rs`(무시됨)가 `connect_voice_runtime_with_options`로 톤을
  재생하고 다른 참가자의 음성을 복호화한다. 실행하지 않았다.

openmls·openmls_basic_credential·openmls_rust_crypto는 dev-dependency다. 이미 `davey`가 같은 버전을
쓰므로 잠금 파일에 새 패키지가 생기지 않는다. 대신 테스트 대상을 빌드할 때는 `voice` 없이도
이 크레이트들을 컴파일한다(한 번 캐시되면 기능 루프에서 재사용).

## 9. 실제 통화 없이 확인하지 못한 것

- Discord SFU가 우리 커밋/키 패키지를 받아들이는지(모의 게이트웨이는 openmls로 만들었고 Discord
  게이트웨이의 검증 규칙을 재현하지 않는다).
- 실제 전환 순서(특히 업그레이드에 EXECUTE가 오는지, CLIENTS_CONNECT가 SESSION_DESCRIPTION보다 먼저
  오는지)와 시간 간격.
- 다른 클라이언트(공식 앱)가 우리 DAVE 프레임을 복호화하는지. 테스트의 양쪽은 모두 `davey`이고,
  송신은 `davey`와 바이트 단위로 같다는 것까지 확인했다.

## 10. 알려진 한계

- **RESUME 없음.** 음성 런타임은 3.0.0부터 RESUME 루프가 없다. 음성 웹소켓이 닫히면 게이트웨이 태스크가
  끝나고 핸들은 재생 불가가 된다. 다시 연결하면 새 IDENTIFY이고, DAVE도 새 세션·새 키 패키지로 MLS 그룹에
  다시 들어간다(게이트웨이는 이전 리프를 제거하고 새 키 패키지를 추가한다). 재개를 넣으려면 MLS 상태를
  유지한 채 RESUME 후 계속 쓸 수 있는지 명세·실측으로 먼저 확인해야 한다.
- **웰컴 사용자 검증.** `davey` 0.1.x의 `process_welcome`은 웰컴의 사용자 목록을 인식된 사용자와
  대조하지 않는다(소스의 TODO, libdave는 대조한다). 제안(Add)과 커밋은 대조한다. 웰컴은 게이트웨이가
  보낸 외부 발신자와 같은 그룹에만 들어가므로, 남는 위험은 게이트웨이가 모르는 멤버를 넣은 그룹이다.
- **davey trace 로그의 키 노출.** `davey` 0.1.5(최신)는 trace 수준에서 키 비밀값을 기록한다:
  `session.rs:799` `trace!("Got base secret for user {:?}: {:?}", user_id, base_secret)`(모든 멤버의
  에포크별 송신 기반 비밀), `cryptor/hash_ratchet.rs:17`("Creating hash ratchet with secret"),
  `cryptor/mlspp_crypto.rs:74,89,91`(래칫 입력·유도 비밀, 세대별 키). debug 수준에는 키 재료가 없다
  (개인정보 코드, 제안 참조, 세대 번호뿐). 3.0.0의 `dave` 기능(`VoiceDaveySession`) 때부터 있던
  문제다. 게시된 크레이트에서 의존성을 고칠 수 없으므로 **완화**만 한다(수정이 아니다):
  - 안내서(영·한), 진입점 rustdoc, CHANGELOG "Security"에 `davey` 대상을 `info` 이하로 두라고 적는다.
  - 경고: 키를 쓰는 세 모듈(`davey::session`, `davey::cryptor::hash_ratchet`,
    `davey::cryptor::mlspp_crypto`) 각각에 `tracing::enabled!`를 확인한다(첫 판은 `davey::session`
    하나만 봐서 `davey::cryptor=trace,davey::session=info`를 놓쳤다). 확인은 컨트롤러를 만들 때 필터
    밖에서 하고, 첫 davey 세션을 만들 때 경고한다. `VoiceDaveySession::new`도 경고한다.
  - 필터(`DaveyTraceFilter`): 런타임이 davey를 부르는 모든 지점(세션 설명, 게이트웨이 메시지 처리,
    수신 복호화, 검증 코드)을 범위 디스패처 아래에서 실행해, davey 대상의 trace 이벤트는 버리고
    나머지는 세션을 만들 때의 기본 디스패처로 넘긴다. 콜사이트 관심도는 davey trace 콜사이트만
    "never"로 답하고 나머지는 원래 디스패처의 답을 그대로 전한다. 최대 수준은 만들 때의
    `LevelFilter::current()`로 답해 전역 최대를 올리지 않는다. 연결마다 디스패처를 한 번 등록한다.
  - 다루지 못하는 것: 런타임 밖의 davey 호출(`VoiceDaveySession`, `session_mut`, davey 직접 호출),
    그리고 `tracing` 디스패처가 하나도 없을 때 `tracing`의 `log` 대체 경로. 범위 디스패처를 설치하면
    `dispatcher::has_been_set()`이 프로세스 전체에서 참이 되어 다른 크레이트의 `log` 대체 출력까지
    꺼지므로, 디스패처가 없을 때는 필터를 설치하지 않는다(그 경로는 경고도 보지 못한다).
  - 수신 복호화를 런타임 자체 구현으로 옮기는 안(송신처럼)은 하지 않았다. 가장 위험한 기반 비밀은
    런타임이 반드시 부르는 MLS 처리(`process_commit`/`process_welcome` 안의 `update_ratchets`)가
    모든 멤버에 대해 기록하므로, 자체 복호화로 없어지는 것은 세대별 유도 로그뿐이다. 그 대가로 davey
    복호화기(세대 창, nonce 재사용 추적, 이전 래칫 만료)를 하나 더 갖게 된다. 필터는 MLS 경로까지
    포함해 런타임의 모든 davey 호출을 덮으므로 그쪽을 택했다.
  런타임 자체 송신 키(5.1절)는 키를 로그에 쓰지 않는다. 아래는 상류 이슈 초안이다(제출하지 않음,
  사용자 확인 후).

  ```text
  Title: Trace-level logs contain media key secrets (MLS-exported sender secrets, ratchet keys)

  davey 0.1.5 logs key material through `tracing` at trace level:

  - src/session.rs:799   trace!("Got base secret for user {:?}: {:?}", user_id, base_secret)
    -> the MLS-exported "Discord Secure Frames v0" secret of every group member, per epoch
  - src/cryptor/hash_ratchet.rs:17   trace!("Creating hash ratchet with secret: {:x?}", secret)
  - src/cryptor/mlspp_crypto.rs:74, 89, 91   the ratchet's input secret and derived secrets
    (per-generation AES-128-GCM keys and nonces)

  Anyone who enables trace logging to debug a voice issue (RUST_LOG=trace, or a davey=trace
  directive) writes these keys to their log, and logs are routinely shipped to aggregators and
  kept. With them, captured call audio of that epoch can be decrypted, which defeats DAVE's
  end-to-end guarantee for that call. Downstream crates cannot patch this; they can only tell
  users to keep the davey target at info or coarser.

  Suggestion: drop the secret values from these messages (keep the events, log the user ID,
  generation, and lengths), or at most log a short non-reversible fingerprint. libdave does not
  log key material. A test that runs a session under a trace-level subscriber and checks that no
  logged field contains exported secrets would keep it from coming back.

  Found while integrating davey into discord.rs (Rust crate `discordrs`); happy to send a PR.
  ```
- **프레임 카운터.** 런타임 송신 키는 에포크마다 2^32 - 1프레임까지 쓰고 그 뒤에는 오류를 낸다(5.1절).
  `davey` 0.1.5의 자체 암호화기는 같은 지점에서 `u32 += 1`이 넘치며(디버그 빌드 패닉), 런타임은 그
  경로를 쓰지 않는다. 저수준 `VoiceDaveySession::encrypt_opus`는 여전히 `davey` 암호화기를 쓴다.

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
  상태 기계)를 새 진입점 `connect_with_options`에서만 켜고(`connect`는 3.0.0 그대로), 송신 키는
  EXECUTE에서 바꾸며(5.1절), 핸드셰이크 메시지 버퍼링, 4017 설명 오류, 재생 가능성 기반 자동
  일시정지, 실패 구간 이벤트를 더한다.
- 다른 대안 대신 이 방식을 선택한 이유 (Why this over the alternatives): 비-Stage 음성은 DAVE 없이 쓸 수
  없으므로 `voice`만 켠 사용자도 동작해야 한다. `davey`는 순수 Rust(시스템 라이브러리 없음)이고
  깨끗한 빌드 비용은 `voice` 41 s → 47 s(+6 s, 이 머신 기준), 의존 크레이트 +103개다. (a)는 상대가
  버리는 평문을 SFU에 노출하고 트랙 앞부분이 사라진다. (b)는 discord.js의 `playable` 의미와 같고
  혼자 있는 채널에서는 아무도 듣지 않으므로 기다리는 편이 맞다. (i)은 망라형 열거형을 깨므로 3.x에서
  불가하다.
- 장점, 단점 및 영향 (Pros, cons, and impact): `Context::connect_voice_runtime_with_dave`와 `connect_with_options`가 DAVE
  채널에서 동작하고 E2EE가 유지되며, 3.0.0 코드는 그대로 컴파일·동작한다(3.1.0 가능). 단점: `voice`
  빌드가 무거워지고, 진입점이 둘이라 문서로 구분해야 하며, 새 경로에서 `NoSubscriberBehavior::Stop`은
  DAVE 키 전 구독을 "없음"으로 보아 트랙을 멈춘다(문서화, `wait_until_playable`). 송신 키 유도와 Opus
  프레임 암호화를 `davey`와 같은 방식으로 직접 갖는다(바이트 단위 비교 테스트). 실제 Discord와의
  상호 운용은 무시된 라이브 테스트로만 확인할 수 있다.
```
