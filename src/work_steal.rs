//! Low-latency persistent worker lanes with lock-free inline steal fallback.
//!
//! Spawning fresh OS threads (`std::thread::scope`) adds ~25–40 µs of kernel scheduling
//! overhead on 1k-row micro-batches (`~300 µs` total). Each [`StealLane`] lazily spawns a
//! single persistent worker thread backed by a bounded channel, while keeping closure state
//! and outputs directly on the caller's stack frame (no `Box` or `Mutex` on the hot path).
//!
//! Dedicated lanes (`DirectEnc`, `MaintFts`, `MaintIdx`) ensure nested maintenance inside
//! `dml` foreground work never contends with or starves the outer WAL encoder lane.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) enum StealLane {
    DirectEnc,
    MaintFts,
    FtsTail,
    MaintIdx,
}

struct StealSlot {
    /// `0` = queued, `1` = claimed by worker, `2` = claimed/cancelled by caller.
    state: AtomicU8,
    done: AtomicBool,
    panicked: AtomicBool,
    caller: Thread,
    run: unsafe fn(usize),
    data: usize,
}

struct WorkerDone<'a>(&'a StealSlot, bool);

impl Drop for WorkerDone<'_> {
    fn drop(&mut self) {
        if self.1 {
            self.0.panicked.store(true, Ordering::Release);
        }
        self.0.done.store(true, Ordering::Release);
        self.0.caller.unpark();
    }
}

fn spawn_lane(name: &'static str) -> Option<SyncSender<Arc<StealSlot>>> {
    let (tx, rx) = sync_channel::<Arc<StealSlot>>(8);
    thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            while let Ok(slot) = rx.recv() {
                if slot
                    .state
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    continue;
                }
                let mut wd = WorkerDone(&slot, true);
                // SAFETY: Transitioning `state` `0 -> 1` gives this worker exclusive access to
                // `slot.data`, and `StealGuard` keeps the caller's stack frame alive until
                // `WorkerDone` stores `done = true` with `Ordering::Release`.
                unsafe { (slot.run)(slot.data) };
                wd.1 = false;
            }
        })
        .ok()
        .map(|_| tx)
}

impl StealLane {
    #[inline]
    fn sender(self) -> Option<&'static SyncSender<Arc<StealSlot>>> {
        static ENC: OnceLock<Option<SyncSender<Arc<StealSlot>>>> = OnceLock::new();
        static FTS: OnceLock<Option<SyncSender<Arc<StealSlot>>>> = OnceLock::new();
        static FTS_TAIL: OnceLock<Option<SyncSender<Arc<StealSlot>>>> = OnceLock::new();
        static IDX: OnceLock<Option<SyncSender<Arc<StealSlot>>>> = OnceLock::new();
        match self {
            Self::DirectEnc => ENC.get_or_init(|| spawn_lane("lin-steal-enc")).as_ref(),
            Self::MaintFts => FTS.get_or_init(|| spawn_lane("lin-steal-fts")).as_ref(),
            Self::FtsTail => FTS_TAIL.get_or_init(|| spawn_lane("lin-steal-fts-tail")).as_ref(),
            Self::MaintIdx => IDX.get_or_init(|| spawn_lane("lin-steal-idx")).as_ref(),
        }
    }
}

#[inline]
fn wait_slot_done(slot: &StealSlot) {
    for _ in 0..64 {
        if slot.done.load(Ordering::Acquire) {
            return;
        }
        std::hint::spin_loop();
    }
    while !slot.done.load(Ordering::Acquire) {
        thread::park_timeout(Duration::from_micros(50));
    }
}

struct StealGuard<'a>(&'a StealSlot);

impl Drop for StealGuard<'_> {
    fn drop(&mut self) {
        if self
            .0
            .state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .err()
            == Some(1)
        {
            wait_slot_done(self.0);
        }
    }
}

/// Enqueue `bg` on `lane` when `enabled` is true, run `fg` on the calling thread, and
/// steal `bg` inline if the worker has not yet claimed it.
pub(crate) fn steal_lane_if<A, B, F, G>(lane: StealLane, enabled: bool, bg: F, fg: G) -> (A, B)
where
    A: Send,
    F: FnOnce() -> A + Send,
    G: FnOnce() -> B,
{
    if !enabled {
        let b = fg();
        let a = bg();
        return (a, b);
    }
    let Some(tx) = lane.sender() else {
        let b = fg();
        let a = bg();
        return (a, b);
    };

    struct StackJob<F, A> {
        func: Option<F>,
        out: Option<A>,
    }

    unsafe fn run_job<F: FnOnce() -> A + Send, A: Send>(data: usize) {
        // SAFETY: Exclusive access guaranteed by `state` `0 -> 1` transition.
        let job = unsafe { &mut *(data as *mut StackJob<F, A>) };
        if let Some(f) = job.func.take() {
            job.out = Some(f());
        }
    }

    let payload = UnsafeCell::new(StackJob::<F, A> {
        func: Some(bg),
        out: None,
    });
    let slot = Arc::new(StealSlot {
        state: AtomicU8::new(0),
        done: AtomicBool::new(false),
        panicked: AtomicBool::new(false),
        caller: thread::current(),
        run: run_job::<F, A>,
        data: payload.get() as usize,
    });
    // Declared after `payload` so reverse drop order blocks on any active worker before
    // `payload` leaves the caller's stack frame (including during panic unwind).
    let _guard = StealGuard(&slot);
    let sent = tx.try_send(Arc::clone(&slot)).is_ok();
    let b = fg();
    let run_inline = if !sent {
        slot.state.store(2, Ordering::Relaxed);
        true
    } else {
        slot.state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    };
    let a = if run_inline {
        // SAFETY: Worker never observed `state == 0` and will never dereference `slot.data`.
        let f = unsafe { (*payload.get()).func.take() }.expect("bg closure present");
        f()
    } else {
        wait_slot_done(&slot);
        if slot.panicked.load(Ordering::Acquire) {
            panic!("work_steal worker panicked");
        }
        // SAFETY: Worker completed `run_job` and stored `done = true` (`Release`),
        // synchronized with `Acquire` in `wait_slot_done`.
        unsafe { (*payload.get()).out.take() }.expect("worker produced output")
    };
    (a, b)
}

#[inline]
pub(crate) fn steal2<A, B, F, G>(lane: StealLane, bg: F, fg: G) -> (A, B)
where
    A: Send,
    F: FnOnce() -> A + Send,
    G: FnOnce() -> B,
{
    steal_lane_if(lane, true, bg, fg)
}

#[inline]
pub(crate) fn steal3<A, B, C, F1, F2, G>(
    lane1: StealLane,
    enable1: bool,
    bg1: F1,
    lane2: StealLane,
    bg2: F2,
    fg: G,
) -> (A, B, C)
where
    A: Send,
    B: Send,
    F1: FnOnce() -> A + Send,
    F2: FnOnce() -> B + Send,
    G: FnOnce() -> C,
{
    let (a, (b, c)) = steal_lane_if(lane1, enable1, bg1, || steal2(lane2, bg2, fg));
    (a, b, c)
}
