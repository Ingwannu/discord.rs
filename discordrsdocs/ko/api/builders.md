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

## 파일 형식 필터 (3.0)

`FileUploadBuilder::file_types(...)`와 `CommandOptionBuilder::file_types(...)`(`ATTACHMENT` 옵션)는 업로드를 미리 정의된 그룹(`"image"`, `"video"`, `"audio"`)이나 점으로 시작하는 확장자(`".pdf"`)로 제한합니다. 최대 10개입니다. `validate()` / `try_build()`는 개수와 각 항목의 모양만 검사합니다. Discord가 그룹 목록이 바뀔 수 있다고 밝혀서 그룹 이름은 고정 목록과 대조하지 않습니다. Discord는 파일 이름만 확인하므로 파일 내용은 직접 검증해야 합니다.
