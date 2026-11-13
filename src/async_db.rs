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
    cached_reader: Arc<RwLock<Option<(u64, ReadDb)>>>,
    /// Writes waiting for the writer lock; whoever gets the lock commits all of them.
    commit_queue: Arc<std::sync::Mutex<Vec<QueuedWrite>>>,
}

struct QueuedWrite {
    work: QueuedWork,
    reply: tokio::sync::oneshot::Sender<Result<Done, Error>>,
}

enum QueuedWork {
    Src(String),
    Prepared(Prepared),
}

impl AsyncDb {
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(Mutex::new(db)),
            write_epoch: Arc::new(AtomicU64::new(1)),
            cached_reader: Arc::new(RwLock::new(None)),
            commit_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn from_arc(inner: Arc<Mutex<Db>>) -> Self {
        Self {
            inner,
            write_epoch: Arc::new(AtomicU64::new(1)),
            cached_reader: Arc::new(RwLock::new(None)),
            commit_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn inner(&self) -> &Arc<Mutex<Db>> {
        &self.inner
    }

    /// Obtain a fresh or cached [`ReadDb`] snapshot synchronized with the latest write epoch.
    /// Concurrent readers after a write share the same snapshot without locking the writer `Mutex`.
    async fn snapshot(&self) -> ReadDb {
        let epoch = self.write_epoch.load(Ordering::Acquire);
        if let Ok(guard) = self.cached_reader.read()
            && let Some((cached_epoch, ref rdb)) = *guard
            && cached_epoch == epoch
        {
            return rdb.clone();
        }
        let mut g = self.inner.lock().await;
        let epoch_now = self.write_epoch.load(Ordering::Acquire);
        if let Ok(guard) = self.cached_reader.read()
            && let Some((cached_epoch, ref rdb)) = *guard
            && cached_epoch == epoch_now
        {
            return rdb.clone();
        }
        let rdb = g.reader();
        if let Ok(mut wguard) = self.cached_reader.write() {
            *wguard = Some((epoch_now, rdb.clone()));
        }
        rdb
    }

    pub async fn run(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let before = (
                g.r#gen(),
                g.store.docs_epoch,
                g.store.collections.len(),
                g.store.edges.len(),
                g.store.facts_len(),
            );
            let res = g.run(&src);
            let after = (
                g.r#gen(),
                g.store.docs_epoch,
                g.store.collections.len(),
                g.store.edges.len(),
                g.store.facts_len(),
            );
            if before != after {
                write_epoch.fetch_add(1, Ordering::Release);
            }
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
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

    /// Execute a write. Concurrent calls (of this method and [`Self::execute_prepared`])
    /// form a commit group: the first task to take the writer lock runs every queued
    /// statement in arrival order (each still atomic on its own) and syncs the WAL once for
    /// all of them, so N concurrent durable writes cost one `fsync` instead of N. A call
    /// returns only after the sync covering it has finished.
    pub async fn execute(&self, src: impl Into<String>) -> Result<Done, Error> {
        self.group_commit(QueuedWork::Src(src.into())).await
    }

    async fn group_commit(&self, work: QueuedWork) -> Result<Done, Error> {
        let (reply, done) = tokio::sync::oneshot::channel();
        self.commit_queue
            .lock()
            .map_err(|_| Error::runtime("async commit queue poisoned"))?
            .push(QueuedWrite { work, reply });
        let db = Arc::clone(&self.inner);
        let queue = Arc::clone(&self.commit_queue);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            // An earlier leader may already have committed this call's statement.
            let batch = match queue.lock() {
                Ok(mut q) => std::mem::take(&mut *q),
                Err(_) => return,
            };
            if batch.is_empty() {
                return;
            }
            let writes: Vec<crate::exec::GroupWrite<'_>> = batch
                .iter()
                .map(|w| match &w.work {
                    QueuedWork::Src(src) => crate::exec::GroupWrite::Src(src),
                    QueuedWork::Prepared(p) => crate::exec::GroupWrite::Prepared(p),
                })
                .collect();
            let results = g.execute_group_sync_mixed(&writes);
            drop(writes);
            write_epoch.fetch_add(1, Ordering::Release);
            drop(g);
            for (w, res) in batch.into_iter().zip(results) {
                let _ = w.reply.send(res);
            }
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?;
        done.await
            .map_err(|_| Error::runtime("async commit group dropped the statement"))?
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
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let before = g.r#gen();
            let res = g.run_stmt(stmt);
            if g.r#gen() != before {
                write_epoch.fetch_add(1, Ordering::Release);
            }
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_prepared(&self, prepared: Prepared) -> Result<Handle, Error> {
        if !prepared.writes {
            let read = self.snapshot().await;
            return spawn_blocking(move || read.run_prepared(&prepared))
                .await
                .map_err(|e| Error::runtime(format!("async join: {e}")))?;
        }
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let res = g.run_prepared(&prepared);
            write_epoch.fetch_add(1, Ordering::Release);
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn execute_prepared(&self, prepared: Prepared) -> Result<Done, Error> {
        if prepared.writes {
            return self.group_commit(QueuedWork::Prepared(prepared)).await;
        }
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let res = g.execute_prepared(&prepared);
            if prepared.writes {
                write_epoch.fetch_add(1, Ordering::Release);
            }
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
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
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let pairs: Vec<(&str, Cell)> =
                params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            let res = prepared.run_with(&mut g, &pairs);
            write_epoch.fetch_add(1, Ordering::Release);
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
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
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
            let res = g.run_group(&refs);
            write_epoch.fetch_add(1, Ordering::Release);
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    /// Execute a synchronous transaction closure on Tokio's blocking thread pool,
    /// committing automatically on `Ok` and rolling back on `Err`.
    pub async fn transaction<T, F>(&self, f: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Transaction<'_>) -> Result<T, Error> + Send + 'static,
    {
        let db = Arc::clone(&self.inner);
        let write_epoch = Arc::clone(&self.write_epoch);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let res = g.transaction_fn(f);
            write_epoch.fetch_add(1, Ordering::Release);
            res
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn checkpoint(&self) -> Result<(), Error> {
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            g.checkpoint()
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn stats(&self) -> Stats {
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

const STREAM_CHUNK_SIZE: usize = 64;

fn spawn_cursor_or_run(
    read: ReadDb,
    job: CursorJob,
) -> impl Stream<Item = Result<Row, Error>> + Send {
    use futures_util::StreamExt;

    let (tx, rx) = mpsc::channel::<Vec<Result<Row, Error>>>(8);
    spawn_blocking(move || {
        let send_rows = |rows: Vec<Row>| {
            for chunk in rows.chunks(STREAM_CHUNK_SIZE) {
                let batch: Vec<Result<Row, Error>> = chunk.iter().cloned().map(Ok).collect();
                if tx.blocking_send(batch).is_err() {
                    return;
                }
            }
        };
        let send_err = |e: Error| {
            let _ = tx.blocking_send(vec![Err(e)]);
        };

        let q = match job {
            CursorJob::Query(Ok(q)) => q,
            CursorJob::Query(Err(e)) => {
                send_err(e);
                return;
            }
            CursorJob::Stmt(Stmt::Query(q)) => q,
            CursorJob::Stmt(other) => {
                match read.run_stmt(other) {
                    Ok(h) => send_rows(h.into_rows()),
                    Err(e) => send_err(e),
                }
                return;
            }
            CursorJob::Src(src) => match crate::parse::parse_program(&src) {
                Ok(stmts) if stmts.len() == 1 => match stmts.into_iter().next().unwrap() {
                    Stmt::Query(q) => q,
                    other => {
                        match read.run_stmt(other) {
                            Ok(h) => send_rows(h.into_rows()),
                            Err(e) => send_err(e),
                        }
                        return;
                    }
                },
                Ok(_) => {
                    match read.run(&src) {
                        Ok(h) => send_rows(h.into_rows()),
                        Err(e) => send_err(e),
                    }
                    return;
                }
                Err(e) => {
                    send_err(e);
                    return;
                }
            },
        };

        let mut cur = match read.cursor(&q) {
            Ok(c) => c,
            Err(e) => {
                send_err(e);
                return;
            }
        };
        let mut chunk = Vec::with_capacity(STREAM_CHUNK_SIZE);
        for item in cur.by_ref() {
            let is_err = item.is_err();
            chunk.push(item);
            if chunk.len() >= STREAM_CHUNK_SIZE || is_err {
                let batch = std::mem::replace(&mut chunk, Vec::with_capacity(STREAM_CHUNK_SIZE));
                if tx.blocking_send(batch).is_err() || is_err {
                    return;
                }
            }
        }
        if !chunk.is_empty() {
            let _ = tx.blocking_send(chunk);
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
