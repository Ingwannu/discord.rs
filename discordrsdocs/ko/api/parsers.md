# 파서 API

원본 JSON payload를 타입 구조체로 안전하게 변환합니다.

## 인터랙션 파서

- `decode_interaction(&str)` (3.0): 인터랙션 객체를 JSON 텍스트(게이트웨이 `INTERACTION_CREATE`의 `d`, 인터랙션 엔드포인트 요청 본문)에서 바로 디코딩합니다. 게이트웨이와 타입 엔드포인트가 쓰는 경로입니다.
- `parse_interaction(&Value)`: 이미 가진 `serde_json::Value`에 같은 디코더를 씁니다.
- `parse_raw_interaction(&Value)`
- `parse_interaction_context(&Value)`

`Interaction`의 `Serialize`/`Deserialize`는 Discord 와이어 형식입니다(3.0. 2.x는 `{"ChatInputCommand": {...}}` 외부 태그 형식이었음). 그래서 `serde_json::from_slice::<Interaction>(bytes)`, `Interaction::deserialize(&*raw_value)`도 됩니다. 디코더는 payload를 한 번만 읽습니다. `Value` 트리를 만들지 않고 `data`·`resolved`·`member`를 복제하지 않습니다. resolved 사용자·멤버·채널·역할·첨부가 든 3.9 KB 채팅 입력 payload에서 할당은 49회입니다(2.x `decode_event`는 652회).

키 순서: JSON 객체에는 순서가 없고 `data`의 모양은 `type`에 따라 다릅니다. `type`이 먼저 오면(Discord 게이트웨이의 순서) `data`를 그 자리에서 디코딩하고, 아니면 `data`를 JSON 텍스트로 잡아 두었다가 객체 끝에서 디코딩합니다(`data` 복사 1회). 결과는 같습니다.

`Interaction::Unknown { context, kind, raw_data }`는 이 버전이 모르는 인터랙션 `type`과, 명령 `type`을 모르는 애플리케이션 명령(예: `PRIMARY_ENTRY_POINT`)을 담습니다. `raw_data`는 `data` 객체의 JSON 텍스트(`Option<Box<RawValue>>`, `data`가 없으면 `None`)이며 `serde_json::from_str(raw.get())`로 읽습니다.

명령 옵션 값(`CommandInteractionOption::value`)은 `CommandOptionValue`입니다. Discord가 보낸 JSON 타입 그대로 `String`, `Integer(i64)`, `Number(f64)`, `Boolean` 중 하나이고, `as_str()`, `as_i64()`, `as_f64()`(`NUMBER` 옵션이 `5`처럼 정수로 올 수 있어 정수도 받음), `as_bool()`, 사용자·채널·역할·멘션 가능·첨부 옵션용 `as_id()`로 읽습니다.

## 모달 파서

- `parse_modal_submission(&Value)`

`V2ModalSubmission`과 `V2ModalComponent`도 Discord 와이어 형식의 `Deserialize`/`Serialize`를 구현합니다(3.0). `serde_json::from_str::<V2ModalSubmission>(data)`는 `Value` 트리 없이 모달 제출 `data`를 읽습니다. 다시 쓸 때는 라벨 감싸개 없이 입력 컴포넌트만 쓰며, 그것을 읽으면 같은 값이 됩니다.

`V2ModalSubmission`은 `Label`, `RadioGroup`, `CheckboxGroup`, `Checkbox` 타입을 보존하며, 제출된 값은 Discord의 타입을 그대로 따릅니다.

- `RadioGroup { value: Option<String> }`: 아무것도 고르지 않으면 `None`(Discord는 `null`을 보냄)
- `CheckboxGroup { values }`: 아무것도 체크하지 않으면 빈 목록
- `Checkbox { checked: bool }`: Discord의 `value` 불리언에서 읽음
- 텍스트 입력은 `value`, 문자열·사용자·역할·멘션 가능·채널 선택과 파일 업로드는 `values`

조회 헬퍼: `get_text`, `get_select_values`(선택 메뉴와 체크박스 그룹), `get_file_values`, `get_radio_value`(선택이 없을 때도 `None`), `get_checkbox`.

### 모달 제출의 resolved 데이터 (3.0)

파일 업로드는 첨부 ID를, 사용자·역할·멘션 가능·채널 선택은 ID를 제출하고, Discord는 그 객체를 `data.resolved`에 담아 보냅니다. `ModalSubmitInteraction`은 이를 보관하고(`resolved: Option<ResolvedData>`) 컴포넌트별로 찾아 줍니다. `get_attachments`, `get_users`, `get_members`, `get_roles`, `get_channels`는 제출 순서대로 resolved 객체를 돌려주고, Discord가 풀어 주지 않은 ID는 건너뜁니다. `parse_modal_submission`은 제출 값만 돌려줍니다.

```rust
if let Interaction::ModalSubmit(modal) = decode_interaction(body)? {
    for file in modal.get_attachments("screenshot") {
        println!("{} ({} bytes): {:?}", file.filename, file.size, file.url);
    }
}
```

컴포넌트 트리를 직접 디코딩하면 `MessageComponent`에도 같은 타입이 있습니다. `value`는 `MessageComponentValue`(`String`, `Bool`, 이 버전이 모르는 JSON 타입은 JSON 텍스트를 `Box<RawValue>`로 담은 `Other`; `value_str()` / `value_bool()`로 읽음), `values`는 목록, `default`는 생성 페이로드에서 체크박스의 초기 상태입니다. `"value": null`과 `value` 생략은 둘 다 `None`입니다. Discord는 아무것도 고르지 않은 라디오 그룹에만 `null`을 보내고, 그 뜻은 값이 없는 것과 같습니다.
