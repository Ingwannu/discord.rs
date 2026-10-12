# 빌더 API

`builders` 모듈은 메시지 본문, Components V2 레이아웃, 임베드, 모달을 플루언트 스타일로 구성합니다. 3.0(5단계)부터 메시지 빌더는 모두 `discordrs::request`의 타입 요청 모델을 들고 있고 `build()`가 그 모델을 돌려줍니다. 중간에 `serde_json::Value`를 만들지 않고, 모델은 REST 클라이언트가 보내는 바이트로 바로 직렬화됩니다.

## 하위 모듈

- `message.rs`: `MessageBuilder`(`request::CreateMessage` 생성)
- `components.rs`: 버튼(`CustomButtonBuilder`, `LinkButtonBuilder`, `PremiumButtonBuilder`, 2.x의 `ButtonBuilder`는 deprecated), `ActionRowBuilder`(버튼 행), 선택 메뉴(`SelectMenuBuilder::string/user/role/mentionable/channel`이 `StringSelectBuilder`, `UserSelectBuilder`, `RoleSelectBuilder`, `MentionableSelectBuilder`, `ChannelSelectBuilder`를 돌려줌), `ComponentsV2Message`
- `container.rs`: `ContainerBuilder`, `TextDisplayBuilder`, `SeparatorBuilder`, `create_container` 헬퍼
- `media.rs`: `SectionBuilder`, `ThumbnailBuilder`, `MediaGalleryBuilder`, `FileBuilder`
- `embed.rs`: `EmbedBuilder`(`request::Embed` 생성)
- `modal.rs`: 모달, Label, 텍스트 입력, 라디오, 체크박스, 파일 업로드(`request` 모달 모델을 만듦)

| 빌더 | `build()` / `try_build()` 반환 타입 |
| --- | --- |
| `MessageBuilder` | `request::CreateMessage`(`InteractionMessage`, `CreateFollowup`으로 변환, `UpdateMessage`, `EditInteractionResponse`로는 `TryFrom`) |
| `ComponentsV2Message` | `Vec<request::Component>` |
| `ContainerBuilder` | `request::Container` |
| `ActionRowBuilder` | `request::ActionRow`(`ButtonRow`) |
| `StringSelectBuilder` / `UserSelectBuilder` / `RoleSelectBuilder` / `MentionableSelectBuilder` / `ChannelSelectBuilder` | `StringSelect` / `UserSelect` / `RoleSelect` / `MentionableSelect` / `ChannelSelect`(각각 `SelectMenu`, `ActionRow`, `Component`로 변환) |
| `CustomButtonBuilder` / `LinkButtonBuilder` / `PremiumButtonBuilder` | `CustomButton` / `LinkButton` / `PremiumButton` |
| `SectionBuilder<SectionAccessory>` | `request::Section` |
| `TextDisplayBuilder`, `SeparatorBuilder`, `ThumbnailBuilder`, `MediaGalleryBuilder`, `FileBuilder` | 같은 이름의 모델(파일은 `FileComponent`) |
| `EmbedBuilder` | `request::Embed` |
| `ModalBuilder` | `request::Modal`(입력은 Label로 감쌈) |
| `TextInputBuilder` / `LabelBuilder` | `request::Label`(`TextInputBuilder`는 텍스트 입력을 감싼 Label을 만듦) |
| `RadioGroupBuilder`, `CheckboxGroupBuilder`, `CheckboxBuilder`, `FileUploadBuilder` | `RadioGroup`, `CheckboxGroup`, `Checkbox`, `FileUpload` |

## 예시

