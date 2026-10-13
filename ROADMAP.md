# 로드맵

## 3.0 계획 (2026-10-10 결정)

2.1.0·2.2.0은 crates.io에 배포되지 않았고(배포된 최신은 2.0.2), 이미
`Message.components`·`resolved` 타입 변경 등 2.0.2와 소스 비호환인 변경을 담고 있습니다.
따라서 다음 배포는 3.0이며, 공개 타입을 깨는 변경은 이번 3.0에 한 번에 모읍니다.

```text
[Decision Log]
- 목적과 의도: discord.js 대비 남은 기능(권한 계산, 포매터, 리액션 컬렉터, 커맨드 검증,
  API drift CI)을 얹기 전에 런타임 정확성과 공개 타입 기반을 바로잡는다.
- 기존 구현 및 제약 조건: 2.2.0 병합본은 clippy -D warnings·테스트 540개 통과. 그러나
  Serial 디스패치에서 핸들러 안 fetch_members가 타임아웃까지 막히고, 5xx·전송 오류 시
  POST까지 재시도하며, 속도 제한을 버킷 해시만으로 걸고, 웹훅 토큰이 로그에 남고,
  READY가 모든 샤드의 캐시를 지우고, 종료 close code도 무한 재접속한다.
  공개 타입은 문자열 기반 Snowflake, u8 판별값, 이벤트마다 raw Value, `_typed`와 중복된
  untyped REST 함수 41쌍, Value를 반환하는 빌더 18개를 가진다.
- 검토한 주요 대안: (1) 기능 먼저 추가 후 정리, (2) 2.x 호환을 유지하며 점진 개선,
  (3) 런타임 수정 후 공개 타입 기반을 3.0에서 한 번에 교체.
- 선택한 방식: (3). 아래 8단계 순서로 진행한다.
- 다른 대안 대신 이 방식을 선택한 이유: 2.2.0이 이미 비호환이라 3.0은 피할 수 없고,
  ID·판별값·요청 모델 교체를 미루면 3.0 직후 4.0 마이그레이션을 또 강요하게 된다.
  권한 계산 등 새 기능은 정확한 캐시와 타입 위에서만 의미가 있다.
- 장점, 단점 및 영향: 사용자 마이그레이션은 한 번으로 끝난다. 대신 3.0까지 작업량이 크고
  (L 5개, M 3개) 모든 예제·문서를 함께 갱신해야 한다. 7단계 기능은 3.1로 분리할 수 있다.
```

이 계획은 Claude(Opus 5.5)와 GPT-6-astra의 독립 리뷰 후 토론으로 합의한 것입니다.

