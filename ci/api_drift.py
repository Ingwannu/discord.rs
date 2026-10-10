#!/usr/bin/env python3
"""Report drift between the official Discord API docs and this crate.

Compares two things:

* REST routes: every ``<Route method="X">path</Route>`` in
  ``discord/discord-api-docs`` against the ``Method::X`` + path literal pairs
  in the crate's REST sources (plus allowlisted helper-built routes).
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


def normalize_crate_path(literal: str) -> str:
    """Normalizes a Rust format-string path literal.

    ``format!("/guilds/{}/members{query}")`` appends a pre-encoded query
    string directly after the last segment, so placeholders glued to the end
    of the final segment are query suffixes, not path parameters.
    """

    path = literal.split("?", 1)[0]
    path = _PLACEHOLDER.sub("{}", path)
    segments = path.split("/")
    if segments:
        segments[-1] = re.sub(r"(?<=.)(?:\{\})+$", "", segments[-1])
    return _tidy_slashes("/".join(segments))


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
class CrateRoute:
    method: str
    path: str
    literal: str
    file: str
    line: int
    function: str


@dataclass
class UnresolvedCall:
    method: str
    expression: str
    file: str
    line: int
    function: str


_METHOD_TOKEN = re.compile(r"\bMethod::(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b")
_BUILDER_CALL = re.compile(r"\.(get|post|put|patch|delete)\(\s*&?\s*format!\(\s*\"")
_FN_NAME = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
_PATH_VARIABLE = re.compile(r"&?\s*([a-z_][a-z0-9_]*)(?:\s*\.\s*as_str\(\s*\))?")


def _enclosing_function(masked: str, offset: int) -> tuple[str, int]:
    name, start = "?", 0
    for match in _FN_NAME.finditer(masked, 0, offset):
        name, start = match.group(1), match.start()
    return name, start


_FORMAT_OPEN = re.compile(r"format!\(\s*$")
_EXPRESSION_START = re.compile(r"^[\s&]*$")


def _path_literals(original: str, masked: str, start: int, end: int) -> list[str]:
    """Route-shaped literals that *are* the path in ``masked[start:end]``.

    Only a ``format!`` string or a bare literal at the start of the expression
    counts. Other literals are arguments to something else - for example
    ``execute_webhook_path_with_query(.., Some("/slack"))`` - and treating
    them as paths would invent routes such as ``POST /slack``.
    """

    found = []
    position = start
    while True:
        quote = masked.find('"', position, end)
        if quote == -1:
            return found
        close = masked.find('"', quote + 1, end + 1)
        if close == -1:
            return found
        before = masked[max(start, quote - 40) : quote]
        literal = original[quote + 1 : close]
        if literal.startswith("/") and (
            _FORMAT_OPEN.search(before) or _EXPRESSION_START.match(masked[start:quote])
        ):
            found.append(literal)
        position = close + 1


def _expression_end(masked: str, start: int, stop_chars: str) -> int:
    """End of the expression starting at ``start``: first depth-0 stop char."""

    position, depth = start, 0
    while position < len(masked):
        char = masked[position]
        if char in _OPENERS:
            depth += 1
        elif char in _CLOSERS:
            if depth == 0:
                return position
            depth -= 1
        elif char in stop_chars and depth == 0:
            return position
        position += 1
    return position


def _local_binding_literals(original: str, masked: str, ident: str, function_start: int,
                            offset: int) -> list[str]:
    """Path literals assigned to ``let [mut] ident = ..`` earlier in the same function.

    One hop only: ``let path = format!("/guilds/{}/members/search{}", ..)``
    resolves, ``let path = webhook_message_path(..)?`` does not, because
    following helper functions would turn this scanner into a Rust evaluator.
    Helper-built paths go into the allowlist instead, with evidence.
    """

    binding = None
    pattern = re.compile(rf"\blet\s+(?:mut\s+)?{re.escape(ident)}\b\s*(?::[^=;]*)?=(?!=)")
    for match in pattern.finditer(masked, function_start, offset):
        binding = match
    if binding is None:
        return []
    end = _expression_end(masked, binding.end(), ";")
    return _path_literals(original, masked, binding.end(), end)


def extract_crate_routes_from_text(
    source: str, file: str
) -> tuple[list[CrateRoute], list[UnresolvedCall]]:
    """Finds REST call sites in one Rust source file.

    Recognized shapes:

    * ``self.request*(Method::X, "/literal", ..)`` and
      ``self.request*(Method::X, &format!("/literal/{}", ..), ..)``
    * reqwest builder calls ``.post(format!("{}/literal", base))`` as used by
      the OAuth2 client, where ``{}`` is the API base URL.

    ``Method::X`` followed by a non-literal path (``&path``, or a format string
    that starts with a helper-built prefix) is returned as unresolved; those
    routes must be listed in the allowlist's ``helper_route`` table.
    """

    original, masked = strip_test_code(source)
    starts = _line_starts(original)
    routes: list[CrateRoute] = []
    unresolved: list[UnresolvedCall] = []

    for match in _METHOD_TOKEN.finditer(masked):
        method = match.group(1)
        position = match.end()
        while position < len(masked) and masked[position].isspace():
            position += 1
        if position >= len(masked) or masked[position] != ",":
            continue  # `Method::GET | Method::HEAD`, `== Method::POST`, ...
        position += 1
        while position < len(masked) and masked[position].isspace():
            position += 1
        if position >= len(masked) or masked[position] == "[":
            continue  # route-pattern match arms such as `(&Method::POST, [..])`
        if method not in HTTP_METHODS:
            continue
        arg_start = position
        arg_end = _expression_end(masked, arg_start, ",")
        line = _line_of(starts, match.start())
        function, function_start = _enclosing_function(masked, match.start())
        literals = _path_literals(original, masked, arg_start, arg_end)
        expression = " ".join(original[arg_start:arg_end].split())
        if not literals and (variable := _PATH_VARIABLE.fullmatch(expression)):
            literals = _local_binding_literals(
                original, masked, variable.group(1), function_start, match.start()
            )
        if not literals:
            unresolved.append(UnresolvedCall(method, expression, file, line, function))
        # A `match` binding can hold several literals (with and without a
        # query string); each distinct normalized path is one route.
        for path in dict.fromkeys(normalize_crate_path(item) for item in literals):
            routes.append(CrateRoute(method, path, literals[0], file, line, function))

    for match in _BUILDER_CALL.finditer(masked):
        quote = match.end() - 1
        close = masked.find('"', quote + 1)
        literal = original[quote + 1 : close]
        if not literal.startswith("{}/"):
            continue
        literal = literal[2:]
        routes.append(
            CrateRoute(
                match.group(1).upper(),
                normalize_crate_path(literal),
                literal,
                file,
                _line_of(starts, match.start()),
                _enclosing_function(masked, match.start())[0],
            )
        )
    return routes, unresolved


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

ROUTE_TABLES = ("helper_route", "ignored_route", "crate_only_route")
EVENT_TABLES = ("non_dispatch_event", "ignored_event", "crate_only_event")
DEFAULT_SETTINGS = {
    "docs_route_root": "developers",
    "docs_route_exclude": ["developers/change-log.mdx", "developers/change-log/"],
    "docs_events_page": "developers/events/gateway-events.mdx",
    "docs_events_section": "Receive Events",
    "rest_sources": ["src/http.rs", "src/http/**/*.rs", "src/oauth2.rs"],
    "rest_sources_exclude": ["src/http/**/tests.rs", "src/http/**/tests/*"],
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
            if table == "helper_route" and not evidence:
                errors.append(f"{where}: helper routes need `evidence` (source substrings)")
            functions = raw.get("functions", [])
            if isinstance(functions, str):
                functions = [functions]
            if functions and table != "helper_route":
                errors.append(f"{where}: `functions` is only meaningful for helper routes")
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
    routes: dict[tuple[str, str], list[CrateRoute]]
    unresolved: list[UnresolvedCall]
    events: dict[str, tuple[str, int]]
    evidence_corpus: str


def scan_crate(repo: Path, allowlist: Allowlist) -> CrateScan:
    settings = allowlist.settings
    routes: dict[tuple[str, str], list[CrateRoute]] = {}
    unresolved: list[UnresolvedCall] = []
    for path in _expand_sources(repo, settings["rest_sources"], settings["rest_sources_exclude"]):
        relative = path.relative_to(repo).as_posix()
        found, missing = extract_crate_routes_from_text(path.read_text(encoding="utf-8"), relative)
        for route in found:
            routes.setdefault(route_key(route.method, route.path), []).append(route)
        unresolved.extend(missing)
    events: dict[str, tuple[str, int]] = {}
    for path in _expand_sources(repo, settings["event_sources"], []):
        relative = path.relative_to(repo).as_posix()
        names = extract_event_names_from_text(
            path.read_text(encoding="utf-8"), settings["event_name_pattern"]
        )
        for name, line in names.items():
            events.setdefault(name, (relative, line))
    return CrateScan(routes, unresolved, events, _evidence_corpus(repo, settings))


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


def unmapped_call_sites(allowlist: Allowlist, scan: CrateScan) -> list[str]:
    """Binds helper routes to the call sites that send them, in both directions.

    Every listed function must still send the route's method with a
    helper-built path, and every helper-built call site must be claimed by
    some entry. Without the second direction a new helper-built route would
    silently show up as "missing" in the weekly report instead of failing the
    PR that added it.
    """

    notes = []
    sites = {(call.method, call.function) for call in scan.unresolved}
    claimed = set()
    for entry in allowlist.entries.get("helper_route", []):
        method = entry.key[0]  # type: ignore[index]
        for function in entry.functions:
            claimed.add((method, function))
            if (method, function) not in sites:
                notes.append(
                    f"[[helper_route]] {format_route(entry.key)}: `fn {function}` no longer sends "
                    f"Method::{method} with a helper-built path (renamed, removed, or now a literal?)"
                )
    for call in scan.unresolved:
        if (call.method, call.function) not in claimed:
            notes.append(
                f"unmapped helper-built call: Method::{call.method} in `fn {call.function}` "
                f"({call.file}:{call.line}) passes `{call.expression}`; add the function to the "
                "matching [[helper_route]] entry (or a new one) in ci/api-drift-allowlist.toml"
            )
    return notes


def crate_maintenance(allowlist: Allowlist, scan: CrateScan) -> list[str]:
    """Allowlist problems that only depend on this repository."""

    notes = missing_evidence(allowlist, scan.evidence_corpus)
    notes += unmapped_call_sites(allowlist, scan)
    literal = set(scan.routes)
    for key in sorted(allowlist.keys("helper_route") & literal):
        notes.append(
            f"[[helper_route]] {format_route(key)}: now found as a path literal; the entry is redundant"
        )
    helper = allowlist.keys("helper_route")
    for key in sorted(allowlist.keys("ignored_route") & (literal | helper)):
        notes.append(f"[[ignored_route]] {format_route(key)}: the crate implements it now")
    for key in sorted(allowlist.keys("crate_only_route") - literal - helper):
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
            f"only {len(scan.routes)} crate route literals found (floor {settings['min_crate_routes']}); "
            "the REST call shape probably changed - update ci/api_drift.py"
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
    literal = set(scan.routes)
    helper = allowlist.keys("helper_route")
    ignored = allowlist.keys("ignored_route")
    crate_only = allowlist.keys("crate_only_route")
    covered = literal | helper

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
        "Add a `RestClient` method, or an allowlist entry with a reason if the route is built by a helper "
        "or intentionally unsupported.",
    )

    extra_lines = []
    for key in report.extra_routes:
        sites = report.crate.routes.get(key)
        where = f"`{sites[0].file}:{sites[0].line}` (`{sites[0].function}`)" if sites else "allowlist `helper_route`"
        extra_lines.append(f"- `{format_route(key)}` - {where}")
    section(
        "Crate REST routes not in the docs",
        extra_lines,
        "Discord removed or renamed the route, or the path literal is not a real route. "
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
        f"- Crate routes: {len(report.crate.routes)} distinct method + path literals from "
        f"{', '.join('`' + item + '`' for item in report.allowlist.settings['rest_sources'])}, "
        f"plus {len(report.allowlist.keys('helper_route'))} allowlisted helper-built routes "
        f"({len(report.crate.unresolved)} call sites pass a helper-built path).",
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
                "crate_literal_routes": len(report.crate.routes),
                "helper_routes": len(report.allowlist.keys("helper_route")),
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
                f"crate: {len(scan.routes)} route literals, {len(allowlist.keys('helper_route'))} helper routes "
                f"covering {len(scan.unresolved)} helper-built call sites, {len(scan.events)} decoded events"
            )
            for error in errors:
                print(f"error: {error}", file=sys.stderr)
            if not errors:
                print("allowlist matches the crate")
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
