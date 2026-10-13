# 성능 기준선과 게이트: 게이트웨이 수신·캐시·컬렉터·송신

작성: 2026-10-11. 3.0 4단계(`3.0-types-events-requests.md` §3 "이벤트 API")가 바꿀 게이트웨이
파이프라인의 **변경 전** 수치와, 같은 수치를 다시 재는 도구(`tests/perf_report/`,
`ci/perf_diff.py`)를 기록한다. 사용자 목표인 낮은 RAM과 높은 속도를 기준으로, 4단계가 개선을
증명하고 이후 회귀를 잡는 데 쓴다.

- **4단계 비교 기준은 §8, main `23775d1`(시리즈 3 이후)이다.** 보고서 JSON은
  `docs/design/perf/23775d1.json`이고, 게이트(§9)와 CI(§10)가 이것을 쓴다.
- §2·§4·§5는 처음 기록한 main `706b7ae`(타입 ID B 단계 `a8b3427`, 음성 전송 암호·샤드 큐 수정
  포함) 기준선으로, 역사 기록으로 남긴다. 같은 하네스로 잰 B 단계 이전 main `15d98a5`와 결정적
  지표가 하나도 다르지 않았다(`size_of` 포함). `Id<M>` 마커는 같은 8바이트 표현이라 비용이 없다.

```text
[Decision Log]
- 목적과 의도: 4단계(얕은 봉투, 빌린 RawValue에서 직접 타입 디코딩, Arc<Event> 공유, 패치 인식
  캐시) 전후를 같은 입력·같은 지표로 비교할 기준선을 남기고, 이후 변경의 RAM·속도 회귀를 잡는다.
- 기존 구현 및 제약 조건: tests/id_footprint.rs는 decode_event의 할당 수만 쟀다. 프레임 파싱,
  process_gateway_dispatch, apply_cache_updates, CollectorHub::publish는 비공개다. 벤치 의존성이
  없고 MSRV 1.88, Cargo [lints] 규칙을 따른다. 같은 시기에 다른 작업자가 src/model, src/http,
  음성·샤딩을 고치므로 src/는 건드리지 않는다.
- 검토한 주요 대안: (1) criterion/divan 벤치, (2) 라이브러리 안의 #[doc(hidden)] 측정 훅,
  (3) 공개 API로 비공개 단계를 그대로 재현(replica), (4) 실제 Client를 가짜 게이트웨이에 붙이는
  e2e(같은 프로세스 또는 별도 프로세스).
- 선택한 방식: (3)과 (4)를 함께 쓴다. e2e 봇과 가짜 게이트웨이는 각각 테스트 바이너리를 다시
  실행한 별도 프로세스다. 계수 전역 할당자(id_footprint 방식)에 할당 바이트·최대 live 바이트와
  시간 측정 중 끄는 스위치를 더했다. 결과는 JSON 한 줄/시나리오 + target/perf-report.json,
  비교는 ci/perf_diff.py(표준 라이브러리만).
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 dev-dependency·MSRV 부담이 있고 할당을 재지
  못한다. (2)는 공개 표면을 늘리고 src/ 작업과 충돌한다. (3)은 단계별로 결정적인 수치를 주지만
  비공개 코드와 어긋나도 스스로는 모르므로 (4)로 교차 확인한다(모든 이벤트에서 e2e = replica +
  정확히 3할당). 같은 프로세스의 e2e는 서버 쪽 할당·RSS가 섞이고, 폭주 시나리오가 glibc 힙을
  조각내 뒤이은 replica 시간이 크게 늘었다.
- 장점, 단점 및 영향: replica와 캐시·팬아웃·송신의 할당 수·바이트·보유·최대 힙은 실행마다
  같아서(두 번 실행해 확인) 작은 회귀도 보인다. 비공개 단계가 바뀌면 replica를 손으로 맞춰야
  한다(단계마다 원본 위치를 주석으로 적었고 e2e와의 차이로 어긋남이 드러난다). 시간과 RSS는
  기계·부하에 따라 흔들린다. 한 번 실행에 약 20초가 든다.
```

## 1. 실행과 비교

```bash
# 보고서(릴리스 필수). 약 20초, target/perf-report.json에 저장
cargo test --release --all-features --locked --test perf_report -- --ignored --nocapture --test-threads=1

# 다른 경로에 저장
DISCORDRS_PERF_REPORT=/tmp/after.json cargo test --release --all-features --locked --test perf_report -- --ignored --nocapture --test-threads=1

# 두 보고서 비교(파일 또는 위 명령의 로그). ~ 표시는 잡음 지표
python3 ci/perf_diff.py before.json after.json
python3 ci/perf_diff.py before.json after.json --deterministic-only --scenario 'inbound.*'
python3 ci/perf_diff.py before.json after.json --metric 'steps.*.allocs' --format markdown
python3 ci/perf_diff.py before.json after.json --fail-on-regression 2   # 결정적 지표가 2% 넘게 늘면 exit 1

# 4단계 작업자가 병합 전에 돌리는 판정(§9): 기준선 대비 회귀 + 입력 일치 + 체크포인트 상한
python3 ci/perf_diff.py docs/design/perf/23775d1.json target/perf-report.json \
    --deterministic-only --strict --fail-on-regression 2
python3 ci/perf_diff.py --gates ci/perf-gates.json --checkpoint B --strict target/perf-report.json
```

- `--all-features`가 필요하다(테스트 파일은 `gateway`+`cache`+`collectors`가 모두 켜졌을 때만
  컴파일된다. 실제 봇과 같은 파이프라인, 특히 컬렉터 저널 복제를 재기 위해서다).
- `--scenario`/`--metric`은 출력과 실패 판정에 같이 적용된다. 입력(`payload_bytes`, `ops` 등)이나
  fixture 다이제스트(`meta.fixtures_digest`)가 다르거나 기준선의 시나리오가 사라지면 경고이고,
  `--strict`에서는 실패다. 4단계에서 단계 이름이 바뀌어 `steps.*` 지표가 사라지는 것은 실패가
  아니다(사라진 지표는 표에 `-`로 보인다).
- 동반 테스트 `perf_fixtures_decode`(기본 `cargo test --all-features`에 포함)는 fixture가 오류나
  `.ok()` 대체 경로로 빠지지 않고 끝까지 디코딩되는지 검사한다.

## 2. 측정 환경 (`706b7ae` 기준선)

