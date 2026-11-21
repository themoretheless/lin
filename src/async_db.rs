//! Async façade: Tokio offload, concurrent snapshot readers, and Stream API over Lin's sync engine.
//!
//! Enable with `lin` feature `async`.
//!
//! - `to_vec_*` / `run_*` — await a full result (offload).
//! - `run_read` / `run_many_read` / `to_vec` / `to_vec_typed` — execute read queries
//!   concurrently on a cached [`ReadDb`] snapshot without serializing readers on the writer mutex.
//! - `stream_query` / cursor-backed streams — pull via [`QueryCursor`] on a
//!   blocking thread into an mpsc channel (lazy for simple pipelines).

use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use futures_util::stream::Stream;
use tokio::sync::{Mutex, mpsc};
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

/// Async wrapper around a writer [`Db`] (`Arc<Mutex<Db>>`) with a lock-free/RwLock-cached
/// concurrent [`ReadDb`] snapshot so multiple async readers can execute in parallel across
/// Tokio's blocking pool without contending on the writer mutex.
#[derive(Clone)]
pub struct AsyncDb {
    inner: Arc<Mutex<Db>>,
    write_epoch: Arc<AtomicU64>,
    cached_reader: Arc<RwLock<Option<(u64, ReadDb, Stats)>>>,
    sync_group: Arc<WalSyncGroup>,
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
        }
    }

    pub fn inner(&self) -> &Arc<Mutex<Db>> {
        &self.inner
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
    async fn run_write_op<T, F>(&self, op: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Db) -> Result<T, Error> + Send + 'static,
    {
        let db = Arc::clone(&self.inner);
        let sync_group = Arc::clone(&self.sync_group);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let (out, pending_sync) = {
                let mut g = db.blocking_lock();
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
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    /// Run a statement on the writer. Like [`Self::execute`], concurrent calls share one
    /// commit group and one WAL sync.
    pub async fn run(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        self.run_write_op(move |db| db.run(&src)).await
    }

    /// Execute a read-only query concurrently against a cached [`ReadDb`] snapshot.
    /// Multiple `run_read` calls run in parallel on Tokio's blocking pool without blocking each other.
    pub async fn run_read(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let read = self.snapshot().await;
        spawn_blocking(move || read.run(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
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
        let read = self.snapshot().await;
        spawn_blocking(move || read.run_parallel(queries))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    /// Execute a write. Concurrent calls (of this method, [`Self::execute_prepared`], and
    /// [`Self::transaction`]) pipeline WAL appends and share [`WalSyncGroup`] barriers so N
    /// concurrent durable writes coalesce into shared `fsync`s instead of N serial flushes.
    pub async fn execute(&self, src: impl Into<String>) -> Result<Done, Error> {
        let src = src.into();
        self.run_write_op(move |db| db.execute(&src)).await
    }

    pub async fn scalar(&self, src: impl Into<String>) -> Result<Cell, Error> {
        self.run_read(src).await?.scalar()
    }

    pub async fn prepare(&self, src: impl Into<String>) -> Result<Prepared, Error> {
        let src = src.into();
        let read = self.snapshot().await;
        spawn_blocking(move || read.prepare(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        self.run_write_op(move |db| db.run_stmt(stmt)).await
    }

    pub async fn run_prepared(&self, prepared: Prepared) -> Result<Handle, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return spawn_blocking(move || read.run_prepared(&prepared))
                .await
                .map_err(|e| Error::runtime(format!("async join: {e}")))?;
        }
        self.run_write_op(move |db| db.run_prepared(&prepared)).await
    }

    pub async fn execute_prepared(&self, prepared: Prepared) -> Result<Done, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return spawn_blocking(move || read.run_prepared(&prepared).map(|h| h.done))
                .await
                .map_err(|e| Error::runtime(format!("async join: {e}")))?;
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
            return spawn_blocking(move || {
                let pairs: Vec<(&str, Cell)> =
                    params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                prepared.run_read_with(&read, &pairs)
            })
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?;
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
        let read = self.snapshot().await;
        spawn_blocking(move || read.run_batch(&src))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_prepared_batch(&self, prepared: Prepared) -> Result<RecordBatch, Error> {
        let read = self.snapshot().await;
        spawn_blocking(move || read.run_prepared_batch(&prepared))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_group(&self, sources: Vec<String>) -> Result<Handle, Error> {
        self.run_write_op(move |db| db.run_group(&sources)).await
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
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            sync_group.wait_idle();
            let res = g.checkpoint();
            if res.is_ok() {
                sync_group.note_checkpointed();
                write_epoch.fetch_add(1, Ordering::Release);
            }
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
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
        let read = self.snapshot().await;
        spawn_blocking(move || Ok(read.run_stmt(stmt)?.into_rows()))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        q.ensure()?;
        let stmt = q.stmt();
        let read = self.snapshot().await;
        spawn_blocking(move || {
            let rows = read.run_stmt(stmt)?.into_rows();
            row::map_rows(&rows)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    /// Stream rows for a string query (cursor when the program is a single query).
    pub async fn stream(
        &self,
        src: impl Into<String>,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        let src = src.into();
        let read = self.snapshot().await;
        spawn_cursor_or_run(read, CursorJob::Src(src))
    }

    pub async fn stream_stmt(&self, stmt: Stmt) -> impl Stream<Item = Result<Row, Error>> + Send {
        let read = self.snapshot().await;
        spawn_cursor_or_run(read, CursorJob::Stmt(stmt))
    }

    pub async fn stream_query(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        let checked = q.ensure().map(|()| q.into_query());
        let read = self.snapshot().await;
        spawn_cursor_or_run(read, CursorJob::Query(checked))
    }

    pub async fn stream_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        let checked = q.ensure().map(|()| q.into_query());
        let read = self.snapshot().await;
        typed_map_stream(spawn_cursor_or_run(read, CursorJob::Query(checked)))
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

    let (tx, rx) = mpsc::channel::<Vec<Result<Row, Error>>>(8);
    spawn_blocking(move || {
        let send_rows = |rows: Vec<Row>| {
            let mut iter = rows.into_iter();
            let mut chunk_cap = STREAM_INITIAL_CHUNK;
            loop {
                let batch: Vec<Result<Row, Error>> = iter.by_ref().take(chunk_cap).map(Ok).collect();
                if batch.is_empty() || tx.blocking_send(batch).is_err() {
                    return;
                }
                chunk_cap = next_stream_chunk_size(chunk_cap);
            }
        };
        let send_err = |e: Error| {
            let _ = tx.blocking_send(vec![Err(e)]);
        };
        let stream_cursor = |q: &Query| {
            let mut cur = match read.cursor(q) {
                Ok(c) => c,
                Err(e) => {
                    send_err(e);
                    return;
                }
            };
            let mut chunk_cap = STREAM_INITIAL_CHUNK;
            let mut chunk = Vec::with_capacity(chunk_cap);
            for item in cur.by_ref() {
                let is_err = item.is_err();
                chunk.push(item);
                if chunk.len() >= chunk_cap || is_err {
                    let next_cap = next_stream_chunk_size(chunk_cap);
                    let batch = std::mem::replace(&mut chunk, Vec::with_capacity(next_cap));
                    chunk_cap = next_cap;
                    if tx.blocking_send(batch).is_err() || is_err {
                        return;
                    }
                }
            }
            if !chunk.is_empty() {
                let _ = tx.blocking_send(chunk);
            }
        };

        match job {
            CursorJob::Query(Ok(q)) => stream_cursor(&q),
            CursorJob::Query(Err(e)) => send_err(e),
            CursorJob::Stmt(Stmt::Query(q)) => stream_cursor(&q),
            CursorJob::Stmt(other) => match read.run_stmt(other) {
                Ok(h) => send_rows(h.into_rows()),
                Err(e) => send_err(e),
            },
            CursorJob::Src(src) => match read.prepare(&src) {
                Ok(prep) => {
                    if !prep.writes
                        && let [Stmt::Query(q)] = &prep.stmts[..]
                    {
                        stream_cursor(q);
                    } else {
                        match read.run_prepared(&prep) {
                            Ok(h) => send_rows(h.into_rows()),
                            Err(e) => send_err(e),
                        }
                    }
                }
                Err(e) => send_err(e),
            },
        }
    });

    ReceiverStream::new(rx).flat_map(futures_util::stream::iter)
}

fn typed_map_stream<S, T>(rows: S) -> impl Stream<Item = Result<T, Error>> + Send
where
    S: Stream<Item = Result<Row, Error>> + Send,
    T: FromRow + Send + 'static,
{
    use futures_util::StreamExt;
    rows.map(|item| item.and_then(|row| T::from_row(&row)))
}
