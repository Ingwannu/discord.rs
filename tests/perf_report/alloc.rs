//! Counting global allocator and the measurement windows built on it
//! (`Mark`, `Recorder`, `Uncounted`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct CountingAllocator;

/// alloc + alloc_zeroed + realloc calls.
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
/// Sum of the sizes requested by those calls (a realloc counts its new size).
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
/// Bytes currently allocated.
pub(crate) static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);
/// High-water mark of `LIVE_BYTES` since the last `Mark::start`.
static PEAK_BYTES: AtomicI64 = AtomicI64::new(0);
/// Off while timing (see `Uncounted`): the counters' atomic updates cost
/// about 65 ns per allocation and free on the reference machine, which
/// would double the time of the allocation-bound gateway path.
static COUNTING: AtomicBool = AtomicBool::new(true);

pub(crate) fn counting() -> bool {
    COUNTING.load(Ordering::Relaxed)
}

/// Turns the counters off until dropped, for the timed iterations.
///
/// `LIVE_BYTES` is not updated either, so it drifts when a block crosses
/// the boundary. Counted windows (`Mark`, `Recorder`) only use differences,
/// so they stay exact as long as nothing allocated while uncounted is freed
/// inside one: every scenario times its iterations after its counted
/// passes, on state it drops before the next scenario's counted passes.
pub(crate) struct Uncounted;

impl Uncounted {
    pub(crate) fn new() -> Self {
        COUNTING.store(false, Ordering::SeqCst);
        Self
    }
}

impl Drop for Uncounted {
    fn drop(&mut self) {
        COUNTING.store(true, Ordering::SeqCst);
    }
}

fn on_alloc(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(u64::try_from(size).unwrap_or(u64::MAX), Ordering::Relaxed);
    let size = i64::try_from(size).unwrap_or(i64::MAX);
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

fn on_realloc(old: usize, new: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(u64::try_from(new).unwrap_or(u64::MAX), Ordering::Relaxed);
    let delta = i64::try_from(new).unwrap_or(i64::MAX) - i64::try_from(old).unwrap_or(i64::MAX);
    let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every method forwards the caller's arguments unchanged to the
// system allocator, which upholds the GlobalAlloc contract; the counters are
// plain atomics and never touch the memory being managed.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same layout the caller passed to us.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && counting() {
            on_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same layout the caller passed to us.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() && counting() {
            on_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if counting() {
            LIVE_BYTES.fetch_sub(
                i64::try_from(layout.size()).unwrap_or(i64::MAX),
                Ordering::Relaxed,
            );
        }
        // SAFETY: ptr was returned by System.alloc/realloc with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged from the caller.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() && counting() {
            on_realloc(layout.size(), new_size);
        }
        new
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Allocator activity between `Mark::start` and `Mark::finish`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Sample {
    pub(crate) allocs: u64,
    pub(crate) alloc_bytes: u64,
    /// Live bytes at the end minus live bytes at the start.
    pub(crate) retained_bytes: i64,
    /// Highest live bytes in between, minus live bytes at the start.
    pub(crate) peak_bytes: i64,
}

pub(crate) struct Mark {
    pub(crate) allocs: u64,
    pub(crate) bytes: u64,
    pub(crate) live: i64,
}

impl Mark {
    /// Starts a measurement and resets the peak tracker. Marks must not
    /// overlap; `Recorder` combines consecutive marks into a total.
    pub(crate) fn start() -> Self {
        let live = LIVE_BYTES.load(Ordering::SeqCst);
        PEAK_BYTES.store(live, Ordering::SeqCst);
        Self {
            allocs: ALLOCATIONS.load(Ordering::SeqCst),
            bytes: ALLOCATED_BYTES.load(Ordering::SeqCst),
            live,
        }
    }

    pub(crate) fn finish(&self) -> Sample {
        Sample {
            allocs: ALLOCATIONS.load(Ordering::SeqCst) - self.allocs,
            alloc_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst) - self.bytes,
            retained_bytes: LIVE_BYTES.load(Ordering::SeqCst) - self.live,
            peak_bytes: PEAK_BYTES.load(Ordering::SeqCst) - self.live,
        }
    }
}

/// Measures a sequence of steps and their total. A counting recorder
/// records allocator activity per step, a timing recorder the duration of
/// each step, and an idle one neither (for the clean timed iterations).
pub(crate) struct Recorder {
    pub(crate) counting: bool,
    pub(crate) timing: bool,
    pub(crate) start_allocs: u64,
    pub(crate) start_bytes: u64,
    pub(crate) start_live: i64,
    pub(crate) peak_abs: i64,
    pub(crate) steps: Vec<(&'static str, Sample)>,
    pub(crate) durations: Vec<(&'static str, Duration)>,
}

pub(crate) struct StepStart {
    mark: Option<Mark>,
    at: Option<Instant>,
}

impl Recorder {
    pub(crate) fn new(counting: bool, timing: bool) -> Self {
        // Allocated before the counters are read, so the recorder's own
        // bookkeeping never shows up in the total.
        let steps = Vec::with_capacity(if counting { 16 } else { 0 });
        let durations = Vec::with_capacity(if timing { 16 } else { 0 });
        let live = LIVE_BYTES.load(Ordering::SeqCst);
        Self {
            counting,
            timing,
            start_allocs: ALLOCATIONS.load(Ordering::SeqCst),
            start_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst),
            start_live: live,
            peak_abs: live,
            steps,
            durations,
        }
    }

    pub(crate) fn counting() -> Self {
        Self::new(true, false)
    }

    pub(crate) fn timing() -> Self {
        Self::new(false, true)
    }

    pub(crate) fn idle() -> Self {
        Self::new(false, false)
    }

    pub(crate) fn step<T>(&mut self, name: &'static str, f: impl FnOnce() -> T) -> T {
        let start = self.begin();
        let value = f();
        self.end(name, start);
        value
    }

    pub(crate) fn begin(&self) -> StepStart {
        StepStart {
            mark: self.counting.then(Mark::start),
            at: self.timing.then(Instant::now),
        }
    }

    pub(crate) fn end(&mut self, name: &'static str, start: StepStart) {
        assert!(
            self.steps.len() < self.steps.capacity() || !self.counting,
            "raise the recorder's step capacity: growing it would be counted"
        );
        if let Some(at) = start.at {
            self.durations.push((name, at.elapsed()));
        }
        if let Some(mark) = start.mark {
            let sample = mark.finish();
            self.peak_abs = self.peak_abs.max(mark.live + sample.peak_bytes);
            self.steps.push((name, sample));
        }
    }

    pub(crate) fn finish(self) -> (Sample, Vec<(&'static str, Sample)>) {
        let total = Sample {
            allocs: ALLOCATIONS.load(Ordering::SeqCst) - self.start_allocs,
            alloc_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst) - self.start_bytes,
            retained_bytes: LIVE_BYTES.load(Ordering::SeqCst) - self.start_live,
            peak_bytes: self.peak_abs - self.start_live,
        };
        (total, self.steps)
    }
}