| 항목 | 값 |
|---|---|
| 커밋 | main `706b7ae`. 보고서 메타의 커밋은 그 위에 하네스·`perf_diff`만 더한 `6d7cac9`(`src/`는 `706b7ae`와 같음, 작업 트리 깨끗함) |
| 기계 | `ingwdev1` (a5302 Proxmox VM200), Intel Xeon Gold 6248 @ 2.50GHz, vCPU 16, RAM 31 GiB, clocksource `kvm-clock` |
| OS | Linux 6.8.0-142-generic x86_64, glibc malloc(기본 할당자) |
| 툴체인 | rustc 1.97.1, `--release`(Cargo.toml 기본 릴리스 프로필), `--all-features` |
| 부하 | 다른 작업자의 빌드와 공유, load average 4–9 (16 vCPU) |
| 실행 | 같은 명령 2회(2026-10-11 09:47–09:48 UTC). 결정적 지표는 두 번이 같았고 `15d98a5`에서의 4회와도 같았다(예외는 `cache.message_insert.50k`의 peak, §3). 시간은 두 실행의 중앙값 범위로 적었다 |

## 3. 시나리오와 지표

| 그룹 | 시나리오 | 무엇을 재나 |
|---|---|---|
| `inbound.*` | message_create.plain / .embeds / .components_v2, message_update.partial, presence_update, interaction_create.chat_input, unknown_dispatch, floor, guild_create.large | 프레임 텍스트에서 핸들러가 받는 `Event`까지를 공개 API로 단계별 재현(replica): `frame_to_string` → `parse_frame` → `envelope` → `drop_frame` → `decode_event` → `cache_apply` → `collector_publish` → `drop_dispatch_value` → `handler_drop` |
| `ref.*` | typed_from_str.message_create.plain / .components_v2 / guild_create.large, outbound.create_message.direct_to_vec | 4·5단계가 다가갈 하한: `serde_json::from_str::<T>(d)`만, 또는 본문을 `Value` 없이 바로 `to_vec` |
| `e2e.*` | idle_client, floor, 위 7개 이벤트, guild_create_storm(서로 다른 대형 길드 25개), journal_flush_after_storm, fanout.message_collectors.k{0,1,8,64} | 별도 프로세스의 실제 `Client` + 가짜 게이트웨이. 웹소켓 읽기·큐·태스크·`Context`까지 포함 |
| `storm.*` | guild_create_25 (`23775d1`부터) | 같은 25길드를 replica로 이 프로세스에서 차례로 처리(핸들러 이벤트는 매번 놓음). 결정적인 폭주 뒤 live 힙(`retained_bytes`)과 그 중 저널 몫(`journal_bytes`, 작은 이벤트 256개로 밀어낼 때 풀리는 양)·캐시 몫(`cache_bytes`) |
| `collectors.*` | payload_copies (`23775d1`부터) | e2e 팬아웃의 할당 수에서 구한 구조 지표: 복사 1회 = 일치하는 컬렉터 하나의 이벤트당 비용(`allocs(k=1) − allocs(k=0)`), `k64_minus_k1` = 일치하지 않는 컬렉터 63개가 더하는 복사 수 |
| `cache.*` | guild_create.10x1000_members, message_insert.50k, message_update_merge.10k | 기본 `CacheConfig`(채널당 100, 전체 10,000 메시지)에서 `CacheHandle` 공개 API |
| `fanout.*` | message_collectors.k{1,8,64} | 이벤트 하나를 컬렉터 K개에 전달(하나만 일치): 저널 복제 1회 + 구독자마다 복제 + 추출 + 필터 |
| `outbound.*` | create_message.components_v2, interaction_reply.components_v2 | 빌더로 만들고 REST 클라이언트처럼 `to_value`(http/body.rs) 후 `to_vec`(reqwest `.json`) |
| `layout` / `process` | size_of, process.rss | 주요 타입 크기와 빈 `CollectorHub`의 힙(`empty_collector_hub_bytes`, `23775d1`부터; 256칸 저널 링 + 고정 136 B), e2e 봇 프로세스의 VmRSS/VmHWM |

fixture: `tests/fixtures/perf/*.json`(Discord 문서 예시를 본뜬 실제 형태). 대형 GUILD_CREATE는
`guild_create_template.json`의 항목 템플릿을 결정적으로 복제한다: 멤버 1,000(전원 온라인, presence
1,000), 채널 200(카테고리 10, 음성 30), 역할 300, 음성 상태 50, 이모지 30, 프레임 1.12 MB.

지표(모든 지표는 낮을수록 좋다):

| 지표 | 의미 | 결정적? |
|---|---|---|
| `allocs` | alloc/alloc_zeroed/realloc 호출 수 | replica·cache·fanout·outbound: 예. e2e: 거의(스케줄링으로 소수점 차이) |
| `alloc_bytes` | 그 호출들이 요청한 바이트 합(realloc은 새 크기) | 위와 같음 |
| `retained_bytes` | 작업 후에도 남은 live 바이트. `inbound.*`는 빈 캐시·빈 저널에서 한 번 디스패치한 뒤(핸들러 `Event` + 저널 사본 + 캐시 증가분) | 예(e2e 제외) |
| `peak_bytes` | 작업 중 최대 live 바이트 − 시작 시 live | 예(e2e 제외. `cache.message_*`는 아니오: std `HashMap`의 프로세스별 무작위 해시 때문에 삭제가 섞인 표가 커질지 제자리 재해시할지가 실행마다 달라 표 크기만큼(128 KiB 관측) 흔들린다) |
| `median_ns`, `min_ns` | 계수를 끈 상태의 반복 시간 중앙값/최솟값 | 아니오 |
| `steps.<step>.*` | `inbound.*`·`outbound.*`의 단계별 값. `allocs`/`alloc_bytes`/`peak_bytes`는 정상 상태(캐시에 이미 있고 저널이 가득 찬 상태), `retained_bytes`는 빈 상태, `mean_ns`는 별도 패스의 평균 | `mean_ns` 외 예 |
| `ops` 있는 시나리오 | `allocs`/`alloc_bytes`는 연산당 평균, `retained_bytes`/`peak_bytes`는 배치 전체 | |

측정 방식 메모:

- 계수 자체가 할당·해제마다 약 65 ns(원자 연산 4회)를 더해, 할당이 지배적인 이 경로의 시간을
  두 배 가까이 부풀린다. 그래서 할당은 계수 패스에서, 시간은 계수를 끈 패스(`Uncounted`)에서 잰다.
- 단계별 `mean_ns`는 참고용이다. glibc가 이전 해제의 정리를 다음 큰 malloc에서 하므로, 예를 들어
  `frame_to_string`(할당 1회 + 복사)이 직전 반복의 해제 비용 일부를 떠안는다. 합계(`median_ns`)가
  믿을 만한 값이다.
- e2e 시간은 잠금 단계(명령 → 자식 게이트웨이 → 웹소켓 → 핸들러 수신) 지연이라 파이프·소켓
  왕복(`e2e.floor` ≈ 90 µs)을 포함한다. 처리량이 아니다.

## 4. 기준선 수치 (main `706b7ae`, 릴리스, 역사 기록)

### 수신 파이프라인 replica (`inbound.*`) 와 하한 (`ref.*`)

