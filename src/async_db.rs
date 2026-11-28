//! Async façade: Tokio offload, concurrent snapshot readers, and Stream API over Lin's sync engine.
//!
//! Enable with `lin` feature `async`.
//!
//! - `to_vec_*` / `run_*` — await a full result (offload).
//! - `run_read` / `run_many_read` / `to_vec` / `to_vec_typed` — execute read queries
//!   concurrently on a cached [`ReadDb`] snapshot without serializing readers on the writer mutex.
//! - `stream_query` / cursor-backed streams — pull via [`QueryCursor`] on a
//!   blocking thread into an mpsc channel (lazy for simple pipelines).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::pin::pin;
use std::sync::{
    Arc, Condvar, OnceLock, RwLock,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use futures_util::future::{Either, select};
use futures_util::stream::Stream;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::spawn_blocking;
use tokio_stream::wrappers::ReceiverStream;

use crate::ast::{Query, Stmt};
use crate::batch::RecordBatch;
use crate::error::Error;
use crate::exec::shared::{WalSyncGroup, defer_db_write_op};
use crate::exec::{Db, Done, Handle, Prepared, ReadDb, Stats, Transaction};
use crate::query::Queryable;
use crate::row::{self, FromRow};
use crate::store::{Cell, Row};

/// Concurrency, admission queue, streaming backpressure, and deadline limits for [`AsyncDb`]
/// (`D33`: `LIN-1281`..`LIN-1320`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncExecutionLimits {
    pub max_concurrent_blocking: usize,
    pub max_queued_queries: usize,
    pub max_queued_input_bytes: usize,
    pub max_stream_buffered_rows: usize,
    pub timeout: Option<Duration>,
}

impl Default for AsyncExecutionLimits {
    fn default() -> Self {
        Self {
            max_concurrent_blocking: 64,
            max_queued_queries: 4096,
            max_queued_input_bytes: 64 * 1024 * 1024,
            max_stream_buffered_rows: 1024,
            timeout: None,
        }
    }
}

/// Receipt emitted upon completion or cancellation of a bounded async row stream (`LIN-1299`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamCompletionReceipt {
    pub yielded_rows: usize,
    pub queue_wait_ns: u64,
    pub execution_ns: u64,
    pub completed_cleanly: bool,
}

/// Point-in-time telemetry snapshot for [`AsyncDb`] execution & backpressure (`LIN-1317`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncTelemetrySnapshot {
    pub active_queries: usize,
    pub queued_queries: usize,
    pub total_queue_wait_ns: u64,
    pub cancelled_by_timeout: u64,
    pub cancelled_by_drop: u64,
    pub rejected_by_queue_limit: u64,
}

struct TimerEntry {
    deadline: Instant,
    seq: u64,
    tx: oneshot::Sender<()>,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.seq == other.seq
    }
}
impl Eq for TimerEntry {}
impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.deadline
            .cmp(&other.deadline)
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

struct DeadlineTimer {
    queue: std::sync::Mutex<(u64, BinaryHeap<Reverse<TimerEntry>>)>,
    cv: Condvar,
}

impl DeadlineTimer {
    fn global() -> &'static Self {
        static TIMER: OnceLock<Arc<DeadlineTimer>> = OnceLock::new();
        TIMER.get_or_init(|| {
            let svc = Arc::new(DeadlineTimer {
                queue: std::sync::Mutex::new((0, BinaryHeap::new())),
                cv: Condvar::new(),
            });
            let bg = Arc::clone(&svc);
            let _ = std::thread::Builder::new()
                .name("lin-async-deadline".into())
                .spawn(move || bg.run_loop());
            svc
        })
    }

    fn arm(&self, deadline: Instant) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut guard = match self.queue.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.0 += 1;
        let seq = guard.0;
        if guard.1.len() > 2048 {
            let drained: Vec<_> = guard.1.drain().filter(| e| !e.0.tx.is_closed()).collect();
            guard.1.extend(drained);
        }
        let notify = guard
            .1
            .peek()
            .is_none_or(|top| deadline < top.0.deadline);
        guard.1.push(Reverse(TimerEntry { deadline, seq, tx }));
        if notify {
            self.cv.notify_one();
        }
        rx
    }

    fn run_loop(&self) {
        let mut guard = match self.queue.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        loop {
            let now = Instant::now();
            while let Some(top) = guard.1.peek() {
                if top.0.deadline <= now || top.0.tx.is_closed() {
                    let Reverse(entry) = guard.1.pop().unwrap();
                    if entry.deadline <= now {
                        let _ = entry.tx.send(());
                    }
                } else {
                    break;
                }
            }
            match guard.1.peek() {
                None => {
                    guard = match self.cv.wait(guard) {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                }
                Some(top) => {
                    let wait = top.0.deadline.saturating_duration_since(Instant::now());
                    guard = match self.cv.wait_timeout(guard, wait) {
                        Ok((g, _)) => g,
                        Err(p) => p.into_inner().0,
                    };
                }
            }
        }
    }
}

struct AsyncControl {
    limits: AsyncExecutionLimits,
    semaphore: Arc<Semaphore>,
    active_queries: AtomicUsize,
    queued_queries: AtomicUsize,
    queued_input_bytes: AtomicUsize,
    total_queue_wait_ns: AtomicU64,
    cancelled_by_timeout: AtomicU64,
    cancelled_by_drop: AtomicU64,
    rejected_by_queue_limit: AtomicU64,
}