```rust
use discordrs::{
    ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    LinkButtonBuilder, MessageBuilder, SelectMenuBuilder, SelectOption, TextDisplayBuilder,
};
use discordrs::request::CreateMessage;

let buttons = ActionRowBuilder::new()
    .add_button(CustomButtonBuilder::new("open_ticket").label("열기").style(ButtonStyle::PRIMARY))
    .add_button(LinkButtonBuilder::new("https://example.com/help").label("도움말"));

let container = ContainerBuilder::new()
    .add_text_display(TextDisplayBuilder::new("## 지원"))
    .add_action_row(buttons)
    // 선택 메뉴는 그 자체가 한 행입니다. 행 자리에 선택 메뉴 빌더를 넘깁니다.
    .add_action_row(SelectMenuBuilder::string("topic").add_option(SelectOption::new("결제", "billing")));

let message: CreateMessage = MessageBuilder::new()
    .components_v2(ComponentsV2Message::new().add_container(container))
    .try_build()?;
```

`components_v2(..)`는 `IS_COMPONENTS_V2` 플래그를 켭니다. 모든 빌더는 자기 자리로 `Into` 변환되므로(컨테이너는 `Component`로, 버튼 빌더는 `Button`이나 `SectionAccessory`로, 선택 메뉴 빌더는 `SelectMenu`나 `ActionRow`로) 빌더와 타입 모델을 섞어 쓸 수 있습니다. 어떤 빌더도 값을 버리지 않습니다. 모델이 담을 수 없는 값은 setter가 없거나 변환이 실패합니다.

### 버튼

버튼은 세 종류이고, 각자 Discord가 허용하는 필드만 가집니다.

- `CustomButtonBuilder::new(custom_id)`: primary(기본), secondary, success, danger. 인터랙션을 보냅니다.
- `LinkButtonBuilder::new(url)`: style 5, URL을 엽니다. `custom_id` setter가 없습니다.
- `PremiumButtonBuilder::new(sku_id)`: style 6, SKU 구매 화면을 엽니다. label, emoji, URL, `custom_id`가 없습니다.

`custom_id`가 있는 링크 버튼이나 label이 있는 프리미엄 버튼은 컴파일되지 않습니다(`tests/ui/link_button_custom_id.rs`, `premium_button_label_url.rs`).

### 행과 선택 메뉴

행에는 버튼 1-5개 또는 선택 메뉴 하나만 들어갑니다. `ActionRowBuilder`는 버튼 행을 만들고, 선택 메뉴 행은 `SelectMenuBuilder` 자체입니다. 행을 받는 곳(`add_action_row(impl Into<ActionRow>)`)에 그대로 넘깁니다. 버튼과 선택 메뉴를 섞은 행은 컴파일되지 않습니다(`tests/ui/mixed_button_select_row.rs`).

`SelectMenuBuilder::string(..)`, `::user(..)`, `::role(..)`, `::mentionable(..)`, `::channel(..)`은 종류마다 다른 빌더를 돌려주고, 각 빌더에는 그 종류가 받는 setter만 있습니다. `add_option` / `add_options`는 문자열 선택, `default_user`는 사용자·mentionable 선택, `default_role`은 역할·mentionable 선택, `default_mentionable`은 mentionable 선택, `default_channel`과 `channel_types`는 채널 선택에만 있고, `default_values(ids)`는 그 종류의 id 타입을 받습니다. 종류가 담을 수 없는 값은 컴파일되지 않으므로(`tests/ui/select_menu_wrong_kind_fields.rs`) 빌더에서 와이어까지 사라지는 값이 없습니다. `required`는 모달 전용(메시지에서는 Discord가 무시), `disabled`는 메시지 전용(모달에서는 오류)입니다.

### 섹션

Discord는 섹션의 accessory를 요구하고, `SectionBuilder`는 이를 타입으로 추적합니다. `SectionBuilder::new()`는 `SectionBuilder<()>`이고 `set_thumbnail_accessory` / `set_button_accessory`가 `SectionBuilder<SectionAccessory>`로 바꿉니다. 이 상태만 `build()`가 있고 `add_section`이 받습니다. 텍스트는 accessory 앞이든 뒤든 더할 수 있습니다.

### Raw 컴포넌트

