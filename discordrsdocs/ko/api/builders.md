# 빌더 API

`builders` 모듈은 Components V2/Modal 페이로드를 플루언트 스타일로 구성합니다.

## 하위 모듈

- `components.rs`: `ButtonBuilder`, `SelectMenuBuilder`, `ActionRowBuilder`
- `container.rs`: 컨테이너 레이아웃 + 편의 헬퍼
- `media.rs`: 섹션/썸네일/미디어 갤러리
- `modal.rs`: 텍스트 입력, 라디오, 체크박스, 파일 업로드

## 예시

```rust
let button = ButtonBuilder::new()
    .label("Open")
    .style(ButtonStyle::PRIMARY)
    .custom_id("open_ticket");
```

## 모달 전용 컴포넌트

`RadioGroupBuilder`, `CheckboxGroupBuilder`, `CheckboxBuilder`에는 `disabled` setter가 없습니다(모달에 비활성 컴포넌트가 있으면 Discord가 오류를 냅니다). 체크박스는 필수로 만들 수 없어 `CheckboxBuilder`에는 `required`도 없습니다. 반드시 체크해야 하는 항목은 옵션 하나짜리 `CheckboxGroupBuilder`에 `required(true)`를 씁니다. `CheckboxBuilder::default_checked(bool)`가 초기 상태를 정합니다(`default`로 전송, deprecated `checked(bool)`도 같은 동작).

## 파일 형식 필터 (3.0)

`FileUploadBuilder::file_types(...)`와 `CommandOptionBuilder::file_types(...)`(`ATTACHMENT` 옵션)는 업로드를 미리 정의된 그룹(`"image"`, `"video"`, `"audio"`)이나 점으로 시작하는 확장자(`".pdf"`)로 제한합니다. 최대 10개입니다. `validate()` / `try_build()`는 개수와 각 항목의 모양만 검사합니다. Discord가 그룹 목록이 바뀔 수 있다고 밝혀서 그룹 이름은 고정 목록과 대조하지 않습니다. Discord는 파일 이름만 확인하므로 파일 내용은 직접 검증해야 합니다.

## Component Embed (링크 미리보기) (3.0)

