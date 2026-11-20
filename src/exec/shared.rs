//! Thread-safe concurrent writer handle [`SharedDb`] and interactive [`SharedTransaction`]
//! with pipelined WAL group-commit durability (`SyncMode::Full`).
//!
//! Each writer thread acquires the execution lock only for fast in-memory mutation and
//! OS page-cache WAL append (`write_all_vectored`), releases the execution lock immediately,
//! and joins a leader-follower [`WalSyncGroup`] barrier so concurrent interactive
//! transactions (`shared.transaction()? ... tx.commit()?`) and single statements share a
//! single `F_BARRIERFSYNC` / `fdatasync` flush without weakening crash durability.

use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};

use crate::batch::RecordBatch;
use crate::catalog::Catalog;
use crate::error::Error;
use crate::persist::{OpenConfig, Pack, SyncMode};
use crate::store::Cell;

use super::prepared::Prepared;
use super::tx::{Transaction, Undo};
use super::{Db, Done, Handle, ReadDb, Stats};

struct SyncState {
    written_seq: u64,
    synced_seq: u64,
    syncing: bool,
    sync_calls: u64,
    last_error: Option<(u64, u64, Error)>,
}

struct WalSyncGroup {
    state: Mutex<SyncState>,
    cv: Condvar,
}

impl WalSyncGroup {
    fn new() -> Self {
        Self {
            state: Mutex::new(SyncState {
                written_seq: 0,
                synced_seq: 0,
                syncing: false,
                sync_calls: 0,
                last_error: None,
            }),
            cv: Condvar::new(),
        }
    }

    /// Record a deferred `SyncMode::Full` WAL append while holding `db` mutex.
    fn note_unsynced_write(&self) -> u64 {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.written_seq += 1;
        g.written_seq
    }

    /// Mark all writes up to `written_seq` as already durable (e.g. after a full checkpoint).
    fn note_checkpointed(&self) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.synced_seq = g.written_seq;
        self.cv.notify_all();
    }

    /// Wait until no background `durable_sync` is mid-flight (called before log compaction).
    fn wait_idle(&self) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while g.syncing {
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Wait until all previously written WAL frames have been synced (used by durable readers).
    fn wait_all_written_synced(&self, sync_file: Option<File>) -> Result<u64, Error> {
        let target = {
            let g = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if g.synced_seq >= g.written_seq {
                return Ok(g.synced_seq);
            }
            g.written_seq
        };
        if let Some(file) = sync_file {
            self.wait_synced(target, file)?;
        } else {
            let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
            while g.synced_seq < target && g.syncing {
                g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
            }
        }
        let g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Ok(g.synced_seq)
    }

    /// Leader-follower group sync barrier: guarantees `target_seq` is durably flushed on disk
    /// before returning `Ok(())`.
    fn wait_synced(&self, target_seq: u64, sync_file: File) -> Result<(), Error> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if g.synced_seq >= target_seq {
                if let Some((lo, hi, ref err)) = g.last_error
                    && target_seq > lo
                    && target_seq <= hi
                {
                    return Err(err.clone());
                }
                return Ok(());
            }
            if g.syncing {
                g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
                continue;
            }
            g.syncing = true;
            let batch_lo = g.synced_seq;
            let batch_hi = g.written_seq.max(target_seq);
            drop(g);

            let sync_res = crate::persist::durable_sync(&sync_file)
                .map_err(|e| Error::runtime(format!("group commit sync failed: {e}")));

            g = self.state.lock().unwrap_or_else(|e| e.into_inner());
            g.syncing = false;
            g.sync_calls += 1;
            match sync_res {
                Ok(()) => {
                    g.synced_seq = g.synced_seq.max(batch_hi);
                    if g.last_error.as_ref().is_some_and(|(_, hi, _)| *hi <= batch_lo) {
                        g.last_error = None;
                    }
                }
                Err(err) => {
                    g.synced_seq = g.synced_seq.max(batch_hi);
                    g.last_error = Some((batch_lo, batch_hi, err));
                }
            }
            self.cv.notify_all();
        }
    }

    fn sync_calls(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sync_calls
    }
}