이 버전이 표현하지 못하는 것은 `request::RawComponent`(`from_json`, `from_value`)로 보내고, 모든 `add_component`가 받습니다. 그대로 쓰이고(`Value` 트리가 아니라 `serde_json::value::RawValue`), 검증에서는 컴포넌트 하나로 세며 그 밖에는 검사하지 않습니다.

## 메시지 본문 (`discordrs::request`)

엔드포인트 계열마다 그 엔드포인트가 받는 필드만 가진 본문 타입이 있습니다.

| 타입 | 엔드포인트 | 의미 |
| --- | --- | --- |
| `CreateMessage` | `POST /channels/{id}/messages` | 생성. 답장/전달, 스티커, nonce, `shared_client_theme` |
| `EditMessage` | `PATCH /channels/{id}/messages/{id}` | 패치. nonce, 스티커, 답장, tts, poll 없음(`clear_stickers()` / `clear_poll()`만) |
| `InteractionMessage` | 콜백 타입 4의 `data` | 생성. ephemeral 플래그, poll |
| `UpdateMessage` | 콜백 타입 7의 `data` | 컴포넌트가 붙은 메시지에 대한 패치 |
| `CreateFollowup` | `POST /webhooks/{app}/{token}` | 생성. ephemeral 플래그, poll |
| `ExecuteWebhook` | `POST /webhooks/{id}/{token}` | 생성. username, avatar, 스레드 이름/태그 |
| `EditWebhookMessage`(= `EditInteractionResponse`) | `PATCH .../messages/{id 또는 @original}` | 패치. 첨부 목록 명시, poll |

생성 본문은 `Option`(`None`은 키 생략, `Some(vec![])`은 `[]`), 패치 본문은 `NullablePatch`(생략은 유지, `null`은 지움, 값은 교체)입니다. 쿼리 파라미터(`wait`, `thread_id`, `with_components`)는 `WebhookExecuteQuery` / `WebhookMessageQuery`에 있습니다. 각 본문에는 setter(`content`, `add_embed`, `components_v2`, `ephemeral` 등), `From<&str>`, 필드를 버리지 않고 실패하는 변환이 있습니다(`InteractionMessage::try_from(create_message)`는 답장 참조, 스티커, nonce가 있으면 실패).

첨부는 `MessageAttachment::Existing(ExistingAttachment { id: AttachmentId, .. })`(저장된 첨부 유지) 또는 `MessageAttachment::Upload(AttachmentUpload { id: UploadId, .. })`(`files[n]` 설명)이고, 두 id 타입은 바꿔 쓸 수 없습니다. `request::Embed`와 `request::CreatePoll`이 보내는 임베드와 투표입니다(투표 질문은 텍스트만).

## 검증

`build()`는 한도를 검사하지 않고, `try_build()`는 검사한 뒤 같은 타입을 돌려주며, `validate()`는 소비하지 않고 검사합니다. 빌더는 모델의 `validate()`에 맡기고 부모는 자식을 재귀적으로 검사하므로 빌더와 모델의 한도가 어긋날 수 없습니다. 메시지 본문은 메시지 전체 규칙을 더합니다.

| 규칙 | 한도 | 출처 |
| --- | --- | --- |
| 내용 | 2000자 | Create Message |
| 임베드 | 메시지당 10개, 전체 합 6000자 | Embed Limits |
| 스티커 / nonce | 3개 / 25자 | Create Message |
| 플래그 | 엔드포인트가 받는 것만(모르는 비트는 통과) | 엔드포인트별 `flags` 행 |
| `IS_COMPONENTS_V2` | 내용, 임베드, poll, 스티커, 공유 테마 불가 | Create Message 각주 |
| 레거시 메시지 | 액션 행만, 최대 5개 | Legacy Message Component Behavior |
| Components V2 | 중첩 포함 최대 40개 | components/reference |
| `id` / `custom_id` | 메시지 안에서 고유(`id` 0은 반복 가능) | components/reference |
| 음성 메시지 | `duration_secs`와 `waveform`이 있는 `Upload` 첨부 하나뿐. 파일은 `audio/*`(`check_voice_files`) | Voice Messages |
| 선택 메뉴 | 생략된 값을 1로 보고 `min_values` ≤ `max_values`, 기본값 개수는 그 범위 안 | 선택 메뉴 구조 |