| 시나리오 | 프레임 B | allocs | alloc B | retained B | peak B | 시간(중앙값, 2회) |
|---|---:|---:|---:|---:|---:|---:|
| `inbound.message_create.plain` | 1,571 | 573 | 54,345 | 27,661 | 28,484 | 56.5 µs – 56.9 µs |
| `inbound.message_create.embeds` | 3,735 | 1,505 | 154,359 | 80,144 | 88,911 | 167.5 µs – 186.3 µs |
| `inbound.message_create.components_v2` | 2,615 | 1,031 | 133,373 | 78,430 | 77,545 | 115.3 µs – 123.9 µs |
| `inbound.message_update.partial` | 553 | 217 | 24,187 | 7,279 | 12,742 | 18.8 µs – 19.0 µs |
| `inbound.presence_update` | 772 | 376 | 43,103 | 20,650 | 26,444 | 26.2 µs – 26.7 µs |
| `inbound.interaction_create.chat_input` | 3,956 | 1,519 | 171,477 | 75,064 | 102,253 | 167.4 µs – 181.3 µs |
| `inbound.unknown_dispatch` | 951 | 278 | 20,740 | 9,528 | 14,292 | 18.7 µs – 19.4 µs |
| `inbound.floor` (빈 `d`) | 46 | 10 | 751 | 34 | 717 | 1.9 µs – 2.0 µs |
| `inbound.guild_create.large` | 1,121,980 | 514,698 | 60,484,635 | 28,076,697 | 34,762,376 | 161.9 ms – 188.9 ms |
| `ref.typed_from_str.message_create.plain` | 1,530 | 15 | 1,914 | 1,906 | 1,914 | 4.4 µs – 4.5 µs |
| `ref.typed_from_str.message_create.components_v2` | 2,574 | 38 | 20,108 | 17,020 | 17,084 | 10.4 µs |
| `ref.typed_from_str.guild_create.large` | 1,121,941 | 14,567 | 4,391,264 | 3,190,816 | 3,190,816 | 4.1 ms – 4.2 ms |

단계별(`steps`), MESSAGE_CREATE plain과 대형 GUILD_CREATE (allocs·bytes는 정상 상태, mean은 별도 패스 평균):

| 단계 (원본 위치) | plain allocs | plain alloc B | plain mean | guild allocs | guild alloc B | guild mean |
|---|---:|---:|---:|---:|---:|---:|
| `frame_to_string` (gateway/compression.rs:16) | 1 | 1,571 | 5.6 µs | 1 | 1,121,980 | 7.8 ms |
| `parse_frame` (gateway/client.rs:586) | 112 | 9,953 | 9.8 µs | 92,571 | 10,549,773 | 37.9 ms |
| `envelope` (`payload["d"].clone()`, client.rs:602) | 107 | 9,060 | 3.3 µs | 92,528 | 10,289,360 | 19.0 ms |
| `drop_frame` | 0 | 0 | 3.0 µs | 0 | 0 | 22.0 ms |
| `decode_event` (pipeline.rs:987, event/decode.rs) | 214 | 18,516 | 15.3 µs | 187,251 | 22,374,776 | 59.9 ms |
| `cache_apply` (bot.rs `apply_cache_updates`) | 19 | 5,533 | 4.3 µs | 35,291 | 3,911,478 | 9.5 ms |
| `collector_publish` (pipeline.rs:1007) | 120 | 9,712 | 14.4 µs | 107,056 | 12,237,268 | 20.6 ms |
| `drop_dispatch_value` | 0 | 0 | 2.6 µs | 0 | 0 | 13.9 ms |
| `handler_drop` | 0 | 0 | 3.2 µs | 0 | 0 | 19.0 ms |

빈 상태 한 번의 보유량 분해(plain): 핸들러 `Event` 9,712 B + 저널 사본 9,712 B + 캐시 8,237 B = 27,661 B. 대형 길드: `Event` 12.24 MB + 저널 사본 12.24 MB + 캐시 3.60 MB.

### 실제 Client (`e2e.*`, 별도 프로세스)

| 시나리오 | allocs/프레임 | alloc B/프레임 | retained B | peak B | 지연(중앙값, 2회) |
|---|---:|---:|---:|---:|---:|
| `e2e.idle_client` (연결·IDENTIFY·READY) | 305 (합계) | 3,066,066 (합계) | 1,789,451 | 2,536,134 | – |
| `e2e.floor` | 13 | 11,656 | 0 | 10,921 | 79.6 µs – 83.7 µs |
| `e2e.message_create.plain` | 576 | 65,251 | 0 | 28,484 | 165.1 µs – 167.4 µs |
| `e2e.message_create.embeds` | 1,508 | 165,265 | 10,904 | 88,911 | 325.6 µs – 326.7 µs |
| `e2e.message_create.components_v2` | 1,034 | 144,279 | 0 | 77,545 | 259.3 µs – 273.3 µs |
| `e2e.message_update.partial` | 220 | 35,093 | 0 | 14,529 | 118.7 µs – 121.8 µs |
| `e2e.presence_update` | 379 | 54,009 | 0 | 26,444 | 122.3 µs – 124.5 µs |
| `e2e.interaction_create.chat_input` | 1,522 | 182,383 | 0 | 102,253 | 307.6 µs – 362.0 µs |
| `e2e.unknown_dispatch` | 281 | 31,646 | 0 | 15,668 | 131.4 µs – 143.3 µs |
| `e2e.guild_create_storm` (25길드, 계수 켠 채 시간 측정) | 514,716 | 63,075,297 | **389,366,829** | 411,898,224 | 129.9 ms – 132.6 ms/길드 |
| `e2e.journal_flush_after_storm` (작은 이벤트 256개) | 13 | 11,657 | **-307,027,864** | 6,174 | 203.3 µs – 231.3 µs |
| `e2e.fanout.message_collectors.k1` | 696 | 74,963 | 349,020 | 379,348 | 313.7 µs – 416.6 µs |
| `e2e.fanout.message_collectors.k8` | 1,536 | 142,947 | 0 | 30,328 | 630.5 µs – 687.8 µs |
| `e2e.fanout.message_collectors.k64` | 8,256 | 686,819 | 0 | 44,985 | 1.1 ms – 1.3 ms |

- 모든 이벤트에서 e2e = replica + 3할당·약 10.9 KB(`e2e.floor` 13/11,656 vs `inbound.floor`
  10/751). replica가 비공개 경로를 그대로 따른다는 근거이며, 3할당은 핸들러 레인의 몫이다(핫스팟 4).
- 작은 이벤트의 `retained_bytes`는 저널을 같은 종류로 256+20개 채운 정상 상태라 0에 가깝다
  (스케줄링에 따라 이벤트 한두 개 크기, 예: embeds 10,904 B).
  `fanout.k1`의 보유량(349,020 B)은 버리는 패스 뒤에도 남는 1회성 증가분이다(§5 마지막 항목).
