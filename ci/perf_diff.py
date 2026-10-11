#!/usr/bin/env python3
"""Compare two runs of the performance report (tests/perf_report).

Reads two reports, each either the JSON file the report writes
(``target/perf-report.json``) or a captured log containing its
``perf-report: {...}`` lines, and prints the change of every metric per
scenario, including the per-step metrics of the inbound replica
(``steps.<step>.<metric>``).

Metrics a scenario lists under ``deterministic`` (allocation counts, bytes)
are exact for a given build; the others (times, RSS, end-to-end counts) are
noisy and marked ``~``. Every metric is "lower is better".

``--fail-on-regression PCT`` exits 1 when a deterministic metric grew by more
than PCT percent, so the comparison can gate a change; noisy metrics never
fail it.

[Decision Log]
- 목적과 의도: 4단계 전후(또는 임의의 두 커밋)의 성능 보고서를 손으로 대조하지 않고
  시나리오·지표별 증감률로 바로 본다. 결정적 지표의 회귀는 종료 코드로도 알린다.
- 기존 구현 및 제약 조건: 보고서는 tests/perf_report가 JSON(파일) 또는 접두사가 붙은 JSON
  줄(로그)로 낸다. ci/의 다른 도구처럼 Python 표준 라이브러리만 쓴다(CI는 3.11+).
- 검토한 주요 대안: (1) Rust 테스트 안에서 이전 보고서와 비교, (2) jq 한 줄,
  (3) 표준 라이브러리 Python 스크립트.
- 선택한 방식: (3). 두 형식을 모두 읽고, steps를 평탄화하고, 결정적/잡음 지표를 구분한다.
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 보고서를 저장소에 두거나 경로를 넘겨야 하고
  결과 형식을 바꾸기 어렵다. (2)는 평탄화·퍼센트·결측 처리를 읽기 힘든 식으로 만든다.
- 장점, 단점 및 영향: 의존성 없이 어디서나 돈다. 지표가 "낮을수록 좋다"는 전제를 둔다
  (보고서에 높을수록 좋은 지표가 생기면 여기서 따로 다뤄야 한다).
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import sys
from dataclasses import dataclass
from pathlib import Path

LINE_PREFIX = "perf-report: "

# Numeric fields that describe the scenario's input rather than measure it.
# A change there means the two runs measured different things.
PARAMETERS = frozenset(
    {
        "iterations",
        "ops",
        "payload_bytes",
        "body_bytes",
        "collectors",
        "members",
        "guilds",
        "members_per_guild",
        "cached_messages",
    }
)


@dataclass
class Report:
    meta: dict
    scenarios: dict[str, dict]


@dataclass
class Change:
    scenario: str
    metric: str
    before: float | None
    after: float | None
    deterministic: bool

    @property
    def percent(self) -> float | None:
        if self.before is None or self.after is None:
            return None
        if self.before == 0:
            return 0.0 if self.after == 0 else float("inf")
        return (self.after - self.before) / abs(self.before) * 100.0


def load(path: Path) -> Report:
    """Reads a report file or a log with prefixed report lines."""
    text = path.read_text(encoding="utf-8")
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        data = None
    if isinstance(data, dict) and "scenarios" in data:
        scenarios = data["scenarios"]
        meta = data.get("meta") or {}
    else:
        scenarios = []
        for line in text.splitlines():
            # libtest may print `test perf_report ... ` before the first line.
            _, sep, payload = line.partition(LINE_PREFIX)
            if sep:
                scenarios.append(json.loads(payload))
        meta = {}
        if not scenarios:
            raise ValueError(f"{path}: neither a report file nor a log with '{LINE_PREFIX}' lines")
    return Report(meta=meta, scenarios={s["scenario"]: s for s in scenarios})


def metrics(scenario: dict) -> dict[str, float]:
    """Numeric metrics of one scenario, with steps flattened."""
    out: dict[str, float] = {}
    for key, value in scenario.items():
        if key in PARAMETERS:
            continue
        if isinstance(value, bool):
            continue
        if isinstance(value, (int, float)):
            out[key] = value
        elif key == "steps" and isinstance(value, dict):
            for step, fields in value.items():
                for field, number in fields.items():
                    if isinstance(number, (int, float)) and not isinstance(number, bool):
                        out[f"steps.{step}.{field}"] = number
    return out


def is_deterministic(scenario: dict, metric: str) -> bool:
    listed = set(scenario.get("deterministic") or [])
    if metric.endswith("_ns"):
        return False
    base = metric.rsplit(".", 1)[-1] if metric.startswith("steps.") else metric
    return base in listed


def compare(before: Report, after: Report) -> tuple[list[Change], list[str], list[str], list[str]]:
    """Changes per metric, plus added/removed scenarios and parameter mismatches."""
    changes: list[Change] = []
    mismatches: list[str] = []
    for name in sorted(set(before.scenarios) & set(after.scenarios)):
        old, new = before.scenarios[name], after.scenarios[name]
        for key in sorted(PARAMETERS):
            if key in old and key in new and old[key] != new[key]:
                mismatches.append(f"{name}: {key} {old[key]} -> {new[key]}")
        old_metrics, new_metrics = metrics(old), metrics(new)
        for metric in sorted(set(old_metrics) | set(new_metrics)):
            changes.append(
                Change(
                    scenario=name,
                    metric=metric,
                    before=old_metrics.get(metric),
                    after=new_metrics.get(metric),
                    deterministic=is_deterministic(new, metric) or is_deterministic(old, metric),
                )
            )
    added = sorted(set(after.scenarios) - set(before.scenarios))
    removed = sorted(set(before.scenarios) - set(after.scenarios))
    return changes, added, removed, mismatches


def fmt_number(value: float | None) -> str:
    if value is None:
        return "-"
    if isinstance(value, float) and not value.is_integer():
        return f"{value:,.2f}"
    return f"{int(value):,}"


def fmt_percent(value: float | None) -> str:
    if value is None:
        return "n/a"
    if value == float("inf"):
        return "new"
    return f"{value:+.1f}%"


def render(changes: list[Change], markdown: bool) -> str:
    header = ("scenario", "metric", "before", "after", "change")
    rows = [
        (
            c.scenario,
            c.metric if c.deterministic else f"~{c.metric}",
            fmt_number(c.before),
            fmt_number(c.after),
            fmt_percent(c.percent),
        )
        for c in changes
    ]
    if markdown:
        lines = ["| " + " | ".join(header) + " |", "|---|---|---:|---:|---:|"]
        lines += ["| " + " | ".join(row) + " |" for row in rows]
        return "\n".join(lines)
    widths = [max(len(row[i]) for row in [header, *rows]) for i in range(len(header))]
    def line(row: tuple[str, ...]) -> str:
        cells = [row[0].ljust(widths[0]), row[1].ljust(widths[1])]
        cells += [cell.rjust(width) for cell, width in zip(row[2:], widths[2:])]
        return "  ".join(cells).rstrip()
    return "\n".join([line(header), *(line(row) for row in rows)])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("before", type=Path, help="baseline report (JSON file or log)")
    parser.add_argument("after", type=Path, help="report to compare (JSON file or log)")
    parser.add_argument("--scenario", action="append", default=[], metavar="GLOB",
                        help="only scenarios matching GLOB (repeatable), e.g. 'inbound.*'")
    parser.add_argument("--metric", action="append", default=[], metavar="GLOB",
                        help="only metrics matching GLOB (repeatable), e.g. 'allocs' or 'steps.*.allocs'")
    parser.add_argument("--min-change", type=float, default=0.0, metavar="PCT",
                        help="hide changes smaller than PCT percent (default: show every change)")
    parser.add_argument("--all", action="store_true", help="also list unchanged metrics")
    parser.add_argument("--deterministic-only", action="store_true",
                        help="hide noisy metrics (times, RSS, end-to-end counts)")
    parser.add_argument("--format", choices=("text", "markdown"), default="text")
    parser.add_argument("--fail-on-regression", type=float, default=None, metavar="PCT",
                        help="exit 1 if a deterministic metric grew by more than PCT percent")
    args = parser.parse_args(argv)

    try:
        before, after = load(args.before), load(args.after)
    except (OSError, ValueError) as error:
        print(f"perf_diff: {error}", file=sys.stderr)
        return 2

    for label, report in (("before", before), ("after", after)):
        meta = report.meta
        if meta:
            print(f"{label}: commit {meta.get('git_commit')}{' (dirty)' if meta.get('git_dirty') else ''}, "
                  f"profile {meta.get('profile')}, {meta.get('cpu')}")
            if meta.get("profile") == "debug":
                print(f"warning: the {label} report is a debug build; compare release reports",
                      file=sys.stderr)

    changes, added, removed, mismatches = compare(before, after)
    shown = [
        c for c in changes
        if (not args.scenario or any(fnmatch.fnmatch(c.scenario, g) for g in args.scenario))
        and (not args.metric or any(fnmatch.fnmatch(c.metric, g) for g in args.metric))
        and (not args.deterministic_only or c.deterministic)
        and (args.all or c.before != c.after)
        and (c.percent is None or abs(c.percent) >= args.min_change)
    ]
    if shown:
        print(render(shown, markdown=args.format == "markdown"))
    else:
        print("no metric changed" if not args.all else "no metrics selected")
    for name in added:
        print(f"added scenario: {name}")
    for name in removed:
        print(f"removed scenario: {name}")
    for mismatch in mismatches:
        print(f"warning: input changed, numbers are not comparable: {mismatch}", file=sys.stderr)

    if args.fail_on_regression is not None:
        regressions = [
            c for c in changes
            if c.deterministic and c.percent is not None and c.percent > args.fail_on_regression
        ]
        for c in regressions:
            print(f"regression: {c.scenario} {c.metric} {fmt_number(c.before)} -> "
                  f"{fmt_number(c.after)} ({fmt_percent(c.percent)})", file=sys.stderr)
        if regressions:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