`IS_COMPONENTS_V2`를 설정하지 않는 수정은 메시지의 원래 모드를 유지하므로 두 모드가 공유하는 규칙만 검사합니다. 한도는 `builders/limits.rs`의 이름 있는 상수이고 각자 문서 출처를 적었습니다.

## 모달 (3.0)

```rust
use discordrs::{CheckboxBuilder, ModalBuilder, RadioGroupBuilder, SelectOption, TextInputBuilder};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    // 모든 입력은 Label 안에 들어갑니다. 텍스트 입력은 빌더의 label 문구를 씁니다.
    .add_text_input(TextInputBuilder::short("nick", "Nickname").description("Shown to others"))
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    )
    .add_text_display("You can change these later.")
    .add_checkbox("Newsletter", None, CheckboxBuilder::new("news"))
    .try_build()?; // -> request::Modal
```

3.0부터 모달 빌더는 `discordrs::request`의 타입 요청 모델을 들고 있고 `build()`는 JSON 대신 그 모델을 돌려줍니다. `ModalBuilder::build()`는 `Modal`, `TextInputBuilder::build()`는 텍스트 입력을 감싼 `Label`(Discord가 입력의 label을 Label 컴포넌트로 옮겼습니다), `LabelBuilder::build()`는 `Label`, `RadioGroupBuilder` / `CheckboxGroupBuilder` / `CheckboxBuilder` / `FileUploadBuilder`는 각각 `RadioGroup` / `CheckboxGroup` / `Checkbox` / `FileUpload`입니다. `show_modal`과 `InteractionResponse::modal`은 `Modal`과 `ModalBuilder`를 모두 받습니다.

| 타입 | 담는 것 |
|---|---|
| `Modal` | `custom_id`(1-100), `title`(1-45), `ModalComponent` 1-5개 |
| `ModalComponent` | `Label`, `TextDisplay`, `Raw(RawComponent)`, `LegacyTextInputRow`(아래 참고) |
| `Label` | `label`(1-45), 선택 `description`(100자 이하), `LabelComponent` 하나 |
| `LabelComponent` | `TextInput`, `StringSelect`, `UserSelect`, `RoleSelect`, `MentionableSelect`, `ChannelSelect`, `FileUpload`, `RadioGroup`, `CheckboxGroup`, `Checkbox`, `Raw` |

모달 입력은 메시지 행에 넣을 수 없고(컴파일 오류), 메시지 전용 컴포넌트(버튼, 섹션, 컨테이너)는 모달에 들어갈 자리가 없습니다. Action Row 안의 텍스트 입력은 Discord가 deprecated로 바꾼 형태이며, deprecated `ModalBuilder::add_text_input_row` / `LegacyTextInputRow::new`로만 만들어지고 텍스트 입력이 자기 `label`을 갖는 유일한 경우입니다. `ModalBuilder::add_component`는 `ModalComponent`를 받습니다(아직 모델에 없는 컴포넌트는 `RawComponent::from_json(..)`). 2.x의 `add_component(Value)`와 `add_action_row(ActionRowBuilder)`는 없어졌습니다.

`RadioGroupBuilder`, `CheckboxGroupBuilder`, `CheckboxBuilder`에는 `disabled` setter가 없습니다(모달에 비활성 컴포넌트가 있으면 Discord가 오류를 냅니다). 체크박스는 필수로 만들 수 없어 `CheckboxBuilder`에는 `required`도 없습니다. 반드시 체크해야 하는 항목은 옵션 하나짜리 `CheckboxGroupBuilder`에 `required(true)`를 씁니다. `CheckboxBuilder::default_checked(bool)`가 초기 상태를 정합니다(`default`로 전송, deprecated `checked(bool)`도 같은 동작).