- 폭주 시간은 계수를 켠 채 쟀다(같은 25길드를 계수 없이 다시 보내면 메모리가 두 배가 된다).
  replica(단일 스레드)보다 짧은 것은 실제 런타임에서 읽기 루프의 프레임 해제가 처리기의
  디코딩과 다른 워커에서 겹치기 때문이다.
- 프로세스 RSS(KiB): 시작 4,156 → READY 후 9,504 → 작은 이벤트 시나리오 후 35,300 → 폭주 후
  **541,644** → 저널을 비운 뒤에도 541,644(glibc가 OS에 돌려주지 않음). VmHWM 541,900(2회차 567,124).
  RSS는 최대 힙에 묶이므로 보유량뿐 아니라 최대 힙(`peak_bytes`)도 RAM 목표의 지표다.

### 캐시, 팬아웃, 송신, 크기

| 시나리오 | allocs | alloc B | retained B | peak B | 시간(중앙값, 2회) |
|---|---:|---:|---:|---:|---:|
| `cache.guild_create.10x1000_members` (길드당 allocs·B, 보유는 10길드) | 35,316.4 | 6,735,818 | 34,449,734 | 34,449,734 | 7.0 ms – 7.3 ms |
| `cache.message_insert.50k` (삽입당, 남는 메시지 10,000) | 1.97 | 5,033 | 58,110,248 | 58,656,802 | 2.1 µs – 2.2 µs |
| `cache.message_update_merge.10k` (갱신당) | 28.17 | 6,066.97 | 290,752 | 297,107 | 4.9 µs – 5.0 µs |
| `fanout.message_collectors.k1` | 240 | 19,424 | 10,378 | 19,424 | 14.1 µs |
| `fanout.message_collectors.k8` | 1,080 | 87,408 | 10,378 | 20,090 | 59.0 µs – 59.2 µs |
| `fanout.message_collectors.k64` | 7,800 | 631,280 | 10,378 | 20,090 | 423.3 µs – 423.7 µs |
| `outbound.create_message.components_v2` (본문 1,011 B) | 313 | 46,567 | 0 | 28,075 | 22.0 µs |
| `outbound.interaction_reply.components_v2` (본문 1,029 B) | 406 | 63,072 | 0 | 30,322 | 28.7 µs – 28.8 µs |
| `ref.outbound.create_message.direct_to_vec` | 224 | 32,750 | 0 | 24,278 | 17.9 µs – 18.0 µs |

송신 단계: create_message = build 220 allocs / `serialize_body`(`to_value`) 89 / `to_vec` 4,
interaction_reply = build 309(내부 `to_value` 포함) / `serialize_body` 92 / `to_vec` 5.

`size_of`(B): `Event` 4,848, `Message` 4,808, `Guild` 856, `Member` 728, `Channel` 536, `Context` 480,
`User` 408, `Presence` 160, `Value` 32.

## 5. 4단계 설계에 넘기는 핫스팟 Top 5

1. **컬렉터 저널이 이벤트를 통째로 복제해 256개까지 붙잡는다.**
   `src/gateway/bot/pipeline.rs:1007` `ctx.collectors.publish(event.clone())`, 저널 링
   `src/collector.rs:245`(`VecDeque::with_capacity`, 기본 256 `:35`), 구독자마다
   `src/collector.rs:400` `event.clone()`(일치하지 않는 컬렉터도 복제 후 필터).
   근거: 대형 길드 25개 폭주 뒤 live 힙 389.4 MB 중 **307.0 MB(79%)가 저널**(작은 이벤트 256개로
   밀어내자 307,027,864 B 해제). GUILD_CREATE 한 번에 107,056 allocs / 12.2 MB / 21 ms,
   MESSAGE_CREATE 한 번에 120 allocs(전체의 21%) / 9.7 KB. 컬렉터 1개당 이벤트마다 +120 allocs,
   +9.7 KB, 약 +6.5 µs(k64: 7,800 allocs·423 µs vs k1: 240·14 µs, e2e k64 8,256 vs k1 696/프레임).
   `Arc<Event>` 저널 + 참조 필터 + 일치 시에만 복제로 이 항목 전체가 포인터 복사가 된다.
2. **`d`를 `Value` 트리로 파싱한 뒤 세 번 더 통째로 복제한다.**
   `src/gateway/client.rs:586`(`from_str::<Value>`), `:602`(`payload["d"].clone()`),
   `src/gateway/bot/pipeline.rs:987`(`decode_event(&event_name, data.clone())`),
   `src/event/decode.rs:21`·`:77` 등(`serde_json::from_value(data.clone())` + `raw: data`).
   근거: MESSAGE_CREATE에서 parse+envelope+decode가 573 중 433 allocs(76%), 54.3 KB 중 37.5 KB.
   같은 `d`를 `from_str::<Message>`로 바로 디코딩하면 15 allocs / 1.9 KB / 4.4 µs(파이프라인 전체
   57 µs). GUILD_CREATE는 514,698 중 372,350 allocs(72%), 60.5 MB 중 43.2 MB이고, 단계 평균 합 약
   209 ms 중 파싱·복제·디코딩·`Value` 해제가 153 ms다. 타입 직접 디코딩은 14,567 allocs /
   4.1–4.2 ms(할당 35배 적고 39–46배 빠름), 최대 힙 3.2 MB vs 34.8 MB(1.1 MB 프레임의 31배).
3. **모든 이벤트가 원본 `Value`(raw)를 타입 결과와 함께 들고 다닌다.**
   `src/event.rs:41-44`(`GuildEvent { guild, raw: Value }`), `:137-140`(`MessageEvent`), 그 외
   이벤트 구조체의 `raw`, 채우는 곳 `src/event/decode.rs:22`·`:78`.
   근거: 이벤트 힙(`decode_event` retained) MESSAGE_CREATE 9,712 B vs 타입만 1,906 B(5.1배),
   GUILD_CREATE 12.24 MB vs 3.19 MB(3.8배). 이 크기가 저널(1번)과 컬렉터마다 곱해지고, 핸들러가
   대형 길드 이벤트를 놓는 데만 19 ms가 든다. MESSAGE_UPDATE 병합은 raw의 키 집합을 읽는다
   (`src/gateway/bot.rs:2716`, 4단계의 타입 `MessageUpdate`가 대체).
4. **`Event`가 4,848 B 인라인 값이라 전달마다 그 크기의 상자 두 개가 생긴다.**
   `src/gateway/bot.rs:1470` `Box::pin(self.handle_event(ctx, event))`,
   `src/gateway/bot/pipeline.rs:707-710` 핸들러 태스크 spawn, 저널 링 `src/collector.rs:245`.
   근거: 내용이 없는 디스패치도 실제 런타임은 replica보다 +3 allocs, +10.9 KB(`e2e.floor`
   11,656 B vs `inbound.floor` 751 B), 이벤트 종류와 무관하게 같다. 대기 중 샤드가 붙잡는
   1.79 MB 중 1.24 MB가 빈 저널 링(256 × 4,848 B)이다. `Arc<Event>`면 링은 2 KB, 전달 상자는
   `Context`(480 B) 수준으로 줄어든다.
