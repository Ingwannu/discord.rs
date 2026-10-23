# 게이트웨이 세션 handoff (프로세스 간 RESUME)

작성: 2026-10-23. 기준: main `0a306d9`(3.1.1 배포본). 브랜치 `feat/session-handoff`, 버전 3.2.0(미배포).
갱신: 2026-10-23, GPT-6.1-sol 리뷰(`f76ee19`) 지적 3건 수정(9절), 라이브 테스트 주의(8절).
구현 위치: `src/gateway/handoff.rs`(공개 타입·세션 매칭·전달 장부), `src/gateway/client.rs`(4000 종료,
스냅샷에서 RESUME, fallback), `src/gateway/bot/pipeline.rs`(정지·전달 추적), `src/gateway/bot.rs`
(빌더·샤드 런타임 연결).

## 1. 문제

운영 요청(2026-10-23): 업데이트 중에 봇이 오프라인이 되면 안 된다. 프로세스를 바꿔도 같은 게이트웨이
세션을 이어 가고, 그 사이에 놓친 이벤트는 Discord가 다시 보내 주어야 한다.

3.1.1까지는 할 수 없었다.

- 정상 종료(`GatewayCommand::Shutdown`, `Context::shutdown_shard`, `ShardSupervisor::shutdown`)는 닫힘
  코드 1000으로 닫는다. Discord는 1000/1001로 닫힌 세션을 끝낸다(Discord docs: Gateway › Closing
  Connections). 새 프로세스는 IDENTIFY부터 다시 해야 하고 READY와 GUILD_CREATE 폭주를 다시 받는다.
- 앱이 흔히 쓰는 `tokio::select! { client.start(), shutdown_signal() }`은 start future를 drop한다.
  close 프레임 없이 TCP만 끊기므로 Discord 쪽 세션은 남지만, 앱이 session_id·sequence·resume URL을
  꺼낼 방법이 없다.
- 읽기 루프는 디스패치를 파이프라인에 넘기기 **전에** sequence를 올린다(같은 프로세스 안 재연결용).
  그래서 읽기 루프의 sequence를 그대로 넘기면 큐에 남은 이벤트는 재전송되지 않고 사라진다.

선행 사례: twilight-gateway는 `CloseFrame::RESUME`(코드 4000)으로 닫으면 세션이 유지되고,
`Shard::session()`으로 꺼낸 `Session`을 `ConfigBuilder::session(Session)`과 `resume_url`로 넘겨
다음 프로세스가 RESUME한다. discord.rs는 샤드 객체를 공개하지 않고 빌더 → `start()` 형태이므로 같은
기능을 그 형태에 맞춰 붙였다.

## 2. 공개 API (3.2.0, `gateway` 기능)

```rust
// 스냅샷. serde Serialize/Deserialize, Debug는 session id 앞 6자만 보인다.
pub struct GatewaySession { /* private */ }
impl GatewaySession {
    pub fn session_id(&self) -> &str;
    pub fn sequence(&self) -> u64;                 // 핸들러에 넘긴 마지막 이벤트
    pub fn resume_gateway_url(&self) -> Option<&str>;
    pub fn shard(&self) -> (u32, u32);
    pub fn intents(&self) -> Intents;
    pub fn application_id(&self) -> Option<ApplicationId>;
    pub fn user_id(&self) -> Option<UserId>;
}

// 제어 핸들. Clone(Arc), Default, Debug.
pub struct GatewayHandoff { /* private */ }
impl GatewayHandoff {
    pub fn new() -> Self;
    pub fn request(&self);                         // 모든 샤드에 handoff 요청(멱등)
    pub fn is_requested(&self) -> bool;
    pub fn take_sessions(&self) -> Vec<GatewaySession>; // start 반환 뒤, 샤드 id 순
}

impl ClientBuilder {
    pub fn handoff(self, handoff: GatewayHandoff) -> Self;
    pub fn resume_sessions(self, sessions: impl IntoIterator<Item = GatewaySession>) -> Self;
}
```

JSON 형태(파일에 0600으로 저장하는 것은 앱 몫):

