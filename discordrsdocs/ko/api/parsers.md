# 파서 API

원본 JSON payload를 타입 구조체로 안전하게 변환합니다.

## 인터랙션 파서

- `parse_raw_interaction(&Value)`
- `parse_interaction_context(&Value)`

## 모달 파서

- `parse_modal_submission(&Value)`

`V2ModalSubmission`은 `Label`, `RadioGroup`, `CheckboxGroup`, `Checkbox` 타입을 보존하며, 제출된 값은 Discord의 타입을 그대로 따릅니다.

- `RadioGroup { value: Option<String> }`: 아무것도 고르지 않으면 `None`(Discord는 `null`을 보냄)
- `CheckboxGroup { values }`: 아무것도 체크하지 않으면 빈 목록
- `Checkbox { checked: bool }`: Discord의 `value` 불리언에서 읽음
- 텍스트 입력은 `value`, 문자열·사용자·역할·멘션 가능·채널 선택과 파일 업로드는 `values`

조회 헬퍼: `get_text`, `get_select_values`(선택 메뉴와 체크박스 그룹), `get_file_values`, `get_radio_value`(선택이 없을 때도 `None`), `get_checkbox`.

컴포넌트 트리를 직접 디코딩하면 `MessageComponent`에도 같은 타입이 있습니다. `value`는 `MessageComponentValue`(`String`, `Bool`, 이 버전이 모르는 JSON 타입은 `Other`; `value_str()` / `value_bool()`로 읽음), `values`는 목록, `default`는 생성 페이로드에서 체크박스의 초기 상태입니다.