5. **캐시 갱신이 이벤트가 이미 가진 데이터를 다시 복제한다.**
   `src/cache.rs:369` `apply_guild_create(&Guild)`(멤버·유저 `:400-403`, presence·채널·역할도
   `clone`, 길드는 `without_create_collections` `:376` 복제), `src/gateway/bot.rs:2702`
   `upsert_message(event.message.clone())`, MESSAGE_UPDATE `src/gateway/bot.rs:2708-2718`의
   `cache.message()` 전체 복제(`src/cache.rs:797-800`)와 키 `HashSet<String>`(`bot.rs:2864-2869`).
   근거: GUILD_CREATE `cache_apply` 35,291 allocs / 3.9 MB / 9.5 ms(길드당 캐시 보유 3.44 MB),
   MESSAGE_CREATE 19 allocs / 5.5 KB, MESSAGE_UPDATE 36 allocs / 6.6 KB(캐시 단독 측정 28.2 allocs
   / 6.1 KB / 4.9–5.0 µs). 이벤트 소유권을 넘겨받거나(`Arc` 공유) 패치만 적용하면 사라진다.

그 밖의 관찰(4단계 범위 밖이거나 작은 것):

- 인터랙션 파서가 `data`를 통째로 복제하고(`src/parsers/interaction.rs:298`) `resolved`를 다시
  복제한다(`:311-314`). 3.9 KB 페이로드의 `decode_event`가 652 allocs로, 비슷한 크기의 임베드
  메시지(547)보다 많다.
- 프레임 텍스트 복사(`src/gateway/compression.rs:16` `text.to_string()`): 이벤트당 할당 1회지만
  프레임 크기만큼(대형 길드 1.12 MB) 복사한다. 얕은 봉투가 `&str`에서 바로 파싱하면 없어진다.
- 송신: `src/http/body.rs:57` `serialize_body`가 본문을 `Value`로 바꾼 뒤 reqwest가 다시
  `to_vec`한다(create_message 313 중 89 allocs, 28%). 빌더 자체도 `Value` 트리를 만든다(220).
  바로 `to_vec`하면 224 allocs, 약 18% 빠름(22.0 → 18.0 µs). 5단계 타입 요청 모델의 근거.
- e2e 팬아웃 k=1의 "1회성" 보유량(1,939,000 B, 버리는 패스를 넣은 뒤 349,020 B)은 할당이 아니라
  저널 교체였다(`23775d1` 작업에서 확인). 앞선 시나리오가 남긴 작은 floor 이벤트 칸을 9,712 B짜리
  MESSAGE_CREATE 사본이 차례로 밀어내며 늘어난 몫이다(200칸 × 9,712 B ≈ 1.94 MB, 36칸 ≈ 0.35 MB).
  지금은 워밍업으로 저널 256칸을 먼저 채우므로 0이다.

## 6. 공개 API로 재지 못한 것과 replica의 한계

- 비공개 단계(`process_gateway_dispatch`, `apply_cache_updates`, `merge_message_update`,
  `presence_from_update`, `CollectorHub::publish`)는 replica가 공개 API로 같은 동작을 다시 짠다.
  `#[doc(hidden)]` 훅은 추가하지 않았다. 대신 e2e가 실제 코드를 돌려 차이를 확인한다(모든 이벤트에서
  +3 allocs로 일정). `merge_message_update`는 fixture가 가진 키만 옮기도록 재현했다.
- 핸들러 레인(입력·핸들러 큐, `Box::pin`, `JoinSet`, `Context` 복제)과 웹소켓 읽기는 e2e로만 보이며
  단계별로 나눌 수 없다.
- `CollectorHub::publish`가 비공개라 컬렉터 복사 수(`collectors.payload_copies`)는 e2e 프로세스의
  할당 수에서 구한다. e2e 할당 수는 지금까지 실행마다 정확히 같았고, 지표는 복사 단위로 나눈 뒤
  소수 둘째 자리에서 반올림하므로 스케줄링 잡음에 강하다.
- 측정하지 않은 경로: 전송 압축(zlib/zstd, 가짜 게이트웨이는 평문), READY 처리, 외부 캐시 백엔드
  전달(`forward_cache_backend_updates`), 음성 상태 갱신, 샤딩 감독, REST 전송 자체(reqwest·TLS).
- 시간: 공유 VM이라 같은 명령에서도 작은 시나리오 중앙값이 최대 ±60% 흔들렸다. 비교는 결정적
  지표를 먼저 보고, 시간은 같은 기계에서 여러 번 돌려 `min_ns`와 함께 본다.
- RSS는 glibc malloc과 런타임 워커 4개 기준의 프로세스 전체 값이다.

## 7. 유지 규칙

- 게이트웨이 수신 경로(`gateway/client.rs` 읽기 루프, `gateway/bot/pipeline.rs`,
  `apply_cache_updates`, `collector.rs` 발행)를 바꾸면 `tests/perf_report/inbound.rs`의 해당 단계도
  바꾼다. 같은 PR에서 보고서를 돌려 e2e와 replica의 차이가 그대로인지 본다. 4단계처럼 단계 자체가
  바뀌면 단계 이름이 바뀌어도 된다(perf_diff가 사라진·새 지표를 보여 준다).
- 시나리오를 추가할 때는 같은 입력을 전후 커밋에서 쓸 수 있게 fixture를 파일로 두고, 결정적이지
  않은 지표는 `deterministic` 목록에서 뺀다.
- 기준선을 다시 기록할 때는 새 절(커밋·환경·표)을 더하고 `docs/design/perf/<커밋>.json`을 커밋하며,
  `.github/workflows/perf-report.yml`의 `PERF_BASELINE`을 바꾼다. 이전 절은 역사 기록으로 둔다.
- `ci/perf-gates.json`의 상한은 지표 이름에 묶인다(`steps.cache_apply.allocs` 등). replica 단계
  이름을 바꾸면 상한 파일도 같이 고친다. layout 시나리오는 4단계 B부터 `event_kind`
  (=`size_of::<EventKind>()`)를 보고한다.
- +2% 회귀 규칙의 명시적 예외는 `ci/perf-gates.json` 체크포인트 B에 상한(측정값 + 2%)으로 두고, `note`에
  기준선 값과 이유를 적으며, 이유는 `3.0-types-events-requests.md`에 기록한다(4단계 B 예외는 "4단계 B 후속").
  부호 있는 단계 증감(해제가 사라진 `drop_frame`·`handler_drop`의 `retained_bytes`)은 상한 대신 그 기록으로
  설명한다.

## 8. 기준선 main `23775d1` (시리즈 3 이후, 4단계 비교 기준)