| 단계 | 내용 | 규모 | 주요 위험 |
|---|---|---|---|
| 1 ✅ | REST 재시도 안전성(메서드·결과 모호성 기준), 버킷 식별(버킷 해시 + major parameter), 진단용 경로의 토큰 마스킹, 종료 close code 처리, 게이트웨이 워커 정리 보장 | M | 모호한 완료·재접속 가능 여부의 오분류 |
| 1.5 ✅ | 2026-05-02 감사 이후 Discord 변경 반영: 채널 가리기(`CHANNEL_OBFUSCATED`, IDENTIFY `capabilities`, 11/16 강제), 대규모 봇 샤딩 `shard` 파라미터(9/15부터 필수), 초대 대상 사용자 API, `file_types`, Component Embed 수신, Application Identity Profile API, 빠져 있던 Social SDK 로비 경로. 공식 문서 `3714862` 기준 REST 234/234 | S–M | 기존 공개 타입 위에 추가하는 작업이라 3단계 타입 교체 때 함께 옮겨야 함 |
| 2 ✅ | 내부 이벤트 처리(캐시·청크 대기·컬렉터)와 사용자 핸들러 실행 분리, 핸들러 작업 상한, 인터랙션 응답 상태(Unacknowledged → InFlight → Acknowledged, 취소 처리), 샤드 단위 캐시 무효화, 외부 캐시 백엔드 쓰기 순서 보장 | L | 순서 보장과 과부하 동작을 지키며 교착을 푸는 것 |
| 3 ✅ | `Id<M>(NonZeroU64)` 타입 ID, 열린 판별값 newtype(`ChannelType(u8)` 등), 권한 표현 통일, `Embed.components`·`flags` 수신 필드(Component Embed) | L | 잘못된 기본 ID, 다형 대상(역할/사용자), 허용 값 축소 |
| 4 ✅ | 누락/null/값을 구분하는 patch 필드 타입, 캐시 병합 수정, `Arc` 이벤트 전달, 직접 타입 디코딩, raw 보관 옵션(알 수 없는 이벤트·디코드 실패는 항상 보관), 참조 기반 컬렉터 필터 | L | 누락/null 구분이나 알 수 없는 payload 손실 |
| 5 ✅ | 타입 요청·컴포넌트 모델과 빌더 출력, REST 이름 정리(`_typed` 제거), 응답 경로 통합, Component Embed 생성 빌더(웹사이트용 payload·`<script>` 태그·제한 검증, 2026-10-11 완료) | L | wire 형태 차이, 필드 누락 |
| 6 ✅ | 멀티 프로세스 샤딩 IPC·종료·IDENTIFY 조율, 취소 가능한 오디오 읽기와 sink 전달 | L | 프로세스·작업 누수, 취소로 인한 PCM 프레임 손상 |
| 7 ✅ | 권한 계산(`permissions_for`), 포매터, 리액션 컬렉터, 커맨드 옵션 검증(아래 부록 1) | M | 불완전한 캐시를 권위 있는 권한으로 오인, 과잉 검증 |
| 8 ✅ | 라우트 기준 drift 리포트(아래 부록 2), 마이그레이션 가이드(`discordrsdocs/docs/guide/migration-3.0.md`, 코드 조각 컴파일 검증), compile-fail 테스트, MSRV 명시·검증, 할당 벤치마크(`tests/perf_report`, `ci/perf-gates.json`, 비차단 `perf-report` CI). 남은 것: 3.0.0 버전 표기와 crates.io 배포(승인 필요) | M | drift 오탐, 검증되지 않은 마이그레이션 경로 |

진행 상태(2026-10-13): 1~8단계 구현이 main에 들어갔다(각 브랜치는 GPT-6-astra 독립 리뷰 후 병합). 측정된 효과는
`docs/design/perf-baseline.md`와 CHANGELOG "Unreleased (3.0.0)"에 있다. 대형 GUILD_CREATE 할당 508,698 → 17,744,
MESSAGE_CREATE 573 → 20, 25길드 폭주 뒤 힙 386 → 64 MB, Components V2 메시지 송신 313 → 26, 인터랙션 응답 406 → 27.
크레이트만 다시 빌드하는 시간(디버그, 같은 기계, 3회 교대 측정, 2026-10-13)은 4단계 직전 `23775d1` 대비 전체 기능
33.7 → 34.8초(+3%), 기본 기능 20.6 → 22.4초(+9%)다. 요청 모델·라우트 목록·타입 빌더가 늘어난 몫이며, 의존성 빌드는 그대로다.

3~5단계의 상세 설계(타입 ID와 0/센티널 예외, 열린 판별값, `Patch`/`Nullable`, `Arc<Event>` 봉투,
요청 모델과 REST 이름, 실행 순서)는 `docs/design/3.0-types-events-requests.md`에 있다.

### 3단계 결과 (2026-10-11)

설계 문서의 시리즈 1~3으로 완료했다: 타입 ID와 다형 ID(시리즈 2 A·B), 열린 판별값으로 모든
판별값 필드·인자·쿼리·이벤트 필드·빌더 setter 이전과 중복 상수 삭제, 프레즌스 status 어휘 타입,
`Permissions` 하나로 권한 통일과 비트필드별 와이어 모양(권한은 문자열, 나머지 플래그는 숫자),
Component Embed 수신 필드(1.5단계). 기록과 측정은 `docs/design/3.0-types-events-requests.md`
§1·§2. 1단계에서 넘긴 "OAuth2 오류 본문 정리"는 아직 남아 있다(토큰 `Debug` 가리기는
`InteractionContextData`에 적용됨).

### 1단계 결과와 다음 단계로 넘긴 항목 (2026-10-10)

