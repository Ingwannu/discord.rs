# discord.rs 한국어 문서

이 문서는 discord.rs 2.0.2 기준으로 갱신되었습니다. 현재 공개 API는 타입드 Gateway 런타임, 타입드 REST 클라이언트, HTTP Interactions Endpoint, Components V2 빌더, 캐시/컬렉터 계층, 샤딩 헬퍼, 고수준 앱 프레임워크, Webhook Events, Lobby 헬퍼, Voice 런타임 기반을 포함합니다.

브랜드 표기는 `discord.rs`를 사용하고, crates.io 패키지 이름과 Rust import 경로는 `discordrs`를 유지합니다.

## 빠른 링크

- [시작하기](#/ko/guide/getting-started)
- [아키텍처](#/ko/guide/architecture)
- [사용 가이드](#/ko/guide/usage-guide)
- [빌더 API](#/ko/api/builders)
- [Gateway API](#/ko/api/gateway)
- [HTTP 및 헬퍼 API](#/ko/api/http-and-helpers)

## 주요 표면

- `Client`: `EventHandler::handle_event(...)` 기반의 타입드 Gateway 봇 런타임
- `RestClient`: route별 rate-limit 상태를 관리하는 타입드 REST 클라이언트
- `try_typed_interactions_endpoint(...)`: Ed25519 검증을 포함한 signed HTTP Interactions Endpoint
- `AppFramework`: slash command, component, modal submit을 라우팅하는 고수준 앱 프레임워크
- 명령 빌더, Components V2 빌더, 캐시 매니저, 컬렉터, 샤딩 supervisor
- `connect_voice_runtime(...)`, `VoiceOpusDecoder`, `voice-encode`, live 검증된 DAVE/MLS hook
- 2026-05-02 기준 공식 Discord REST route shape 223개 전체 매핑 audit, 그리고 Webhook Events, Lobby, Poll, Subscription, Entitlement, Soundboard, Thread, Forum, Invite, Integration 등 Discord API 표면의 타입드 모델과 래퍼

## 설치 예시

```toml
[dependencies]
# 기본 REST, 모델, 빌더
discordrs = "2.0.2"

# 타입드 Gateway 봇 런타임
discordrs = { version = "2.0.2", features = ["gateway"] }

# Interactions Endpoint와 앱 프레임워크
discordrs = { version = "2.0.2", features = ["interactions"] }

# Gateway 캐시와 컬렉터
discordrs = { version = "2.0.2", features = ["gateway", "cache"] }
discordrs = { version = "2.0.2", features = ["gateway", "collectors"] }

# Voice receive / Opus decode
discordrs = { version = "2.0.2", features = ["voice"] }

# PCM to Opus encode
discordrs = { version = "2.0.2", features = ["voice", "voice-encode"] }

# DAVE/MLS hook
discordrs = { version = "2.0.2", features = ["voice", "dave"] }
```

## DAVE 상태

`voice` 기능은 Discord voice handshake, UDP/RTP transport, Opus frame 송수신, transport 암호화 처리, Opus PCM decode를 제공합니다. `dave` 기능은 DAVE/MLS 명령과 frame decrypt/encrypt hook을 노출하며, 2.0.0 릴리스에서 실제 Discord voice gateway MLS transition 검증을 통과했습니다.

## 언어 전환

문서 셸의 `LANG` 버튼으로 영어와 한국어 문서 섹션을 전환할 수 있습니다.