시리즈 3(열린 판별값)이 `Channel`·`Guild`를 줄이고 GUILD_CREATE 할당을 줄인 뒤의 main이다. 4단계의 전후 비교와 게이트는 이 절과 `docs/design/perf/23775d1.json`을 기준으로 한다. `3.0-types-events-requests.md`의 게이트 표는 기준 커밋을 `8599632`로 적었고, 같은 날 그보다 16커밋 뒤인
`23775d1`에서 재기로 했다. `8599632`도 같은 하네스로 쟀다(rustc 1.99.0). 할당 수와 모든 게이트 지표가
같았고, 다른 것은 컴포넌트가 많은 메시지의 바이트뿐이다(`inbound.message_create.components_v2`
alloc B +0.8%, embeds +0.3%, `ref.typed_from_str.message_create.components_v2` +3.1%). `ea06a27`(모달
제출 값)이 모든 `MessageComponent`에 `values`·`default`를 더해 컴포넌트당 약 24 B 커졌기 때문이다.
그래서 두 커밋 중 어느 쪽을 기준으로 해도 게이트 판정은 같다.

| 항목 | 값 |
|---|---|
| 커밋 | main `23775d1`. 보고서 메타의 커밋은 그 위에 새 시나리오·`perf_diff`·CI만 더한 `f438ed2`(`src/`는 `23775d1`과 같음, 작업 트리 깨끗함) |
| 기계 | §2와 같음(`ingwdev1`, Xeon Gold 6248, vCPU 16, glibc malloc) |
| 툴체인 | rustc 1.99.0(CI 고정 버전), `--release --all-features`. 같은 커밋을 rustc 1.97.1로 잰 결과와 결정적 지표가 모두 같았다 |
| 실행 | 1.99.0으로 2회 + 1.97.1로 1회(2026-10-11 15:00–15:10 UTC), load average 10–11. 결정적 지표는 세 번 모두 같았다(해시 시드에 묶인 `cache.message_*` peak 제외). 시간은 1.99.0 두 번의 중앙값 범위 |
| 소요 | 한 번 약 26초(새 `storm.*`가 약 5초) |

시리즈 3의 효과(`706b7ae` → `23775d1`, 결정적 지표, 바뀐 것만):

| 시나리오 | 지표 | `706b7ae` | `23775d1` | 변화 |
|---|---|---:|---:|---:|
| `inbound.guild_create.large` | allocs | 514,698 | 508,698 | -1.2% |
| `inbound.guild_create.large` | steps.cache_apply.allocs | 35,291 | 31,291 | -11.3% |
| `inbound.guild_create.large` | steps.collector_publish.allocs | 107,056 | 105,056 | -1.9% |
| `inbound.guild_create.large` | peak_bytes | 34,762,376 | 34,722,506 | -0.1% |
| `inbound.presence_update` | allocs | 376 | 370 | -1.6% |
| `ref.typed_from_str.guild_create.large` | allocs | 14,567 | 12,567 | -13.7% |
| `cache.guild_create.10x1000_members` | allocs | 35,316.4 | 31,316.4 | -11.3% |
| `cache.guild_create.10x1000_members` | retained_bytes | 34,449,734 | 34,148,624 | -0.9% |
| `layout.size_of` | channel | 536 | 488 | -9.0% |
| `layout.size_of` | guild | 856 | 784 | -8.4% |

MESSAGE_CREATE·INTERACTION_CREATE·업데이트 이벤트의 할당 수, `Event`(4,848 B)·`Message` 크기, 저널·팬아웃 비용은 그대로다.

### 수신 replica와 하한

| 시나리오 | 프레임 B | allocs | alloc B | retained B | peak B | 시간(중앙값, 2회) |
|---|---:|---:|---:|---:|---:|---:|
| `inbound.message_create.plain` | 1,571 | 573 | 54,345 | 27,661 | 28,484 | 54.3 µs – 54.5 µs |
| `inbound.message_create.embeds` | 3,735 | 1,505 | 154,359 | 80,144 | 88,911 | 166.7 µs – 167.4 µs |
| `inbound.message_create.components_v2` | 2,615 | 1,031 | 133,373 | 78,430 | 77,545 | 112.3 µs – 118.2 µs |
| `inbound.message_update.partial` | 553 | 217 | 24,187 | 7,279 | 12,742 | 18.9 µs |
| `inbound.presence_update` | 772 | 370 | 43,071 | 20,602 | 26,412 | 23.8 µs – 26.9 µs |
| `inbound.interaction_create.chat_input` | 3,956 | 1,519 | 171,093 | 74,680 | 101,869 | 161.5 µs – 169.6 µs |
| `inbound.unknown_dispatch` | 951 | 278 | 20,740 | 9,528 | 14,292 | 17.7 µs – 19.5 µs |
| `inbound.floor` | 46 | 10 | 751 | 34 | 717 | 1.8 µs – 1.9 µs |
| `inbound.guild_create.large` | 1,121,980 | 508,698 | 60,424,830 | 28,013,916 | 34,722,506 | 166.6 ms – 195.9 ms |
| `ref.typed_from_str.message_create.plain` | 1,530 | 15 | 1,914 | 1,906 | 1,914 | 4.1 µs – 4.3 µs |
| `ref.typed_from_str.message_create.components_v2` | 2,574 | 38 | 20,108 | 17,020 | 17,084 | 10.5 µs – 10.8 µs |
| `ref.typed_from_str.guild_create.large` | 1,121,941 | 12,567 | 4,356,545 | 3,168,193 | 3,168,193 | 3.9 ms – 4.0 ms |

단계별 allocs(정상 상태): plain `cache_apply` 19, `collector_publish` 120, `decode_event` 214, `envelope` 107, `frame_to_string` 1, `parse_frame` 112. 대형 길드 `cache_apply` 31,291, `collector_publish` 105,056, `decode_event` 187,251, `envelope` 92,528, `frame_to_string` 1, `parse_frame` 92,571.

### 새 결정적 시나리오

| 시나리오 | 값 |
|---|---|
| `storm.guild_create_25` | 길드당 allocs 508,713.04 / alloc B 62,822,706; 폭주 뒤 live 힙(`retained_bytes`) **385,960,728 B** = 저널 305,433,000 B(79%) + 캐시 80,527,728 B; 최대 힙 408,467,444 B |
| `collectors.payload_copies` | 복사 1회 = 120 allocs / 9,712 B(이벤트 전체 사본); k8 8회, k64 64회; **`k64_minus_k1` = 63**(바이트 기준 63), `k8_minus_k1` = 7 |
| `layout.size_of` | `Event` 4,848 B, `Message` 4,808, `Guild` 784, `Member` 728, `Channel` 488, `Context` 480, `User` 408, `Presence` 160, `Value` 32; 빈 `CollectorHub` **1,241,224 B**(256 × 4,848 + 136) |

