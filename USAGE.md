**# discordrs 사용법

`discordrs`는 serenity 기반 봇에서 **Discord Components V2**를 쉽게 쓰기 위한 라이브러리입니다.

## 1) 설치

`Cargo.toml`:

```toml
[dependencies]
discordrs = "0.1.0"
serenity = { version = "0.12.5", features = ["client", "gateway", "model", "http", "rustls_backend"] }
```

## 2) 채널에 컨테이너 메시지 보내기

```rust
use discordrs::{button_style, create_container, send_container_message, ButtonConfig};
use serenity::all::ChannelId;
use serenity::http::Http;

async fn send_panel(http: &Http, channel_id: ChannelId) -> Result<(), discordrs::Error> {
    let buttons = vec![
        ButtonConfig::new("ticket_open", "티켓 열기")
            .style(button_style::PRIMARY)
            .emoji("🎫"),
    ];

    let container = create_container(
        "고객지원 패널",
        "아래 버튼으로 문의 티켓을 생성하세요.",
        buttons,
        None,
    );

    send_container_message(http, channel_id, container).await?;
    Ok(())
}
```

## 3) Slash Command 응답 (ephemeral)

```rust
use discordrs::{respond_with_container, create_container};
use serenity::all::CommandInteraction;
use serenity::http::Http;

async fn respond_cmd(http: &Http, interaction: &CommandInteraction) -> Result<(), discordrs::Error> {
    let container = create_container("알림", "설정이 완료되었습니다.", vec![], None);
    respond_with_container(http, interaction, container, true).await
}
```

## 4) 버튼/셀렉트(Component) 응답

```rust
use discordrs::{respond_component_with_container, create_container};
use serenity::all::ComponentInteraction;
use serenity::http::Http;

async fn respond_component(http: &Http, interaction: &ComponentInteraction) -> Result<(), discordrs::Error> {
    let container = create_container("처리 결과", "선택값이 저장되었습니다.", vec![], None);
    respond_component_with_container(http, interaction, container, true).await
}
```

## 5) 모달 제출 응답

```rust
use discordrs::{respond_modal_with_container, create_container};
use serenity::all::ModalInteraction;
use serenity::http::Http;

async fn respond_modal(http: &Http, interaction: &ModalInteraction) -> Result<(), discordrs::Error> {
    let container = create_container("완료", "모달 입력이 반영되었습니다.", vec![], None);
    respond_modal_with_container(http, interaction, container, true).await
}
```

## 6) 자주 쓰는 API

- `create_container(...)`: 제목/설명/버튼/이미지로 표준 컨테이너 생성
- `send_container_message(...)`: 채널 전송
- `respond_with_container(...)`: 슬래시 커맨드 응답
- `respond_component_with_container(...)`: 버튼/셀렉트 응답
- `respond_modal_with_container(...)`: 모달 제출 응답
- `followup_with_container(...)`: defer 이후 후속 응답
- `respond_with_modal(...)`: raw 모달 응답

## 7) 참고

- `discordrs`는 serenity가 아직 완전 지원하지 않는 V2 컴포넌트를 **raw HTTP payload**로 전송합니다.
- 버튼/셀렉트의 `custom_id`는 핸들러 라우팅 규칙과 반드시 일치시켜야 합니다.**
