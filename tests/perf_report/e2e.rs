//! End-to-end scenarios: the real `Client` in its own process against a fake
//! gateway in another (both re-executions of this test binary).

use std::hint::black_box;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use discordrs::{
    decode_event, ApplicationId, Client, CollectorHub, Context, Event, EventHandler,
    GatewayConnectionConfig, Intents, MessageId,
};
use serde_json::{json, Value};

use crate::alloc::{Mark, Sample, Uncounted};
use crate::fixtures::{
    fixture, frame, inbound_cases, synthesized_guild_create, LARGE_GUILD, MESSAGE_CREATE_PLAIN,
    STORM_GUILDS,
};
use crate::inbound::HUB_JOURNAL_CAPACITY;
use crate::report::{rss_kib, Report, Scenario, LINE_PREFIX};

/// Set for the helper processes this binary re-executes: the end-to-end
/// bot and the fake gateway it connects to.
const E2E_CHILD_ENV: &str = "DISCORDRS_PERF_E2E_CHILD";
const GATEWAY_CHILD_ENV: &str = "DISCORDRS_PERF_GATEWAY_CHILD";
const CHILD_PORT_PREFIX: &str = "perf-gateway-port=";

/// Re-executes this test binary to run one ignored helper test.
fn spawn_helper(test: &str, env: &str) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            test,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(env, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}