`e2e.guild_create_storm`(실제 런타임, 388.2 MB)과 `storm.guild_create_25`(replica, 386.0 MB)는 같은 크기의 길드 25개라 거의 같다. 차이 2.2 MB는 실제 런타임 쪽(큐, 웹소켓 버퍼 등)으로 보이며 스케줄링에 따라 흔들린다. 게이트는 결정적인 replica 쪽을 쓴다.

### 캐시, 팬아웃, 송신, e2e

| 시나리오 | allocs | alloc B | retained B | peak B | 시간(중앙값, 2회) |
|---|---:|---:|---:|---:|---:|
| `cache.guild_create.10x1000_members` (길드당; 보유는 10길드) | 31,316.4 | 6,666,044 | 34,148,624 | 34,148,624 | 6.7 ms – 6.9 ms |
| `cache.message_insert.50k` (삽입당) | 1.97 | 5,033 | 58,110,248 | 58,526,010 | 2.2 µs |
| `cache.message_update_merge.10k` (갱신당) | 28.17 | 6,066.97 | 290,752 | 297,107 | 4.9 µs |
| `fanout.message_collectors.k1` | 240 | 19,424 | 10,378 | 19,424 | 13.4 µs |
| `fanout.message_collectors.k64` | 7,800 | 631,280 | 10,378 | 20,090 | 424.5 µs – 430.6 µs |
| `outbound.create_message.components_v2` | 313 | 46,567 | 0 | 28,075 | 20.5 µs – 21.5 µs |
| `outbound.interaction_reply.components_v2` | 406 | 63,072 | 0 | 30,322 | 26.8 µs – 28.5 µs |
| `e2e.floor` (프레임당) | 13 | 11,656 | 0 | 10,921 | 77.3 µs – 82.9 µs |
| `e2e.message_create.plain` (프레임당) | 576 | 65,251 | 0 | 28,484 | 161.3 µs – 163.9 µs |
| `e2e.fanout.message_collectors.k0` (프레임당) | 576 | 65,251 | 0 | 28,484 | 302.0 µs – 344.6 µs |
| `e2e.fanout.message_collectors.k1` (프레임당) | 696 | 74,963 | 0 | 28,678 | 342.8 µs – 521.8 µs |
| `e2e.fanout.message_collectors.k64` (프레임당) | 8,256 | 686,819 | 0 | 43,756 | 1.1 ms – 1.3 ms |
| `e2e.guild_create_storm` (길드당, 계수 켠 채 시간 측정) | 508,715.88 | 62,982,889.08 | 388,203,773 | 410,709,673 | 122.1 ms – 163.2 ms |

대기 중 샤드: 305 allocs, 보유 1,776,427 B. e2e 봇 RSS(KiB): 시작 4,216 → READY 후 9,804 → 폭주 전 34,736 → 폭주 후 542,152(2회차 542,072), VmHWM 541,328.

### 게이트 현황 (`23775d1`)

| 시나리오 | 지표 | `23775d1` | B 상한 | C 상한 |
|---|---|---:|---:|---:|
| `inbound.guild_create.large` | allocs | 508,698 | 55,000 | 30,000 |
| `inbound.guild_create.large` | alloc_bytes | 60,424,830 | 12,000,000 | 9,000,000 |
| `inbound.guild_create.large` | peak_bytes | 34,722,506 | 10,000,000 | 8,000,000 |
| `inbound.message_create.plain` | allocs | 573 | 60 | 40 |
| `inbound.message_create.plain` | alloc_bytes | 54,345 | 16,000 | 12,000 |
| `inbound.guild_create.large` | steps.cache_apply.allocs | 31,291 | 31,291 | 12,000 |
| `layout.size_of` | event | 4,848 | 192 | 192 |
| `layout.size_of` | event_kind | (아직 없음) | 64 | 64 |
| `layout.size_of` | empty_collector_hub_bytes | 1,241,224 | 4,096 | 4,096 |
| `collectors.payload_copies` | k64_minus_k1 | 63 | 0.25 | 0.25 |
| `storm.guild_create_25` | retained_bytes | 385,960,728 | 190,000,000 | 120,000,000 |

`23775d1`에서 B를 만족하는 것은 길드 `cache_apply`(상한이 기준선 자체)뿐이다. `event_kind`는 4단계 B부터 보고된다(48).

## 9. 게이트와 판정

`ci/perf-gates.json`은 `3.0-types-events-requests.md` "4단계 실행 결정"의 게이트 표를 체크포인트별
절대 상한으로 옮긴 것이다. C는 B를 상속하고("유지" 행) 바뀌는 행만 덮어쓴다. 단위는 바이트이고 표의
MB·KB는 십진(1 MB = 1,000,000 B), KiB는 1,024 B다.

| 게이트(표) | 보고서 지표 | 메모 |
|---|---|---|
| GUILD_CREATE allocs / alloc B / peak B | `inbound.guild_create.large` `allocs` / `alloc_bytes` / `peak_bytes` | 정상 상태 한 번 |
| MESSAGE_CREATE allocs / alloc B | `inbound.message_create.plain` `allocs` / `alloc_bytes` | |
| 길드 `cache_apply` allocs | `inbound.guild_create.large` `steps.cache_apply.allocs` | B의 "기준선 이하"는 `23775d1`의 31,291 |
| `size_of::<Event>()` / `size_of::<EventKind>()` | `layout.size_of` `event` / `event_kind` | `event_kind`는 4단계 B부터 보고(48) |
| 256칸 빈 저널 저장공간 | `layout.size_of` `empty_collector_hub_bytes` | `CollectorHub::new()`의 힙을 잰 값이라 허브 고정 비용 136 B도 들어간다 |
| 컬렉터 `k64 − k1` 추가 복사 | `collectors.payload_copies` `k64_minus_k1` | 상한 0.25: 복사 0회에 e2e 계수 잡음 여유(복사 1회는 할당 수십 개) |
| 25길드 폭주 뒤 live 힙 | `storm.guild_create_25` `retained_bytes` | replica, 핸들러 이벤트를 매번 놓은 뒤 |

판정 명령과 의미:

```bash
# 체크포인트 상한(절대값). --strict: 지표가 보고서에 없으면 실패
python3 ci/perf_diff.py --gates ci/perf-gates.json --checkpoint B --strict target/perf-report.json
# 기준선 대비 회귀(결정적 지표 +2%) + 입력·fixture 일치 + 기준선 시나리오 누락 없음
python3 ci/perf_diff.py docs/design/perf/23775d1.json target/perf-report.json \
    --deterministic-only --strict --fail-on-regression 2
# 범위를 좁혀 판정(필터는 출력과 실패 판정에 같이 적용된다)
python3 ci/perf_diff.py docs/design/perf/23775d1.json target/perf-report.json \
    --scenario 'inbound.*' --metric allocs --fail-on-regression 2
```