```json
[{"session_id":"…","sequence":3,"resume_gateway_url":"wss://gateway-us-east1-b.discord.gg/",
  "shard_id":0,"shard_count":1,"intents":1,"application_id":"…","user_id":"…"}]
```

선택 필드(`resume_gateway_url`, `application_id`, `user_id`)는 없어도 읽힌다. 모르는 필드는 무시한다.
이후 버전은 선택 필드만 더한다. 그래서 3.2 프로세스가 쓴 스냅샷을 이후 3.x 프로세스가 읽을 수 있다.

사용 흐름(앱):

```rust
let handoff = GatewayHandoff::new();
let run = Client::builder(&token, intents)
    .event_handler(handler)
    .resume_sessions(read_and_delete_snapshot()?)   // 없으면 빈 Vec
    .handoff(handoff.clone())
    .start();
tokio::pin!(run);
tokio::select! {
    result = &mut run => return result,
    _ = update_signal() => handoff.request(),
}
run.await?;                                         // 4000으로 닫고 실행 중 핸들러를 기다린다
write_snapshot_0600(&handoff.take_sessions())?;
exec_new_binary();
```

`spawn_shards`/`spawn_auto_shards`는 `handoff.request()` 뒤 `ShardSupervisor::wait()`를 기다린다.

## 3. 흐름

### 3.1 넘기는 쪽

1. `request()`는 watch 채널을 true로 바꾼다. 샤드마다 작은 전달 태스크가 이를 보고 자기 게이트웨이
   명령 채널에 `GatewayCommand::Handoff`를 넣는다. 게이트웨이 클라이언트의 모든 대기 지점(읽기 루프,
   재연결 backoff, IDENTIFY permit 대기)이 이미 이 채널을 읽으므로 새 대기 경로가 필요 없다.
2. 읽기 루프: 세션이 있으면 닫힘 코드 4000(`CLOSE_HANDOFF`, 이유 "session handoff")으로 닫고, 세션이
   없으면(READY 전) 정상 종료처럼 1000으로 닫는다. close 프레임은 Shutdown과 같이 최대 1초 flush한다.
   backoff 중이면 연결이 없어도 세션을 그대로 넘긴다(1000으로 닫은 적이 없으므로 Discord가 유지한다).
   permit 대기 중에는 세션이 없으므로 종료한다. 부트 웨이브를 기다리는 샤드도 연결하지 않고 멈춘다.
3. 샤드 런타임: `GatewayExit::HandedOff`를 받으면 파이프라인을 **정지(halt)**한다. 처리기는 이미 읽은
   이벤트를 캐시·컬렉터·멤버 청크에 계속 반영하지만 핸들러 레인에는 더 넣지 않는다. 디스패처는 새
   핸들러 호출을 시작하지 않는다. 실행 중 호출은 `handler_shutdown_timeout`까지 기다리고 그 뒤 abort한다
   (기존 종료와 같다).
4. 전달 장부(`DeliveryLedger`)에서 재개점을 계산해 `GatewaySession`을 기록한다. `start()`는 `Ok(())`.

### 3.2 받는 쪽

1. `resume_sessions`의 세션을 `match_sessions`가 샤드별로 고른다. session id가 비었거나, 샤드 수·샤드
   id·intents가 다르거나, 같은 샤드에 두 번째인 세션은 경고와 함께 버리고 그 샤드는 IDENTIFY한다.
   intents를 비교하는 이유: 재개된 세션은 처음 IDENTIFY의 intents를 유지하므로, 업데이트로 intents를
   바꾼 앱이 조용히 옛 intents로 돌게 된다.
2. 세션이 있는 샤드는 부트 웨이브를 건너뛰고 바로 연결한다. `GatewayClient::resume_from`이 session id,
   sequence, 자기 설정으로 정규화한 resume URL을 넣는다. RESUME은 원래부터 IDENTIFY gate와 세션 시작
   예산을 거치지 않는다.
3. Discord가 sequence 이후 이벤트를 재전송하고 RESUMED를 보낸다. `resumed` 핸들러가 돈다. READY는
   오지 않고 캐시는 비어 있다. 스냅샷의 application id를 빌더가 지정하지 않았을 때 REST 클라이언트에
   **임시값**으로 넣는다. RESUME이 실패해 IDENTIFY하면 새 READY의 id가 그것을 대신한다. 빌더가 정한
   id는 READY도 바꾸지 않는다. `voice` 기능이면 bot user id를 음성 관리자에 넣는다(READY가 오면
   기존처럼 덮어쓴다).
