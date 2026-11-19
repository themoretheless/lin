//! Cold-tier lifecycle on [`Db`]: object-store push, hydration and remote runs.
//! Lives with the executor so the disk engine does not depend on it.

use std::path::Path;
use std::sync::Arc;

use super::Db;
use crate::disk::DiskEngine;
use crate::disk::remote::*;
use crate::error::Error;

impl Db {
    /// Push cold disk segments and portable metadata snapshot to an object store.
    pub fn push_cold_to_object_store(
        &mut self,
        store: &dyn ColdObjectStore,
        prefix: &str,
        prune_remote_orphans: bool,
    ) -> Result<RemoteSyncStats, Error> {
        let needs_ckpt = self.persist.as_ref().is_some_and(|p| {
            p.writes_since_snapshot > 0 || (p.cold && !crate::disk::is_disk_dir(&p.dir))
        });
        if needs_ckpt {
            let _ = self.checkpoint();
        }
        let snap_bytes = self.capture_snapshot_wire_bytes()?;
        if let Some(eng_lock) = self.cold_disk_engine_lock() {
            let mut eng = eng_lock
                .write()
                .map_err(|_| Error::runtime("cold disk engine lock poisoned"))?;
            return eng.push_to_object_store_with_snapshot(
                store,
                prefix,
                prune_remote_orphans,
                Some(&snap_bytes),
            );
        }
        if let Some(p) = self
            .persist
            .as_ref()
            .filter(|p| p.cold && crate::disk::is_disk_dir(&p.dir))
        {
            let mut eng = DiskEngine::open(&p.dir, false)?;
            return eng.push_to_object_store_with_snapshot(
                store,
                prefix,
                prune_remote_orphans,
                Some(&snap_bytes),
            );
        }
        Err(Error::runtime("database has no cold DiskEngine attached"))
    }

    /// Open a database from an object-store cold tier (`lazy` leaves tables unhydrated until queried).
    pub fn open_cold_from_object_store(
        local_cache_dir: impl AsRef<Path>,
        store: Arc<dyn ColdObjectStore>,
        prefix: &str,
        cfg: RemoteCacheConfig,
    ) -> Result<Self, Error> {
        let dir = local_cache_dir.as_ref();
        let (eng, snap_bytes) =
            DiskEngine::open_from_object_store_with_snapshot(dir, store, prefix, cfg)?;
        Self::from_cold_engine_and_snapshot(dir, eng, snap_bytes.as_deref(), crate::Quotas::default())
    }

    /// Explicitly hydrate a lazy remote cold collection and refresh cached readers.
    pub fn hydrate_cold_collection(&mut self, table: &str) -> Result<usize, Error> {
        self.hydrate_cold_collection_inner(table)
    }

    /// Run a Kusto/Lin statement or query on an object-store-backed `Db`, auto-hydrating lazy cold tables referenced by the query.
    pub fn run_remote(&mut self, src: &str) -> Result<crate::Handle, Error> {
        if let Ok(crate::Stmt::Query(q)) = crate::parse(src) {
            let _ = self.hydrate_query_cold_tables(&q)?;
        }
        self.run(src)
    }

    /// Evict least-recently-used hydrated cold tables (at whole-table granularity) down to `max_bytes`.
    pub fn evict_cold_cache_to(&mut self, max_bytes: u64) -> Result<u64, Error> {
        self.evict_cold_cache_inner(max_bytes)
    }

    /// Total bytes of locally hydrated cold segments in this `Db`.
    pub fn cold_local_cache_bytes(&self) -> u64 {
        self.cold_local_cache_bytes_inner()
    }
}
