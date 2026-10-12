#!/usr/bin/env python3
"""Report drift between the official Discord API docs and this crate.

Compares two things:

* REST routes: every ``<Route method="X">path</Route>`` in
  ``discord/discord-api-docs`` against the rows of the crate's route manifest
  (``src/http/routes.rs``), the table the REST transport itself executes.
* Gateway dispatch events: the "Receive Events" table and headings of the
  docs' gateway events page against the event-name match arms in the crate's
  event decoder.

The script only reports. Drift exits 0 unless ``--fail-on-drift`` is given;
exit code 2 means the extraction itself is broken (configuration error, or
far fewer routes/events than any real docs or crate revision has), which must
not be mistaken for "no drift".

[Decision Log]
- 목적과 의도: Discord가 라우트·게이트웨이 이벤트를 추가하거나 없앨 때 사람이 감사를 다시
  돌리지 않아도 주 1회 차이를 이슈로 받는다. 2026-10-10 수동 감사(234/234)를 재현 가능한
  스크립트로 고정한다.
- 기존 구현 및 제약 조건: 크레이트에는 정적 라우트 테이블이 없다. `RequestRoute::new`는
  런타임에 (Method, path)를 받으므로 소스에서 같은 쌍을 읽는 것이 가장 가까운 원본이다.
  일부 경로는 헬퍼(`global_commands_path`, `webhook_message_path` 등)가 조립해 리터럴로
  보이지 않는다. 이 단계에서 src/는 수정할 수 없다.
- 검토한 주요 대안: (1) 작은 Rust 바이너리(ci/ 별도 크레이트), (2) 표준 라이브러리만 쓰는
  Python 스크립트, (3) 크레이트에 라우트 상수 테이블을 만들고 테스트에서 덤프,
  (4) discord-api-spec(OpenAPI)을 1차 소스로 사용.
- 선택한 방식: (2) + discord-api-docs MDX + 사유가 강제된 allowlist
  (`ci/api-drift-allowlist.toml`).
- 다른 대안 대신 이 방식을 선택한 이유: 텍스트 추출과 집합 비교가 전부라 Python 표준
  라이브러리(re, tomllib, json, unittest)로 충분하고, cron 잡에서 컴파일 없이 수 초에
  끝나며, Cargo.lock에 의존성을 더하지 않는다. (1)은 정규식·TOML 크레이트를 끌어오거나
  직접 구현해야 하고 별도 lockfile·빌드 시간이 든다. (3)은 src/ 변경이 필요해 3.0 5단계의
  REST 정리와 함께 할 일이다. (4)는 preview 스펙이라 누락 가능성이 있고 게이트웨이 이벤트가
  없으며, 기존 234/234 기준이 MDX라서 기준선이 바뀐다.
- 장점, 단점 및 영향: 로컬에서 `python3 ci/api_drift.py --docs <clone>`으로 바로 재현된다.
  대신 소스 텍스트 패턴에 의존하므로 REST 호출 형태가 바뀌면 추출 하한(sanity floor)과
  `--self-check`가 깨져 알려 준다. 헬퍼 경로는 사람이 관리하는 목록이라 evidence 문자열과
  `functions`(해당 메서드를 보내는 래퍼)로 소스와의 일치를 PR CI에서 양방향 검사한다.
  docs/design/3.0-types-events-requests.md 4절은 5단계에서 `RequestRoute`를 확장해 실행과 drift
  리포트가 같은 출처를 쓰게 한다. 그 기술자가 생기면 크레이트 쪽 소스 스캔(`scan_crate`)만 그
  덤프로 바꾸고, 문서 추출·비교·allowlist·워크플로·이슈 처리는 그대로 둔다.

[Decision Log] 라우트 목록 전환 (3.0 5단계 B, 2026-10-12)
- 목적과 의도: 런타임과 drift 리포트가 같은 라우트 메타데이터를 쓰고, 메서드 이름을 지우거나
  바꿔도 라우트가 조용히 빠지지 않게 한다.
- 기존 구현 및 제약 조건: 위 방식은 `Method::X, "/literal"` 쌍을 소스에서 긁고, 헬퍼가 만든
  경로 20개를 allowlist `helper_route`에 evidence·functions와 함께 손으로 적었다. 같은
  경로의 두 작업(Create DM / Create Group DM, Execute Webhook / Create Followup Message)을
  구분하지 못했다.
- 검토한 주요 대안: (1) 소스 스캔 유지, (2) 테스트가 덤프한 JSON을 커밋, (3) `src/http/routes.rs`의
  `route_manifest!` 행을 이 스크립트가 텍스트로 읽기.
- 선택한 방식: (3). 행마다 RouteId, 메서드, 문서 표기 경로, 인증, 쿼리, 요청·응답 타입, 메서드
  목록, 3.0에서 지운 이름(`was`)을 한 줄에 적는다. 엔드포인트는 `RouteId`를 transport에 넘기고
  transport는 메서드와 인증을 행에서 가져온다. 이 스크립트는 행을 읽고(`parse_manifest_text`),
  `--self-check`에서 양방향으로 묶는다(`manifest_binding`): 목록의 메서드가 있고 그 라우트를
  실제로 보내는지(본문의 `RouteId::X` 또는 `self.` 호출), 클라이언트의 엔드포인트 메서드가 모두
  목록에 있는지, `was` 이름이 없어졌거나 deprecated 전달 메서드로만 남았는지, 엔드포인트 파일이
  HTTP 메서드를 직접 쓰지 않는지.
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 헬퍼 경로를 계속 손으로 관리해야 하고 작업을
  구분하지 못한다. (2)는 코드와 덤프 사이에 재생성 단계가 끼어 낡을 수 있고 cron 잡이 여전히
  컴파일 없이 돌아야 한다. (3)은 컴파일 없이 읽히고, 행 하나가 실행과 리포트 둘 다에 쓰인다.
  Rust 쪽은 `cfg(test)`에서 행의 타입이 실제로 있는지 컴파일러가 검사한다.
- 장점, 단점 및 영향: `helper_route` 표와 리터럴 스캐너가 없어졌다. 행은 한 줄이어야 하고
  이해하지 못한 줄은 오류다(조용히 빠지지 않음). 문서 추출·비교·allowlist의 나머지 표·
  워크플로·이슈 처리는 그대로다.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    sys.exit("api_drift.py needs Python 3.11+ (tomllib)")

HTTP_METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE")
EXIT_OK = 0
EXIT_DRIFT = 1
EXIT_BROKEN = 2
REPORT_MARKER = "<!-- api-drift-report -->"
FINGERPRINT_PREFIX = "<!-- api-drift-fingerprint: "
# GitHub rejects issue bodies above 65,536 characters; leave room for the
# marker lines and the truncation note.
DEFAULT_MAX_CHARS = 60_000
# Long lists are capped per section so one huge section (for example a broken
# docs layout) cannot push every other section out of the issue body.
MAX_ITEMS_PER_SECTION = 150


class ConfigError(Exception):
    """The allowlist or settings are unusable; extraction cannot be trusted."""


# ---------------------------------------------------------------------------
# Path normalization
# ---------------------------------------------------------------------------

_MD_LINK = re.compile(r"\[([^\]]*)\]\([^)]*\)")
_PLACEHOLDER = re.compile(r"\{[^{}]*\}")


def normalize_template(path: str) -> str:
    """Normalizes a docs- or allowlist-style route template.

    ``/channels/[\\{channel.id\\}](/link)/messages`` -> ``/channels/{}/messages``.
    Parameter names differ between the docs and the crate (``{channel.id}``
    vs ``{channel_id}`` vs ``{}``), so only the parameter position matters.
    """

    path = _MD_LINK.sub(r"\1", path)
    path = path.replace("\\", "")
    # The changelog sometimes writes ``[\{channel.id\}]`` without a URL.
    path = path.replace("[", "").replace("]", "").replace("`", "")
    path = "".join(path.split())
    path = path.split("?", 1)[0].split("#", 1)[0]
    path = _PLACEHOLDER.sub("{}", path)
    return _tidy_slashes(path)


def _tidy_slashes(path: str) -> str:
    path = re.sub(r"/{2,}", "/", path)
    if len(path) > 1:
        path = path.rstrip("/")
    return path


def route_key(method: str, path: str) -> tuple[str, str]:
    return (method.upper(), path)


def format_route(key: tuple[str, str]) -> str:
    return f"{key[0]} {key[1]}"


# ---------------------------------------------------------------------------
# Docs extraction
# ---------------------------------------------------------------------------

_ROUTE_TAG = re.compile(r"<Route\b([^>]*)>(.*?)</Route>", re.S)
_METHOD_ATTR = re.compile(r"""method\s*=\s*["']([A-Za-z]+)["']""")
_HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")


@dataclass
class DocRoute:
    method: str
    path: str
    raw: str
    file: str
    line: int
    heading: str

    @property
    def deprecated(self) -> bool:
        return "deprecated" in self.heading.lower()


def _line_starts(text: str) -> list[int]:
    starts = [0]
    for index, char in enumerate(text):
        if char == "\n":
            starts.append(index + 1)
    return starts


def _line_of(starts: list[int], offset: int) -> int:
    low, high = 0, len(starts) - 1
    while low < high:
        mid = (low + high + 1) // 2
        if starts[mid] <= offset:
            low = mid
        else:
            high = mid - 1
    return low + 1


def extract_doc_routes_from_text(text: str, file: str) -> list[DocRoute]:
    starts = _line_starts(text)
    lines = text.splitlines()
    routes = []
    for match in _ROUTE_TAG.finditer(text):
        method_match = _METHOD_ATTR.search(match.group(1))
        if not method_match:
            continue
        method = method_match.group(1).upper()
        raw = match.group(2).strip()
        line = _line_of(starts, match.start())
        heading = ""
        for previous in range(line - 1, -1, -1):
            heading_match = _HEADING.match(lines[previous]) if previous < len(lines) else None
            if heading_match:
                heading = heading_match.group(2)
                break
        routes.append(DocRoute(method, normalize_template(raw), raw, file, line, heading))
    return routes


def _is_excluded(relative: str, patterns: list[str]) -> bool:
    """Matches ``dir/`` prefixes and fnmatch globs; ``**/`` may match no directory."""

    for pattern in patterns:
        if pattern.endswith("/"):
            if relative.startswith(pattern):
                return True
        elif fnmatch.fnmatch(relative, pattern) or fnmatch.fnmatch(relative, pattern.replace("**/", "")):
            return True
    return False


def extract_doc_routes(docs_root: Path, settings: dict) -> dict[tuple[str, str], list[DocRoute]]:
    base = docs_root / settings["docs_route_root"]
    if not base.is_dir():
        raise ConfigError(f"docs route root not found: {base}")
    routes: dict[tuple[str, str], list[DocRoute]] = {}
    for file in sorted(base.rglob("*")):
        if file.suffix not in (".md", ".mdx") or not file.is_file():
            continue
        relative = file.relative_to(docs_root).as_posix()
        if _is_excluded(relative, settings["docs_route_exclude"]):
            continue
        for route in extract_doc_routes_from_text(file.read_text(encoding="utf-8"), relative):
            routes.setdefault(route_key(route.method, route.path), []).append(route)
    return routes


def event_name_from_title(title: str) -> str:
    """``Guild Members Chunk`` -> ``GUILD_MEMBERS_CHUNK`` (the docs' stated rule)."""

    return re.sub(r"[^A-Za-z0-9]+", "_", title.strip()).strip("_").upper()


@dataclass
class DocEvent:
    name: str
    sources: set[str] = field(default_factory=set)
    line: int = 0


def extract_doc_events_from_text(text: str, section_title: str = "Receive Events") -> dict[str, DocEvent]:
    """Reads event names from the receive-events table and level-4 headings.

    The table is the canonical list; headings are read too because the docs
    have shipped an event heading without updating the table before, and a
    missed event is worse than a reported inconsistency.
    """

    lines = text.splitlines()
    start = None
    for index, line in enumerate(lines):
        heading = _HEADING.match(line)
        if heading and len(heading.group(1)) == 2 and heading.group(2).strip() == section_title:
            start = index + 1
            break
    if start is None:
        return {}
    end = len(lines)
    for index in range(start, len(lines)):
        heading = _HEADING.match(lines[index])
        if heading and len(heading.group(1)) <= 2:
            end = index
            break

    events: dict[str, DocEvent] = {}

    def add(title: str, source: str, line: int) -> None:
        name = event_name_from_title(title)
        if not name:
            return
        event = events.setdefault(name, DocEvent(name, line=line))
        event.sources.add(source)

    in_code = False
    # Only the first table is the event list; later tables in the section are
    # per-event field tables (`| Field | Type | Description |`).
    table_state = "before"
    for index in range(start, end):
        line = lines[index]
        if line.lstrip().startswith("```"):
            in_code = not in_code
            continue
        if in_code:
            continue
        stripped = line.strip()
        if table_state == "inside" and not stripped.startswith("|"):
            table_state = "after"
        if stripped.startswith("|") and table_state != "after":
            table_state = "inside"
            cells = [cell.strip() for cell in stripped.strip("|").split("|")]
            if not cells or not cells[0] or set(cells[0]) <= set("-: "):
                continue
            link = _MD_LINK.match(cells[0])
            title = link.group(1) if link else cells[0]
            if title.lower() == "name":
                continue
            add(title, "table", index + 1)
            continue
        if stripped.startswith("|"):
            continue
        heading = _HEADING.match(line)
        if heading and len(heading.group(1)) == 4:
            title = heading.group(2).strip()
            if title.lower().endswith(" object"):
                continue
            add(title, "heading", index + 1)
    return events


# ---------------------------------------------------------------------------
# Rust source scanning
# ---------------------------------------------------------------------------


_RAW_STRING_OPEN = re.compile(r'r(#*)"')
_CHAR_LITERAL = re.compile(r"'(?:\\u\{[0-9A-Fa-f]+\}|\\.|[^\\'])'")


def mask_rust(source: str, blank_strings: bool = True) -> str:
    """Returns ``source`` with comments (and, by default, string contents) blanked.

    Offsets and newlines are preserved, so positions found in the mask can be
    read back from the original text. String delimiters stay in place so the
    scanner can still locate literals; braces inside strings and comments no
    longer affect brace matching. ``blank_strings=False`` only removes
    comments, for lookups that must see literal text but not commented-out
    code (event names, allowlist evidence).
    """

    out = list(source)
    length = len(source)
    index = 0

    def blank(start: int, end: int) -> None:
        for position in range(start, end):
            if out[position] != "\n":
                out[position] = " "

    while index < length:
        char = source[index]
        nxt = source[index + 1] if index + 1 < length else ""
        if char == "/" and nxt == "/":
            end = source.find("\n", index)
            end = length if end == -1 else end
            blank(index, end)
            index = end
        elif char == "/" and nxt == "*":
            depth, position = 1, index + 2
            while position < length and depth:
                if source.startswith("/*", position):
                    depth, position = depth + 1, position + 2
                elif source.startswith("*/", position):
                    depth, position = depth - 1, position + 2
                else:
                    position += 1
            blank(index, position)
            index = position
        elif (
            char == "r"
            and (raw := _RAW_STRING_OPEN.match(source, index))
            and (index == 0 or not (source[index - 1].isalnum() or source[index - 1] == "_"))
        ):
            open_end = raw.end()
            terminator = '"' + raw.group(1)
            close = source.find(terminator, open_end)
            close = length if close == -1 else close
            if blank_strings:
                blank(open_end, close)
            index = close + len(terminator)
        elif char == '"':
            position = index + 1
            while position < length and source[position] != '"':
                position += 2 if source[position] == "\\" else 1
            if blank_strings:
                blank(index + 1, position)
            index = position + 1
        elif char == "'":
            # Char literal ('x', '\n', '\u{1F600}') versus lifetime ('a).
            literal = _CHAR_LITERAL.match(source, index)
            if literal:
                if blank_strings:
                    blank(index + 1, literal.end() - 1)
                index = literal.end()
            else:
                index += 1
        else:
            index += 1
    return "".join(out)


_OPENERS = {"(": ")", "[": "]", "{": "}"}
_CLOSERS = {")", "]", "}"}


def _matching_close(masked: str, open_index: int) -> int:
    depth = 0
    for position in range(open_index, len(masked)):
        char = masked[position]
        if char in _OPENERS:
            depth += 1
        elif char in _CLOSERS:
            depth -= 1
            if depth == 0:
                return position
    return len(masked) - 1


_CFG_TEST = re.compile(r"#\[cfg\(test\)\]")
_ITEM_AFTER_CFG = re.compile(
    r"\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(mod|fn|impl)\b"
)


def strip_test_code(source: str) -> tuple[str, str]:
    """Blanks ``#[cfg(test)]`` modules, functions, and impls.

    Returns ``(original_with_tests_blanked, mask_with_tests_blanked)``. Test
    harnesses are full of route-shaped literals that do not prove the crate
    calls those routes, so they must never count as coverage. ``#[cfg(test)]``
    on fields or statements is left alone; those never carry routes.
    """

    masked = mask_rust(source)
    original = list(source)
    masked_list = list(masked)
    for match in _CFG_TEST.finditer(masked):
        item = _ITEM_AFTER_CFG.match(masked, match.end())
        if not item:
            continue
        brace = masked.find("{", item.end())
        semicolon = masked.find(";", item.end())
        if item.group(1) == "mod" and semicolon != -1 and (brace == -1 or semicolon < brace):
            end = semicolon
        elif brace == -1:
            continue
        else:
            end = _matching_close(masked, brace)
        for position in range(match.start(), end + 1):
            if original[position] != "\n":
                original[position] = " "
                masked_list[position] = " "
    return "".join(original), "".join(masked_list)


@dataclass
class ManifestRoute:
    """One row of the crate's route manifest (``src/http/routes.rs``)."""

    id: str
    method: str
    template: str
    path: str  # normalized like the docs: ``{}`` per parameter
    auth: str
    query: list[str]
    body: str
    response: str
    client: str
    api: list[str]
    was: list[tuple[str, str]]
    file: str
    line: int


MANIFEST_MACRO = "route_manifest!"
# One row per line; see the module documentation of src/http/routes.rs.
_MANIFEST_ROW = re.compile(
    r"^\s*(?P<id>[A-Za-z_][A-Za-z0-9_]*):\s+(?P<method>GET|POST|PUT|PATCH|DELETE)\s+"
    r'"(?P<template>[^"]+)"\s+(?P<auth>[a-z]+),\s+'
    r"query\s+\[(?P<query>[^\]]*)\],\s+"
    r"body\s+(?P<body>.+?)\s+=>\s+(?P<response>.+?),\s+"
    r"(?P<client>api|oauth2)\s+\[(?P<api>[^\]]*)\]"
    r"(?:,\s+was\s+\[(?P<was>[^\]]*)\])?;\s*$"
)
MANIFEST_AUTH = ("bot", "bearer", "token", "none", "client")
CLIENT_TYPES = {"api": "RestClient", "oauth2": "OAuth2Client"}


def _names(text: str) -> list[str]:
    return [item.strip() for item in text.split(",") if item.strip()]


def parse_manifest_text(text: str, file: str) -> tuple[list[ManifestRoute], list[str]]:
    """Reads the rows of the ``route_manifest! { ... }`` invocation.

    Every non-blank, non-comment line inside the invocation must be a row: a
    line the pattern does not understand is an error, never a silently
    skipped route.
    """

    masked = mask_rust(text, blank_strings=False)
    errors: list[str] = []
    invocation = None
    for match in re.finditer(r"(?<![A-Za-z0-9_])route_manifest!\s*\{", masked):
        if masked[: match.start()].rstrip().endswith("macro_rules!"):
            continue
        invocation = match
    if invocation is None:
        return [], [f"{file}: no `{MANIFEST_MACRO} {{ ... }}` invocation found"]
    open_index = invocation.end() - 1
    close_index = _matching_close(mask_rust(text), open_index)
    starts = _line_starts(text)
    first_line = _line_of(starts, open_index) + 1
    last_line = _line_of(starts, close_index)
    lines = masked.split("\n")
    rows: list[ManifestRoute] = []
    for number in range(first_line, last_line):
        line = lines[number - 1]
        if not line.strip():
            continue
        match = _MANIFEST_ROW.match(line)
        if not match:
            errors.append(f"{file}:{number}: not a manifest row: `{line.strip()[:120]}`")
            continue
        if match["auth"] not in MANIFEST_AUTH:
            errors.append(f"{file}:{number}: unknown auth `{match['auth']}`")
        was = []
        for item in _names(match["was"] or ""):
            old, arrow, new = item.partition("=>")
            if not arrow or not old.strip() or not new.strip():
                errors.append(f"{file}:{number}: `was` entries are `old => new`, got `{item}`")
                continue
            was.append((old.strip(), new.strip()))
        rows.append(
            ManifestRoute(
                id=match["id"],
                method=match["method"],
                template=match["template"],
                path=normalize_template(match["template"]),
                auth=match["auth"],
                query=_names(match["query"]),
                body=match["body"].strip(),
                response=match["response"].strip(),
                client=match["client"],
                api=_names(match["api"]),
                was=was,
                file=file,
                line=number,
            )
        )
    return rows, errors


@dataclass
class FunctionInfo:
    name: str
    owner: str  # the `impl` block's type, or "" for free functions
    file: str
    line: int
    body: str  # comments, string contents, and test code blanked
    endpoint: bool  # `pub` or `pub(crate)` `async fn` inside `impl`
    deprecated: bool


_FN_ITEM = re.compile(
    r"(?P<attrs>(?:#\[[^\]]*\]\s*)*)(?P<vis>pub(?:\([a-z]+\))?\s+)?(?P<async>async\s+)?fn\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
_IMPL_BLOCK = re.compile(r"\bimpl(?:<[^>{]*>)?\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>{]*>)?\s*\{")


def extract_functions_from_text(source: str, file: str) -> list[FunctionInfo]:
    """Functions with bodies in one Rust file, test code excluded."""

    original, masked = strip_test_code(source)
    starts = _line_starts(original)
    impls = []
    for match in _IMPL_BLOCK.finditer(masked):
        impls.append((match.end() - 1, _matching_close(masked, match.end() - 1), match.group(1)))
    functions = []
    for match in _FN_ITEM.finditer(masked):
        brace = masked.find("{", match.end())
        semicolon = masked.find(";", match.end())
        if brace == -1 or (semicolon != -1 and semicolon < brace):
            continue  # trait method declaration without a body
        end = _matching_close(masked, brace)
        owner = ""
        for impl_start, impl_end, name in impls:
            if impl_start < match.start() < impl_end:
                owner = name
        attrs = masked[match.start("attrs") : match.end("attrs")]
        functions.append(
            FunctionInfo(
                name=match["name"],
                owner=owner,
                file=file,
                line=_line_of(starts, match.start("name")),
                body=masked[brace : end + 1],
                endpoint=bool(owner and match["vis"] and match["async"]),
                deprecated="#[deprecated" in attrs,
            )
        )
    return functions


_DIRECT_METHOD = re.compile(r"\bMethod::(GET|POST|PUT|PATCH|DELETE)\b")


def code_text(source: str) -> str:
    """Source without test code and comments, string literals intact."""

    original, _ = strip_test_code(source)
    return mask_rust(original, blank_strings=False)


def extract_event_names_from_text(source: str, pattern: str) -> dict[str, int]:
    text = code_text(source)
    starts = _line_starts(text)
    names: dict[str, int] = {}
    for match in re.finditer(pattern, text):
        names.setdefault(match.group(1), _line_of(starts, match.start()))
    return names


def _expand_sources(repo: Path, patterns: list[str], exclude: list[str]) -> list[Path]:
    files: list[Path] = []
    for pattern in patterns:
        matches = sorted(repo.glob(pattern))
        if not matches:
            raise ConfigError(f"source pattern matched no files: {pattern}")
        for path in matches:
            relative = path.relative_to(repo).as_posix()
            if path.is_file() and path not in files and not _is_excluded(relative, exclude):
                files.append(path)
    return files


def _evidence_corpus(repo: Path, settings: dict) -> str:
    """Non-test source text with whitespace collapsed, for evidence lookups."""

    chunks = []
    for path in _expand_sources(repo, settings["evidence_sources"], settings["evidence_exclude"]):
        # Comments are dropped so a doc comment that quotes a path cannot
        # stand in for the code that builds it.
        chunks.append(" ".join(code_text(path.read_text(encoding="utf-8")).split()))
    return "\n".join(chunks)


# ---------------------------------------------------------------------------
# Allowlist
# ---------------------------------------------------------------------------

ROUTE_TABLES = ("ignored_route", "crate_only_route")
EVENT_TABLES = ("non_dispatch_event", "ignored_event", "crate_only_event")
DEFAULT_SETTINGS = {
    "docs_route_root": "developers",
    "docs_route_exclude": ["developers/change-log.mdx", "developers/change-log/"],
    "docs_events_page": "developers/events/gateway-events.mdx",
    "docs_events_section": "Receive Events",
    "route_manifest": "src/http/routes.rs",
    "rest_sources": ["src/http.rs", "src/http/**/*.rs", "src/oauth2.rs"],
    "rest_sources_exclude": ["src/http/**/tests.rs", "src/http/**/tests/*"],
    # Files that may name an HTTP method directly: the transport core (raw
    # requests, replay policy) and the manifest itself. Anywhere else a
    # `Method::X` means an endpoint bypassing the manifest.
    "direct_method_files": ["src/http.rs", "src/http/routes.rs", "src/http/paths.rs"],
    # Public `async fn`s of the clients that are not endpoints of one route
    # (the raw escape hatch).
    "non_endpoint_functions": [],
    "event_sources": ["src/event/decode.rs"],
    "event_name_pattern": r'"([A-Z][A-Z0-9_]+)"\s*(?:=>|\|)',
    "evidence_sources": ["src/**/*.rs"],
    "evidence_exclude": ["src/**/tests.rs", "src/**/tests/*"],
    "min_docs_routes": 150,
    "min_crate_routes": 100,
    "min_docs_events": 50,
    "min_crate_events": 50,
}
MIN_REASON_CHARS = 15


@dataclass
class AllowEntry:
    table: str
    key: object  # tuple[str, str] for routes, str for events
    reason: str
    evidence: list[str]
    functions: list[str] = field(default_factory=list)


@dataclass
class Allowlist:
    settings: dict
    entries: dict[str, list[AllowEntry]]

    def keys(self, table: str) -> set:
        return {entry.key for entry in self.entries.get(table, [])}


def load_allowlist(path: Path) -> Allowlist:
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ConfigError(f"cannot read allowlist {path}: {error}") from error
    return parse_allowlist(data)


def parse_allowlist(data: dict) -> Allowlist:
    settings = dict(DEFAULT_SETTINGS)
    unknown_settings = set(data.get("settings", {})) - set(DEFAULT_SETTINGS)
    if unknown_settings:
        raise ConfigError(f"unknown settings: {sorted(unknown_settings)}")
    settings.update(data.get("settings", {}))
    unknown_tables = set(data) - set(ROUTE_TABLES) - set(EVENT_TABLES) - {"settings"}
    if unknown_tables:
        raise ConfigError(f"unknown allowlist tables: {sorted(unknown_tables)}")

    entries: dict[str, list[AllowEntry]] = {}
    seen: dict[object, str] = {}
    errors: list[str] = []
    for table in ROUTE_TABLES + EVENT_TABLES:
        for index, raw in enumerate(data.get(table, [])):
            where = f"[[{table}]] #{index + 1}"
            reason = str(raw.get("reason", "")).strip()
            # An entry without a reason silences drift with no record of why,
            # which is exactly how an allowlist rots into a blanket ignore.
            if len(reason) < MIN_REASON_CHARS:
                errors.append(f"{where}: `reason` is required (at least {MIN_REASON_CHARS} characters)")
            evidence = raw.get("evidence", [])
            if isinstance(evidence, str):
                evidence = [evidence]
            functions = raw.get("functions", [])
            if isinstance(functions, str):
                functions = [functions]
            if functions:
                errors.append(
                    f"{where}: `functions` belonged to the removed helper_route table; "
                    "list endpoint methods in the route manifest instead"
                )
            if table in ROUTE_TABLES:
                method = str(raw.get("method", "")).upper()
                path = str(raw.get("path", ""))
                if method not in HTTP_METHODS:
                    errors.append(f"{where}: `method` must be one of {', '.join(HTTP_METHODS)}")
                if not path.startswith("/"):
                    errors.append(f"{where}: `path` must start with '/'")
                key: object = route_key(method, normalize_template(path))
                label = format_route(key)  # type: ignore[arg-type]
            else:
                name = str(raw.get("name", "")).strip()
                if not re.fullmatch(r"[A-Z][A-Z0-9_]*", name):
                    errors.append(f"{where}: `name` must be an UPPER_SNAKE event name")
                key = name
                label = name
            unknown_keys = set(raw) - {"method", "path", "name", "reason", "evidence", "functions"}
            if unknown_keys:
                errors.append(f"{where}: unknown keys {sorted(unknown_keys)}")
            family = "route" if table in ROUTE_TABLES else "event"
            if (family, key) in seen:
                errors.append(f"{where}: {label} is already listed in [[{seen[(family, key)]}]]")
            seen[(family, key)] = table
            entries.setdefault(table, []).append(
                AllowEntry(
                    table,
                    key,
                    reason,
                    [str(item) for item in evidence],
                    [str(item) for item in functions],
                )
            )
    if errors:
        raise ConfigError("invalid allowlist:\n  " + "\n  ".join(errors))
    return Allowlist(settings, entries)


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------


@dataclass
class CrateScan:
    routes: dict[tuple[str, str], list[ManifestRoute]]
    manifest: list[ManifestRoute]
    manifest_errors: list[str]
    functions: list[FunctionInfo]
    direct_methods: list[str]
    events: dict[str, tuple[str, int]]
    evidence_corpus: str


def scan_crate(repo: Path, allowlist: Allowlist) -> CrateScan:
    settings = allowlist.settings
    manifest_path = repo / settings["route_manifest"]
    if not manifest_path.is_file():
        raise ConfigError(f"route manifest not found: {manifest_path}")
    manifest, manifest_errors = parse_manifest_text(
        manifest_path.read_text(encoding="utf-8"), settings["route_manifest"]
    )
    routes: dict[tuple[str, str], list[ManifestRoute]] = {}
    for row in manifest:
        routes.setdefault(route_key(row.method, row.path), []).append(row)
    functions: list[FunctionInfo] = []
    direct_methods: list[str] = []
    for path in _expand_sources(repo, settings["rest_sources"], settings["rest_sources_exclude"]):
        relative = path.relative_to(repo).as_posix()
        source = path.read_text(encoding="utf-8")
        functions += extract_functions_from_text(source, relative)
        if relative not in settings["direct_method_files"]:
            original, masked = strip_test_code(source)
            starts = _line_starts(original)
            for match in _DIRECT_METHOD.finditer(masked):
                direct_methods.append(f"{relative}:{_line_of(starts, match.start())}: Method::{match.group(1)}")
    events: dict[str, tuple[str, int]] = {}
    for path in _expand_sources(repo, settings["event_sources"], []):
        relative = path.relative_to(repo).as_posix()
        names = extract_event_names_from_text(
            path.read_text(encoding="utf-8"), settings["event_name_pattern"]
        )
        for name, line in names.items():
            events.setdefault(name, (relative, line))
    return CrateScan(
        routes, manifest, manifest_errors, functions, direct_methods, events,
        _evidence_corpus(repo, settings),
    )


def missing_evidence(allowlist: Allowlist, corpus: str) -> list[str]:
    problems = []
    for table in ROUTE_TABLES + EVENT_TABLES:
        for entry in allowlist.entries.get(table, []):
            for snippet in entry.evidence:
                if " ".join(snippet.split()) not in corpus:
                    label = format_route(entry.key) if table in ROUTE_TABLES else entry.key
                    problems.append(
                        f"[[{table}]] {label}: evidence `{snippet}` no longer appears in src/"
                    )
    return problems


def manifest_binding(allowlist: Allowlist, scan: CrateScan) -> list[str]:
    """Binds the manifest rows to the code that sends them, in both directions.

    - every row's methods exist and actually send that row's route: the body
      names `RouteId::<Id>`, or calls (`self.f(`) a method of the same client
      that does;
    - every endpoint method of a client (`pub`/`pub(crate)` `async fn` in an
      `impl RestClient` / `impl OAuth2Client` block) is listed by some row,
      so a new endpoint cannot bypass the manifest;
    - a `was` name (removed in 3.0) is gone, or survives only as a
      `#[deprecated]` forwarding method, and its replacement is listed in the
      same row;
    - no endpoint file names an HTTP method directly.

    Without the second direction a new route would show up as "missing" in
    the weekly report instead of failing the PR that added it; without the
    first, a renamed method would leave a row that nothing sends.
    """

    notes = list(scan.manifest_errors)
    seen: dict[str, ManifestRoute] = {}
    for row in scan.manifest:
        if row.id in seen:
            notes.append(f"route manifest: `{row.id}` is listed twice ({row.file}:{seen[row.id].line} and :{row.line})")
        seen[row.id] = row

    by_owner: dict[tuple[str, str], FunctionInfo] = {}
    for function in scan.functions:
        if function.owner:
            by_owner.setdefault((function.owner, function.name), function)
    calls = re.compile(r"\b(?:self|Self)\s*(?:\.|::)\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:::<[^>]*>)?\s*\(")
    cache: dict[tuple[str, str], set[str]] = {}

    def routes_sent(owner: str, name: str, stack: frozenset = frozenset()) -> set[str]:
        key = (owner, name)
        if key in cache:
            return cache[key]
        function = by_owner.get(key)
        if function is None or key in stack:
            return set()
        sent = set(re.findall(r"\bRouteId::([A-Za-z_][A-Za-z0-9_]*)", function.body))
        for callee in calls.findall(function.body):
            sent |= routes_sent(owner, callee, stack | {key})
        cache[key] = sent
        return sent

    listed: set[tuple[str, str]] = set()
    for row in scan.manifest:
        owner = CLIENT_TYPES[row.client]
        if not row.api:
            notes.append(f"route manifest: `{row.id}` lists no methods ({row.file}:{row.line})")
        for name in row.api:
            listed.add((owner, name))
            if (owner, name) not in by_owner:
                notes.append(f"route manifest: `{row.id}` lists `{owner}::{name}`, which does not exist")
            elif row.id not in routes_sent(owner, name):
                notes.append(
                    f"route manifest: `{owner}::{name}` is listed under `{row.id}` but never sends "
                    f"`RouteId::{row.id}` (directly or through a `self.` call)"
                )
        for old, new in row.was:
            listed.add((owner, old))
            if new not in row.api:
                notes.append(f"route manifest: `{row.id}` maps `{old}` to `{new}`, which the row does not list")
            function = by_owner.get((owner, old))
            if function is not None and not function.deprecated:
                notes.append(
                    f"route manifest: `{owner}::{old}` is recorded as removed (`was`) under `{row.id}` "
                    f"but still exists without #[deprecated] ({function.file}:{function.line})"
                )
    exempt = set(allowlist.settings["non_endpoint_functions"])
    for function in scan.functions:
        if (
            function.endpoint
            and function.owner in CLIENT_TYPES.values()
            and (function.owner, function.name) not in listed
            and function.name not in exempt
        ):
            notes.append(
                f"endpoint `{function.owner}::{function.name}` ({function.file}:{function.line}) is not "
                f"in the route manifest; add it to its row in {allowlist.settings['route_manifest']}"
            )
    for site in scan.direct_methods:
        notes.append(f"{site}: endpoints name their route with `RouteId`, not an HTTP method")
    return notes


def crate_maintenance(allowlist: Allowlist, scan: CrateScan) -> list[str]:
    """Allowlist and manifest problems that only depend on this repository."""

    notes = missing_evidence(allowlist, scan.evidence_corpus)
    notes += manifest_binding(allowlist, scan)
    covered = set(scan.routes)
    for key in sorted(allowlist.keys("ignored_route") & covered):
        notes.append(f"[[ignored_route]] {format_route(key)}: the crate implements it now")
    for key in sorted(allowlist.keys("crate_only_route") - covered):
        notes.append(f"[[crate_only_route]] {format_route(key)}: no longer found in the crate")
    events = set(scan.events)
    for name in sorted(allowlist.keys("crate_only_event") - events):
        notes.append(f"[[crate_only_event]] {name}: no longer decoded by the crate")
    for name in sorted(allowlist.keys("ignored_event") & events):
        notes.append(f"[[ignored_event]] {name}: the crate decodes it now")
    return notes


def sanity_errors(allowlist: Allowlist, scan: CrateScan, docs: dict | None = None,
                  doc_events: dict | None = None) -> list[str]:
    settings = allowlist.settings
    errors = []
    if len(scan.routes) < settings["min_crate_routes"]:
        errors.append(
            f"only {len(scan.routes)} distinct routes in the route manifest (floor {settings['min_crate_routes']}); "
            "the manifest row shape probably changed - update ci/api_drift.py"
        )
    if len(scan.events) < settings["min_crate_events"]:
        errors.append(
            f"only {len(scan.events)} decoded event names found (floor {settings['min_crate_events']}); "
            "update `event_sources` / `event_name_pattern` in the allowlist settings"
        )
    if docs is not None and len(docs) < settings["min_docs_routes"]:
        errors.append(
            f"only {len(docs)} documented routes found (floor {settings['min_docs_routes']}); "
            "the discord-api-docs layout probably changed"
        )
    if doc_events is not None and len(doc_events) < settings["min_docs_events"]:
        errors.append(
            f"only {len(doc_events)} documented receive events found (floor {settings['min_docs_events']}); "
            "the gateway events page layout probably changed"
        )
    return errors


@dataclass
class Report:
    docs_commit: str
    crate_commit: str
    generated_at: str
    docs_routes: dict
    crate: CrateScan
    allowlist: Allowlist
    doc_events: dict
    missing_routes: list
    extra_routes: list
    missing_events: list
    extra_events: list
    maintenance: list
    heading_only_events: list

    @property
    def has_findings(self) -> bool:
        return bool(
            self.missing_routes
            or self.extra_routes
            or self.missing_events
            or self.extra_events
            or self.maintenance
        )

    @property
    def fingerprint(self) -> str:
        """Stable digest of the findings only (no commits or timestamps).

        The issue updater comments only when this changes, so an unchanged
        drift does not ping subscribers every week.
        """

        payload = json.dumps(
            {
                "missing_routes": [format_route(key) for key in self.missing_routes],
                "extra_routes": [format_route(key) for key in self.extra_routes],
                "missing_events": self.missing_events,
                "extra_events": self.extra_events,
                "maintenance": self.maintenance,
            },
            sort_keys=True,
        )
        return hashlib.sha256(payload.encode()).hexdigest()[:16]


def compare(docs_routes: dict, doc_events: dict, scan: CrateScan, allowlist: Allowlist,
            docs_commit: str = "unknown", crate_commit: str = "unknown") -> Report:
    documented = set(docs_routes)
    ignored = allowlist.keys("ignored_route")
    crate_only = allowlist.keys("crate_only_route")
    covered = set(scan.routes)

    missing_routes = sorted(documented - covered - ignored)
    extra_routes = sorted(covered - documented - crate_only)

    documented_events = set(doc_events) - allowlist.keys("non_dispatch_event")
    crate_events = set(scan.events)
    missing_events = sorted(documented_events - crate_events - allowlist.keys("ignored_event"))
    extra_events = sorted(crate_events - documented_events - allowlist.keys("crate_only_event"))

    maintenance = crate_maintenance(allowlist, scan)
    for key in sorted(ignored - documented):
        maintenance.append(f"[[ignored_route]] {format_route(key)}: no longer documented by Discord")
    for key in sorted(crate_only & documented):
        maintenance.append(f"[[crate_only_route]] {format_route(key)}: Discord documents it now")
    for name in sorted(allowlist.keys("non_dispatch_event") - set(doc_events)):
        maintenance.append(f"[[non_dispatch_event]] {name}: no longer listed by Discord")
    for name in sorted(allowlist.keys("ignored_event") - set(doc_events)):
        maintenance.append(f"[[ignored_event]] {name}: no longer listed by Discord")
    for name in sorted(allowlist.keys("crate_only_event") & documented_events):
        maintenance.append(f"[[crate_only_event]] {name}: Discord documents it now")

    heading_only = sorted(name for name, event in doc_events.items() if event.sources == {"heading"})
    return Report(
        docs_commit=docs_commit,
        crate_commit=crate_commit,
        generated_at=datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        docs_routes=docs_routes,
        crate=scan,
        allowlist=allowlist,
        doc_events=doc_events,
        missing_routes=missing_routes,
        extra_routes=extra_routes,
        missing_events=missing_events,
        extra_events=extra_events,
        maintenance=maintenance,
        heading_only_events=heading_only,
    )


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------


def _capped(lines: list[str]) -> list[str]:
    if len(lines) <= MAX_ITEMS_PER_SECTION:
        return lines
    hidden = len(lines) - MAX_ITEMS_PER_SECTION
    return lines[:MAX_ITEMS_PER_SECTION] + [f"- ... and {hidden} more (see the workflow artifact)"]


def render_markdown(report: Report, max_chars: int = DEFAULT_MAX_CHARS) -> str:
    documented = set(report.docs_routes)
    covered_routes = documented - set(report.missing_routes) - report.allowlist.keys("ignored_route")
    documented_events = set(report.doc_events) - report.allowlist.keys("non_dispatch_event")
    covered_events = documented_events - set(report.missing_events) - report.allowlist.keys("ignored_event")

    out = [REPORT_MARKER, "## Discord API drift report", ""]
    status = "Drift found" if report.has_findings else "No drift"
    out += [
        f"**{status}.** discord-api-docs `{report.docs_commit}` vs discordrs `{report.crate_commit}`, "
        f"generated {report.generated_at} by `ci/api_drift.py`.",
        "",
        "| Area | Documented | Covered | Missing in crate | Not documented |",
        "| --- | ---: | ---: | ---: | ---: |",
        f"| REST routes (method + path) | {len(documented)} | {len(covered_routes)} | "
        f"{len(report.missing_routes)} | {len(report.extra_routes)} |",
        f"| Gateway dispatch events | {len(documented_events)} | {len(covered_events)} | "
        f"{len(report.missing_events)} | {len(report.extra_events)} |",
        "",
    ]

    def section(title: str, lines: list[str], hint: str) -> None:
        if not lines:
            return
        out.extend([f"### {title}", "", hint, ""])
        out.extend(_capped(lines))
        out.append("")

    missing_lines = []
    for key in report.missing_routes:
        doc = report.docs_routes[key][0]
        deprecated = " (deprecated)" if doc.deprecated else ""
        missing_lines.append(
            f"- `{format_route(key)}` - {doc.heading or 'no heading'}{deprecated} (`{doc.file}:{doc.line}`)"
        )
    section(
        "Documented REST routes missing from the crate",
        missing_lines,
        "Add a `RestClient` method and its row in the route manifest (`src/http/routes.rs`), or an "
        "`ignored_route` allowlist entry with a reason if the route is intentionally unsupported.",
    )

    extra_lines = []
    for key in report.extra_routes:
        rows = report.crate.routes[key]
        where = ", ".join(f"`{row.id}` (`{row.file}:{row.line}`)" for row in rows)
        extra_lines.append(f"- `{format_route(key)}` - {where}")
    section(
        "Crate REST routes not in the docs",
        extra_lines,
        "Discord removed or renamed the route, or the manifest row is wrong. "
        "Deprecate the method, or allowlist it under `crate_only_route` with a reason.",
    )

    section(
        "Documented Gateway dispatch events the crate does not decode",
        [f"- `{name}` (`{report.allowlist.settings['docs_events_page']}:{report.doc_events[name].line}`)"
         for name in report.missing_events],
        "Add a decoder branch (unknown events still arrive as raw payloads), or allowlist the name.",
    )
    section(
        "Decoded events not listed by the docs",
        [f"- `{name}` (`{report.crate.events[name][0]}:{report.crate.events[name][1]}`)"
         for name in report.extra_events],
        "Discord removed or renamed the event, or it is undocumented; allowlist it under `crate_only_event` "
        "with a reason if it stays.",
    )
    section(
        "Allowlist maintenance",
        [f"- {note}" for note in report.maintenance],
        "Entries in `ci/api-drift-allowlist.toml` that no longer match reality.",
    )

    deprecated = sorted(
        key for key, docs in report.docs_routes.items() if any(doc.deprecated for doc in docs)
    )
    out += [
        "<details><summary>Method and counts</summary>",
        "",
        f"- Docs routes: `<Route>` tags under `{report.allowlist.settings['docs_route_root']}/` "
        f"excluding {', '.join('`' + item + '`' for item in report.allowlist.settings['docs_route_exclude'])}; "
        "markdown links and backslashes stripped, `{params}` normalized to `{}`.",
        f"- Crate routes: {len(report.crate.routes)} distinct method + path pairs from "
        f"{len(report.crate.manifest)} rows of the route manifest "
        f"`{report.allowlist.settings['route_manifest']}` (the same rows the transport executes).",
        f"- Ignored documented routes: {len(report.allowlist.keys('ignored_route'))}; "
        f"allowlisted crate-only routes: {len(report.allowlist.keys('crate_only_route'))}.",
        f"- Docs routes whose heading says deprecated: {len(deprecated)}"
        + (f" ({', '.join('`' + format_route(key) + '`' for key in deprecated)})" if deprecated else "")
        + ".",
        f"- Events: `{report.allowlist.settings['docs_events_section']}` table and level-4 headings of "
        f"`{report.allowlist.settings['docs_events_page']}` vs {len(report.crate.events)} decoder names; "
        f"non-dispatch opcodes excluded: {', '.join(sorted(report.allowlist.keys('non_dispatch_event'))) or 'none'}.",
    ]
    if report.heading_only_events:
        out.append(
            "- Events with a heading but no receive-events table row (docs inconsistency): "
            + ", ".join(f"`{name}`" for name in report.heading_only_events)
            + "."
        )
    out += ["", "</details>", "", f"{FINGERPRINT_PREFIX}{report.fingerprint} -->", ""]

    text = "\n".join(out)
    if len(text) > max_chars:
        note = "\n\n_Report truncated to fit an issue body; the full report is in the workflow artifact._\n"
        tail = f"\n{FINGERPRINT_PREFIX}{report.fingerprint} -->\n"
        text = text[: max_chars - len(note) - len(tail)] + note + tail
    return text


def render_json(report: Report) -> str:
    def route_entry(key):
        return {"method": key[0], "path": key[1]}

    return json.dumps(
        {
            "docs_commit": report.docs_commit,
            "crate_commit": report.crate_commit,
            "generated_at": report.generated_at,
            "has_findings": report.has_findings,
            "fingerprint": report.fingerprint,
            "counts": {
                "docs_routes": len(report.docs_routes),
                "crate_routes": len(report.crate.routes),
                "manifest_rows": len(report.crate.manifest),
                "docs_events": len(report.doc_events),
                "crate_events": len(report.crate.events),
            },
            "missing_routes": [route_entry(key) for key in report.missing_routes],
            "extra_routes": [route_entry(key) for key in report.extra_routes],
            "missing_events": report.missing_events,
            "extra_events": report.extra_events,
            "maintenance": report.maintenance,
            "heading_only_events": report.heading_only_events,
        },
        indent=2,
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _git_short_head(path: Path) -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "--short=7", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        )
        return result.stdout.strip() or "unknown"
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def run(argv: list[str] | None = None) -> int:
    script_dir = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--docs", type=Path, help="path to a discord/discord-api-docs checkout")
    parser.add_argument("--repo", type=Path, default=script_dir.parent, help="crate root (default: repo of this script)")
    parser.add_argument("--allowlist", type=Path, default=script_dir / "api-drift-allowlist.toml")
    parser.add_argument("--format", choices=("markdown", "json"), default="markdown")
    parser.add_argument("--output", type=Path, help="write the report here instead of stdout")
    parser.add_argument("--json-output", type=Path, help="also write the JSON report here")
    parser.add_argument("--docs-commit", help="override the docs commit shown in the report")
    parser.add_argument("--max-chars", type=int, default=DEFAULT_MAX_CHARS)
    parser.add_argument("--fail-on-drift", action="store_true", help="exit 1 when drift is found (local use)")
    parser.add_argument(
        "--self-check",
        action="store_true",
        help="validate the allowlist and crate extraction only; no docs checkout needed (used by CI)",
    )
    args = parser.parse_args(argv)

    try:
        allowlist = load_allowlist(args.allowlist)
        repo = args.repo.resolve()
        scan = scan_crate(repo, allowlist)
        if args.self_check:
            # Any crate-side mismatch fails: the allowlist must describe the
            # crate exactly, or the weekly report stops being trustworthy.
            errors = sanity_errors(allowlist, scan) + crate_maintenance(allowlist, scan)
            print(
                f"crate: {len(scan.manifest)} manifest rows ({len(scan.routes)} distinct routes), "
                f"{len(scan.events)} decoded events"
            )
            for error in errors:
                print(f"error: {error}", file=sys.stderr)
            if not errors:
                print("allowlist and route manifest match the crate")
            return EXIT_BROKEN if errors else EXIT_OK

        if args.docs is None:
            parser.error("--docs is required unless --self-check is given")
        docs_root = args.docs.resolve()
        docs_routes = extract_doc_routes(docs_root, allowlist.settings)
        events_page = docs_root / allowlist.settings["docs_events_page"]
        if not events_page.is_file():
            raise ConfigError(f"gateway events page not found: {events_page}")
        doc_events = extract_doc_events_from_text(
            events_page.read_text(encoding="utf-8"), allowlist.settings["docs_events_section"]
        )
        errors = sanity_errors(allowlist, scan, docs_routes, doc_events)
        if errors:
            raise ConfigError("extraction looks broken:\n  " + "\n  ".join(errors))
        report = compare(
            docs_routes,
            doc_events,
            scan,
            allowlist,
            docs_commit=args.docs_commit or _git_short_head(docs_root),
            crate_commit=_git_short_head(repo),
        )
    except ConfigError as error:
        print(f"error: {error}", file=sys.stderr)
        return EXIT_BROKEN

    text = render_json(report) if args.format == "json" else render_markdown(report, args.max_chars)
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text if text.endswith("\n") else text + "\n")
    if args.json_output:
        args.json_output.write_text(render_json(report) + "\n", encoding="utf-8")
    if args.fail_on_drift and report.has_findings:
        return EXIT_DRIFT
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(run())