/// Frames the child can send, by command name.
fn child_frames() -> Vec<(String, &'static str, String)> {
    let mut frames: Vec<(String, &'static str, String)> = inbound_cases()
        .into_iter()
        .map(|case| (case.name.to_string(), case.event, case.d.to_string()))
        .collect();
    for index in 1..=STORM_GUILDS {
        frames.push((
            format!("storm.{index}"),
            "GUILD_CREATE",
            synthesized_guild_create(index, &LARGE_GUILD).to_string(),
        ));
    }
    frames
}

/// Helper process for `perf_report`: a fake Discord gateway on 127.0.0.1
/// that sends HELLO and READY, answers heartbeats, and sends one dispatch
/// per command line read from stdin.
#[test]
#[ignore = "helper process spawned by perf_report; does nothing on its own"]
fn perf_fake_gateway_child() {
    if std::env::var_os(GATEWAY_CHILD_ENV).is_none() {
        return;
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(fake_gateway());
}

async fn fake_gateway() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let frames = child_frames();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!(
        "{CHILD_PORT_PREFIX}{}",
        listener.local_addr().unwrap().port()
    );
    std::io::stdout().flush().unwrap();

    let (commands_tx, mut commands) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if commands_tx.send(line).is_err() {
                break;
            }
        }
    });

    let (stream, _) = listener.accept().await.unwrap();
    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
    // A one-hour interval keeps heartbeats out of the measurements.
    let hello = json!({"op": 10, "d": {"heartbeat_interval": 3_600_000}});
    socket
        .send(WsMessage::Text(hello.to_string().into()))
        .await
        .unwrap();
    loop {
        match socket.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                let payload: Value = serde_json::from_str(&text).unwrap();
                if payload["op"] == json!(2) {
                    break;
                }
            }
            Some(Ok(_)) => {}
            _ => return,
        }
    }
    let ready = json!({
        "v": 10,
        "user": {"id": "1290011223344556677", "username": "raid-planner", "discriminator": "4821", "bot": true},
        "session_id": "0123456789abcdef0123456789abcdef",
        "resume_gateway_url": "ws://127.0.0.1:1/",
        "guilds": [],
        "application": {"id": "1290011223344556677", "flags": 0}
    });
    let mut sequence = 1_u64;
    socket
        .send(WsMessage::Text(frame("READY", sequence, &ready).into()))
        .await
        .unwrap();

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                if command == "quit" {
                    break;
                }
                let (_, event, d) = frames
                    .iter()
                    .find(|(name, _, _)| *name == command)
                    .unwrap_or_else(|| panic!("unknown command {command}"));
                sequence += 1;
                let text = format!(r#"{{"t":"{event}","s":{sequence},"op":0,"d":{d}}}"#);
                socket.send(WsMessage::Text(text.into())).await.unwrap();
            }
            message = socket.next() => match message {
                Some(Ok(WsMessage::Text(text))) => {
                    let payload: Value = serde_json::from_str(&text).unwrap();
                    if payload["op"] == json!(1) {
                        let ack = json!({"op": 11});
                        socket.send(WsMessage::Text(ack.to_string().into())).await.unwrap();
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

/// The parent's handle on the child gateway.
struct ChildGateway {
    child: Child,
    stdin: ChildStdin,
    port: u16,
}

impl ChildGateway {
    pub(crate) fn spawn() -> Self {
        let mut child = spawn_helper("e2e::perf_fake_gateway_child", GATEWAY_CHILD_ENV);
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let port = loop {
            let mut line = String::new();
            assert!(
                stdout.read_line(&mut line).unwrap() > 0,
                "fake gateway exited before reporting its port"
            );
            // libtest prints `test perf_fake_gateway_child ... ` on the same line.
            if let Some((_, port)) = line.trim().split_once(CHILD_PORT_PREFIX) {
                break port.parse().unwrap();
            }
        };
        // Keep draining the child's stdout so it can never block on a full pipe.
        std::thread::spawn(move || {
            let mut sink = String::new();
            while stdout.read_line(&mut sink).unwrap_or(0) > 0 {
                sink.clear();
            }
        });
        Self { child, stdin, port }
    }

    /// Asks the child to send one dispatch. Unbuffered pipe writes of a
    /// borrowed `&str`: no allocation in this process.
    pub(crate) fn send(&mut self, command: &str) {
        self.stdin.write_all(command.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
    }
}

impl Drop for ChildGateway {
    fn drop(&mut self) {
        // Best effort: the child exits on "quit"; kill it if it does not
        // (for example when the client never connected).
        self.stdin.write_all(b"quit\n").ok();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// Counts every event the handler receives and keeps the collector hub.
#[derive(Clone)]
struct ProbeHandler {
    seen: Arc<AtomicUsize>,
    notify: Arc<tokio::sync::Notify>,
    // Written once by the READY handler call, read by the report thread.
    hub: Arc<Mutex<Option<CollectorHub>>>,
}

impl EventHandler for ProbeHandler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        if matches!(event, Event::Ready(_)) {
            *self.hub.lock().unwrap() = Some(ctx.collectors().clone());
        }
        drop(black_box(event));
        drop(ctx);
        self.seen.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_one();
    }
}

/// Longest wait for one dispatch before the report fails instead of
/// hanging (the pipeline drops handler deliveries when its queue is full).
const E2E_WAIT: Duration = Duration::from_secs(30);

impl ProbeHandler {
    pub(crate) async fn wait_seen(&self, count: usize) {
        let wait = async {
            while self.seen.load(Ordering::SeqCst) < count {
                self.notify.notified().await;
            }
        };
        tokio::time::timeout(E2E_WAIT, wait)
            .await
            .unwrap_or_else(|_| panic!("the handler did not receive event {count}"));
    }
}

/// Waits until `f` holds, polling, up to 10 s.
async fn wait_until(what: &str, f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

async fn lockstep(
    gateway: &mut ChildGateway,
    handler: &ProbeHandler,
    commands: &[String],
) -> (Sample, Vec<Duration>) {
    let mut timings = Vec::with_capacity(commands.len());
    let mark = Mark::start();
    for command in commands {
        let target = handler.seen.load(Ordering::SeqCst) + 1;
        let start = Instant::now();
        gateway.send(command);
        handler.wait_seen(target).await;
        timings.push(start.elapsed());
    }
    (mark.finish(), timings)
}

fn e2e_scenario(name: &str, sample: Sample, ops: usize, timings: Vec<Duration>) -> Scenario {
    Scenario::new(name, "e2e")
        .batch(sample, u64::try_from(ops).unwrap())
        .timings(timings)
        .deterministic(&[])
}

/// The end-to-end scenarios, run in their own process (`perf_e2e_child`):
/// the counters and the RSS then belong to a bot that did nothing else, and
/// the replica scenarios of the report process run on an unfragmented heap.
pub(crate) fn run_e2e(report: &mut Report) {
    let rss_start = rss_kib();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let mut gateway = ChildGateway::spawn();
    let handler = ProbeHandler {
        seen: Arc::new(AtomicUsize::new(0)),
        notify: Arc::new(tokio::sync::Notify::new()),
        hub: Arc::new(Mutex::new(None)),
    };
    let rss = rt.block_on(async {
        // Idle shard: everything a connected client holds after READY.
        let mark = Mark::start();
        let client = Client::builder("perf-report-token", Intents::from_bits(0))
            .event_handler(handler.clone())
            .gateway_config(GatewayConnectionConfig::new(format!(
                "ws://127.0.0.1:{}/",
                gateway.port
            )))
            .application_id(ApplicationId::try_from(1_290_011_223_344_556_677_u64).unwrap())
            .handler_shutdown_timeout(Duration::from_millis(100))
            .start();
        let client_task = tokio::spawn(client);
        handler.wait_seen(1).await;
        let hub_present = || handler.hub.lock().unwrap().is_some();
        wait_until("the READY handler call", hub_present).await;
        let idle = mark.finish();
        let rss_idle = rss_kib();
        report.push(
            Scenario::new("e2e.idle_client", "e2e")
                .sample(idle)
                .deterministic(&[])
                .note("connect, IDENTIFY, READY: allocations made and heap a connected shard keeps"),
        );

        // One scenario per inbound case, frame by frame.
        let commands_for = |name: &str, count: usize| vec![name.to_string(); count];
        for (name, count) in [
            ("floor", 300),
            ("message_create.plain", 300),
            ("message_create.embeds", 300),
            ("message_create.components_v2", 300),
            ("message_update.partial", 300),
            ("presence_update", 300),
            ("interaction_create.chat_input", 300),
            ("unknown_dispatch", 300),
        ] {
            if name == "message_update.partial" {
                // The update merges into the cached original.
                let warm = commands_for("message_create.plain", 1);
                lockstep(&mut gateway, &handler, &warm).await;
            }
            // Warm the queues and fill the hub journal with this event kind,
            // so retained bytes show steady-state growth, not the eviction
            // of the previous scenario's events.
            let warm = commands_for(name, HUB_JOURNAL_CAPACITY + 20);
            lockstep(&mut gateway, &handler, &warm).await;
            let commands = commands_for(name, count);
            let (sample, _) = lockstep(&mut gateway, &handler, &commands).await;
            let timings = {
                let _uncounted = Uncounted::new();
                lockstep(&mut gateway, &handler, &commands).await.1
            };
            let scenario_name = if name == "floor" {
                "e2e.floor".to_string()
            } else {
                format!("e2e.{name}")
            };
            let mut scenario = e2e_scenario(&scenario_name, sample, count, timings);
            if name == "floor" {
                scenario = scenario.note("unknown dispatch with an empty d: socket-to-handler overhead per frame");
            }
            report.push(scenario);
        }

        // GUILD_CREATE storm: distinct large guilds into the real cache.
        let storm: Vec<String> = (1..=STORM_GUILDS).map(|index| format!("storm.{index}")).collect();
        let rss_before = rss_kib();
        let (sample, timings) = lockstep(&mut gateway, &handler, &storm).await;
        let rss_after = rss_kib();
        report.push(
            e2e_scenario("e2e.guild_create_storm", sample, storm.len(), timings)
                .set("guilds", STORM_GUILDS)
                .set("members_per_guild", LARGE_GUILD.members)
                .set("rss_before_kib", rss_before.map(|(rss, _)| rss))
                .set("rss_after_kib", rss_after.map(|(rss, _)| rss))
                .set("hwm_after_kib", rss_after.map(|(_, hwm)| hwm))
                .note("retained_bytes: heap held after the storm (cache + collector hub journal + queues); time measured with allocation counting on (about 65 ns per allocation)"),
        );

        // Push the GUILD_CREATE events out of the hub journal: what the
        // journal released is the share it held.
        let flush = commands_for("floor", HUB_JOURNAL_CAPACITY);
        let (sample, timings) = lockstep(&mut gateway, &handler, &flush).await;
        let rss_flushed = rss_kib();
        report.push(
            e2e_scenario("e2e.journal_flush_after_storm", sample, flush.len(), timings)
                .set("rss_after_kib", rss_flushed.map(|(rss, _)| rss))
                .note("256 tiny dispatches evict the storm from the collector hub journal; -retained_bytes is what the journal held"),
        );

        // Collector fan-out on the real hub: K message collectors, one match.
        let hub = handler.hub.lock().unwrap().clone().unwrap();
        let event = decode_event("MESSAGE_CREATE", fixture(MESSAGE_CREATE_PLAIN)).unwrap();
        let Event::MessageCreate(created) = event else {
            panic!("MESSAGE_CREATE fixture did not decode to MessageCreate");
        };
        let target = created.message.id;
        let other = MessageId::try_from(1_u64).unwrap();
        for k in [1_usize, 8, 64] {
            let matched = Arc::new(AtomicUsize::new(0));
            let matched_notify = Arc::new(tokio::sync::Notify::new());
            let mut tasks = Vec::with_capacity(k);
            for i in 0..k {
                let wanted = if i == 0 { target } else { other };
                let mut collector = hub.message_collector().filter(move |message| message.id == wanted);
                let matched = Arc::clone(&matched);
                let matched_notify = Arc::clone(&matched_notify);
                tasks.push(tokio::spawn(async move {
                    while let Some(message) = collector.next().await {
                        drop(black_box(message));
                        matched.fetch_add(1, Ordering::SeqCst);
                        matched_notify.notify_one();
                    }
                }));
            }
            let warm = commands_for("message_create.plain", 20);
            lockstep(&mut gateway, &handler, &warm).await;
            let matched_now = || matched.load(Ordering::SeqCst);
            wait_until("warm-up matches", || matched_now() >= 20).await;
            // Let the non-matching collectors drain the warm-up.
            tokio::time::sleep(Duration::from_millis(50)).await;

            const FANOUT_FRAMES: usize = 200;
            let fanout_pass = async |gateway: &mut ChildGateway| {
                let mut timings = Vec::with_capacity(FANOUT_FRAMES);
                let mark = Mark::start();
                for _ in 0..FANOUT_FRAMES {
                    let target_seen = handler.seen.load(Ordering::SeqCst) + 1;
                    let target_matched = matched.load(Ordering::SeqCst) + 1;
                    let start = Instant::now();
                    gateway.send("message_create.plain");
                    handler.wait_seen(target_seen).await;
                    let collected = async {
                        while matched.load(Ordering::SeqCst) < target_matched {
                            matched_notify.notified().await;
                        }
                    };
                    tokio::time::timeout(E2E_WAIT, collected)
                        .await
                        .expect("the matching collector did not yield the message");
                    timings.push(start.elapsed());
                }
                // Non-matching collectors may still be filtering the last
                // frames.
                tokio::time::sleep(Duration::from_millis(50)).await;
                (mark.finish(), timings)
            };
            // A discarded pass first: the first run of the collector path under
            // load makes a one-time ~1.9 MB allocation that would otherwise
            // land in k=1 only (see docs/design/perf-baseline.md).
            fanout_pass(&mut gateway).await;
            let (sample, _) = fanout_pass(&mut gateway).await;
            let timings = {
                let _uncounted = Uncounted::new();
                fanout_pass(&mut gateway).await.1
            };
            for task in tasks {
                task.abort();
            }
            report.push(
                e2e_scenario(&format!("e2e.fanout.message_collectors.k{k}"), sample, FANOUT_FRAMES, timings)
                    .set("collectors", k)
                    .note("MESSAGE_CREATE through the real pipeline with K hub collectors (one matches); time: until the handler and the matching collector have it"),
            );
        }

        client_task.abort();
        [rss_idle, rss_before, rss_after, rss_flushed]
    });
    rt.shutdown_timeout(Duration::from_secs(2));
    drop(gateway);
    let [rss_idle, rss_before_storm, rss_after_storm, rss_flushed] = rss;
    let kib = |sample: Option<(u64, u64)>| sample.map(|(rss, _)| rss);
    report.push(
        Scenario::new("process.rss", "process")
            .set("rss_start_kib", kib(rss_start))
            .set("rss_idle_client_kib", kib(rss_idle))
            .set("rss_before_storm_kib", kib(rss_before_storm))
            .set("rss_after_storm_kib", kib(rss_after_storm))
            .set("rss_after_journal_flush_kib", kib(rss_flushed))
            .set("hwm_kib", rss_kib().map(|(_, hwm)| hwm))
            .deterministic(&[])
            .note("VmRSS/VmHWM of the end-to-end bot process from /proc/self/status (glibc malloc, 4 runtime workers); null off Linux"),
    );
}

/// Helper process for `perf_report`: runs the end-to-end scenarios and
/// prints them as report lines for the parent to collect.
#[test]
#[ignore = "helper process spawned by perf_report; does nothing on its own"]
fn perf_e2e_child() {
    if std::env::var_os(E2E_CHILD_ENV).is_none() {
        return;
    }
    run_e2e(&mut Report::default());
}

/// Runs `perf_e2e_child` and adds the scenarios it prints to `report`.
pub(crate) fn run_e2e_in_child(report: &mut Report) {
    let mut child = spawn_helper("e2e::perf_e2e_child", E2E_CHILD_ENV);
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut received = 0;
    for line in stdout.lines() {
        let line = line.unwrap();
        // libtest prints `test perf_e2e_child ... ` before the first line.
        if let Some((_, json)) = line.split_once(LINE_PREFIX) {
            report.push_value(serde_json::from_str(json).unwrap());
            received += 1;
        }
    }
    assert!(
        child.wait().unwrap().success(),
        "the end-to-end child failed"
    );
    assert!(received > 0, "the end-to-end child reported no scenarios");
}