`Modal::validate()`(와 `ModalBuilder::validate()` / `try_build()`)는 모달 전체를 Discord 문서의 한도로 검사합니다: 모달 `custom_id`·`title`·컴포넌트 1-5개, label 문구와 설명, 텍스트 입력 `custom_id`·placeholder(100)·value(4000)·`min_length` 0-4000·`max_length` 1-4000, 파일 업로드 `min_values` 0-10(0은 `required(false)`일 때만)·`max_values` 1-10·`file_types` 10개 이하, 라디오 그룹 옵션 2-10개, 체크박스 그룹 옵션 1-10개, 옵션 label/value/description 100자 이하와 emoji 없음(그룹 옵션에는 emoji가 없습니다), 체크박스 그룹 `min_values` 0-10(0은 `required(false)`일 때만)·`max_values` 1-10, 선택 메뉴 자체 한도와 모달 규칙(`disabled` 금지, `min_values: 0`은 `required: false`일 때만), 텍스트 표시 1-4000자, `custom_id`와 0이 아닌 컴포넌트 `id`의 중복 금지. Raw 컴포넌트는 그대로 보내고 검사하지 않습니다. 개별 컴포넌트 빌더의 `validate()`는 그 컴포넌트만 검사하고, `build()`는 검사하지 않습니다.

## 파일 형식 필터 (3.0)

`FileUploadBuilder::file_types(...)`와 `CommandOptionBuilder::file_types(...)`(`ATTACHMENT` 옵션)는 업로드를 미리 정의된 그룹(`"image"`, `"video"`, `"audio"`)이나 점으로 시작하는 확장자(`".pdf"`)로 제한합니다. 최대 10개입니다. `validate()` / `try_build()`는 개수와 각 항목의 모양만 검사합니다. Discord가 그룹 목록이 바뀔 수 있다고 밝혀서 그룹 이름은 고정 목록과 대조하지 않습니다. Discord는 파일 이름만 확인하므로 파일 내용은 직접 검증해야 합니다.

## Component Embed (링크 미리보기) (3.0)