struct SharedDbInner {
    db: Mutex<Db>,
    sync_group: WalSyncGroup,
    cached_reader: RwLock<Option<(u64, u64, ReadDb)>>,
}

/// Concurrent multi-thread handle over [`Db`] supporting pipelined group commit for both
/// interactive transactions ([`SharedDb::transaction`]) and single statements
/// ([`SharedDb::execute`], [`SharedDb::run`]), plus lock-free snapshot reads
/// ([`SharedDb::reader`], [`SharedDb::run_read`]).
#[derive(Clone)]
pub struct SharedDb {
    inner: Arc<SharedDbInner>,
}

impl SharedDb {
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(SharedDbInner {
                db: Mutex::new(db),
                sync_group: WalSyncGroup::new(),
                cached_reader: RwLock::new(None),
            }),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self::new(Db::open(path)?))
    }

    pub fn open_with(path: impl AsRef<Path>, opts: impl Into<OpenConfig>) -> Result<Self, Error> {
        Ok(Self::new(Db::open_with(path, opts)?))
    }

    /// Number of physical `durable_sync` barriers executed across all group-committed writes.
    pub fn group_sync_count(&self) -> u64 {
        self.inner.sync_group.sync_calls()
    }

    fn lock_db(&self) -> Result<MutexGuard<'_, Db>, Error> {
        self.inner
            .db
            .lock()
            .map_err(|_| Error::runtime("SharedDb lock poisoned"))
    }

    /// Prepare an interactive ACID [`SharedTransaction`] whose [`SharedTransaction::commit`]
    /// automatically coalesces its WAL `fsync` with concurrent writers.
    pub fn transaction(&self) -> Result<SharedTransaction<'_>, Error> {
        let mut guard = self.lock_db()?;
        if !guard.store.fts_deferred_doc_swaps.is_empty() {
            guard.store.flush_fts_deferred_docs();
        }
        let base_gen = guard.store.r#gen;
        let base_next_id = guard.store.next_id;
        Ok(SharedTransaction {
            shared: self,
            guard: Some(guard),
            base_gen,
            base_next_id,
            committed: false,
            undo: Undo::None,
            cat_backup: None,
            packs: Vec::new(),
            schema_changed: false,
            disk_savepoint: false,
            mutation_count: 0,
            journal_bytes: 0,
        })
    }

    /// Run a closure inside an interactive group-committed transaction, committing on `Ok`
    /// and rolling back on `Err` or panic.
    pub fn transaction_fn<T, F>(&self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&mut SharedTransaction<'_>) -> Result<T, Error>,
    {
        let mut tx = self.transaction()?;
        let out = f(&mut tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Execute a statement on the writer, coalescing `SyncMode::Full` WAL flushes across
    /// concurrent threads.
    pub fn run(&self, src: &str) -> Result<Handle, Error> {
        self.run_write_op(|db| db.run(src))
    }

    /// Execute a write statement without materializing output rows, coalescing `SyncMode::Full`
    /// WAL flushes across concurrent threads.
    pub fn execute(&self, src: &str) -> Result<Done, Error> {
        self.run_write_op(|db| db.execute(src))
    }

    pub fn run_prepared(&self, prep: &Prepared) -> Result<Handle, Error> {
        if !prep.writes {
            return self.reader()?.run_prepared(prep);
        }
        self.run_write_op(|db| db.run_prepared(prep))
    }

    pub fn execute_prepared(&self, prep: &Prepared) -> Result<Done, Error> {
        if !prep.writes {
            return self.reader()?.run_prepared(prep).map(|h| h.done);
        }
        self.run_write_op(|db| db.execute_prepared(prep))
    }

    fn run_write_op<T>(&self, op: impl FnOnce(&mut Db) -> Result<T, Error>) -> Result<T, Error> {
        let (res, pending_sync) = {
            let mut db = self.lock_db()?;
            let prev_gen = db.store.r#gen;
            let prev_log_bytes = db.persist.as_ref().map(|p| p.log_bytes).unwrap_or(0);
            let will_auto_ckpt = db.persist.as_ref().is_some_and(|p| {
                (p.writes_since_snapshot + 1 >= crate::persist::SNAPSHOT_EVERY
                    && p.log_bytes >= 4 * 1024 * 1024)
                    || p.writes_since_snapshot + 1 >= 4096
                    || p.log_bytes > db.quotas.max_log_bytes
            });
            if will_auto_ckpt {
                self.inner.sync_group.wait_idle();
            }
            let defer_sync = !will_auto_ckpt
                && db.persist.as_mut().is_some_and(|p| {
                    if p.sync == SyncMode::Full {
                        p.sync = SyncMode::Normal;
                        true
                    } else {
                        false
                    }
                });
            let out = op(&mut db);
            if defer_sync && let Some(p) = db.persist.as_mut() {
                p.sync = SyncMode::Full;
            }
            let pending = match (&out, defer_sync, db.persist.as_ref()) {
                (Ok(_), true, Some(p)) if db.store.r#gen > prev_gen || p.log_bytes != prev_log_bytes => {
                    if p.log_bytes == 0 {
                        self.inner.sync_group.note_checkpointed();
                        None
                    } else {
                        let f = p.log.try_clone().map_err(crate::persist::io_err)?;
                        let seq = self.inner.sync_group.note_unsynced_write();
                        Some((seq, f))
                    }
                }
                (Ok(_), false, Some(p)) if p.log_bytes == 0 && prev_log_bytes > 0 => {
                    self.inner.sync_group.note_checkpointed();
                    None
                }
                _ => None,
            };
            (out, pending)
        };
        let val = res?;
        if let Some((seq, file)) = pending_sync {
            self.inner.sync_group.wait_synced(seq, file)?;
        }
        Ok(val)
    }

    /// Obtain a durable [`ReadDb`] snapshot (shared across concurrent readers without holding
    /// the writer lock).
    pub fn reader(&self) -> Result<ReadDb, Error> {
        // Fast path when all writes are synced and cached reader matches current generation.
        {
            let sg = self
                .inner
                .sync_group
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if sg.synced_seq >= sg.written_seq
                && let Ok(rg) = self.inner.cached_reader.read()
                && let Some((cached_seq, _, ref rdb)) = *rg
                && cached_seq == sg.synced_seq
            {
                return Ok(rdb.clone());
            }
        }
        let mut db = self.lock_db()?;
        let sync_file = db
            .persist
            .as_ref()
            .and_then(|p| (p.sync == SyncMode::Full && p.log_bytes > 0).then(|| p.log.try_clone().ok()).flatten());
        let cur_gen = db.store.r#gen;
        let rdb = db.reader();
        drop(db);
        let synced_seq = self.inner.sync_group.wait_all_written_synced(sync_file)?;
        if let Ok(mut wg) = self.inner.cached_reader.write() {
            *wg = Some((synced_seq, cur_gen, rdb.clone()));
        }
        Ok(rdb)
    }

    pub fn run_read(&self, src: &str) -> Result<Handle, Error> {
        self.reader()?.run(src)
    }

    pub fn run_batch(&self, src: &str) -> Result<RecordBatch, Error> {
        self.reader()?.run_batch(src)
    }

    pub fn prepare(&self, src: &str) -> Result<Prepared, Error> {
        self.reader()?.prepare(src)
    }

    pub fn checkpoint(&self) -> Result<(), Error> {
        let mut db = self.lock_db()?;
        self.inner.sync_group.wait_idle();
        db.checkpoint()?;
        self.inner.sync_group.note_checkpointed();
        Ok(())
    }

    pub fn close(&self) -> Result<(), Error> {
        let mut db = self.lock_db()?;
        self.inner.sync_group.wait_idle();
        db.close()?;
        self.inner.sync_group.note_checkpointed();
        Ok(())
    }

    pub fn stats(&self) -> Result<Stats, Error> {
        Ok(self.lock_db()?.stats())
    }
}

/// Interactive ACID transaction created by [`SharedDb::transaction`].
///
/// Supports read-your-own-writes (`run`, `execute`, `run_prepared`, `run_with`), automatic
/// rollback on `Drop` if uncommitted, and pipelined group-commit WAL durability on [`Self::commit`].
pub struct SharedTransaction<'a> {
    shared: &'a SharedDb,
    guard: Option<MutexGuard<'a, Db>>,
    base_gen: u64,
    base_next_id: u64,
    committed: bool,
    undo: Undo,
    cat_backup: Option<Box<Catalog>>,
    packs: Vec<Pack>,
    schema_changed: bool,
    disk_savepoint: bool,
    mutation_count: usize,
    journal_bytes: usize,
}

