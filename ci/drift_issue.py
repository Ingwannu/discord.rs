#!/usr/bin/env python3
"""Open, update, or close the single `api-drift` issue from a drift report.

Used by .github/workflows/api-drift.yml after ci/api_drift.py. Talks to
GitHub through the `gh` CLI (preinstalled on GitHub-hosted runners, reads
GH_TOKEN / GH_REPO), so this file needs only the Python standard library.

Rules:

* Only issues whose body contains the report marker are managed. A human
  issue that merely carries the `api-drift` label is never edited or closed.
* At most one managed issue stays open: drift updates the oldest open one
  (or opens one); any newer managed duplicates are closed.
* The body is refreshed on every run (edits do not notify), but a comment is
  added only when the findings fingerprint changes, so an unchanged drift
  does not ping subscribers weekly.
* A clean report closes the managed issue with a comment; the next drift
  opens a fresh issue, so each drift episode has its own thread.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPORT_MARKER = "<!-- api-drift-report -->"
FINGERPRINT = re.compile(r"<!-- api-drift-fingerprint: ([0-9a-f]+) -->")
LABEL = "api-drift"
LABEL_COLOR = "D93F0B"
LABEL_DESCRIPTION = "Discord API docs and discordrs disagree (ci/api_drift.py)"


@dataclass
class Action:
    kind: str  # create | edit | comment | close
    number: int | None = None
    title: str | None = None
    body: str | None = None


def title_for(report: dict) -> str:
    parts = []
    if report["missing_routes"]:
        parts.append(f"{len(report['missing_routes'])} missing routes")
    if report["extra_routes"]:
        parts.append(f"{len(report['extra_routes'])} undocumented routes")
    events = len(report["missing_events"]) + len(report["extra_events"])
    if events:
        parts.append(f"{events} event changes")
    if report["maintenance"]:
        parts.append(f"{len(report['maintenance'])} allowlist fixes")
    return "Discord API drift: " + (", ".join(parts) or "findings")


def plan(report: dict, body: str, issues: list[dict]) -> list[Action]:
    """Decides what to do; pure so the policy is unit-testable offline."""

    managed = sorted(
        (issue for issue in issues if REPORT_MARKER in (issue.get("body") or "")),
        key=lambda issue: issue["number"],
    )
    actions: list[Action] = []
    primary, duplicates = (managed[0], managed[1:]) if managed else (None, [])
    for duplicate in duplicates:
        target = f"#{primary['number']}" if primary else "the newest report"
        actions.append(
            Action("close", duplicate["number"], body=f"Closing as a duplicate of {target}.")
        )

    if report["has_findings"]:
        if primary is None:
            actions.append(Action("create", title=title_for(report), body=body))
            return actions
        actions.append(Action("edit", primary["number"], title=title_for(report), body=body))
        previous = FINGERPRINT.search(primary.get("body") or "")
        if previous is None or previous.group(1) != report["fingerprint"]:
            actions.append(
                Action(
                    "comment",
                    primary["number"],
                    body=(
                        f"The drift changed (discord-api-docs `{report['docs_commit']}`, "
                        f"discordrs `{report['crate_commit']}`); the description now holds the "
                        "current report."
                    ),
                )
            )
        return actions

    if primary is not None:
        actions.append(
            Action(
                "close",
                primary["number"],
                body=(
                    f"No drift against discord-api-docs `{report['docs_commit']}` "
                    f"(discordrs `{report['crate_commit']}`). Closing; the next drift opens a new issue."
                ),
            )
        )
    return actions


def _gh(*args: str, capture: bool = False) -> str:
    result = subprocess.run(["gh", *args], check=True, text=True, capture_output=capture)
    return result.stdout if capture else ""


def _with_body_file(body: str, *args: str) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8") as handle:
        handle.write(body)
        path = handle.name
    try:
        _gh(*args, "--body-file", path)
    finally:
        Path(path).unlink(missing_ok=True)


def apply(actions: list[Action]) -> None:
    for action in actions:
        if action.kind == "create":
            labels = json.loads(_gh("label", "list", "--search", LABEL, "--json", "name", capture=True))
            if not any(label["name"] == LABEL for label in labels):
                _gh("label", "create", LABEL, "--color", LABEL_COLOR, "--description", LABEL_DESCRIPTION)
            _with_body_file(action.body or "", "issue", "create", "--title", action.title or "", "--label", LABEL)
        elif action.kind == "edit":
            _with_body_file(action.body or "", "issue", "edit", str(action.number), "--title", action.title or "")
        elif action.kind == "comment":
            _with_body_file(action.body or "", "issue", "comment", str(action.number))
        elif action.kind == "close":
            _gh("issue", "close", str(action.number), "--comment", action.body or "")
        else:  # pragma: no cover - plan() only emits the kinds above
            raise ValueError(action.kind)


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("report_json", type=Path, help="JSON report from ci/api_drift.py --json-output")
    parser.add_argument("report_markdown", type=Path, help="markdown report used as the issue body")
    parser.add_argument("--dry-run", action="store_true", help="print the planned actions instead of calling gh")
    parser.add_argument(
        "--issues-json",
        type=Path,
        help="read open issues from this file instead of `gh issue list` (offline dry runs)",
    )
    args = parser.parse_args(argv)

    report = json.loads(args.report_json.read_text(encoding="utf-8"))
    body = args.report_markdown.read_text(encoding="utf-8")
    if args.issues_json:
        issues = json.loads(args.issues_json.read_text(encoding="utf-8"))
    else:
        issues = json.loads(
            _gh(
                "issue", "list", "--label", LABEL, "--state", "open",
                "--json", "number,body", "--limit", "100",
                capture=True,
            )
        )
    actions = plan(report, body, issues)
    for action in actions:
        target = f"#{action.number}" if action.number else "new issue"
        print(f"{action.kind}: {target}" + (f" - {action.title}" if action.title else ""))
    if not actions:
        print("nothing to do")
    if not args.dry_run:
        apply(actions)
    return 0


if __name__ == "__main__":
    sys.exit(run())