4. RESUMED는 supervisor 상태를 `Running`으로 바꾸고, handoff 세션이면 `SessionEstablished`도 보낸다
   (이전에는 같은 프로세스 RESUME 뒤 상태가 `Reconnecting`에 남았다).

### 3.3 fallback

| 상황 | 동작 |
|---|---|
| INVALID_SESSION `d=false` | 2초 뒤 세션을 버리고 경고 → IDENTIFY → READY (기존 경로) |
| 닫힘 4007 / 4009 | 세션을 버리고 경고 → IDENTIFY (기존 경로) |
| 기타 닫힘(4000 등), 읽기 오류 | 같은 세션으로 다시 RESUME (기존 경로) |
| resume URL 연결 실패 | URL만 버리고(3.1.1 규칙) 설정된 게이트웨이로 RESUME. 거기서 INVALID_SESSION이면 IDENTIFY |
| 연결은 받지만 HELLO가 없음 | 모든 연결의 설정(연결·업그레이드·HELLO)이 15초로 제한된다. 넘으면 연결 실패와 같다(URL을 버리고 재시도). 그동안 Shutdown·handoff 요청은 바로 처리한다 |
| 스냅샷 불일치(샤드 수·id·intents·빈 id) | 시작 전에 경고하고 IDENTIFY |
| 4004 등 종료 코드 | 기존처럼 샤드 오류 |

경고는 `handoff_resume` 플래그로 구분한다. RESUMED가 오거나 세션을 버릴 때 내려간다.

## 4. sequence 의미

스냅샷 sequence = **현재 세션에서 아직 핸들러에 넘기지 않은 가장 오래된 이벤트 − 1**. 그런 이벤트가
없으면 처리기가 마지막으로 레인에 넣은 이벤트다. 정상적인 handoff(큐가 비어 있음)에서는 읽기 루프가
마지막으로 읽은 sequence와 같다.

"넘겼다"의 정의: 디스패처가 핸들러 호출 태스크를 spawn했거나, 입장 기한이 지난 인터랙션을 overload
hook에 넘겼거나, 가득 찬 큐 때문에 버렸다(버린 것은 기존처럼 손실이다).

결과:

- 핸들러 호출이 시작된 이벤트는 다시 오지 않는다. 그 호출이 timeout에 abort되면 나머지 작업은
  사라진다: **최대 한 번(at-most-once)**. 앱은 `handler_shutdown_timeout`을 충분히 주고, 핸들러가
  spawn한 작업(태스크, 큐)을 프로세스 종료 전에 끝내야 한다.
- 큐에 있던 이벤트, 소켓에서 읽었지만 처리기에 닿지 않은 이벤트, 정지 뒤 읽힌 이벤트는 Discord가
  다음 프로세스에 재전송한다: 손실 없음.
- 인터랙션 레인은 일반 레인을 앞지른다. 일반 이벤트가 큐에 남은 채 handoff되면 재개점이 그 앞을
  가리키므로, 이미 처리한 인터랙션을 다음 프로세스가 다시 받는다: **최소 한 번(at-least-once)**.
  두 번째 응답은 Discord가 40060으로 거절한다.
- 정지 뒤에도 처리기는 읽은 이벤트를 컬렉터·`fetch_members`에 넘긴다. 실행 중 핸들러가 그것을
  소비했어도 다음 프로세스가 다시 받는다.
- 현재 세션의 READY가 아직 핸들러에 닿지 않았으면(또는 처리기가 마지막 READY를 아직 못 봤으면)
  스냅샷을 만들지 않는다. 다음 프로세스는 IDENTIFY해서 READY를 받는다.
- Discord의 3초 인터랙션 기한은 handoff 동안에도 흐른다. 요청부터 다음 프로세스의 RESUME까지를 짧게
  유지해야 재전송된 인터랙션에 응답할 수 있다.

### 4.1 전달 장부

