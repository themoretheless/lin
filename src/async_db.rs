//! Async façade: Tokio offload + Stream API over Lin's sync engine.
//!
//! Enable with `lin` feature `async`.
//!
//! - `to_vec_*` / `run_*` — await a full result (offload).
//! - `stream_query` / cursor-backed streams — pull via [`QueryCursor`] on a
//!   blocking thread into an mpsc channel (lazy for simple pipelines).

use std::sync::Arc;

use futures_util::stream::Stream;
use tokio::sync::{Mutex, mpsc};
use tokio::task::spawn_blocking;
use tokio_stream::wrappers::ReceiverStream;

use crate::ast::{Query, Stmt};
use crate::error::Error;
use crate::exec::{Db, Handle, Prepared, ReadDb};
use crate::query::Queryable;
use crate::row::{self, FromRow};
use crate::store::Row;

/// Async wrapper around a writer [`Db`] (`Arc<Mutex<Db>>`).
#[derive(Clone)]
pub struct AsyncDb {
    inner: Arc<Mutex<Db>>,
}

impl AsyncDb {
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(Mutex::new(db)),
        }
    }

    pub fn from_arc(inner: Arc<Mutex<Db>>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<Mutex<Db>> {
        &self.inner
    }

    pub async fn run(&self, src: impl Into<String>) -> Result<Handle, Error> {
        let src = src.into();
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            g.run(&src)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            g.run_stmt(stmt)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn run_prepared(&self, prepared: Prepared) -> Result<Handle, Error> {
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            g.run_prepared(&prepared)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec(&self, q: Queryable) -> Result<Vec<Row>, Error> {
        Ok(self.run_stmt(q.stmt()).await?.rows)
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        let rows = self.to_vec(q).await?;
        spawn_blocking(move || row::map_rows(&rows))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    async fn snapshot(&self) -> ReadDb {
        let mut g = self.inner.lock().await;
        g.reader()
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

    pub async fn stream_stmt(
        &self,
        stmt: Stmt,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        let read = self.snapshot().await;
        spawn_cursor_or_run(read, CursorJob::Stmt(stmt))
    }

    pub async fn stream_query(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        let read = self.snapshot().await;
        spawn_cursor_or_run(read, CursorJob::Query(q.into_query()))
    }

    pub async fn stream_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        let read = self.snapshot().await;
        typed_map_stream(spawn_cursor_or_run(
            read,
            CursorJob::Query(q.into_query()),
        ))
    }

    pub fn reader(&self) -> impl std::future::Future<Output = AsyncReadDb> + '_ {
        async move {
            let mut g = self.inner.lock().await;
            AsyncReadDb::new(g.reader())
        }
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

    pub async fn run_stmt(&self, stmt: Stmt) -> Result<Handle, Error> {
        let db = self.inner.clone();
        spawn_blocking(move || db.run_stmt(stmt))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec(&self, q: Queryable) -> Result<Vec<Row>, Error> {
        Ok(self.run_stmt(q.stmt()).await?.rows)
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        let rows = self.to_vec(q).await?;
        spawn_blocking(move || row::map_rows(&rows))
            .await
            .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn stream(
        &self,
        src: impl Into<String>,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Src(src.into()))
    }

    pub async fn stream_stmt(
        &self,
        stmt: Stmt,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Stmt(stmt))
    }

    pub async fn stream_query(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<Row, Error>> + Send {
        spawn_cursor_or_run(self.inner.clone(), CursorJob::Query(q.into_query()))
    }

    pub async fn stream_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> impl Stream<Item = Result<T, Error>> + Send {
        typed_map_stream(spawn_cursor_or_run(
            self.inner.clone(),
            CursorJob::Query(q.into_query()),
        ))
    }
}

impl Queryable {
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
    Query(Query),
}

fn spawn_cursor_or_run(
    read: ReadDb,
    job: CursorJob,
) -> impl Stream<Item = Result<Row, Error>> + Send {
    let (tx, rx) = mpsc::channel::<Result<Row, Error>>(64);
    spawn_blocking(move || {
        let send = |item: Result<Row, Error>| tx.blocking_send(item).is_ok();
        let q = match job {
            CursorJob::Query(q) => q,
            CursorJob::Stmt(Stmt::Query(q)) => q,
            CursorJob::Stmt(other) => {
                match read.run_stmt(other) {
                    Ok(h) => {
                        for row in h.rows {
                            if !send(Ok(row)) {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = send(Err(e));
                    }
                }
                return;
            }
            CursorJob::Src(src) => match crate::parse::parse_program(&src) {
                Ok(stmts) if stmts.len() == 1 => match stmts.into_iter().next().unwrap() {
                    Stmt::Query(q) => q,
                    other => {
                        match read.run_stmt(other) {
                            Ok(h) => {
                                for row in h.rows {
                                    if !send(Ok(row)) {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = send(Err(e));
                            }
                        }
                        return;
                    }
                },
                Ok(_) => {
                    match read.run(&src) {
                        Ok(h) => {
                            for row in h.rows {
                                if !send(Ok(row)) {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = send(Err(e));
                        }
                    }
                    return;
                }
                Err(e) => {
                    let _ = send(Err(e));
                    return;
                }
            },
        };

        let mut cur = match read.cursor(&q) {
            Ok(c) => c,
            Err(e) => {
                let _ = send(Err(e));
                return;
            }
        };
        for item in cur.by_ref() {
            if !send(item) {
                break;
            }
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
