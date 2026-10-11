//! Task spawning shared by the runtime modules.

use std::future::Future;
use std::pin::Pin;

use tokio::task::JoinHandle;

/// A spawned `()` task with its concrete future type erased.
type ErasedTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Spawns `future` behind a `Box<dyn Future>`, so every caller shares one
/// copy of tokio's task machinery.
///
/// [Decision Log]
/// - Purpose: cut the code generated per long-lived background task.
/// - Constraint: `tokio::spawn::<F>` compiles the task harness, cell, and
///   waker vtable for each future type `F`, four times over (boxed and
///   unboxed `F`, for both schedulers): about 9,000 LLVM IR lines per
///   spawned future type in a debug build of this crate.
/// - Alternatives: (1) keep `tokio::spawn` at each site; (2) erase the type
///   at the call site with `Box::pin`; (3) this helper.
/// - Chosen: (3), so the rule lives in one place and every `()` task spawned
///   through it shares a single `tokio::spawn::<ErasedTask>` instance.
/// - Trade-offs: one extra allocation per spawn and a dynamic call per poll.
///   Use it for tasks spawned once per player, track, connection, or child
///   process, not for per-message work on a hot path. `#[track_caller]`
///   keeps tokio's recorded spawn location at the caller.
#[track_caller]
pub(crate) fn spawn_erased(future: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
    let task: ErasedTask = Box::pin(future);
    tokio::spawn(task)
}