```text
[Decision Log]
- 목적과 의도: 위 4절의 재개점을 정확히 계산한다.
- 기존 구현 및 제약 조건: 읽기 루프 → (ingress 큐) → 처리기 → 일반 레인 / 상호작용 레인. 각 레인은
  FIFO지만 레인끼리는 순서가 다르다. 읽기 루프는 기다리면 안 된다. 성능 게이트가 디스패치당 할당
  수를 본다(docs/design/perf-baseline.md).
- 검토한 주요 대안: (1) 읽기 루프 sequence 그대로 — 큐의 이벤트가 사라진다. (2) 레인별 "마지막으로
  넘긴 sequence"의 최솟값(원자값만) — 다른 레인 이벤트가 사이에 있으면 이미 넘긴 이벤트를 다시 받는다.
  (3) 레인별 대기 FIFO + 처리기 진행점, handoff를 설정한 클라이언트에서만. (4) handoff 때 큐를 끝까지
  비운다(drain) — 느린 Serial 핸들러는 timeout 안에 못 비우고, 남은 것을 알 방법이 여전히 필요하다.
- 선택한 방식: (3). 처리기가 레인에 넣기 전에 (세션 번호, sequence, READY 여부)를 FIFO에 적고,
  디스패처가 넘길 때 앞에서 뺀다. 넣기에 실패하면 뒤에서 빼고 "넘김"으로 친다. 세션 번호는 처리기가
  READY마다 올리고, ingress가 센 READY 수와 비교한다. handoff 때 큐는 비우지 않고 정지한다.
- 다른 대안 대신 이 방식을 선택한 이유: 레인 안 순서가 유지되는 한 정확하다. 손실이 없고 중복은
  상호작용이 앞지른 경우에만 생긴다. 정지(halt)는 실행 중 호출만 기다리므로 옛 프로세스가 빨리 끝나고,
  큐에 있던 이벤트는 새 프로세스가 받는다.
- 장점, 단점 및 영향: handoff 클라이언트는 이벤트마다 짧은 std 뮤텍스를 두 번 잡는다(await를 넘지
  않음). FIFO는 큐 깊이만큼 자라고 이후 재할당하지 않는다. handoff가 없는 클라이언트는 Option 분기
  하나와 샤드당 watch 채널 하나(halt)만 더한다.
- 리뷰 수정(9.1절): 기록과 `try_send`를 한 잠금 안에서 하고(`DeliveryLedger::enqueue`), 정지도 같은
  잠금을 잡는다. 닫힌 레인(`Closed`)은 넘김으로 세지 않는다.
```

## 5. 앱이 알아야 할 것

- **READY에 기대지 말 것.** 재개된 프로세스는 READY를 받지 않는다. 명령 등록, 초기 상태 로드 같은
  초기화는 시작 시점에 하거나 `resumed`에서도 해야 한다.
- **캐시는 비어 있다.** GUILD_CREATE가 다시 오지 않으므로 길드·채널·역할 캐시는 이후 이벤트로만
  채워진다. 필요하면 REST로 읽는다.
- presence는 다시 보내지 않는다. 세션이 옛 presence를 유지한다. 바꾸려면 RESUMED 뒤
  `update_presence`를 부른다.
- 세션은 한 번만 재개한다. 스냅샷 파일은 읽자마자 지운다. 같은 스냅샷을 다시 쓰면 이미 처리한
  이벤트를 다시 받거나 RESUME이 실패한다.
- 실행 중 컬렉터·`fetch_members` 대기는 timeout에 끊긴다. 새 프로세스는 그 대기를 모른다.
- 스냅샷과 토큰이 있으면 세션을 이어 갈 수 있다. 파일은 0600, 봇 계정만 읽게 둔다. `Debug`와 로그는
  session id 앞 6자만 보인다(READY 로그도 이번에 같은 형식으로 바꿨다).
- 멀티 프로세스 샤딩(`sharding::process`)의 자식은 각자 일반 클라이언트이므로 같은 API를 쓴다.
  부모 코디네이터에는 handoff 개념을 더하지 않았다.

## 6. 전체 API 결정

