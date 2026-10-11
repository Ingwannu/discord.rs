# ci/

Repository tooling that is not part of the published crate API.

| File | Purpose |
| --- | --- |
| `api_drift.py` | Compares discord-api-docs REST routes and Gateway receive events with the crate (report-only; Python 3.11+ standard library). |
| `api-drift-allowlist.toml` | Helper-built routes and deliberate mismatches, each with a mandatory reason, plus extractor settings. |
| `doc_examples.py` | Regenerates `src/model/doc_examples/payloads.rs`, Discord's documented example payloads used by the model round-trip tests (`--docs <discord-api-docs clone>`; standard library only). |
| `drift_issue.py` | Opens, updates, or closes the single `api-drift` issue from a report (used by `.github/workflows/api-drift.yml`). |
| `perf_diff.py` | Compares two runs of the performance report (`tests/perf_report`) per metric and scenario, and checks absolute caps: `--fail-on-regression PCT` for deterministic metrics, `--strict` for changed inputs and missing scenarios, `--gates perf-gates.json --checkpoint B|C` for the phase 4 gates (Python 3.11+ standard library). |
| `perf-gates.json` | The phase 4 gate table (checkpoints B and C) as absolute caps for `perf_diff.py --gates`. |
| `perf_summary.sh` | Markdown summary of a PR's report against its base and the committed baseline, plus gate status; used by `.github/workflows/perf-report.yml` (non-blocking). |
| `tests/` | `python3 -m unittest discover -s ci/tests -v`, with fixture MDX and Rust sources for the drift tools and synthetic reports for `perf_diff.py`. |
| `voice_transport_kat.py` | Prints the voice transport known-answer vectors pinned in `src/voice_runtime.rs` tests (AES-256-GCM and XChaCha20-Poly1305 rtpsize packets: plain, with CSRCs and a header extension, and with RTP padding). Needs the `cryptography` package; checks its HChaCha20 against draft-irtf-cfrg-xchacha-03 before printing. |

Usage, allowlist rules, CI behaviour, and the MSRV procedure are documented in
`discordrsdocs/docs/guide/testing-and-coverage.md`; the design rationale is the Decision Log at
the top of `api_drift.py`. The performance report, its baseline, and the hot spots it found are in
`docs/design/perf-baseline.md`.