1단계는 astra 리뷰 지적(채널 메시지 생성 경로 한정 nonce 예외와 60초 창, 버킷 게이트 재확인,
종료 대기 중 abort 가드 유지, HELLO 전 close 분류, 관리 전송의 리다이렉트 비활성화, 요청
토큰 값 기준 진단 문자열 정리)을 반영해 완료했다. 실행 판단은 `RequestRoute`(키, major scope,
operation)를 읽고, 8단계 drift CI가 같은 메타데이터를 쓴다.

리뷰에서 나왔지만 범위가 달라 넘긴 항목:

- 2단계: `ClientBuilder::start()`가 이벤트 처리기 종료를 기다린 뒤에야 반환하는 문제,
  `gateway/supervisor.rs`가 샤드를 순서대로 await해서 뒤 샤드 실패가 가려지고 앞 샤드 오류 시
  나머지 핸들을 멈추지 않고 버리는 문제(다중 샤드 클라이언트의 즉시·완전 종료).
- 3단계: 토큰을 담은 타입(인터랙션 컨텍스트 등)의 `Debug` 출력 가리기, OAuth2 오류 본문 정리.
- 선택: 만료되지 않은 긴 차단이 남아 있는 버킷의 route → bucket 매핑을 보존 기간 정리에서 유지.

공통 원칙:

- 각 단계는 테스트·예제·문서까지 갱신된 녹색 커밋 묶음으로 끝낸다.
- 넓은 교체는 새 표현 추가 → 사용처 이전 → 옛 표현 제거 순서로 진행한다.
- 1단계에서 도입하는 라우트 기술자(메서드, 인증, major parameter, 재시도 성질)를 8단계 drift
  CI가 그대로 사용한다. 실행 코드와 별개로 손으로 관리하는 목록을 만들지 않는다.
- 공개 async trait의 `Send` 계약, 타입 ID 오용 시 컴파일 실패, 알 수 없는 판별값 왕복을
  compile-fail·왕복 테스트로 고정한다.

### Component Embed (2026-10-05 Discord 변경 로그)

웹사이트가 `<script id="discord:component-embed" type="application/json">` 또는
`<link rel="discord:component-embed" href="...">`로 Components V2 레이아웃을 링크 미리보기로
제공하는 기능입니다. 출처: discord-api-docs PR #8606 (`developers/link-previews/component-embeds.mdx`).

- 수신: 해당 미리보기가 붙은 메시지는 `type: "components"` 임베드로 오며 `components` 배열에
  단일 Container가 들어 있다. 1.5단계에서 `Embed::components`·`flags`·`is_component_embed()`로 반영했다.
- 생성: payload는 `{"component": Container}` 하나, 3,000바이트(이스케이프 시퀀스는 전체 길이로
  계산), 컨테이너 포함 컴포넌트 40개, 갤러리 항목은 임베드 전체 10개, 중첩 Container 금지,
  버튼은 `style: 5` 링크 버튼만 허용하며 `id`/`type`/`url`/`style`/`label`/`emoji`/`disabled`
  외 키가 있으면 전체 무효, 미디어 URL은 http(s)·2,048자 이하. 5단계 첫 항목으로 구현 완료
  (2026-10-11, 규격 기준 discord-api-docs `3714862`): `discordrs::request`의 `ComponentEmbed`·
  `ComponentEmbedBuilder`·`PreviewComponent` 계열, `to_json()`/`to_script_tag()`/`link_tag()`,
  `TryFrom<ContainerBuilder>`/`TryFrom<&Value>` 변환. 결정 기록은
  `docs/design/3.0-types-events-requests.md` §4 "Component Embed 생성 구현".
- 규격은 Discord가 "변경될 수 있다"고 명시하므로 한도 상수에 출처를 남기고 과잉 차단하지 않는다.

### 의존성 정리 (2026-10-11)

깨끗한 디버그 빌드가 기본 기능 34초, 전체 기능 50초였고 `syn`이 v2(serde_derive·tokio-macros·
tracing-attributes)와 v3(async-trait 0.1.90+)로 두 번 빌드됐다. 우리 lockfile을 async-trait 0.1.89로
고정해도 사용자 Cargo는 최신 0.1.x를 고르므로 의존성 자체를 없앴다.

