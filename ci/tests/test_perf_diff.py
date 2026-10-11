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

    def write_gates(self, checkpoints: dict) -> Path:
        path = self.dir / "gates.json"
        path.write_text(json.dumps({"schema": 1, "checkpoints": checkpoints}))
        return path

    def run_main(self, *argv: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = perf_diff.main(list(argv))
            except SystemExit as exit_:  # argparse errors
                code = exit_.code
        return code, out.getvalue(), err.getvalue()

    # -- reading and comparing ------------------------------------------------

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

    def test_flattens_steps_and_skips_inputs(self) -> None:
        metrics = perf_diff.metrics(scenario(
            "inbound.x", allocs=5, iterations=100, payload_bytes=900, ops=3, median_ns=1000,
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

    # -- regression decisions -------------------------------------------------

    def test_fail_on_regression_only_counts_deterministic_metrics(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=100, median_ns=10)])
        noisy = self.write_report("b.json", [scenario("inbound.x", allocs=100, median_ns=50)])
        worse = self.write_report("c.json", [scenario("inbound.x", allocs=120, median_ns=10)])
        code, out, _ = self.run_main(str(before), str(noisy), "--fail-on-regression", "5")
        self.assertEqual(code, 0)
        self.assertIn("~median_ns", out)
        code, _, err = self.run_main(str(before), str(worse), "--fail-on-regression", "5")
        self.assertEqual(code, 1)
        self.assertIn("FAIL: regression: inbound.x allocs 100 -> 120 (+20.0%)", err)

    def test_filters_decide_which_regressions_fail(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=100, peak_bytes=100),
                                              scenario("cache.y", allocs=100)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=100, peak_bytes=150),
                                             scenario("cache.y", allocs=200)])
        # The cache regression is outside the selection, so it cannot fail.
        code, _, err = self.run_main(str(before), str(after), "--fail-on-regression", "2",
                                     "--scenario", "inbound.*", "--metric", "allocs")
        self.assertEqual(code, 0, err)
        code, _, err = self.run_main(str(before), str(after), "--fail-on-regression", "2",
                                     "--scenario", "inbound.*")
        self.assertEqual(code, 1)
        self.assertIn("inbound.x peak_bytes", err)
        self.assertNotIn("cache.y", err)

    # -- strict ---------------------------------------------------------------

    def test_input_changes_warn_without_strict_and_fail_with_it(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=1, payload_bytes=10),
                                              scenario("old", allocs=1)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=1, payload_bytes=12),
                                             scenario("new", allocs=1)])
        code, out, err = self.run_main(str(before), str(after))
        self.assertEqual(code, 0)
        self.assertIn("added scenario: new", out)
        self.assertIn("warning: input changed: inbound.x: payload_bytes 10 -> 12", err)
        self.assertIn("warning: scenario missing from the new report: old", err)
        code, _, err = self.run_main(str(before), str(after), "--strict")
        self.assertEqual(code, 1)
        self.assertIn("FAIL: input changed: inbound.x: payload_bytes 10 -> 12", err)
        self.assertIn("FAIL: scenario missing from the new report: old", err)

    def test_strict_respects_the_scenario_filter(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=1),
                                              scenario("e2e.y", allocs=1)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=1)])
        code, _, err = self.run_main(str(before), str(after), "--strict", "--scenario", "inbound.*")
        self.assertEqual(code, 0, err)
        code, _, _ = self.run_main(str(before), str(after), "--strict")
        self.assertEqual(code, 1)

    def test_strict_fails_on_a_changed_fixture_digest(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=1)],
                                   meta={"profile": "release", "fixtures_digest": "aaaa"})
        after = self.write_report("b.json", [scenario("inbound.x", allocs=1)],
                                  meta={"profile": "release", "fixtures_digest": "bbbb"})
        code, _, err = self.run_main(str(before), str(after), "--strict")
        self.assertEqual(code, 1)
        self.assertIn("fixtures changed: digest aaaa -> bbbb", err)
        same = self.write_report("c.json", [scenario("inbound.x", allocs=1)],
                                 meta={"profile": "release", "fixtures_digest": "aaaa"})
        self.assertEqual(self.run_main(str(before), str(same), "--strict")[0], 0)

    # -- gates ----------------------------------------------------------------

    def gate_file(self) -> Path:
        return self.write_gates({
            "B": {"gates": [
                {"scenario": "inbound.x", "metric": "allocs", "max": 60},
                {"scenario": "layout.size_of", "metric": "event", "max": 192},
                {"scenario": "inbound.x", "metric": "steps.cache_apply.allocs", "max": 100},
            ]},
            "C": {"inherits": "B", "gates": [
                {"scenario": "inbound.x", "metric": "allocs", "max": 40},
            ]},
        })

    def test_checkpoints_inherit_and_override(self) -> None:
        gates = perf_diff.load_gates(self.gate_file(), "C")
        caps = {(g["scenario"], g["metric"]): g["max"] for g in gates}
        self.assertEqual(caps[("inbound.x", "allocs")], 40)
        self.assertEqual(caps[("layout.size_of", "event")], 192)
        self.assertEqual(len(gates), 3)
        with self.assertRaises(ValueError):
            perf_diff.load_gates(self.gate_file(), "D")

    def test_gates_pass_fail_and_missing(self) -> None:
        report = self.write_report("r.json", [
            scenario("inbound.x", allocs=50, steps={"cache_apply": {"allocs": 90}}),
            scenario("layout.size_of", event=4848),
        ])
        gates = self.gate_file()
        code, out, err = self.run_main("--gates", str(gates), "--checkpoint", "B", str(report))
        self.assertEqual(code, 1)
        self.assertIn("FAIL: gate B: layout.size_of event 4,848 > 192", err)
        self.assertRegex(out, r"inbound\.x\s+allocs\s+50\s+60\s+PASS")
        code, _, err = self.run_main("--gates", str(gates), "--checkpoint", "C", str(report))
        self.assertIn("FAIL: gate C: inbound.x allocs 50 > 40", err)
        # A gate whose metric the report does not have: warning, or failure under --strict.
        partial = self.write_report("p.json", [scenario("inbound.x", allocs=10),
                                               scenario("layout.size_of", event=100)])
        code, _, err = self.run_main("--gates", str(gates), "--checkpoint", "B", str(partial))
        self.assertEqual(code, 0, err)
        self.assertIn("warning: gate B: inbound.x steps.cache_apply.allocs is not in the report", err)
        code, _, err = self.run_main("--gates", str(gates), "--checkpoint", "B", "--strict",
                                     str(partial))
        self.assertEqual(code, 1)
        self.assertIn("FAIL: gate B: inbound.x steps.cache_apply.allocs is not in the report", err)

    def test_gates_follow_the_filters(self) -> None:
        report = self.write_report("r.json", [scenario("inbound.x", allocs=50),
                                              scenario("layout.size_of", event=4848)])
        code, out, err = self.run_main("--gates", str(self.gate_file()), "--checkpoint", "B",
                                       "--scenario", "inbound.*", "--metric", "allocs",
                                       str(report))
        self.assertEqual(code, 0, err)
        self.assertNotIn("layout.size_of", out)

    def test_gates_check_the_after_report_of_a_comparison(self) -> None:
        before = self.write_report("a.json", [scenario("inbound.x", allocs=500)])
        after = self.write_report("b.json", [scenario("inbound.x", allocs=55)])
        code, out, err = self.run_main(str(before), str(after), "--gates", str(self.gate_file()),
                                       "--checkpoint", "B", "--metric", "allocs")
        self.assertEqual(code, 0, err)
        self.assertIn("-89.0%", out)
        self.assertIn("PASS", out)

    def test_argument_errors(self) -> None:
        report = self.write_report("r.json", [scenario("inbound.x", allocs=1)])
        self.assertEqual(self.run_main(str(report))[0], 2)
        self.assertEqual(self.run_main(str(report), "--gates", str(self.gate_file()))[0], 2)
        self.assertEqual(self.run_main(str(report), str(report), str(report))[0], 2)

    def test_repository_gate_file_is_valid(self) -> None:
        path = CI_DIR / "perf-gates.json"
        for checkpoint in ("B", "C"):
            gates = perf_diff.load_gates(path, checkpoint)
            self.assertTrue(gates)
            for gate in gates:
                self.assertIsInstance(gate["max"], (int, float))
        # C tightens or keeps every cap of B.
        b = {(g["scenario"], g["metric"]): g["max"] for g in perf_diff.load_gates(path, "B")}
        c = {(g["scenario"], g["metric"]): g["max"] for g in perf_diff.load_gates(path, "C")}
        self.assertEqual(set(b), set(c))
        for key, cap in b.items():
            self.assertLessEqual(c[key], cap, key)


if __name__ == "__main__":
    unittest.main()