impl AsyncControl {
    fn new(limits: AsyncExecutionLimits) -> Self {
        let permits = limits.max_concurrent_blocking.max(1);
        Self {
            limits,
            semaphore: Arc::new(Semaphore::new(permits)),
            active_queries: AtomicUsize::new(0),
            queued_queries: AtomicUsize::new(0),
            queued_input_bytes: AtomicUsize::new(0),
            total_queue_wait_ns: AtomicU64::new(0),
            cancelled_by_timeout: AtomicU64::new(0),
            cancelled_by_drop: AtomicU64::new(0),
            rejected_by_queue_limit: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AsyncTelemetrySnapshot {
        AsyncTelemetrySnapshot {
            active_queries: self.active_queries.load(Ordering::Relaxed),
            queued_queries: self.queued_queries.load(Ordering::Relaxed),
            total_queue_wait_ns: self.total_queue_wait_ns.load(Ordering::Relaxed),
            cancelled_by_timeout: self.cancelled_by_timeout.load(Ordering::Relaxed),
            cancelled_by_drop: self.cancelled_by_drop.load(Ordering::Relaxed),
            rejected_by_queue_limit: self.rejected_by_queue_limit.load(Ordering::Relaxed),
        }
    }

    fn try_reserve_queue(self: &Arc<Self>, input_bytes: usize) -> Result<QueueReservation, Error> {
        if input_bytes > self.limits.max_queued_input_bytes {
            self.rejected_by_queue_limit.fetch_add(1, Ordering::Relaxed);
            return Err(Error::resource_limit(format!(
                "async query input size {input_bytes} bytes exceeds max_queued_input_bytes {}",
                self.limits.max_queued_input_bytes
            )));
        }
        loop {
            let cur = self.queued_queries.load(Ordering::Relaxed);
            if cur >= self.limits.max_queued_queries {
                self.rejected_by_queue_limit.fetch_add(1, Ordering::Relaxed);
                return Err(Error::resource_limit(format!(
                    "async queue depth {} exceeds max_queued_queries {}",
                    cur + 1,
                    self.limits.max_queued_queries
                )));
            }
            if self
                .queued_queries
                .compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        let prev_bytes = self
            .queued_input_bytes
            .fetch_add(input_bytes, Ordering::AcqRel);
        if prev_bytes.saturating_add(input_bytes) > self.limits.max_queued_input_bytes {
            self.queued_input_bytes
                .fetch_sub(input_bytes, Ordering::Release);
            self.queued_queries.fetch_sub(1, Ordering::Release);
            self.rejected_by_queue_limit.fetch_add(1, Ordering::Relaxed);
            return Err(Error::resource_limit(format!(
                "async queued input bytes {} exceeds max_queued_input_bytes {}",
                prev_bytes.saturating_add(input_bytes),
                self.limits.max_queued_input_bytes
            )));
        }
        Ok(QueueReservation {
            ctrl: Arc::clone(self),
            input_bytes,
            disarmed: false,
            timed_out: false,
        })
    }

    async fn acquire_slot(
        self: &Arc<Self>,
        input_bytes: usize,
        deadline: Option<Instant>,
    ) -> Result<(ActiveSlot, u64), Error> {
        if input_bytes > self.limits.max_queued_input_bytes {
            self.rejected_by_queue_limit.fetch_add(1, Ordering::Relaxed);
            return Err(Error::resource_limit(format!(
                "async query input size {input_bytes} bytes exceeds max_queued_input_bytes {}",
                self.limits.max_queued_input_bytes
            )));
        }
        if let Some(dl) = deadline
            && Instant::now() >= dl
        {
            self.cancelled_by_timeout.fetch_add(1, Ordering::Relaxed);
            return Err(Error::deadline_exceeded("async query timeout exceeded before start"));
        }

        // Fast path: uncontended semaphore slot available immediately.
        if self.queued_queries.load(Ordering::Relaxed) == 0
            && let Ok(permit) = Arc::clone(&self.semaphore).try_acquire_owned()
        {
            self.active_queries.fetch_add(1, Ordering::AcqRel);
            return Ok((
                ActiveSlot {
                    ctrl: Arc::clone(self),
                    _permit: permit,
                    settled: false,
                },
                0,
            ));
        }

        let mut qres = self.try_reserve_queue(input_bytes)?;
        let wait_start = Instant::now();
        let sem = Arc::clone(&self.semaphore);
        let permit = match deadline {
            None => sem
                .acquire_owned()
                .await
                .map_err(|_| Error::runtime("async semaphore closed"))?,
            Some(dl) => {
                let rx = DeadlineTimer::global().arm(dl);
                let acq = pin!(sem.acquire_owned());
                let tmr = pin!(rx);
                match select(acq, tmr).await {
                    Either::Left((res, _)) => {
                        res.map_err(|_| Error::runtime("async semaphore closed"))?
                    }
                    Either::Right((_, _)) => {
                        qres.timed_out = true;
                        let waited = wait_start.elapsed().as_nanos() as u64;
                        self.total_queue_wait_ns.fetch_add(waited, Ordering::Relaxed);
                        self.cancelled_by_timeout.fetch_add(1, Ordering::Relaxed);
                        return Err(Error::deadline_exceeded(
                            "async query timed out while waiting for execution slot",
                        ));
                    }
                }
            }
        };
        let waited_ns = wait_start.elapsed().as_nanos() as u64;
        self.total_queue_wait_ns
            .fetch_add(waited_ns, Ordering::Relaxed);
        qres.disarm();
        self.active_queries.fetch_add(1, Ordering::AcqRel);
        Ok((
            ActiveSlot {
                ctrl: Arc::clone(self),
                _permit: permit,
                settled: false,
            },
            waited_ns,
        ))
    }
}

struct QueueReservation {
    ctrl: Arc<AsyncControl>,
    input_bytes: usize,
    disarmed: bool,
    timed_out: bool,
}

impl QueueReservation {
    fn disarm(&mut self) {
        if !self.disarmed {
            self.disarmed = true;
            self.ctrl
                .queued_input_bytes
                .fetch_sub(self.input_bytes, Ordering::Release);
            self.ctrl.queued_queries.fetch_sub(1, Ordering::Release);
        }
    }
}

impl Drop for QueueReservation {
    fn drop(&mut self) {
        if !self.disarmed {
            self.ctrl
                .queued_input_bytes
                .fetch_sub(self.input_bytes, Ordering::Release);
            self.ctrl.queued_queries.fetch_sub(1, Ordering::Release);
            if !self.timed_out {
                self.ctrl.cancelled_by_drop.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

struct ActiveSlot {
    ctrl: Arc<AsyncControl>,
    _permit: OwnedSemaphorePermit,
    settled: bool,
}

impl ActiveSlot {
    #[inline]
    fn settle_completed(&mut self) {
        self.settled = true;
    }

    #[inline]
    fn settle_timeout(&mut self) {
        if !self.settled {
            self.settled = true;
            self.ctrl
                .cancelled_by_timeout
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn settle_dropped(&mut self) {
        if !self.settled {
            self.settled = true;
            self.ctrl.cancelled_by_drop.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for ActiveSlot {
    fn drop(&mut self) {
        self.ctrl.active_queries.fetch_sub(1, Ordering::Release);
        if !self.settled {
            self.ctrl.cancelled_by_drop.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Async wrapper around a writer [`Db`] (`Arc<Mutex<Db>>`) with a lock-free/RwLock-cached
/// concurrent [`ReadDb`] snapshot so multiple async readers can execute in parallel across
/// Tokio's blocking pool without contending on the writer mutex.
#[derive(Clone)]
pub struct AsyncDb {
    inner: Arc<Mutex<Db>>,
    write_epoch: Arc<AtomicU64>,
    cached_reader: Arc<RwLock<Option<(u64, ReadDb, Stats)>>>,
    sync_group: Arc<WalSyncGroup>,
    control: Arc<AsyncControl>,
}

impl AsyncDb {
    pub fn new(db: Db) -> Self {
        Self::from_arc(Arc::new(Mutex::new(db)))
    }

    pub fn from_arc(inner: Arc<Mutex<Db>>) -> Self {
        Self {
            inner,
            write_epoch: Arc::new(AtomicU64::new(1)),
            cached_reader: Arc::new(RwLock::new(None)),
            sync_group: Arc::new(WalSyncGroup::new()),
            control: Arc::new(AsyncControl::new(AsyncExecutionLimits::default())),
        }
    }

    /// Configure concurrency, admission queue, streaming backpressure, and timeout limits (`LIN-1285`..`LIN-1304`).
    pub fn with_limits(mut self, limits: AsyncExecutionLimits) -> Self {
        self.control = Arc::new(AsyncControl::new(limits));
        self
    }

    /// Construct an [`AsyncDb`] directly with explicit [`AsyncExecutionLimits`].
    pub fn new_with_limits(db: Db, limits: AsyncExecutionLimits) -> Self {
        Self::new(db).with_limits(limits)
    }

    /// Current execution limits configured on this [`AsyncDb`].
    #[inline]
    pub fn limits(&self) -> AsyncExecutionLimits {
        self.control.limits
    }

    /// Inspect live admission queue, active concurrency, queue wait, timeout, drop, and rejection counters (`LIN-1317`).
    #[inline]
    pub fn telemetry_snapshot(&self) -> AsyncTelemetrySnapshot {
        self.control.snapshot()
    }

    pub fn inner(&self) -> &Arc<Mutex<Db>> {
        &self.inner
    }

    async fn run_blocking_gated<T, F>(&self, input_bytes: usize, work: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(Arc<AtomicBool>) -> Result<T, Error> + Send + 'static,
    {
        let deadline = self.control.limits.timeout.map(|d| Instant::now() + d);
        let (mut slot, _) = self.control.acquire_slot(input_bytes, deadline).await?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancelled);
        let jh = spawn_blocking(move || {
            if cancel_worker.load(Ordering::Acquire) {
                return Err(Error::canceled("async operation cancelled before start"));
            }
            work(cancel_worker)
        });

        match deadline {
            None => {
                let res = jh
                    .await
                    .map_err(|e| Error::runtime(format!("async join: {e}")))?;
                slot.settle_completed();
                res
            }
            Some(dl) => {
                if Instant::now() >= dl {
                    cancelled.store(true, Ordering::Release);
                    slot.settle_timeout();
                    return Err(Error::deadline_exceeded("async query execution timed out"));
                }
                let rx = DeadlineTimer::global().arm(dl);
                let fut = pin!(jh);
                let tmr = pin!(rx);
                match select(fut, tmr).await {
                    Either::Left((join_res, _)) => {
                        let out = join_res
                            .map_err(|e| Error::runtime(format!("async join: {e}")))?;
                        slot.settle_completed();
                        out
                    }
                    Either::Right((_, _)) => {
                        cancelled.store(true, Ordering::Release);
                        slot.settle_timeout();
                        Err(Error::deadline_exceeded("async query execution timed out"))
                    }
                }
            }
        }
    }

    /// Obtain a fresh or cached [`ReadDb`] snapshot synchronized with the latest write epoch.
    /// Concurrent readers after a write share the same snapshot without locking the writer `Mutex`.
    async fn snapshot(&self) -> ReadDb {
        let epoch = self.write_epoch.load(Ordering::Acquire);
        if self.sync_group.is_all_synced()
            && let Ok(guard) = self.cached_reader.read()
            && let Some((cached_epoch, ref rdb, _)) = *guard
            && cached_epoch == epoch
        {
            return rdb.clone();
        }
        // Building a snapshot copies the memory image: keep it off the executor threads.
        let inner = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        let cached_reader = Arc::clone(&self.cached_reader);
        let sync_group = Arc::clone(&self.sync_group);
        let built = spawn_blocking(move || {
            let mut g = inner.blocking_lock();
            let epoch_now = write_epoch.load(Ordering::Acquire);
            if sync_group.is_all_synced()
                && let Ok(guard) = cached_reader.read()
                && let Some((cached_epoch, ref rdb, _)) = *guard
                && cached_epoch == epoch_now
            {
                return rdb.clone();
            }
            let sync_file = g.persist.as_ref().and_then(|p| {
                (p.sync == crate::persist::SyncMode::Full && p.log_bytes > 0)
                    .then(|| p.log.try_clone().ok())
                    .flatten()
            });
            let stats = g.stats();
            let rdb = g.reader();
            drop(g);
            let _ = sync_group.wait_all_written_synced(sync_file);
            if let Ok(mut wguard) = cached_reader.write() {
                *wguard = Some((epoch_now, rdb.clone(), stats));
            }
            rdb
        })
        .await;
        match built {
            Ok(rdb) => rdb,
            Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            // Runtime is shutting down and refused the task: build inline.
            Err(_) => self.inner.lock().await.reader(),
        }
    }

    /// Run a write closure under the writer lock for in-memory + OS page-cache WAL append,
    /// release the writer lock immediately, and join the shared [`WalSyncGroup`] barrier.
    async fn run_write_op_sized<T, F>(&self, input_bytes: usize, op: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Db) -> Result<T, Error> + Send + 'static,
    {
        let db = Arc::clone(&self.inner);
        let sync_group = Arc::clone(&self.sync_group);
        let write_epoch = Arc::clone(&self.write_epoch);
        self.run_blocking_gated(input_bytes, move |cancelled| {
            let (out, pending_sync) = {
                let mut g = db.blocking_lock();
                if cancelled.load(Ordering::Acquire) {
                    return Err(Error::canceled("async write cancelled before execution"));
                }
                let before = (
                    g.r#gen(),
                    g.store.docs_epoch,
                    g.store.collections.len(),
                    g.store.edges.len(),
                    g.store.facts_len(),
                );
                let (out, pending_sync) = defer_db_write_op(&mut g, &sync_group, op);
                let after = (
                    g.r#gen(),
                    g.store.docs_epoch,
                    g.store.collections.len(),
                    g.store.edges.len(),
                    g.store.facts_len(),
                );
                if after != before || pending_sync.is_some() {
                    write_epoch.fetch_add(1, Ordering::Release);
                }
                (out, pending_sync)
            };
            let val = out?;
            if let Some((seq, file)) = pending_sync {
                sync_group.wait_synced(seq, file)?;
            }
            Ok(val)
        })
        .await
    }

    async fn run_write_op<T, F>(&self, op: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Db) -> Result<T, Error> + Send + 'static,
    {
        self.run_write_op_sized(0, op).await
    }

    /// Run a statement on the writer. Like [`Self::execute`], concurrent calls share one
    /// commit group and one WAL sync.
    pub async fn run(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let len = src.len();
        self.run_write_op_sized(len, move |db| db.run(&src)).await
    }

    /// Execute a read-only query concurrently against a cached [`ReadDb`] snapshot.
    /// Multiple `run_read` calls run in parallel on Tokio's blocking pool without blocking each other.
    pub async fn run_read(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let len = src.len();
        let read = self.snapshot().await;
        self.run_blocking_gated(len, move |_| read.run(&src)).await
    }

    /// Execute multiple read-only queries concurrently in parallel against a single shared [`ReadDb`] snapshot.
    pub async fn run_many_read<I, S>(&self, queries: I) -> Result<Vec<Handle>, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let queries: Vec<String> = queries.into_iter().map(Into::into).collect();
        if queries.is_empty() {
            return Ok(Vec::new());
        }
        let bytes: usize = queries.iter().map(String::len).sum();
        let read = self.snapshot().await;
        self.run_blocking_gated(bytes, move |_| read.run_parallel(queries))
            .await
    }

    /// Execute a write. Concurrent calls (of this method, [`Self::execute_prepared`], and
    /// [`Self::transaction`]) pipeline WAL appends and share [`WalSyncGroup`] barriers so N
    /// concurrent durable writes coalesce into shared `fsync`s instead of N serial flushes.
    pub async fn execute(&self, src: impl Into<String>) -> Result<Done, Error> {
        let src = src.into();
        let len = src.len();
        self.run_write_op_sized(len, move |db| db.execute(&src))
            .await
    }

    pub async fn scalar(&self, src: impl Into<String>) -> Result<Cell, Error> {
        self.run_read(src).await?.scalar()
    }

    pub async fn prepare(&self, src: impl Into<String>) -> Result<Prepared, Error> {
        let src = src.into();
        let len = src.len();
        let read = self.snapshot().await;
        self.run_blocking_gated(len, move |_| read.prepare(&src))
            .await
    }

    pub async fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        let bytes = stmt.estimate_bytes();
        self.run_write_op_sized(bytes, move |db| db.run_stmt(stmt))
            .await
    }

    pub async fn run_prepared(&self, prepared: Prepared) -> Result<Handle, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return self
                .run_blocking_gated(0, move |_| read.run_prepared(&prepared))
                .await;
        }
        self.run_write_op(move |db| db.run_prepared(&prepared)).await
    }

    pub async fn execute_prepared(&self, prepared: Prepared) -> Result<Done, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return self
                .run_blocking_gated(0, move |_| read.run_prepared(&prepared).map(|h| h.done))
                .await;
        }
        self.run_write_op(move |db| db.execute_prepared(&prepared))
            .await
    }

    pub async fn run_with(
        &self,
        prepared: Prepared,
        params: Vec<(String, Cell)>,
    ) -> Result<Handle, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return self
                .run_blocking_gated(0, move |_| {
                    let pairs: Vec<(&str, Cell)> =
                        params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                    prepared.run_read_with(&read, &pairs)
                })
                .await;
        }
        self.run_write_op(move |db| {
            let pairs: Vec<(&str, Cell)> =
                params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            prepared.run_with(db, &pairs)
        })
        .await
    }

    pub async fn run_batch(&self, src: impl Into<String>) -> Result<RecordBatch, Error> {
        let src = src.into();
        let len = src.len();
        let read = self.snapshot().await;
        self.run_blocking_gated(len, move |_| read.run_batch(&src))
            .await
    }

    pub async fn run_prepared_batch(&self, prepared: Prepared) -> Result<RecordBatch, Error> {
        let read = self.snapshot().await;
        self.run_blocking_gated(0, move |_| read.run_prepared_batch(&prepared))
            .await
    }

    pub async fn run_group(&self, sources: Vec<String>) -> Result<Handle, Error> {
        let bytes: usize = sources.iter().map(String::len).sum();
        self.run_write_op_sized(bytes, move |db| db.run_group(&sources))
            .await
    }

    /// Execute a synchronous transaction closure on Tokio's blocking thread pool,
    /// committing automatically on `Ok` and rolling back on `Err`.
    pub async fn transaction<T, F>(&self, f: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Transaction<'_>) -> Result<T, Error> + Send + 'static,
    {
        self.run_write_op(move |db| db.transaction_fn(f)).await
    }

    pub async fn checkpoint(&self) -> Result<(), Error> {
        let db = Arc::clone(&self.inner);
        let sync_group = Arc::clone(&self.sync_group);
        let write_epoch = Arc::clone(&self.write_epoch);
        self.run_blocking_gated(0, move |cancelled| {
            let mut g = db.blocking_lock();
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::canceled("async checkpoint cancelled before execution"));
            }
            sync_group.wait_idle();
            let res = g.checkpoint();
            if res.is_ok() {
                sync_group.note_checkpointed();
                write_epoch.fetch_add(1, Ordering::Release);
            }
            res
        })
        .await
    }

    /// Asynchronously flush and compact WAL into snapshot under [`AsyncExecutionLimits`] (`LIN-1312`).
    pub async fn checkpoint_async(&self) -> Result<(), Error> {
        self.checkpoint().await
    }

    pub async fn stats(&self) -> Stats {
        let epoch = self.write_epoch.load(Ordering::Acquire);
        if self.sync_group.is_all_synced()
            && let Ok(guard) = self.cached_reader.read()
            && let Some((cached_epoch, _, ref stats)) = *guard
            && cached_epoch == epoch
        {
            return stats.clone();
        }
        let g = self.inner.lock().await;
        g.stats()
    }

    pub async fn export_wal_since(&self, since: u64) -> Result<Vec<u8>, Error> {
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let g = db.blocking_lock();
            g.export_wal_since(since)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn apply_wal(&self, frames: Vec<u8>) -> Result<usize, Error> {
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let res = g.apply_wal(&frames);
            write_epoch.fetch_add(1, Ordering::Release);
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec(&self, q: Queryable) -> Result<Vec<Row>, Error> {
        q.ensure()?;
        let stmt = q.stmt();
        let bytes = stmt.estimate_bytes();
        let read = self.snapshot().await;
        self.run_blocking_gated(bytes, move |_| Ok(read.run_stmt(stmt)?.into_rows()))
            .await
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        q.ensure()?;
        let stmt = q.stmt();
        let bytes = stmt.estimate_bytes();
        let read = self.snapshot().await;
        self.run_blocking_gated(bytes, move |_| {
            let rows = read.run_stmt(stmt)?.into_rows();
            row::map_rows(&rows)
        })
        .await
    }

    async fn start_bounded_stream(
        &self,
        input_bytes: usize,
        job: CursorJob,
        receipt_tx: Option<oneshot::Sender<StreamCompletionReceipt>>,
    ) -> impl Stream<Item = Result<Row, Error>> + Send + use<> {
        use futures_util::StreamExt;

        let deadline = self.control.limits.timeout.map(|d| Instant::now() + d);
        let rx_stream = match self.control.acquire_slot(input_bytes, deadline).await {
            Err(err) => {
                if let Some(tx) = receipt_tx {
                    let _ = tx.send(StreamCompletionReceipt {
                        yielded_rows: 0,
                        queue_wait_ns: 0,
                        execution_ns: 0,
                        completed_cleanly: false,
                    });
                }
                let (tx, rx) = mpsc::channel::<Vec<Result<Row, Error>>>(1);
                let _ = tx.try_send(vec![Err(err)]);
                ReceiverStream::new(rx)
            }
            Ok((slot, queue_wait_ns)) => {
                let read = self.snapshot().await;
                spawn_bounded_cursor(
                    read,
                    job,
                    Some((slot, queue_wait_ns, deadline)),
                    self.control.limits.max_stream_buffered_rows,
                    receipt_tx,
                )
            }
        };
        rx_stream.flat_map(futures_util::stream::iter)
    }

    /// Stream rows with an attached [`StreamCompletionReceipt`] receiver (`LIN-1299`).
    pub async fn stream_with_receipt<S: Into<String>>(
        &self,
        src: S,
    ) -> (
        impl Stream<Item = Result<Row, Error>> + Send + use<S>,
        oneshot::Receiver<StreamCompletionReceipt>,
    ) {
        let src = src.into();
        let len = src.len();
        let (tx, rx) = oneshot::channel();
        let stream = self
            .start_bounded_stream(len, CursorJob::Src(src), Some(tx))
            .await;
        (stream, rx)
    }

    /// Stream rows for a string query (cursor when the program is a single query).
    pub async fn stream<S: Into<String>>(
        &self,
        src: S,
    ) -> impl Stream<Item = Result<Row, Error>> + Send + use<S> {
        let src = src.into();
        let len = src.len();
        self.start_bounded_stream(len, CursorJob::Src(src), None)
            .await
    }

    pub async fn stream_stmt(
        &self,
        stmt: Stmt,
    ) -> impl Stream<Item = Result<Row, Error>> + Send + use<> {
        let bytes = stmt.estimate_bytes();
        self.start_bounded_stream(bytes, CursorJob::Stmt(stmt), None)
            .await
    }

    pub async fn stream_query(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<Row, Error>> + Send + use<> {
        let checked = q.ensure().map(|()| q.into_query());
        let bytes = match &checked {
            Ok(qq) => qq.estimate_bytes(),
            Err(_) => 0,
        };
        self.start_bounded_stream(bytes, CursorJob::Query(checked), None)
            .await
    }

    pub async fn stream_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<T, Error>> + Send + use<T> {
        let checked = q.ensure().map(|()| q.into_query());
        let bytes = match &checked {
            Ok(qq) => qq.estimate_bytes(),
            Err(_) => 0,
        };
        let s = self
            .start_bounded_stream(bytes, CursorJob::Query(checked), None)
            .await;
        typed_map_stream(s)
    }

    pub async fn reader(&self) -> AsyncReadDb {
        let read = self.snapshot().await;
        AsyncReadDb::new(read)
    }
}

/// Async wrapper around a snapshot [`ReadDb`].
#[derive(Clone)]
pub struct AsyncReadDb {
    inner: ReadDb,
}

impl AsyncReadDb {
    pub fn new(db: ReadDb) -> Self {
        Self { inner: db }
    }

    pub fn inner(&self) -> &ReadDb {
        &self.inner
    }

    pub async fn run(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let db = self.inner.clone();
        spawn_blocking(move || db.run(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    /// Execute multiple read queries concurrently across worker threads on this snapshot.
    pub async fn run_many<I, S>(&self, queries: I) -> Result<Vec<Handle>, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let queries: Vec<String> = queries.into_iter().map(Into::into).collect();
        if queries.is_empty() {
            return Ok(Vec::new());
        }
        let db = self.inner.clone();
        spawn_blocking(move || db.run_parallel(queries))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn prepare(&self, src: impl Into<String>) -> Result<Prepared, Error> {
        let src = src.into();
        let db = self.inner.clone();
        spawn_blocking(move || db.prepare(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_prepared(&self, prepared: Prepared) -> Result<Handle, Error> {
        let db = self.inner.clone();
        spawn_blocking(move || db.run_prepared(&prepared))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_with(
        &self,
        prepared: Prepared,
        params: Vec<(String, Cell)>,
    ) -> Result<Handle, Error> {
        let db = self.inner.clone();
        spawn_blocking(move || {
            let pairs: Vec<(&str, Cell)> =
                params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            prepared.run_read_with(&db, &pairs)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_batch(&self, src: impl Into<String>) -> Result<RecordBatch, Error> {
        let src = src.into();
        let db = self.inner.clone();
        spawn_blocking(move || db.run_batch(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_prepared_batch(&self, prepared: Prepared) -> Result<RecordBatch, Error> {
        let db = self.inner.clone();
        spawn_blocking(move || db.run_prepared_batch(&prepared))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn scalar(&self, src: impl Into<String>) -> Result<Cell, Error> {
        self.run(src).await?.scalar()
    }

    pub async fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        let db = self.inner.clone();
        spawn_blocking(move || db.run_stmt(stmt))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec(&self, q: Queryable) -> Result<Vec<Row>, Error> {
        q.ensure()?;
        Ok(self.run_stmt(q.stmt()).await?.into_rows())
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        q.ensure()?;
        let stmt = q.stmt();
        let db = self.inner.clone();
        spawn_blocking(move || {
            let rows = db.run_stmt(stmt)?.into_rows();
            row::map_rows(&rows)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn stream(
        &self,
        src: impl Into<String>,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Src(src.into()))
    }

    pub async fn stream_stmt(&self, stmt: Stmt) -> impl Stream<Item = Result<Row, Error>> + Send {
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Stmt(stmt))
    }

    pub async fn stream_query(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        let checked = q.ensure().map(|()| q.into_query());
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Query(checked))
    }

    pub async fn stream_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        let checked = q.ensure().map(|()| q.into_query());
        typed_map_stream(spawn_cursor_or_run(
            self.inner.clone(),
            CursorJob::Query(checked),
        ))
    }
}

impl Queryable {
    pub async fn first_async(self, db: &AsyncDb) -> Result<Row, Error> {
        let rows = db.to_vec(self.take(1)).await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| Error::runtime("no rows"))
    }

    pub async fn first_or_async(self, db: &AsyncDb) -> Result<Option<Row>, Error> {
        Ok(db.to_vec(self.take(1)).await?.into_iter().next())
    }

    pub async fn first_typed_async<T: FromRow + Send>(self, db: &AsyncDb) -> Result<T, Error> {
        T::from_row(&self.first_async(db).await?)
    }

    pub async fn scalar_async(self, db: &AsyncDb) -> Result<crate::store::Cell, Error> {
        let row = self.first_async(db).await?;
        row.values()
            .next()
            .cloned()
            .ok_or_else(|| Error::runtime("empty row"))
    }

    pub async fn to_vec_async(self, db: &AsyncDb) -> Result<Vec<Row>, Error> {
        db.to_vec(self).await
    }

    pub async fn to_vec_typed_async<T: FromRow + Send + 'static>(
        self,
        db: &AsyncDb,
    ) -> Result<Vec<T>, Error> {
        db.to_vec_typed(self).await
    }

    pub async fn to_vec_async_read(self, db: &AsyncReadDb) -> Result<Vec<Row>, Error> {
        db.to_vec(self).await
    }

    pub async fn to_vec_typed_async_read<T: FromRow + Send + 'static>(
        self,
        db: &AsyncReadDb,
    ) -> Result<Vec<T>, Error> {
        db.to_vec_typed(self).await
    }

    pub async fn to_stream_async(
        self,
        db: &AsyncDb,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        db.stream_query(self).await
    }

    pub async fn to_stream_typed_async<T: FromRow + Send + 'static>(
        self,
        db: &AsyncDb,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        db.stream_typed::<T>(self).await
    }

    pub async fn to_stream_async_read(
        self,
        db: &AsyncReadDb,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        db.stream_query(self).await
    }

    pub async fn to_stream_typed_async_read<T: FromRow + Send + 'static>(
        self,
        db: &AsyncReadDb,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        db.stream_typed::<T>(self).await
    }
}

enum CursorJob {
    Src(String),
    Stmt(Stmt),
    Query(Result<Query, Error>),
}

const STREAM_INITIAL_CHUNK: usize = 64;
const STREAM_MID_CHUNK: usize = 256;
const STREAM_MAX_CHUNK: usize = 512;

#[inline]
fn next_stream_chunk_size(cur: usize) -> usize {
    if cur < STREAM_MID_CHUNK {
        STREAM_MID_CHUNK
    } else {
        STREAM_MAX_CHUNK
    }
}

fn spawn_cursor_or_run(
    read: ReadDb,
    job: CursorJob,
) -> impl Stream<Item = Result<Row, Error>> + Send {
    use futures_util::StreamExt;
    spawn_bounded_cursor(
        read,
        job,
        None,
        AsyncExecutionLimits::default().max_stream_buffered_rows,
        None,
    )
    .flat_map(futures_util::stream::iter)
}

struct StreamProducerState {
    tx: mpsc::Sender<Vec<Result<Row, Error>>>,
    deadline: Option<Instant>,
    initial_chunk: usize,
    max_chunk: usize,
    yielded_rows: usize,
    timed_out: bool,
    dropped_early: bool,
    had_err: bool,
}

impl StreamProducerState {
    #[inline]
    fn deadline_reached(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    fn send_batch(&mut self, batch: Vec<Result<Row, Error>>) -> Result<(), bool> {
        if let Some(dl) = self.deadline {
            let mut pending = batch;
            loop {
                if Instant::now() >= dl {
                    return Err(true);
                }
                match self.tx.try_send(pending) {
                    Ok(()) => return Ok(()),
                    Err(mpsc::error::TrySendError::Closed(_)) => return Err(false),
                    Err(mpsc::error::TrySendError::Full(returned)) => {
                        pending = returned;
                        let rem = dl.saturating_duration_since(Instant::now());
                        if rem.is_zero() {
                            return Err(true);
                        }
                        std::thread::sleep(rem.min(Duration::from_millis(1)));
                    }
                }
            }
        }
        self.tx.blocking_send(batch).map_err(|_| false)
    }

    fn send_err(&mut self, e: Error) {
        self.had_err = true;
        let _ = self.tx.blocking_send(vec![Err(e)]);
    }

    fn send_rows(&mut self, rows: Vec<Row>) {
        let mut iter = rows.into_iter().peekable();
        let mut chunk_cap = self.initial_chunk;
        while iter.peek().is_some() {
            if self.deadline_reached() {
                self.timed_out = true;
                let _ = self.tx.try_send(vec![Err(Error::deadline_exceeded(
                    "async stream execution timed out",
                ))]);
                return;
            }
            let batch: Vec<Result<Row, Error>> = iter.by_ref().take(chunk_cap).map(Ok).collect();
            let batch_len = batch.len();
            match self.send_batch(batch) {
                Ok(()) => self.yielded_rows += batch_len,
                Err(true) => {
                    self.timed_out = true;
                    let _ = self.tx.try_send(vec![Err(Error::deadline_exceeded(
                        "async stream execution timed out under backpressure",
                    ))]);
                    return;
                }
                Err(false) => {
                    self.dropped_early = true;
                    return;
                }
            }
            chunk_cap = next_stream_chunk_size(chunk_cap).min(self.max_chunk);
        }
    }

    fn stream_cursor(&mut self, read: &ReadDb, q: &Query) {
        let mut cur = match read.cursor(q) {
            Ok(c) => c,
            Err(e) => {
                self.send_err(e);
                return;
            }
        };
        let mut chunk_cap = self.initial_chunk;
        let mut chunk = Vec::with_capacity(chunk_cap);
        while let Some(item) = cur.next() {
            if self.deadline_reached() {
                self.timed_out = true;
                let _ = cur.cancel_with_reason("async stream timeout");
                let _ = self.tx.try_send(vec![Err(Error::deadline_exceeded(
                    "async stream execution timed out",
                ))]);
                return;
            }
            if self.tx.is_closed() {
                self.dropped_early = true;
                let _ = cur.cancel_with_reason("async stream dropped by consumer");
                return;
            }
            let is_err = item.is_err();
            if is_err {
                self.had_err = true;
            }
            chunk.push(item);
            if chunk.len() >= chunk_cap || is_err {
                let ok_in_chunk = chunk.iter().filter(|r| r.is_ok()).count();
                let next_cap = next_stream_chunk_size(chunk_cap).min(self.max_chunk);
                let batch = std::mem::replace(&mut chunk, Vec::with_capacity(next_cap));
                chunk_cap = next_cap;
                match self.send_batch(batch) {
                    Ok(()) => {
                        self.yielded_rows += ok_in_chunk;
                        if is_err {
                            let _ = cur.cancel_with_reason("stream error");
                            return;
                        }
                    }
                    Err(true) => {
                        self.timed_out = true;
                        let _ = cur.cancel_with_reason("async stream timeout");
                        let _ = self.tx.try_send(vec![Err(Error::deadline_exceeded(
                            "async stream execution timed out under backpressure",
                        ))]);
                        return;
                    }
                    Err(false) => {
                        self.dropped_early = true;
                        let _ = cur.cancel_with_reason("async stream dropped by consumer");
                        return;
                    }
                }
            }
        }
        if !chunk.is_empty() {
            let ok_in_chunk = chunk.iter().filter(|r| r.is_ok()).count();
            match self.send_batch(chunk) {
                Ok(()) => self.yielded_rows += ok_in_chunk,
                Err(true) => self.timed_out = true,
                Err(false) => self.dropped_early = true,
            }
        }
    }
}

fn spawn_bounded_cursor(
    read: ReadDb,
    job: CursorJob,
    mut gated: Option<(ActiveSlot, u64, Option<Instant>)>,
    max_buffered_rows: usize,
    receipt_tx: Option<oneshot::Sender<StreamCompletionReceipt>>,
) -> ReceiverStream<Vec<Result<Row, Error>>> {
    let cap_rows = max_buffered_rows.max(1);
    let (channel_slots, initial_chunk, max_chunk) = if cap_rows <= STREAM_INITIAL_CHUNK {
        (cap_rows, 1usize, 1usize)
    } else {
        let slots = (cap_rows / STREAM_INITIAL_CHUNK).clamp(1, 16);
        let per_msg_max = (cap_rows / slots).clamp(1, STREAM_MAX_CHUNK);
        let init = STREAM_INITIAL_CHUNK.min(per_msg_max);
        (slots, init, per_msg_max)
    };

    let (tx, rx) = mpsc::channel::<Vec<Result<Row, Error>>>(channel_slots);
    spawn_blocking(move || {
        let exec_start = Instant::now();
        let deadline = gated.as_ref().and_then(|(_, _, dl)| *dl);
        let queue_wait_ns = gated.as_ref().map_or(0, |(_, w, _)| *w);
        let mut st = StreamProducerState {
            tx,
            deadline,
            initial_chunk,
            max_chunk,
            yielded_rows: 0,
            timed_out: false,
            dropped_early: false,
            had_err: false,
        };

        match job {
            CursorJob::Query(Ok(q)) => st.stream_cursor(&read, &q),
            CursorJob::Query(Err(e)) => st.send_err(e),
            CursorJob::Stmt(Stmt::Query(q)) => st.stream_cursor(&read, &q),
            CursorJob::Stmt(other) => match read.run_stmt(other) {
                Ok(h) => st.send_rows(h.into_rows()),
                Err(e) => st.send_err(e),
            },
            CursorJob::Src(src) => match read.prepare(&src) {
                Ok(prep) => {
                    if !prep.writes
                        && let [Stmt::Query(q)] = &prep.stmts[..]
                    {
                        st.stream_cursor(&read, q);
                    } else {
                        match read.run_prepared(&prep) {
                            Ok(h) => st.send_rows(h.into_rows()),
                            Err(e) => st.send_err(e),
                        }
                    }
                }
                Err(e) => st.send_err(e),
            },
        }

        let execution_ns = exec_start.elapsed().as_nanos() as u64;
        if let Some((ref mut slot, _, _)) = gated {
            if st.timed_out {
                slot.settle_timeout();
            } else if st.dropped_early {
                slot.settle_dropped();
            } else {
                slot.settle_completed();
            }
        }
        if let Some(rtx) = receipt_tx {
            let _ = rtx.send(StreamCompletionReceipt {
                yielded_rows: st.yielded_rows,
                queue_wait_ns,
                execution_ns,
                completed_cleanly: !st.timed_out && !st.dropped_early && !st.had_err,
            });
        }
    });

    ReceiverStream::new(rx)
}

fn typed_map_stream<S, T>(rows: S) -> impl Stream<Item = Result<T, Error>> + Send
where
    S: Stream<Item = Result<Row, Error>> + Send,
    T: FromRow + Send + 'static,
{
    use futures_util::StreamExt;
    rows.map(|item| item.and_then(|row| T::from_row(&row)))
}