```text
[Decision Log]
- 목적과 의도: 사용자 빌드에서 syn v3와 작은 의존성을 빼 컴파일 시간을 줄인다.
- 기존 구현 및 제약 조건: EventHandler·CacheBackend·인터랙션 핸들러·CachedManager가 #[async_trait],
  런타임은 Arc<dyn EventHandler>/Arc<dyn CacheBackend>를 저장. axum 0.7도 async-trait에 의존.
  MSRV 1.88. hex는 서명 검증의 decode 두 곳뿐.
- 검토한 주요 대안: (1) async-trait 버전 고정, (2) 런타임 전체를 핸들러 타입으로 제네릭화,
  (3) 공개 trait은 RPITIT(fn -> impl Future + Send), 내부는 비공개 박싱 trait + 블랭킷 impl.
- 선택한 방식: (3). axum은 async-trait를 쓰지 않는 0.8로 올리고, hex는 비공개 코덱으로 바꾼다.
  tokio-tungstenite는 올리지 않는다: 0.26~0.30 모두 webpki-roots ^0.26(1.0을 재수출하는 얇은 shim)을
  요구하고, 0.30은 digest 0.11·rand 0.10·getrandom 0.4 중복을 새로 들여온다.
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 사용자에게 효과가 없고, (2)는 ClientBuilder·감독자·
  파이프라인에 타입 매개변수가 번진다. (3)은 사용자 impl에서 매크로만 지우면 되고 호출당 박싱 한 번이
  유지된다(기본 handle_event 라우팅은 두 번에서 한 번으로 줄었다).
- 장점, 단점 및 영향: 파괴적 변경 — dyn EventHandler/dyn CacheBackend 불가, cache_backend가 Arc<B>를
  받음, From<hex::FromHexError> 제거, 엔드포인트 Router가 axum 0.8 타입. 사용자 future는 Send여야 하며
  tests/ui/cache_backend_future_not_send.rs가 이를 고정한다.
```

---

# 부록: 남은 감사 항목 설계 노트

2.2.0 기준으로 남아 있던 두 가지 감사 항목의 구현 방향과 주의점입니다(3.0 계획의 7·8단계).
실제 구현 시 이 문서를 기준으로 진행하되, Discord 공식 문서의 최신 한도를 항상 재확인합니다.

---

## 1. 서브커맨드 중첩 검증 — 7단계 구현 결과 (2026-10-10)

7단계에서 아래 1–8을 `src/command.rs`에 구현했다. 기준 문서는 discord-api-docs `3714862`(2026-10-09)이며,
이 설계 노트를 처음 쓴 뒤 문서가 바뀐 두 가지를 반영했다.

- **합산 한도는 8000자.** 현행 문서: "Slash commands can have a maximum of 8000 characters for combined
  name, description, and value properties ... only the longest localization for each field (including the
  default value) is counted". 처음 노트에 적었던 4000은 낡은 값이다.
- **이름 정규식은 `^[-_\u02BC\p{L}\p{N}\p{sc=Deva}\p{sc=Thai}]{1,32}$`.** ASCII `'`는 #7303이 문서에
  잘못 넣었던 것이고, PR #8567(2026-08-27)이 Discord 코드 기준인 U+02BC(ʼ)로 바로잡았다.

### 구현된 규칙 (`SlashCommandBuilder::validate`, `CommandOptionBuilder::validate`)

한도 상수와 로케일 표는 공개 모듈 `discordrs::command::limits`에 출처 주석과 함께 모았다.

1. **혼재 금지.** 같은 레벨에 `SUB_COMMAND`/`SUB_COMMAND_GROUP`과 일반 옵션이 섞이면 거부(모든 레벨).
2. **required 순서.** required 옵션은 optional 옵션보다 앞에 둔다. 레벨별로 검사하며, `required`가 적용되지
   않는 서브커맨드는 제외한다.
3. **레벨 내 이름 유일성.** 기본 이름 중복을 거부한다. 현지화 이름은 같은 레벨 다른 옵션의 기본 이름,
   같은 로케일의 다른 현지화 이름과 겹치면 거부한다. 기본값과 같은 현지화는 Discord가 무시하므로 허용한다.
4. **커맨드 전체 8000자.** 커맨드와 옵션 트리 전체의 name/description, 초이스 name, 문자열 초이스 value를
   필드별 최장 변형(기본값 포함)으로 합산한다.