```text
[Decision Log]
- 목적과 의도: 앱이 프로세스를 바꿔도 같은 게이트웨이 세션을 RESUME하게 한다(무중단 업데이트).
- 기존 구현 및 제약 조건: 정상 종료는 1000(세션 폐기). start()를 select!에서 drop하면 close 없이
  끊기고 세션 정보를 꺼낼 수 없다. 샤드 객체는 공개되지 않고 Client::builder → start / start_shards /
  start_auto_shards / spawn_shards(ShardSupervisor) 경로가 있다. 3.x 안에서는 추가형 변경만 가능하다.
- 검토한 주요 대안: (A) twilight처럼 샤드 객체를 공개하고 session()/close(RESUME)를 준다.
  (B) start_until(signal) -> Result<Vec<GatewaySession>> 같은 새 start 변형. (C) 공유 핸들
  GatewayHandoff + ClientBuilder::handoff / resume_sessions. (D) 라이브러리가 스냅샷 파일 경로·권한까지
  관리. (E) 4000 대신 close 없이 TCP를 끊는다.
- 선택한 방식: (C), 닫힘 코드 4000, serde 스냅샷. 파일 저장은 앱 몫.
- 다른 대안 대신 이 방식을 선택한 이유: (A)는 파이프라인·캐시·supervisor와 묶인 런타임 구조를
  드러내야 하는 큰 변경이다. (B)는 start 변형마다 메서드가 늘고 ShardSupervisor 경로를 덮지 못한다.
  (C)는 한 핸들이 모든 시작 경로와 supervisor에 붙고, 기존 select! 패턴을 유지하며(future를 pin하고
  다시 await), 핸들러 안에서도(TypeMap에 넣어) 요청할 수 있다. (D)는 파일 위치·권한·원자적 교체가
  배포 환경(Pterodactyl, systemd, 컨테이너)마다 달라 라이브러리가 정할 일이 아니다. (E)는 Discord가
  세션을 유지하긴 하지만 정상적인 WebSocket 종료가 아니고, twilight와 같은 4000이 의도를 드러낸다.
- 장점, 단점 및 영향: 추가형 API라 3.2.0(minor)이다. 기존 정상 종료는 1000 그대로다. 앱은 요청 뒤
  start future를 계속 poll해야 하고(문서화), take_sessions는 start 반환 뒤에 부른다. resume URL 연결
  실패는 바로 IDENTIFY하지 않고 설정된 게이트웨이로 한 번 더 RESUME한다(예산 절약, 3.1.1 규칙 재사용).
  세션이 있는 샤드는 부트 웨이브를 건너뛴다: handoff 뒤 뒤쪽 샤드가 웨이브 지연만큼 오프라인이 되지
  않게 하려는 것이며, RESUME이 실패해 IDENTIFY할 때는 공유 IDENTIFY gate가 간격과 예산을 지킨다.
```

## 7. 테스트

- `src/gateway/handoff.rs`: serde 왕복(필드 이름 고정, 선택 필드 누락·모르는 필드 허용), `Debug` 가림,
  핸들 요청·세션 회수, `match_sessions` 거절 사유 5가지, 장부 재개점(레인 대기, 상호작용 앞지르기,
  큐 실패, 정지, READY 미전달, 세션 교체, seed).
- `src/gateway/client.rs`(모의 게이트웨이): handoff 시 닫힘 코드 4000과 세션·기본 URL 반환, 세션 없는
  handoff는 1000, backoff 중 handoff, 스냅샷에서 resume URL로 RESUME(설정 게이트웨이 미접속, permit 0,
  RESUMED로 플래그 해제, 정상 종료는 1000), fallback 4가지(INVALID_SESSION, 4009, 4007, 죽은 resume
  URL)에서 IDENTIFY와 permit 1개.
- `src/gateway/bot/tests.rs`(빌더 경로): 두 프로세스 시뮬레이션(멈춘 핸들러 뒤 큐 2개 → sequence 2,
  JSON 왕복, 재전송 2개 + RESUMED, READY 없음, 스냅샷 application id 사용, 두 번째 handoff sequence 5),
  불일치 스냅샷 → IDENTIFY, 세션 없는 handoff → 기록 없음, 예산 0에서도 즉시 RESUME + permit 대기 샤드
  정지, 부트 웨이브 대기 샤드 정지.
