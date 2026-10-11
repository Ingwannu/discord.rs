#!/usr/bin/env bash
# Writes the performance comparison of a PR as markdown: to
# $GITHUB_STEP_SUMMARY in GitHub Actions, otherwise to stdout.
#
#   ci/perf_summary.sh HEAD_JSON [BASE_JSON] [BASELINE_JSON]
#
# HEAD_JSON / BASE_JSON: reports of the PR and of its base, built and run
# on the same runner with the same toolchain (the only pair whose
# deterministic metrics are comparable to the byte). BASELINE_JSON: the
# committed baseline (docs/design/perf/*.json), recorded on another machine
# and possibly another toolchain, shown for orientation only.
#
# Never fails because of numbers (the job is non-blocking): a deterministic
# regression over 2% against the base becomes a warning annotation. Exits 2
# only if HEAD_JSON is missing, i.e. the report itself did not run.

set -u

head_report=${1:?usage: ci/perf_summary.sh HEAD_JSON [BASE_JSON] [BASELINE_JSON]}
base_report=${2:-}
baseline_report=${3:-}
ci_dir=$(cd "$(dirname "$0")" && pwd)
diff_py="$ci_dir/perf_diff.py"
gates="$ci_dir/perf-gates.json"
summary=${GITHUB_STEP_SUMMARY:-/dev/stdout}

annotate() {
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
    echo "::warning title=perf report::$1"
  else
    echo "warning: $1" >&2
  fi
}

if [ ! -f "$head_report" ]; then
  echo "perf_summary: no report at $head_report; the perf report did not run" >&2
  exit 2
fi

{
  echo "## Performance report"
  echo
  echo "Metrics marked \`~\` (times, RSS, end-to-end counts) are noisy and never fail anything."
  echo "Thresholds and the gating decision: \`docs/design/perf-baseline.md\`."
  echo

  if [ -n "$base_report" ] && [ -f "$base_report" ]; then
    echo "### This PR against its base (same runner, same toolchain)"
    echo
    out=$(python3 "$diff_py" "$base_report" "$head_report" --deterministic-only --strict \
      --fail-on-regression 2 --format markdown 2>&1)
    status=$?
    if [ "$status" -eq 0 ]; then
      echo "No deterministic metric grew by more than 2%, and the inputs match."
    else
      echo "**A deterministic metric grew by more than 2%, or the inputs differ** (details below)."
      annotate "deterministic perf regression over 2% or changed inputs against the base; see the job summary"
    fi
    echo
    problems=$(echo "$out" | grep -E '^(FAIL|warning):' || true)
    if [ -n "$problems" ]; then
      echo '```text'
      echo "$problems"
      echo '```'
      echo
    fi
    echo "$out" | grep -vE '^(FAIL|warning):'
    echo
    echo "<details><summary>All metrics, including noisy ones</summary>"
    echo
    python3 "$diff_py" "$base_report" "$head_report" --format markdown 2>&1
    echo
    echo "</details>"
    echo
  else
    echo "_No base report (push event, or the base predates tests/perf_report)._"
    echo
  fi

  if [ -n "$baseline_report" ] && [ -f "$baseline_report" ]; then
    echo "<details><summary>Against the committed baseline $(basename "$baseline_report") (other machine; orientation only)</summary>"
    echo
    python3 "$diff_py" "$baseline_report" "$head_report" --deterministic-only --format markdown 2>&1
    echo
    echo "</details>"
    echo
  fi

  for checkpoint in B C; do
    out=$(python3 "$diff_py" --gates "$gates" --checkpoint "$checkpoint" --format markdown \
      "$head_report" 2>&1)
    passed=$(echo "$out" | grep -c '| PASS |' || true)
    total=$(echo "$out" | grep -cE '\| (PASS|FAIL|MISSING) \|' || true)
    echo "<details><summary>Phase 4 checkpoint $checkpoint gates: $passed of $total met</summary>"
    echo
    echo "$out" | grep -vE '^(FAIL|warning):'
    echo
    echo "</details>"
    echo
  done
} >>"$summary"