5. **초이스/자동완성.** 초이스는 STRING/INTEGER/NUMBER에만, `autocomplete: true`와 동시 지정 금지,
   최대 25개, 이름 1–100자, 문자열 value 100자 이하.
6. **범위.** `min_value ≤ max_value`, `min_length` 0–6000, `max_length` 1–6000, `min_length ≤ max_length`.
   비어 있지 않은 `channel_types`는 CHANNEL 옵션에만 허용한다.
7. **현지화.** 커맨드·옵션·초이스 현지화의 로케일 키를 Discord 32개 로케일 표와 대조하고(ASCII 대소문자
   무시), 값에는 원래 필드 규칙(이름 규칙, 설명 1–100자, 초이스 이름 1–100자)을 적용한다. 엔트리 포인트
   커맨드는 길이와 로케일만 검사한다.
8. **이름 문자 집합.** 표준 라이브러리만 쓴다. 확실히 거부되는 문자만 거부한다: `[A-Za-z0-9_-]` 밖의
   ASCII, 공백·제어 문자, 구두점·기호·이모지·사용자 정의 영역 블록의 문자. 영숫자와 데바나가리·타이 블록은
   통과하고, 그 밖의 코드포인트(컴파일러 유니코드 표가 아직 모르는 신규 문자 포함)도 통과시킨다. 그래서
   결과가 Rust 버전(1.88 = Unicode 16, 1.99 = Unicode 17)에 따라 달라지지 않는다. 소문자 규칙은
   `to_lowercase()` 결과가 자기 자신이 아닌 문자를 거부한다.

### 일부러 느슨하게 둔 부분 (불확실하면 허용)

- 현지화 이름 충돌은 레벨 단위로만 본다. 문서의 "of that command"를 커맨드 전체로 읽을 근거가 확인되지 않았다.
- 숫자 초이스 value는 합산에 넣지 않는다. 문서가 숫자의 계산 방식을 정의하지 않는다.
- 다음은 검사하지 않는다: 로케일 키 대소문자, `channel_types` 값 자체와 빈 목록, `autocomplete`·
  `min/max_value`·`min/max_length`의 옵션 타입 제한과 ±2^53 범위, 초이스 value 타입과 옵션 타입의 일치,
  초이스 이름 중복, 컨텍스트 메뉴 커맨드의 빈 설명.
- 문자 집합 검사는 Discord가 거부하는 일부 문자(결합 악센트 U+0301, 목록 밖 기호, 단다 U+0964/U+0965,
  Other_Alphabetic 기호 U+24B6 등)를 통과시킨다. 이 경우 Discord가 등록 시 오류를 낸다.

### 주의점

- **로컬 검증은 Discord보다 엄격하면 안 됩니다.** 과잉 차단은 실제로는 등록 가능한
  커맨드를 막는 버그입니다. 규칙이 불확실하면 통과시키는 것이 원칙입니다 — 로컬 검증은
  fail-fast 편의 기능이지 게이트가 아닙니다.
- Discord가 한도를 상향하면 `limits` 모듈 한 곳만 고칩니다. 로케일 표는 Discord가 로케일을
  추가하면 갱신해야 합니다(그 전까지 새 로케일은 `validate()`에서만 실패하고 `build()`는 영향 없음).
- 기존 계약 유지: `validate()`는 `&self`, `build()`는 계속 무검증.
- 길이는 전부 코드포인트(`chars().count()`) 기준.
- 테스트는 경계값(정확히 25개 / 8000자 / 100자), 멀티바이트 문자열, 로케일, 컴파일러 유니코드
  버전에 따라 분류가 달라지는 신규 문자(U+088F, U+1E6C0)를 포함합니다.

---

## 2. 공식 API 문서 자동 동기화 CI (감사 7번)