2026-10-05부터 웹사이트는 `discord:component-embed` JSON 문서를 제공해 Discord 링크 미리보기를 Components V2 레이아웃으로 바꿀 수 있습니다([Discord 문서](https://docs.discord.com/developers/link-previews/component-embeds)). 봇이나 앱은 필요 없고, Discord 크롤러가 페이지에서 문서를 읽습니다. `discordrs::request::ComponentEmbed`는 이 문서를 제한된 모델로 만듭니다. 메시지 빌더와도, 수신용 `Embed::components`와도 분리되어 있습니다.

- `ComponentEmbed { component: PreviewContainer }`. 컨테이너에는 `PreviewComponent`(텍스트 디스플레이, 링크 버튼이나 썸네일을 accessory로 가진 섹션, 미디어 갤러리, 구분선, 링크 버튼 행)만 들어갑니다.
- 버튼은 `PreviewLinkButton`뿐이고 `id`, `type`, `url`, `style`(항상 5), `label`, `emoji`, `disabled`만 내보냅니다. 중첩 컨테이너, 선택 메뉴, 파일, custom-ID 버튼, raw JSON은 표현할 수 없습니다. `PreviewActionRow::add_button(CustomButtonBuilder)`는 컴파일되지 않습니다.

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

`link_tag(href)`는 URL 파서가 직렬화한 형태(ASCII만: 비ASCII는 퍼센트 인코딩, 호스트는 punycode)로 URL을 쓰므로 XHTML에서도 요소가 깨지지 않습니다. 파서가 조용히 지우는 제어 문자나 앞뒤 공백이 있는 href는 거부합니다.

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

기존 메시지 빌더는 `TryFrom`으로 바꿀 수 있고, 무엇도 버리지 않고 문제 위치와 함께 실패합니다. `ComponentEmbed::try_from(container_builder)`와 `PreviewContainer::try_from(container)`는 타입 메시지 컨테이너를 필드별로 옮기고, `PreviewContainer::try_from(&value)`, `PreviewComponent::try_from(&value)`, `ComponentEmbed::try_from(&document)`는 JSON(손으로 쓴 `{"component": ...}` 파일 등)을 읽습니다. 컨테이너 안의 `RawComponent` 자식도 같은 읽기 코드를 거칩니다. custom-ID·프리미엄 버튼, 선택 메뉴, 텍스트 입력, 파일, 중첩 컨테이너, 미리보기에 맞지 않는 raw 컴포넌트, 알 수 없는 키는 실패합니다. 변환은 손실 없이 하거나 실패합니다. 규격상 null이 허용되는 필드(`accent_color`, 썸네일과 갤러리 항목의 `description`)는 `NullablePatch`라서 명시적 `null`과 생략을 구분해 보존하고, 그 밖의 `null`은 경로와 함께 `NullNotAllowed`로 실패합니다(공용 `Emoji` 타입이 `null`을 담을 수 없으므로 emoji 키도 마찬가지). 변환은 형태만 검사하고, 구분선 `spacing`이 1 또는 2인지를 포함한 한도는 출력할 때 검사합니다.

## 2.x 빌더에서 옮기기

| 2.x | 3.0 |
| --- | --- |
| `ButtonBuilder::new().style(..).custom_id(id)` | `CustomButtonBuilder::new(id).style(..)` |
| `ButtonBuilder::new().url(url)` | `LinkButtonBuilder::new(url)` |
| `ButtonBuilder`의 프리미엄 style(SKU 없음) | `PremiumButtonBuilder::new(sku_id)` |
| `ButtonBuilder`를 계속 쓰기 | deprecated. `Button::try_from(builder)?` 또는 `builder.build()?`(프리미엄 style이나 `custom_id` 누락은 실패) |
| `ActionRowBuilder::new().add_select_menu(menu)` | 행 자리에 `menu`를 그대로 넘김(`add_action_row(menu)`) |
| `ActionRowBuilder::add_text_input` / `add_component(Value)` | 제거. 텍스트 입력은 모달 전용, raw JSON은 `RawComponent` |
| 컨테이너·메시지의 `add_component(json!(..))` | `add_component(RawComponent::from_value(&json!(..))?)` |
| accessory 없는 `SectionBuilder::new()`(`validate()`에서 검사) | 섹션을 더하는 곳에서 컴파일되지 않음. accessory를 지정 |
| `X::build() -> serde_json::Value` | `X::build()`는 타입 모델을 돌려줌. JSON이 필요하면 `serde_json::to_value(&x)` |
| `MessageBuilder::build() -> model::CreateMessage`, `components(Vec<Value>)` | `-> request::CreateMessage`, `components(Vec<Component>)` / `add_component(..)` |
| `MessageBuilder::flags(u64)` | `flags(impl Into<MessageFlags>)`(`u64`도 그대로 받음) |
| `EmbedBuilder::build() -> Value` | `-> request::Embed`, `MessageBuilder::add_embed(builder)` |
| `SelectMenuBuilder`(타입 하나)가 다른 종류의 필드를 조용히 버림 | 생성자가 종류별 빌더(`StringSelectBuilder` 등)를 돌려주고 그 setter가 없음. 잘못된 종류의 값은 컴파일되지 않음 |
| `default_value(SelectDefaultValue::role(id))`, `default_values(vec![SelectDefaultValue::..])` | `default_role(id)`(또는 `default_user`, `default_channel`, `default_mentionable`), 종류의 id 타입으로 `default_values(ids)` |
| `SelectMenuBuilder`를 매개변수·반환 타입으로 사용 | 종류별 빌더 또는 `impl Into<SelectMenu>` |
| `message_flags::SUPPRESS_NOTIFICATIONS` = 1 << 10, `IS_VOICE_MESSAGE` = 1 << 12 | Discord 값인 1 << 12, 1 << 13. `HAS_SNAPSHOT` = 1 << 14 추가 |
