# 전체 기술 매뉴얼

자세한 기술 문서는 현재 영문 원문 기준으로 유지합니다.

- [영문 Full Manual 열기](#/docs/guide/full-manual)
- [PDF 열기](#/ko/guide/pdf-manual)

## 요약

- Builders: Components V2/Modal 구조를 플루언트하게 생성
- Gateway: heartbeat/resume/reconnect 포함 런타임
- Parsers: 인터랙션/모달을 타입 안전하게 파싱
- HTTP + Helpers: 응답 API를 간결하게 제공
- Interactions Endpoint: Ed25519 검증 기반 HTTP 콜백 처리
- 2.1.0: 감사 로그 사유(`with_reason`), 길드 생성/삭제/템플릿/MFA 라우트, `with_response=true` 콜백, 메시지 전달, Poll 인텐트, IDENTIFY 초기 presence, `fetch_members` 멤버 청크 수집, 동시 이벤트 디스패치, GUILD_CREATE 캐시 반영, 재시도 강화 REST 전송