- 종료 코드: 0 통과, 1 회귀·상한 초과·`--strict` 위반, 2 인자·입력 오류.
- 4단계 작업 중에는 줄어드는 것이 목표라 회귀 판정은 "늘지 않음"을, 체크포인트 판정은 "목표에
  닿음"을 본다. 체크포인트 판정은 그 체크포인트를 끝냈다고 주장할 때만 쓴다.
- 시간·RSS·e2e 할당 수는 어느 판정에도 들어가지 않는다(시나리오의 `deterministic` 목록 밖).

## 10. CI

`.github/workflows/perf-report.yml`(`perf-report (non-blocking)`)이 PR마다 PR과 그 base를 같은
러너·같은 고정 툴체인(`RUST_TOOLCHAIN`, `ci.yml`과 같게 유지)으로 빌드해 보고서를 돌리고,
`ci/perf_summary.sh`가 잡 요약에 다음을 쓴다: base 대비 결정적 지표 diff와 +2%·`--strict` 판정,
모든 지표(접힘), 커밋된 기준선(`PERF_BASELINE`) 대비 diff(참고용), 체크포인트 B·C 게이트 현황.
보고서 두 개는 아티팩트로 올린다.

```text
[Decision Log]
- 목적과 의도: 4단계 PR마다 성능 변화를 사람이 직접 돌리지 않아도 보이게 하되, 시간·RSS 잡음이나
  러너 차이로 PR이 막히지 않게 한다.
- 기존 구현 및 제약 조건: 결정적 지표(할당 수·바이트·보유·최대 힙)는 할당 요청을 세므로 glibc와
  무관하고, 이 기계에서 실행 3회와 rustc 1.97.1·1.99.0 사이에서 모두 같았다(해시 시드에 묶인
  `cache.message_*` peak만 예외로 비결정 처리). 하지만 GitHub 러너에서는 아직 확인하지 않았고,
  std 내부 변경(툴체인 올림)이나 Cargo.lock 변경은 PR과 무관하게 수치를 옮길 수 있다. 4단계
  PR은 단계·시나리오를 일부러 바꾼다. 빌드는 릴리스 + 전체 기능이라 러너에서 10분 이상 걸린다.
- 검토한 주요 대안: (1) 커밋된 기준선 JSON과 비교해 실패, (2) 같은 잡에서 base와 head를 빌드해
  비교하고 실패, (3) (2)와 같지만 경고 주석과 요약만 내고 실패하지 않음, (4) CI에 넣지 않고
  로컬 판정만.
- 선택한 방식: (3). 잡은 `continue-on-error`이고 필수 검사로 두지 않는다. base 대비 결정적 +2%
  회귀나 입력 변경은 `::warning` 주석과 요약의 경고로만 낸다. 커밋된 기준선 비교와 게이트
  현황은 참고용이다. 병합 판정은 §9의 명령을 로컬에서 돌려 하고(작업자·조정자), CI 요약은 그
  확인 수단이다.
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 기준선을 잰 기계·툴체인과 러너가 달라 PR과 무관한
  차이로 실패할 수 있다. (2)는 같은 러너라 비교 자체는 안정적이지만, 4단계처럼 일부러 바꾸는
  PR과 의존성 갱신 PR을 막고 러너에서의 안정성도 아직 증명되지 않았다. (4)는 리뷰어가 수치를
  보지 못한다.
- 장점, 단점 및 영향: PR마다 같은 조건의 전후 비교가 요약에 남고 막히는 일은 없다. 대신 경고를
  무시하면 회귀가 들어갈 수 있다. 러너에서 몇 번 돌려 무관한 PR의 base·head 결정적 지표가
  같음이 확인되면, 요약 스크립트가 회귀 때 실패하게 하고 `continue-on-error`를 빼서 (2)로 올릴
  수 있다(이 문서와 워크플로를 함께 고친다).
```

## 11. 실전 읽기 전용 검증 (2026-10-13, main `5a81f20`)

운영 중인 봇 하나의 토큰으로, 서버를 바꾸지 않는 범위에서 3.0을 실제 Discord에 붙여 봤다. 토큰과 길드 이름은
기록하지 않는다. REST는 GET만 보냈고, 게이트웨이는 presence 없이 특권 없는 인텐트
(GUILDS, GUILD_MESSAGES, GUILD_MESSAGE_REACTIONS, GUILD_VOICE_STATES, GUILD_EXPRESSIONS)로
IDENTIFY했다. 인터랙션에는 응답하지 않았고 메시지를 보내지 않았다. 측정 프로그램은 리포 밖 임시 크레이트다
(`discordrs` path 의존, release 빌드).

| 항목 | 결과 |
|---|---|
| REST GET 15건 (`/users/@me`, `/gateway/bot`, 길드 목록 30개, 글로벌 커맨드 15개, 길드 3곳의 길드·채널 70/69/21개·최근 메시지 5개·이모지 101/34/37개) | 전부 성공, 타입 디코딩 오류 0 |
| 게이트웨이 60초 | READY 1, GUILD_CREATE 30(채널 최대 118개, 역할 최대 72개), 디코딩 실패 0, 알 수 없는 디스패치 0 |
| 게이트웨이 600초 | 13종 99개(MESSAGE_CREATE 13, MESSAGE_UPDATE 6, MESSAGE_DELETE 3, MESSAGE_REACTION_ADD 3, CHANNEL_CREATE/UPDATE/DELETE 3/6/3, THREAD_CREATE 1, VOICE_STATE_UPDATE 19, VOICE_CHANNEL_START_TIME_UPDATE 8, VOICE_CHANNEL_STATUS_UPDATE 3 등), 디코딩 실패 0, 알 수 없는 디스패치 0 |
| 프로세스 RSS | 시작 4.4 MB → REST 뒤 9.5 MB → 길드 30개 접속·캐시 뒤 12.9 MB → 600초 뒤 13.6 MB |

검증하지 못한 것: 음성 연결(봇이 음성 채널에 들어가면 서버에 보이는 변화가 생겨 하지 않았다), 인터랙션
응답 경로, 쓰기 REST, 특권 인텐트(GUILD_MEMBERS, GUILD_PRESENCES, MESSAGE_CONTENT)가 있는 큰 길드의
멤버·프레즌스 페이로드.

특권 인텐트 재검증(2026-10-13, 같은 봇, 같은 30개 길드, 읽기 전용): GUILD_CREATE 30개 중 11개가
`invalid type: string "…", expected struct ActivityButton`으로 디코딩에 실패해 캐시에 들어가지 못했다(활동 `buttons`는
받을 때 라벨 문자열 배열이다). PRESENCE_UPDATE 249개는 실패로 보고되지 않았지만 같은 모양의 활동이 있는 presence는
활동 목록 전체가 지워졌다. 원인, 수정, 다른 모양 차이 감사, 성능은 `3.0-types-events-requests.md`
"실전 검증 후속: 받은 활동 모양과 원소 단위 디코딩"에 있다.
