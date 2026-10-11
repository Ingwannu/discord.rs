"""Tests for ci/perf_diff.py.

Run with: python3 -m unittest discover -s ci/tests -v
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

CI_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(CI_DIR))

import perf_diff  # noqa: E402


def scenario(name: str, **fields) -> dict:
    base = {"scenario": name, "group": name.split(".", 1)[0],
            "deterministic": ["allocs", "alloc_bytes", "retained_bytes", "peak_bytes"]}
    base.update(fields)
    return base


class PerfDiffTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def write_report(self, name: str, scenarios: list[dict], meta: dict | None = None) -> Path:
        path = self.dir / name
        path.write_text(json.dumps({"schema": 1, "meta": meta or {"profile": "release"},
                                    "scenarios": scenarios}))
        return path

    def run_main(self, *argv: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = perf_diff.main(list(argv))
        return code, out.getvalue(), err.getvalue()

    def test_reads_report_file_and_prefixed_log(self) -> None:
        line = json.dumps(scenario("inbound.x", allocs=10))
        log = self.dir / "run.log"
        log.write_text(f"running 1 test\ntest perf_report ... perf-report: {line}\nok\n")
        self.assertEqual(perf_diff.load(log).scenarios["inbound.x"]["allocs"], 10)
        report = self.write_report("r.json", [scenario("inbound.x", allocs=11)])
        self.assertEqual(perf_diff.load(report).scenarios["inbound.x"]["allocs"], 11)

    def test_rejects_unrelated_input(self) -> None:
        junk = self.dir / "junk.txt"
        junk.write_text("nothing here\n")
        with self.assertRaises(ValueError):
            perf_diff.load(junk)

    def test_flattens_steps_and_skips_parameters(self) -> None:
        metrics = perf_diff.metrics(scenario(
            "inbound.x", allocs=5, iterations=100, payload_bytes=900, median_ns=1000,
            steps={"decode_event": {"allocs": 3, "mean_ns": 40}}))
        self.assertEqual(metrics, {"allocs": 5, "median_ns": 1000,
                                   "steps.decode_event.allocs": 3,
                                   "steps.decode_event.mean_ns": 40})

    def test_percent_change_and_determinism(self) -> None:
        before = self.write_report("a.json", [scenario(
            "inbound.x", allocs=200, median_ns=1000, steps={"parse": {"allocs": 100}})])
        after = self.write_report("b.json", [scenario(
            "inbound.x", allocs=100, median_ns=1500, steps={"parse": {"allocs": 0}})])
        changes, added, removed, mismatches = perf_diff.compare(
            perf_diff.load(before), perf_diff.load(after))
        by_metric = {c.metric: c for c in changes}
        self.assertAlmostEqual(by_metric["allocs"].percent, -50.0)
        self.assertAlmostEqual(by_metric["median_ns"].percent, 50.0)
        self.assertAlmostEqual(by_metric["steps.parse.allocs"].percent, -100.0)
        self.assertTrue(by_metric["allocs"].deterministic)
        self.assertTrue(by_metric["steps.parse.allocs"].deterministic)
        self.assertFalse(by_metric["median_ns"].deterministic)
        self.assertEqual((added, removed, mismatches), ([], [], []))

    def test_zero_baseline_is_reported_as_new(self) -> None:
        change = perf_diff.Change("s", "allocs", 0, 3, True)
        self.assertEqual(perf_diff.fmt_percent(change.percent), "new")
        self.assertEqual(perf_diff.Change("s", "allocs", 0, 0, True).percent, 0.0)

    def test_fail_on_regression_only_counts_deterministic_metrics(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=100, median_ns=10)])
        noisy = self.write_report("b.json", [scenario("inbound.x", allocs=100, median_ns=50)])
        worse = self.write_report("c.json", [scenario("inbound.x", allocs=120, median_ns=10)])
        code, out, _ = self.run_main(str(before), str(noisy), "--fail-on-regression", "5")
        self.assertEqual(code, 0)
        self.assertIn("~median_ns", out)
        code, _, err = self.run_main(str(before), str(worse), "--fail-on-regression", "5")
        self.assertEqual(code, 1)
        self.assertIn("regression: inbound.x allocs 100 -> 120 (+20.0%)", err)

    def test_reports_added_removed_and_changed_inputs(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=1, payload_bytes=10),
                                              scenario("old", allocs=1)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=1, payload_bytes=12),
                                             scenario("new", allocs=1)])
        code, out, err = self.run_main(str(before), str(after))
        self.assertEqual(code, 0)
        self.assertIn("added scenario: new", out)
        self.assertIn("removed scenario: old", out)
        self.assertIn("payload_bytes 10 -> 12", err)

    def test_filters_and_markdown(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=10, peak_bytes=5),
                                              scenario("cache.y", allocs=10)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=20, peak_bytes=6),
                                             scenario("cache.y", allocs=30)])
        code, out, _ = self.run_main(str(before), str(after), "--scenario", "inbound.*",
                                     "--metric", "allocs", "--format", "markdown")
        self.assertEqual(code, 0)
        self.assertIn("| inbound.x | allocs | 10 | 20 | +100.0% |", out)
        self.assertNotIn("cache.y", out)
        self.assertNotIn("peak_bytes", out)

    def test_bad_path_exits_2(self) -> None:
        code, _, err = self.run_main(str(self.dir / "missing.json"), str(self.dir / "missing.json"))
        self.assertEqual(code, 2)
        self.assertIn("perf_diff:", err)


if __name__ == "__main__":
    unittest.main()