- 리뷰 회귀 테스트(9절): 장부 `a_dispatch_a_closed_lane_refused_stays_pending`(두 레인),
  `a_halt_waits_for_an_enqueue_in_progress_and_stops_later_ones`(스레드로 enqueue 도중 halt);
  client `stop_requests_end_a_connection_setup_that_never_finishes`(업그레이드 무응답·HELLO 무응답 ×
  Shutdown·Handoff, 기본 15초 제한 전에 반환), `a_resume_url_that_never_sends_hello_times_out_and_falls_back`;
  빌더 `a_handoff_while_a_resume_waits_for_hello_hands_the_session_back`,
  `a_rejected_sessions_application_id_gives_way_to_the_fresh_ready`(빌더 id 없음 → 999, 있음 → 555).
- **라이브 테스트 주의(2026-10-23).** 운영 토큰으로 수동 세션을 하나 더 열면 Discord가 실제 사용자의
  인터랙션을 그 세션으로 보낼 수 있다(아래 8절에서 s=5 인터랙션이 테스트 세션에 왔고, 운영 봇 대신 그쪽으로
  간 것으로 보인다). 응답하지 않는 테스트 세션이 받으면 사용자는 "상호작용 실패"를 본다. 앞으로 운영
  토큰으로 라이브 세션을 열지 않는다. 검증은 모의·루프백 게이트웨이 테스트로 하고, 라이브가 꼭 필요하면
  운영과 분리된 테스트 봇 토큰을 쓴다.

## 8. 라이브 기록 (2026-10-23, 수동)

운영 봇(Palladium V2) 토큰, GUILDS intent만, REST 쓰기·명령 등록·인터랙션 응답 없음. harness는 저장소
밖(`/tmp/drs-live-handoff`), 스냅샷은 0600 임시 파일로 쓰고 읽은 즉시 지웠다. 토큰은 로그·출력에 없음을
확인했다(grep 0건).

| 단계 | 결과 |
|---|---|
| 프로세스 1 | IDENTIFY → READY(s=1), GUILD_CREATE 2개, 3초 뒤 handoff. `start()`는 요청 1 ms 뒤 `Ok(())`. 스냅샷 sequence 3, resume URL `wss://gateway-us-east1-b.discord.gg/`, application/user id 포함 |
| 프로세스 2 | 스냅샷을 읽고 지움 → resume URL로 연결, RESUME → **RESUMED(s=4), READY 없음**. handoff부터 RESUMED까지 약 0.47초. 이후 heartbeat ACK, INTERACTION_CREATE(s=5) 수신(응답하지 않음). 60초 뒤 정상 종료(1000) |
| 잘못된 스냅샷 | 만든 session id로 RESUME → INVALID_SESSION(false) → 2초 뒤 경고 → IDENTIFY → READY |

IDENTIFY는 모두 2회(프로세스 1, 잘못된 스냅샷)였다. 프로세스 2의 60초 동안 실제 사용자의 인터랙션 1건이
테스트 세션으로 왔다. 같은 토큰의 다른 세션(운영 봇)도 그 인터랙션을 받았는지는 확인하지 않았지만,
Discord가 인터랙션을 한 세션에만 보낸 것으로 보고 이후 라이브 테스트를 중단했다(7절 주의).

## 9. 리뷰 수정 (2026-10-23, GPT-6.1-sol의 `f76ee19` 리뷰)

리뷰어는 `/tmp`의 탐침으로 세 문제를 각각 재현했다. 공개 API는 바뀌지 않았다.

### 9.1 정지와 enqueue 경합으로 이벤트를 영구히 건너뜀 (P1)

처리기가 sequence 4242를 장부에 기록한 직후, handoff가 레인을 닫고 나서 `try_send`가 `Closed`를 받으면
`unroute()`가 그 이벤트를 "넘김"으로 세었다. 재개점이 4242로 올라가 RESUME이 그 이벤트를 건너뛰었다.
두 레인 모두 같은 경로가 있었다. 정지 뒤 가득 찬 레인의 `Full`도 같은 손실을 낳을 수 있었다.