2026-10-05부터 웹사이트는 `discord:component-embed` JSON 문서를 제공해 Discord 링크 미리보기를 Components V2 레이아웃으로 바꿀 수 있습니다([Discord 문서](https://docs.discord.com/developers/link-previews/component-embeds)). 봇이나 앱은 필요 없고, Discord 크롤러가 페이지에서 문서를 읽습니다. `discordrs::request::ComponentEmbed`는 이 문서를 제한된 모델로 만듭니다. 메시지 빌더와도, 수신용 `Embed::components`와도 분리되어 있습니다.

- `ComponentEmbed { component: PreviewContainer }`. 컨테이너에는 `PreviewComponent`(텍스트 디스플레이, 링크 버튼이나 썸네일을 accessory로 가진 섹션, 미디어 갤러리, 구분선, 링크 버튼 행)만 들어갑니다.
- 버튼은 `PreviewLinkButton`뿐이고 `id`, `type`, `url`, `style`(항상 5), `label`, `emoji`, `disabled`만 내보냅니다. 중첩 컨테이너, 선택 메뉴, 파일, custom-ID 버튼, raw JSON은 표현할 수 없습니다. `PreviewActionRow::add_button(ButtonBuilder)`는 컴파일되지 않습니다.

```rust
use discordrs::request::{
    ComponentEmbed, PreviewActionRow, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem,
    PreviewSection, PreviewThumbnail,
};

let embed = ComponentEmbed::builder()
    .accent_color(0x1ABC9C)
    .add_component(
        PreviewSection::new(PreviewThumbnail::new("https://example.com/boss.png"))
            .add_text("# 새 버전 출시!\n던전이 생각보다 어려워졌습니다."),
    )
    .add_component(
        PreviewMediaGallery::new().add_item(PreviewMediaItem::new("https://example.com/1.png")),
    )
    .add_component(
        PreviewActionRow::new()
            .add_button(PreviewLinkButton::new("https://example.com/store").label("상점")),
    )
    .try_build()?;

// 인라인: 서버에서 <head>에 렌더링합니다. Discord는 스크립트를 실행하지 않으므로
// 클라이언트 코드가 나중에 넣은 태그는 보지 못합니다.
let script = embed.to_script_tag()?;

// 또는 링크: `embed.to_json()?`을 같은 사이트에서 제공하고 가리킵니다.
let link = ComponentEmbed::link_tag("https://example.com/embeds/v2.json")?;
// <link rel="discord:component-embed" type="application/json" href="…" />
```

Open Graph 태그는 그대로 두세요. Component Embed를 보여 줄 수 없을 때 Discord가 그 태그로 돌아갑니다.

`build()`는 검사하지 않습니다. `try_build()`, `validate()`, `to_json()`, `to_script_tag()`는 아래 한도를 검사하고, 규칙과 위치(예: `component.components[2].accessory.media.url`)를 담은 `ComponentEmbedError` variant로 실패합니다. 한도는 `ComponentEmbed`의 연관 상수(`MAX_PAYLOAD_BYTES`, `MAX_COMPONENTS` 등)입니다.

| 규칙 | 한도 | 규격 위치 |
| --- | --- | --- |
| 문서 크기 | 이스케이프된 JSON 3,000바이트(이스케이프 시퀀스는 전체 길이로 계산) | Component Embeds "Adding the Payload" |
| 컴포넌트 수 | 컨테이너 포함 40개(버튼, 섹션 텍스트, 썸네일도 셈) | "Compatible Components" |
| 갤러리 항목 | 임베드 전체 10개, 갤러리마다 1-10개. 썸네일은 세지 않음 | "Compatible Components" |
| 미디어 URL | 절대 `http`/`https`, 최대 2,048자 | "Media" |
| 링크 버튼 | label이나 emoji(또는 둘 다), label 최대 80자, URL 최대 512자 | "Compatible Components", Button Structure |
| 섹션 | 텍스트 디스플레이 1-3개와 accessory | Section Structure |
| 액션 행 | 링크 버튼 1-5개 | Action Row Structure |
| 미디어 설명 | 최대 1,024자 | Thumbnail / Media Gallery Item Structure |
| 강조 색 | 최대 `0xFFFFFF` | Container Structure |
| 컴포넌트 `id` | 0이 아니면 고유 | Anatomy of a Component |
| 컨테이너 | 자식 1개 이상(`ContainerBuilder::validate`와 같음) | 크레이트 규칙 |

규격이 요구하지 않거나 여기서 알 수 없는 것은 검사하지 않습니다. 미디어 확장자(Discord가 받은 내용으로 판단), `link_tag` href의 호스트(페이지 호스트, 서브도메인, 상위 도메인이어야 함), 텍스트 디스플레이 길이(어차피 3,000바이트로 묶임)입니다.

이스케이프: `to_json()`은 문자열 안의 `<`, `>`, `&`, U+2028, U+2029, U+FFFE, U+FFFF를 `\u003c`, `\u003e`, `\u0026`, `\u2028`, `\u2029`, `\ufffe`, `\uffff`로 쓴 compact JSON을 돌려줍니다. C0 제어 문자는 JSON이 이미 이스케이프합니다. 그래서 `</script>`나 `<!--`가 요소를 끝내거나 바꿀 수 없고, Discord가 함께 받는 XHTML 페이지에서도 올바른 XML로 남습니다. Rust 문자열에 들어갈 수 있는 문자 중 XML 1.0이 금지하는 것은 C0 제어 문자와 U+FFFE/U+FFFF뿐입니다. JSON 파서는 이스케이프를 원래 문자로 읽습니다. 3,000바이트 한도는 바로 이 문자열로 재며, `to_script_tag()`는 공백 없이 그대로 요소 안에 넣습니다. 그 밖의 비ASCII 문자는 UTF-8 그대로이므로 페이지를 UTF-8로 제공하세요.

기존 메시지 빌더는 `TryFrom`으로 바꿀 수 있고, 무엇도 버리지 않고 문제 위치와 함께 실패합니다. `ComponentEmbed::try_from(container_builder)`, `PreviewContainer::try_from(&value)`, `PreviewComponent::try_from(&section_builder.build())`, 손으로 쓴 `{"component": ...}` 파일용 `ComponentEmbed::try_from(&document)`가 있습니다. custom-ID·프리미엄 버튼, 선택 메뉴, 텍스트 입력, 파일, 중첩 컨테이너, raw 컴포넌트, 알 수 없는 키는 실패합니다. 변환은 손실 없이 하거나 실패합니다. 규격상 null이 허용되는 필드(`accent_color`, 썸네일과 갤러리 항목의 `description`)는 `NullablePatch`라서 명시적 `null`과 생략을 구분해 보존하고, 그 밖의 `null`은 경로와 함께 `NullNotAllowed`로 실패합니다(공용 `Emoji` 타입이 `null`을 담을 수 없으므로 emoji 키도 마찬가지). 변환은 형태만 검사하고, 구분선 `spacing`이 1 또는 2인지를 포함한 한도는 출력할 때 검사합니다.