> **구현 상태 (2026-10-10, 8단계 일부 완료):** 아래 원래 설계와 달리 OpenAPI 스펙 대신
> `discord-api-docs`의 `<Route>` 태그와 게이트웨이 이벤트 표를 직접 대조하는 방식으로 먼저
> 구현했다(`ci/api_drift.py`, 파이썬 표준 라이브러리만 사용, `.github/workflows/api-drift.yml`
> 주간 보고 전용, `ci/api-drift-allowlist.toml`에 항목별 사유 필수, PR마다 `--self-check`).
> 첫 실행 결과는 REST 234/234, 디스패치 이벤트 79/79이고, Discord가 문서에서 지운 길드 소유
> 경로 4개는 3.0에서 deprecated로 표시했다. OpenAPI 기반 상수·모델 필드 대조와 `RequestRoute`
> 메타데이터 기반 대조는 아직 하지 않았다(5단계 이후). MSRV 1.88을 선언하고 CI에서 검증하며,
> 차단 CI 작업은 Rust 1.99.0으로 고정하고 최신 stable은 비차단 canary로 확인한다.

### 방향 (원래 설계)

**소스는 `discord/discord-api-spec`(공식 OpenAPI 3.1)을 1차로 사용합니다.**
`discord-api-docs` 마크다운 파싱보다 훨씬 안정적입니다. 단, preview 스펙이라 일부
미문서 필드가 누락될 수 있음을 감안합니다.

파이프라인(GitHub Actions, 주 1회 cron + `workflow_dispatch`):

1. 스펙을 받아 커밋 해시 고정 파일(`ci/api-spec.lock`)과 비교 — 변경 없으면 즉시 종료.
2. 추출기(`ci/spec-sync/`, 작은 러스트 바이너리)로 OpenAPI에서 추출:
   (a) 라우트 목록(method + path template), (b) 열거 상수(컴포넌트 타입, 인텐트 비트 등),
   (c) 모델 필드 목록.
3. 크레이트 실측과 대조:
   - **라우트**: `src/http/*.rs`의 경로 문자열과 대조. 선행 리팩터로 라우트를 상수
     테이블(또는 매크로)로 모으면 대조가 기계적이 되므로 강력 추천.
   - **상수/모델 필드**: 크레이트에 `#[cfg(test)]` schema 덤프 테스트를 넣어 JSON으로
     내보낸 뒤 대조. serde rename이 반영된 실제 와이어 이름을 얻는 가장 유지보수 쉬운 방법.
4. 차이 발견 시 이슈를 자동 생성/갱신합니다(라벨 `api-drift`). 이미 열린 drift 이슈가
   있으면 새 이슈 대신 그 이슈의 본문/코멘트를 갱신해 이슈 스팸을 방지합니다.

### 주의점

- **CI를 실패시키지 않습니다.** drift는 정보이지 빌드 실패 사유가 아닙니다. PR을
  블로킹하면 Discord가 필드 하나 추가할 때마다 무관한 PR이 깨집니다. 리포트 전용
  워크플로로 유지합니다.
- **allowlist 필수.** 의도적으로 지원하지 않는 라우트/필드는 `ci/spec-ignore.toml`로
  침묵시키되, 각 항목에 사유 주석을 강제합니다. 사유 없는 allowlist는 drift 감지를
  무력화합니다.
- **이름 매핑은 단순하게.** OpenAPI `operationId` ↔ 크레이트 메서드명 자동 매핑은
  시도하지 않습니다. `(method, path)` 문자열 대조만 하고, 경로 파라미터명 차이는
  `{...}` 플레이스홀더로 정규화해 흡수합니다.
- **스펙이 실제 API보다 뒤처질 수 있습니다.** 보조 신호로 `discord-api-docs` 리포의
  changelog 디렉토리 diff를 같은 잡에서 요약해 이슈에 첨부하는 정도가 현실적입니다.
  changelog 본문의 자연어를 파싱하려 들지 않습니다.
- **게이트웨이 이벤트는 OpenAPI에 없습니다.** dispatch 이벤트 이름 목록은
  `discord-api-docs`의 게이트웨이 이벤트 문서에서 헤더/테이블만 최소 파싱으로 추출해
  별도 대조합니다.
- **권한 최소화.** 이슈 작성은 `permissions: issues: write`만 부여하고, cron 워크플로에
  `if: github.repository == 'Ingwannu/discord.rs'` 가드를 넣어 포크에서 돌지 않게 합니다.

### 단계 제안

1. **1단계 — 라우트 diff만.** 가장 값어치가 높고 가장 기계적입니다.
2. **2단계 — 상수/인텐트 비트.**
3. **3단계 — 모델 필드.** false positive가 가장 많은 영역이므로 allowlist 체계가
   자리잡은 뒤에 붙입니다.
