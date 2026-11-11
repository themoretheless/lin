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

    pub async fn execute(&self, src: impl Into<String>) -> Result<crate::exec::Done, Error> {
        let src = src.into();
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            g.execute(&src)
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn scalar(&self, src: impl Into<String>) -> Result<crate::store::Cell, Error> {
        self.run(src).await?.scalar()
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
        q.ensure()?;
        let query = q.into_query();
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let prep = g.prepare_query(&query)?;
            Ok(g.run_prepared(&prep)?.into_rows())
        })
        .await
        .map_err(|e| Error::runtime(format!("async join: {e}")))?
    }

    pub async fn to_vec_typed<T: FromRow + Send + 'static>(
        &self,
        q: Queryable,
    ) -> Result<Vec<T>, Error> {
        q.ensure()?;
        let query = q.into_query();
        let db = Arc::clone(&self.inner);
        spawn_blocking(move || {
            let mut g = db.blocking_lock();
            let prep = g.prepare_query(&query)?;
            let rows = g.run_prepared(&prep)?.into_rows();
            row::map_rows(&rows)
        })
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
        let mut g = self.inner.lock().await;
        AsyncReadDb::new(g.reader())
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