수정: `DeliveryLedger::enqueue`가 기록과 `try_send`(기다리지 않음)를 한 잠금 안에서 하고, `halt`도
그 잠금을 잡은 뒤 정지 표시를 한다. 파이프라인은 장부를 먼저 정지하고 그다음 디스패처에 알린다. 그래서
기록된 이벤트는 반드시 레인에 들어가 있거나(정지한 레인이 대기로 남긴다) 정지 전에 거절된 것이다.
`Closed`는 대기로 남긴다. `Full`은 정지 전의 정상 넘침이므로 기존처럼 넘김(일반 레인은 손실, 상호작용
레인은 overload hook)으로 센다.

### 9.2 RESUME 연결 설정에서 handoff가 영원히 멈춤 (P1)

RESUME은 IDENTIFY permit을 받지 않으므로 연결 설정에 제한 시간이 없었고, 설정 중에는 명령 채널을 읽지
않았다. 업그레이드는 받고 HELLO를 보내지 않는 서버를 만나면 `request()`가 끝나지 않았다. 정상 종료도
같았다.

수정: 연결·업그레이드·HELLO 대기를 자기 상태를 빌리지 않는 `open_connection`으로 분리하고,
`connect_and_run`이 그것과 명령 채널을 함께 기다린다(명령 우선). Shutdown은 바로 종료하고, handoff는
세션이 있으면 그대로 넘긴다. 이 연결에는 아직 아무것도 보내지 않았으므로 세션은 바뀌지 않는다.
Reconnect는 무시하고, 다른 명령은 backoff 때처럼 다음 연결로 미룬다. 설정 전체는 모든 연결에서
15초(`CONNECTION_SETUP_TIMEOUT`)로 제한된다. 넘으면 연결 실패처럼 resume URL을 버리고 재시도한다.
IDENTIFY 간격 대기(최대 5초)와 INVALID_SESSION 대기(2초)는 제한된 시간이라 그대로 두었다.

```text
[Decision Log]
- 목적과 의도: 어떤 연결 단계에서도 handoff·종료 요청이 제한된 시간 안에 끝나게 한다.
- 기존 구현 및 제약 조건: 설정 제한 시간은 permit을 쥔 IDENTIFY 연결에만 있었다. 명령은 permit 대기,
  backoff, 읽기 루프에서만 읽었다. 설정 단계는 &mut self를 빌려 명령 채널과 함께 select할 수 없었다.
- 검토한 주요 대안: (1) RESUME에도 제한 시간만 둔다 — 요청이 최대 15초 늦는다. (2) handoff를
  CancellationToken 같은 별도 신호로 모든 await에 넣는다 — 대기 지점마다 새 경로가 생긴다.
  (3) 설정을 self를 빌리지 않는 future로 분리하고 기존 명령 채널과 select한다 + 모든 연결에 제한 시간.
- 선택한 방식: (3).
- 다른 대안 대신 이 방식을 선택한 이유: 요청이 즉시 처리되고, 명령 처리 규칙(미루기·한도)이 permit
  대기·backoff와 같다. 제한 시간은 요청이 없을 때도 샤드가 멈추지 않게 한다.
- 장점, 단점 및 영향: 설정 중에 받은 Shutdown은 다음 IDENTIFY 전에 종료한다(기존 테스트 두 개의 명령
  시점을 맞췄다). 15초 안에 HELLO를 못 받는 느린 네트워크에서는 재시도가 생긴다(Discord HELLO는 보통
  1초 안에 온다).
```

### 9.3 fallback 뒤에도 거절된 스냅샷의 application id가 남음 (P2)

스냅샷의 application id(예: 123)를 쓴 뒤 RESUME이 실패하고 IDENTIFY의 READY가 999를 가져와도, READY
처리기는 id가 없을 때만 채웠으므로 REST 경로가 계속 123을 썼다.

수정: 스냅샷에서 온 id에 공유 플래그(`application_id_provisional`, 모든 샤드의 `Context`가 공유)를
세운다. READY 처리기는 id가 없거나 임시값이면 READY의 id로 바꾸고 플래그를 내린다
(`Context::adopt_ready_application_id`). 빌더가 정한 id는 임시값이 아니므로 바뀌지 않는다. 재개에
성공한 세션은 READY가 없으므로 임시값을 그대로 쓴다(같은 세션이라 맞는 값이다).
