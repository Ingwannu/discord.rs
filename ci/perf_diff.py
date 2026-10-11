#!/usr/bin/env python3
"""Compare runs of the performance report (tests/perf_report) and check gates.

Reads reports that are either the JSON file the report writes
(``target/perf-report.json``) or a captured log containing its
``perf-report: {...}`` lines.

* ``perf_diff.py BEFORE AFTER`` prints the change of every metric per
  scenario, including the per-step metrics (``steps.<step>.<metric>``).
  Metrics a scenario lists under ``deterministic`` (allocation counts,
  bytes) are exact for a given build and toolchain; the others (times, RSS,
  end-to-end counts) are noisy and marked ``~``. Every metric is "lower is
  better".
* ``--fail-on-regression PCT`` exits 1 when a selected deterministic metric
  grew by more than PCT percent. ``--scenario`` and ``--metric`` select what
  is printed *and* what can fail.
* ``--strict`` turns input problems into failures: a scenario whose inputs
  changed (``payload_bytes``, ``ops``, ...), a different fixture digest, a
  selected baseline scenario missing from AFTER, or a gate metric missing
  from the report. Without it they are warnings.
* ``--gates FILE --checkpoint NAME`` checks absolute caps (``ci/perf-gates.json``
  encodes the phase 4 gate table) against AFTER, or against the only report
  when one is given: ``perf_diff.py --gates ci/perf-gates.json --checkpoint B
  report.json``.

Exit codes: 0 nothing failed, 1 a regression, gate, or strict check failed,
2 bad arguments or unreadable input.

[Decision Log]
- 목적과 의도: 4단계 전후(또는 임의의 두 커밋)의 성능 보고서를 손으로 대조하지 않고
  시나리오·지표별 증감률로 보고, 결정적 지표의 회귀와 4단계 게이트(B·C 상한)를 종료 코드로
  판정한다.
- 기존 구현 및 제약 조건: 보고서는 tests/perf_report가 JSON(파일) 또는 접두사가 붙은 JSON
  줄(로그)로 낸다. ci/의 다른 도구처럼 Python 표준 라이브러리만 쓴다(CI는 3.11+). 첫 판은
  필터가 출력에만 적용되고 실패 판정은 모든 지표를 봤으며, 입력 불일치와 사라진 시나리오는
  경고뿐이었다(astra 리뷰).
- 검토한 주요 대안: (1) Rust 테스트 안에서 이전 보고서·상한과 비교, (2) jq, (3) 표준 라이브러리
  Python 스크립트 + 상한 파일(TOML 또는 JSON).
- 선택한 방식: (3). 필터는 출력과 판정에 같이 적용하고, --strict에서 입력 불일치·누락을 실패로
  바꾸며, 상한은 ci/perf-gates.json(체크포인트별, C가 B를 상속)으로 둔다.
- 다른 대안 대신 이 방식을 선택한 이유: (1)은 보고서·상한을 테스트에 묶어 기준선을 바꿀 때마다
  Rust 코드를 고쳐야 한다. (2)는 평탄화·퍼센트·결측 처리를 읽기 힘든 식으로 만든다. 상한 파일을
  JSON으로 둔 것은 보고서와 같은 형식이고 3.11의 tomllib 없이도 쓸 수 있어서다.
- 장점, 단점 및 영향: 의존성 없이 어디서나 돈다. 지표가 "낮을수록 좋다"는 전제를 둔다. 상한이
  지표 이름(예: steps.cache_apply.allocs)에 묶이므로 단계 이름을 바꾸면 상한 파일도 함께 고쳐야
  한다(--strict가 누락으로 알려 준다).
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

LINE_PREFIX = "perf-report: "

# Numeric fields that describe a scenario's input. If one differs between
# two reports, the two runs measured different things.
INPUT_PARAMETERS = frozenset(
    {
        "payload_bytes",
        "body_bytes",
        "collectors",
        "members",
        "guilds",
        "members_per_guild",
        "ops",
    }
)
# Numeric fields that are neither inputs nor "lower is better" metrics.
INFORMATIONAL = frozenset({"iterations", "cached_messages"})
NON_METRICS = INPUT_PARAMETERS | INFORMATIONAL


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


@dataclass
class Selection:
    scenarios: list[str] = field(default_factory=list)
    metrics: list[str] = field(default_factory=list)

    def scenario(self, name: str) -> bool:
        return not self.scenarios or any(fnmatch.fnmatch(name, g) for g in self.scenarios)

    def metric(self, name: str) -> bool:
        return not self.metrics or any(fnmatch.fnmatch(name, g) for g in self.metrics)

    def __call__(self, scenario: str, metric: str) -> bool:
        return self.scenario(scenario) and self.metric(metric)


@dataclass
class GateResult:
    scenario: str
    metric: str
    cap: float
    value: float | None
    note: str = ""

    @property
    def status(self) -> str:
        if self.value is None:
            return "MISSING"
        return "PASS" if self.value <= self.cap else "FAIL"


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
        if key in NON_METRICS or isinstance(value, bool):
            continue
        if isinstance(value, (int, float)):
            out[key] = value
        elif key == "steps" and isinstance(value, dict):
            for step, fields in value.items():
                for name, number in fields.items():
                    if isinstance(number, (int, float)) and not isinstance(number, bool):
                        out[f"steps.{step}.{name}"] = number
    return out


def is_deterministic(scenario: dict, metric: str) -> bool:
    listed = set(scenario.get("deterministic") or [])
    if metric.endswith("_ns"):
        return False
    base = metric.rsplit(".", 1)[-1] if metric.startswith("steps.") else metric
    return base in listed


def compare(before: Report, after: Report) -> tuple[list[Change], list[str], list[str], list[str]]:
    """Changes per metric, plus added/removed scenarios and input mismatches."""
    changes: list[Change] = []
    mismatches: list[str] = []
    for name in sorted(set(before.scenarios) & set(after.scenarios)):
        old, new = before.scenarios[name], after.scenarios[name]
        for key in sorted(INPUT_PARAMETERS):
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


def regressions(changes: list[Change], threshold: float, select: Selection) -> list[Change]:
    """Selected deterministic metrics that grew by more than `threshold` percent."""
    return [
        c for c in changes
        if c.deterministic and select(c.scenario, c.metric)
        and c.percent is not None and c.percent > threshold
    ]


def input_problems(before: Report, after: Report, mismatches: list[str], removed: list[str],
                   select: Selection) -> list[str]:
    """Reasons the two reports are not comparable (failures under --strict)."""
    problems = [f"input changed: {m}" for m in mismatches if select.scenario(m.split(":", 1)[0])]
    problems += [f"scenario missing from the new report: {name}"
                 for name in removed if select.scenario(name)]
    old_digest = before.meta.get("fixtures_digest")
    new_digest = after.meta.get("fixtures_digest")
    if old_digest and new_digest and old_digest != new_digest:
        problems.append(f"fixtures changed: digest {old_digest} -> {new_digest}")
    return problems


def load_gates(path: Path, checkpoint: str) -> list[dict]:
    """The gates of `checkpoint`, with inherited checkpoints resolved."""
    data = json.loads(path.read_text(encoding="utf-8"))
    checkpoints = data.get("checkpoints") or {}
    chain: list[str] = []
    name: str | None = checkpoint
    while name is not None:
        if name in chain:
            raise ValueError(f"{path}: checkpoint inheritance loops through {name}")
        if name not in checkpoints:
            raise ValueError(f"{path}: no checkpoint {name!r} (have {', '.join(sorted(checkpoints))})")
        chain.append(name)
        name = checkpoints[name].get("inherits")
    resolved: dict[tuple[str, str], dict] = {}
    for name in reversed(chain):  # parents first, children override
        for gate in checkpoints[name].get("gates") or []:
            if not {"scenario", "metric", "max"} <= set(gate):
                raise ValueError(f"{path}: gate needs scenario, metric, and max: {gate}")
            resolved[(gate["scenario"], gate["metric"])] = gate
    return list(resolved.values())


def evaluate_gates(report: Report, gates: list[dict], select: Selection) -> list[GateResult]:
    results = []
    for gate in gates:
        if not select(gate["scenario"], gate["metric"]):
            continue
        scenario = report.scenarios.get(gate["scenario"])
        value = metrics(scenario).get(gate["metric"]) if scenario is not None else None
        results.append(GateResult(gate["scenario"], gate["metric"], gate["max"], value,
                                  gate.get("note", "")))
    return results


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


def table(header: tuple[str, ...], rows: list[tuple[str, ...]], markdown: bool,
          left: int = 2) -> str:
    """Rows as a markdown table or aligned text; the first `left` columns are left-aligned."""
    if markdown:
        align = ["---"] * left + ["---:"] * (len(header) - left)
        lines = ["| " + " | ".join(header) + " |", "|" + "|".join(align) + "|"]
        lines += ["| " + " | ".join(row) + " |" for row in rows]
        return "\n".join(lines)
    widths = [max(len(row[i]) for row in [header, *rows]) for i in range(len(header))]

    def line(row: tuple[str, ...]) -> str:
        cells = [cell.ljust(width) for cell, width in zip(row[:left], widths[:left])]
        cells += [cell.rjust(width) for cell, width in zip(row[left:], widths[left:])]
        return "  ".join(cells).rstrip()

    return "\n".join([line(header), *(line(row) for row in rows)])


def render_changes(changes: list[Change], markdown: bool) -> str:
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
    return table(("scenario", "metric", "before", "after", "change"), rows, markdown)


def render_gates(results: list[GateResult], markdown: bool) -> str:
    rows = [(r.scenario, r.metric, fmt_number(r.value), fmt_number(r.cap), r.status)
            for r in results]
    return table(("scenario", "metric", "value", "cap", "status"), rows, markdown, left=2)


def print_meta(label: str, report: Report) -> None:
    meta = report.meta
    if not meta:
        return
    print(f"{label}: commit {meta.get('git_commit')}{' (dirty)' if meta.get('git_dirty') else ''}, "
          f"profile {meta.get('profile')}, {meta.get('rustc')}, {meta.get('cpu')}")
    if meta.get("profile") == "debug":
        print(f"warning: the {label} report is a debug build; compare release reports",
              file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("reports", type=Path, nargs="+", metavar="REPORT",
                        help="BEFORE AFTER to compare, or one report to check against --gates")
    parser.add_argument("--scenario", action="append", default=[], metavar="GLOB",
                        help="only scenarios matching GLOB (repeatable); applies to output and failures")
    parser.add_argument("--metric", action="append", default=[], metavar="GLOB",
                        help="only metrics matching GLOB (repeatable), e.g. 'allocs' or 'steps.*.allocs'")
    parser.add_argument("--min-change", type=float, default=0.0, metavar="PCT",
                        help="hide changes smaller than PCT percent (output only)")
    parser.add_argument("--all", action="store_true", help="also list unchanged metrics")
    parser.add_argument("--deterministic-only", action="store_true",
                        help="hide noisy metrics (times, RSS, end-to-end counts)")
    parser.add_argument("--format", choices=("text", "markdown"), default="text")
    parser.add_argument("--fail-on-regression", type=float, default=None, metavar="PCT",
                        help="exit 1 if a selected deterministic metric grew by more than PCT percent")
    parser.add_argument("--strict", action="store_true",
                        help="fail on changed inputs or fixtures and on missing scenarios or gate metrics")
    parser.add_argument("--gates", type=Path, default=None, metavar="FILE",
                        help="absolute caps to check, e.g. ci/perf-gates.json")
    parser.add_argument("--checkpoint", default=None, metavar="NAME",
                        help="which checkpoint of --gates to check (e.g. B or C)")
    args = parser.parse_args(argv)

    if len(args.reports) > 2:
        parser.error("give BEFORE AFTER, or one report with --gates")
    if len(args.reports) == 1 and args.gates is None:
        parser.error("a single report needs --gates (and --checkpoint)")
    if (args.gates is None) != (args.checkpoint is None):
        parser.error("--gates and --checkpoint go together")
    markdown = args.format == "markdown"
    select = Selection(args.scenario, args.metric)

    try:
        reports = [load(path) for path in args.reports]
        gates = load_gates(args.gates, args.checkpoint) if args.gates else None
    except (OSError, ValueError) as error:
        print(f"perf_diff: {error}", file=sys.stderr)
        return 2

    failures: list[str] = []
    warnings: list[str] = []

    if len(reports) == 2:
        before, after = reports
        print_meta("before", before)
        print_meta("after", after)
        changes, added, removed, mismatches = compare(before, after)
        shown = [
            c for c in changes
            if select(c.scenario, c.metric)
            and (not args.deterministic_only or c.deterministic)
            and (args.all or c.before != c.after)
            and (c.percent is None or abs(c.percent) >= args.min_change)
        ]
        if shown:
            print(render_changes(shown, markdown))
        else:
            print("no metric changed" if not args.all else "no metrics selected")
        for name in added:
            if select.scenario(name):
                print(f"added scenario: {name}")
        problems = input_problems(before, after, mismatches, removed, select)
        (failures if args.strict else warnings).extend(problems)
        if args.fail_on_regression is not None:
            failures += [
                f"regression: {c.scenario} {c.metric} {fmt_number(c.before)} -> "
                f"{fmt_number(c.after)} ({fmt_percent(c.percent)})"
                for c in regressions(changes, args.fail_on_regression, select)
            ]
    else:
        after = reports[0]
        print_meta("report", after)

    if gates is not None:
        results = evaluate_gates(after, gates, select)
        print()
        print(f"gates: checkpoint {args.checkpoint} ({args.gates})")
        print(render_gates(results, markdown) if results else "no gates selected")
        for r in results:
            if r.status == "FAIL":
                failures.append(f"gate {args.checkpoint}: {r.scenario} {r.metric} "
                                f"{fmt_number(r.value)} > {fmt_number(r.cap)}")
            elif r.status == "MISSING":
                message = f"gate {args.checkpoint}: {r.scenario} {r.metric} is not in the report"
                (failures if args.strict else warnings).append(message)

    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)
    for failure in failures:
        print(f"FAIL: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