impl SharedTransaction<'_> {
    fn with_tx<R>(&mut self, f: impl FnOnce(&mut Transaction<'_>) -> R) -> Result<R, Error> {
        let Some(guard) = self.guard.as_mut() else {
            return Err(Error::runtime("transaction already finished"));
        };
        let mut tmp = Transaction {
            db: &mut *guard,
            base_gen: self.base_gen,
            base_next_id: self.base_next_id,
            committed: false,
            undo: std::mem::replace(&mut self.undo, Undo::None),
            cat_backup: self.cat_backup.take(),
            packs: std::mem::take(&mut self.packs),
            schema_changed: self.schema_changed,
            disk_savepoint: self.disk_savepoint,
            mutation_count: self.mutation_count,
            journal_bytes: self.journal_bytes,
        };
        let out = f(&mut tmp);
        self.base_gen = tmp.base_gen;
        self.base_next_id = tmp.base_next_id;
        self.undo = std::mem::replace(&mut tmp.undo, Undo::None);
        self.cat_backup = tmp.cat_backup.take();
        self.packs = std::mem::take(&mut tmp.packs);
        self.schema_changed = tmp.schema_changed;
        self.disk_savepoint = tmp.disk_savepoint;
        self.mutation_count = tmp.mutation_count;
        self.journal_bytes = tmp.journal_bytes;
        // Suppress tmp's Drop rollback; SharedTransaction owns the lifecycle.
        tmp.committed = true;
        Ok(out)
    }

    pub fn run(&mut self, src: &str) -> Result<Handle, Error> {
        self.with_tx(|tx| tx.run(src))?
    }

    pub fn execute(&mut self, src: &str) -> Result<Done, Error> {
        self.with_tx(|tx| tx.execute(src))?
    }

    pub fn scalar(&mut self, src: &str) -> Result<Cell, Error> {
        self.with_tx(|tx| tx.scalar(src))?
    }

    pub fn prepare(&mut self, src: &str) -> Result<Prepared, Error> {
        self.with_tx(|tx| tx.prepare(src))?
    }

    pub fn run_prepared(&mut self, prep: &Prepared) -> Result<Handle, Error> {
        self.with_tx(|tx| tx.run_prepared(prep))?
    }

    pub fn execute_prepared(&mut self, prep: &Prepared) -> Result<Done, Error> {
        self.with_tx(|tx| tx.execute_prepared(prep))?
    }

    pub fn run_with(&mut self, prep: &Prepared, params: &[(&str, Cell)]) -> Result<Handle, Error> {
        self.with_tx(|tx| tx.run_with(prep, params))?
    }

    pub fn execute_with(
        &mut self,
        prep: &Prepared,
        params: &[(&str, Cell)],
    ) -> Result<Done, Error> {
        self.with_tx(|tx| tx.execute_with(prep, params))?
    }

    /// Commit this interactive transaction, releasing the execution lock immediately after
    /// appending to the OS page cache and coalescing the durable WAL barrier (`SyncMode::Full`)
    /// with concurrent writers.
    pub fn commit(mut self) -> Result<(), Error> {
        if self.committed {
            return Ok(());
        }
        let Some(mut guard) = self.guard.take() else {
            return Ok(());
        };
        let had_packs = !self.packs.is_empty();
        let prev_log_bytes = guard.persist.as_ref().map(|p| p.log_bytes).unwrap_or(0);
        let will_auto_ckpt = had_packs
            && guard.persist.as_ref().is_some_and(|p| {
                (p.writes_since_snapshot + 1 >= crate::persist::SNAPSHOT_EVERY
                    && p.log_bytes >= 4 * 1024 * 1024)
                    || p.writes_since_snapshot + 1 >= 4096
                    || p.log_bytes > guard.quotas.max_log_bytes
            });
        if will_auto_ckpt {
            self.shared.inner.sync_group.wait_idle();
        }
        let defer_sync = had_packs
            && !will_auto_ckpt
            && guard.persist.as_mut().is_some_and(|p| {
                if p.sync == SyncMode::Full {
                    p.sync = SyncMode::Normal;
                    true
                } else {
                    false
                }
            });

        let tmp = Transaction {
            db: &mut *guard,
            base_gen: self.base_gen,
            base_next_id: self.base_next_id,
            committed: false,
            undo: std::mem::replace(&mut self.undo, Undo::None),
            cat_backup: self.cat_backup.take(),
            packs: std::mem::take(&mut self.packs),
            schema_changed: self.schema_changed,
            disk_savepoint: self.disk_savepoint,
            mutation_count: self.mutation_count,
            journal_bytes: self.journal_bytes,
        };
        let commit_res = tmp.commit();
        if defer_sync && let Some(p) = guard.persist.as_mut() {
            p.sync = SyncMode::Full;
        }
        self.committed = true;

        let pending_sync = match (&commit_res, defer_sync, guard.persist.as_ref()) {
            (Ok(()), true, Some(p)) => {
                if p.log_bytes == 0 {
                    self.shared.inner.sync_group.note_checkpointed();
                    None
                } else {
                    let f = p.log.try_clone().map_err(crate::persist::io_err)?;
                    let seq = self.shared.inner.sync_group.note_unsynced_write();
                    Some((seq, f))
                }
            }
            (Ok(()), false, Some(p)) if p.log_bytes == 0 && prev_log_bytes > 0 => {
                self.shared.inner.sync_group.note_checkpointed();
                None
            }
            _ => None,
        };
        // Release execution lock BEFORE durable_sync so concurrent writers pipeline behind us.
        drop(guard);

        commit_res?;
        if let Some((seq, file)) = pending_sync {
            self.shared.inner.sync_group.wait_synced(seq, file)?;
        }
        Ok(())
    }

    pub fn rollback(mut self) -> Result<(), Error> {
        if self.committed {
            return Err(Error::runtime("transaction already committed"));
        }
        let Some(mut guard) = self.guard.take() else {
            return Err(Error::runtime("transaction already finished"));
        };
        let tmp = Transaction {
            db: &mut *guard,
            base_gen: self.base_gen,
            base_next_id: self.base_next_id,
            committed: false,
            undo: std::mem::replace(&mut self.undo, Undo::None),
            cat_backup: self.cat_backup.take(),
            packs: std::mem::take(&mut self.packs),
            schema_changed: self.schema_changed,
            disk_savepoint: self.disk_savepoint,
            mutation_count: self.mutation_count,
            journal_bytes: self.journal_bytes,
        };
        self.committed = true;
        tmp.rollback()
    }
}

impl Drop for SharedTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed
            && let Some(mut guard) = self.guard.take()
        {
            let tmp = Transaction {
                db: &mut *guard,
                base_gen: self.base_gen,
                base_next_id: self.base_next_id,
                committed: false,
                undo: std::mem::replace(&mut self.undo, Undo::None),
                cat_backup: self.cat_backup.take(),
                packs: std::mem::take(&mut self.packs),
                schema_changed: self.schema_changed,
                disk_savepoint: self.disk_savepoint,
                mutation_count: self.mutation_count,
                journal_bytes: self.journal_bytes,
            };
            drop(tmp);
            self.committed = true;
        }
    }
}
