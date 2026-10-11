//! Report output: scenario JSON, the report file, run metadata, and RSS.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use crate::alloc::Sample;

pub(crate) const LINE_PREFIX: &str = "perf-report: ";
const REPORT_SCHEMA: u64 = 1;

/// Metrics that do not depend on timing or the machine for a given build:
/// exact for single-threaded scenarios.
pub(crate) const DETERMINISTIC: &[&str] =
    &["allocs", "alloc_bytes", "retained_bytes", "peak_bytes"];

pub(crate) struct Scenario(serde_json::Map<String, Value>);

impl Scenario {
    pub(crate) fn new(name: &str, group: &str) -> Self {
        let mut map = serde_json::Map::new();
        map.insert("scenario".into(), json!(name));
        map.insert("group".into(), json!(group));
        Self(map)
    }

    pub(crate) fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// One operation measured as a whole.
    pub(crate) fn sample(self, sample: Sample) -> Self {
        self.set("allocs", sample.allocs)
            .set("alloc_bytes", sample.alloc_bytes)
            .set("retained_bytes", sample.retained_bytes)
            .set("peak_bytes", sample.peak_bytes)
    }

    /// `ops` operations measured as a batch: allocations are per operation,
    /// retained and peak bytes are for the whole batch.
    pub(crate) fn batch(self, sample: Sample, ops: u64) -> Self {
        let per_op = |total: u64| (total as f64 / ops.max(1) as f64 * 100.0).round() / 100.0;
        self.set("ops", ops)
            .set("allocs", per_op(sample.allocs))
            .set("alloc_bytes", per_op(sample.alloc_bytes))
            .set("retained_bytes", sample.retained_bytes)
            .set("peak_bytes", sample.peak_bytes)
    }

    /// Per-step allocator samples, with the mean step time when a timing
    /// pass measured it.
    pub(crate) fn steps(
        self,
        steps: &[(&'static str, Sample)],
        mean_ns: &[(&'static str, u64)],
    ) -> Self {
        let steps: serde_json::Map<String, Value> = steps
            .iter()
            .map(|(name, sample)| {
                let mut step = json!({
                    "allocs": sample.allocs,
                    "alloc_bytes": sample.alloc_bytes,
                    "retained_bytes": sample.retained_bytes,
                    "peak_bytes": sample.peak_bytes,
                });
                if let Some((_, ns)) = mean_ns.iter().find(|(step, _)| step == name) {
                    step["mean_ns"] = json!(ns);
                }
                ((*name).to_string(), step)
            })
            .collect();
        self.set("steps", Value::Object(steps))
    }

    pub(crate) fn timings(self, mut samples: Vec<Duration>) -> Self {
        samples.sort_unstable();
        let nanos = |d: Duration| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
        let median = samples.get(samples.len() / 2).copied().unwrap_or_default();
        let min = samples.first().copied().unwrap_or_default();
        self.set("iterations", samples.len())
            .set("median_ns", nanos(median))
            .set("min_ns", nanos(min))
    }

    pub(crate) fn deterministic(self, metrics: &[&str]) -> Self {
        self.set("deterministic", json!(metrics))
    }

    pub(crate) fn note(self, note: &str) -> Self {
        self.set("note", note)
    }
}

#[derive(Default)]
pub(crate) struct Report {
    scenarios: Vec<Value>,
}

impl Report {
    pub(crate) fn push(&mut self, scenario: Scenario) {
        self.push_value(Value::Object(scenario.0));
    }

    pub(crate) fn push_value(&mut self, value: Value) {
        println!("{LINE_PREFIX}{value}");
        self.scenarios.push(value);
    }

    pub(crate) fn write(self, meta: Value) -> PathBuf {
        let path = std::env::var_os("DISCORDRS_PERF_REPORT")
            .map(PathBuf::from)
            .unwrap_or_else(default_report_path);
        let report = json!({
            "schema": REPORT_SCHEMA,
            "meta": meta,
            "scenarios": self.scenarios,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }
}

/// `target/perf-report.json`: `CARGO_TARGET_TMPDIR` is `<target dir>/tmp`.
fn default_report_path() -> PathBuf {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    tmp.parent()
        .map(|target| target.join("perf-report.json"))
        .unwrap_or_else(|| tmp.join("perf-report.json"))
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn meta() -> Value {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(':').nth(1))
                .map(|name| name.trim().to_string())
        });
    let dirty = command_output("git", &["status", "--porcelain", "--untracked-files=no"])
        .map(|status| !status.is_empty());
    json!({
        "crate_version": env!("CARGO_PKG_VERSION"),
        "git_commit": command_output("git", &["rev-parse", "--short", "HEAD"]),
        "git_dirty": dirty,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpu": cpu,
        "logical_cpus": std::thread::available_parallelism().map(usize::from).ok(),
        "rustc": command_output("rustc", &["--version"]),
        "features": "gateway,cache,collectors (+ whatever else the build enabled)",
        "deterministic_metrics": DETERMINISTIC,
    })
}

/// (VmRSS, VmHWM) in KiB from /proc/self/status; `None` off Linux.
pub(crate) fn rss_kib() -> Option<(u64, u64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        status
            .lines()
            .find(|line| line.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
    };
    Some((field("VmRSS:")?, field("VmHWM:")?))
}

pub(crate) fn current_thread_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
