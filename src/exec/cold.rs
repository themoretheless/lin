//! Cold-tier lifecycle on [`Db`]: object-store push, hydration, remote runs, and bootstrap bundles.
//! Lives with the executor so the disk engine does not depend on it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::Db;
use crate::ast::{Query, Source, Step, Stmt};
use crate::disk::DiskEngine;
use crate::disk::remote::*;
use crate::error::Error;
use crate::store::Row;

fn collect_query_collections(query: &Query, out: &mut Vec<String>) {
    if let Source::Collection(c) = &query.source
        && !out.contains(c)
    {
        out.push(c.clone());
    }
    for step in &query.steps {
        match step {
            Step::Join { collection, .. } => {
                if !out.contains(collection) {
                    out.push(collection.clone());
                }
            }
            Step::Union(sub) => collect_query_collections(sub, out),
            _ => {}
        }
    }
}

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
        if let Some(d) = self.store.disk.as_mut() {
            return d.engine_mut().push_to_object_store_with_snapshot(
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
        Self::from_cold_engine_and_snapshot(
            dir,
            eng,
            snap_bytes.as_deref(),
            crate::Quotas::default(),
        )
    }

    /// Explicitly hydrate a lazy remote cold collection and refresh cached readers.
    pub fn hydrate_cold_collection(&mut self, table: &str) -> Result<usize, Error> {
        self.hydrate_cold_collection_inner(table)
    }

    /// Run a Kusto/Lin statement or query on an object-store-backed `Db`, auto-hydrating lazy cold tables referenced by the query.
    pub fn run_remote(&mut self, src: &str) -> Result<crate::Handle, Error> {
        if !self.has_unhydrated_cold_tables() {
            return self.run(src);
        }
        if src.len() > self.quotas.max_query_bytes {
            return Err(Error::new(format!(
                "query length {} exceeds max_query_bytes quota {}",
                src.len(),
                self.quotas.max_query_bytes
            )));
        }
        let stmts = crate::parse::parse_program_with_limits(
            src,
            self.quotas.max_query_bytes,
            self.quotas.max_ast_depth,
        )?;
        if let Some(result) = self.try_run_remote_filtered(src, &stmts)? {
            return Ok(result);
        }
        let _ = self.hydrate_stmts_cold_tables(&stmts)?;
        self.run_preparsed_ext(src, stmts, true)
    }

    fn try_run_remote_filtered(
        &mut self,
        src: &str,
        stmts: &[Stmt],
    ) -> Result<Option<crate::Handle>, Error> {
        let [Stmt::Query(query)] = stmts else {
            return Ok(None);
        };
        let Source::Collection(table) = &query.source else {
            return Ok(None);
        };
        let Some(Step::Filter(pred)) = query.steps.first() else {
            return Ok(None);
        };
        if query.explain.is_some()
            || !self.store.is_disk_backed(table)
            || !query.steps.iter().all(|step| {
                matches!(
                    step,
                    Step::Filter(_)
                        | Step::Project(_)
                        | Step::Take { .. }
                        | Step::Skip { .. }
                        | Step::Count { .. }
                )
            })
        {
            return Ok(None);
        }
        let Some(disk) = self.store.disk.as_ref() else {
            return Ok(None);
        };
        let engine = &*disk.engine;
        let Some(segments) = engine.remote_pruned_segments(table, pred) else {
            return Ok(None);
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let staging = engine.dir().join(format!(
            ".remote-scan-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&staging).map_err(|err| Error::runtime(err.to_string()))?;
        struct Staging(std::path::PathBuf);
        impl Drop for Staging {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let staging = Staging(staging);
        let view = engine.remote_filtered_view(table, segments, &staging.0)?;
        let mut store = self.store.clone_mem();
        let disk = store
            .disk
            .as_mut()
            .expect("disk backing copied into read snapshot");
        disk.engine = Arc::new(view);
        let mut scan = Self::bare(self.catalog.clone(), store);
        scan.quotas = self.quotas.clone();
        #[cfg(feature = "gpu")]
        {
            scan.gpu = self.gpu.clone();
        }
        scan.run_preparsed_ext(src, stmts.to_vec(), true).map(Some)
    }

    /// Evict least-recently-used hydrated cold tables (at whole-table granularity) down to `max_bytes`.
    pub fn evict_cold_cache_to(&mut self, max_bytes: u64) -> Result<u64, Error> {
        self.evict_cold_cache_inner(max_bytes)
    }

    /// Total bytes of locally hydrated cold segments in this `Db`.
    pub fn cold_local_cache_bytes(&self) -> u64 {
        self.cold_local_cache_bytes_inner()
    }

    pub(crate) fn capture_snapshot_wire_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut cold_names: Vec<String> = self
            .store
            .disk
            .as_ref()
            .map(|d| d.tables.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(d) = self.store.disk.as_ref() {
            for name in d.engine.manifest.tables.keys() {
                if !cold_names.contains(name) {
                    cold_names.push(name.clone());
                }
            }
        } else if let Some(p) = self
            .persist
            .as_ref()
            .filter(|p| p.cold && crate::disk::is_disk_dir(&p.dir))
            && let Ok(eng) = DiskEngine::open_read_only(&p.dir)
        {
            for name in eng.manifest.tables.keys() {
                if !cold_names.contains(name) {
                    cold_names.push(name.clone());
                }
            }
        }
        let mut collections_map: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        for (name, rows) in self.store.collections.iter() {
            if cold_names.iter().any(|c| c == name) {
                collections_map.insert(name.clone(), Vec::new());
            } else {
                collections_map.insert(name.clone(), rows.clone());
            }
        }
        let cat_hash = self
            .persist
            .as_ref()
            .map(|p| p.catalog_hash.clone())
            .unwrap_or_else(|| self.catalog.hash_str().to_string());
        let snap_ref = crate::persist::SnapshotRef {
            r#gen: self.store.r#gen,
            embed_id: &self.store.embed_id,
            next_id: self.store.next_id,
            catalog_hash: &cat_hash,
            log_offset: 0,
            collections: &collections_map,
            edges: &self.store.edges,
            extra_collections: &self.store.extra_collections,
            extra_rels: &self.store.extra_rels,
            extra_indexes: &self.store.extra_indexes,
            extra_filters: &self.store.extra_filters,
            extra_owned: &self.store.extra_owned,
            cold_collections: &cold_names,
        };
        let mut buf = Vec::with_capacity(4096);
        crate::persist::encode_snapshot_ref_to_writer(&mut buf, &snap_ref)?;
        Ok(buf)
    }

    pub(crate) fn from_cold_engine_and_snapshot(
        dir: &Path,
        eng: DiskEngine,
        snap_bytes: Option<&[u8]>,
        quotas: crate::Quotas,
    ) -> Result<Self, Error> {
        let mut catalog = crate::catalog::fixture();
        let mut store = if let Some(raw) = snap_bytes {
            let mut snap = crate::persist::decode_snapshot_bytes(raw)
                .ok_or_else(|| Error::runtime("corrupt or unsupported cold snapshot payload"))?;
            if snap.cold_collections.is_empty() && !eng.manifest.tables.is_empty() {
                snap.cold_collections = eng.manifest.tables.keys().cloned().collect();
            }
            crate::store::Store::from_snapshot_with_disk_engine(dir, snap, Some(eng))?
        } else {
            crate::store::checkpoint::populate_store_from_native_engine(eng, &catalog, None)
        };
        store.merge_extras_into(&mut catalog);
        let col_names: Vec<String> = store
            .collections
            .keys()
            .filter(|name| !store.is_disk_backed(name))
            .cloned()
            .collect();
        for cname in col_names {
            store.rebuild_fts_collection(&catalog, &cname);
        }
        let mut db = Self::bare(catalog, store);
        db.quotas = quotas;
        Ok(db)
    }

    pub(crate) fn hydrate_cold_collection_inner(&mut self, table: &str) -> Result<usize, Error> {
        let Some(d) = self.store.disk.as_mut() else {
            return Ok(0);
        };
        if d.engine.remote_tier.is_none() || d.engine.is_table_hydrated(table) {
            return Ok(0);
        }
        let n = d.engine_mut().hydrate_table(table)?;
        if n > 0 {
            self.reader_cache = None;
            self.query_cache.clear();
        }
        Ok(n)
    }

    pub(crate) fn hydrate_query_cold_tables(&mut self, query: &Query) -> Result<bool, Error> {
        if self.store.disk.is_none() {
            return Ok(false);
        }
        let mut cols = Vec::new();
        collect_query_collections(query, &mut cols);
        cols.retain(|c| self.store.is_disk_backed(c));
        if cols.is_empty() {
            return Ok(false);
        }
        let Some(d) = self.store.disk.as_mut() else {
            return Ok(false);
        };
        if d.engine.remote_tier.is_none() || cols.iter().all(|col| d.engine.is_table_hydrated(col)) {
            return Ok(false);
        }
        let mut hydrated_any = false;
        let eng = d.engine_mut();
        for col in cols {
            if !eng.is_table_hydrated(&col) {
                eng.hydrate_table(&col)?;
                hydrated_any = true;
            }
        }
        if hydrated_any {
            self.reader_cache = None;
            self.query_cache.clear();
        }
        Ok(hydrated_any)
    }

    pub(crate) fn has_unhydrated_cold_tables(&self) -> bool {
        self.store
            .disk
            .as_ref()
            .is_some_and(|d| d.engine.has_unhydrated_tables())
    }

    pub(crate) fn evict_cold_cache_inner(&mut self, max_bytes: u64) -> Result<u64, Error> {
        let Some(d) = self.store.disk.as_mut() else {
            return Ok(0);
        };
        let remaining = d.engine_mut().evict_local_cache_to(max_bytes)?;
        self.reader_cache = None;
        self.query_cache.clear();
        Ok(remaining)
    }

    pub(crate) fn cold_local_cache_bytes_inner(&self) -> u64 {
        self.store
            .disk
            .as_ref()
            .map_or(0, |d| d.engine.local_cache_bytes())
    }

    pub(crate) fn capture_bootstrap_bundle(
        &mut self,
    ) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>, Vec<(u64, Vec<u8>)>), Error> {
        let needs_ckpt = self.persist.as_ref().is_some_and(|p| {
            p.writes_since_snapshot > 0
                || ((self.store.is_native_disk() || self.store.disk.is_some() || p.cold)
                    && !crate::disk::is_disk_dir(&p.dir))
        });
        if needs_ckpt {
            let _ = self.checkpoint();
        }
        let cur_gen = self.store.r#gen;
        let snap_bytes = self.capture_snapshot_wire_bytes()?;
        let (manifest_json, tombstones_raw, segs) = if let Some(d) = self.store.disk.as_ref() {
            d.engine.export_cold_bootstrap_parts()?
        } else if let Some(p) = self
            .persist
            .as_ref()
            .filter(|p| p.cold && crate::disk::is_disk_dir(&p.dir))
        {
            let eng = DiskEngine::open_read_only(&p.dir)?;
            eng.export_cold_bootstrap_parts()?
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        Ok((cur_gen, snap_bytes, manifest_json, tombstones_raw, segs))
    }

    pub(crate) fn apply_bootstrap_bundle(
        &mut self,
        snap_bytes: &[u8],
        cold_manifest_json: &[u8],
        cold_tombstones: &[u8],
        cold_segments: &[(u64, Vec<u8>)],
    ) -> Result<u64, Error> {
        let snap = crate::persist::decode_snapshot_bytes(snap_bytes)
            .ok_or_else(|| Error::runtime("corrupt or unsupported bootstrap snapshot payload"))?;
        let new_gen = snap.r#gen;
        let has_cold = !cold_manifest_json.is_empty();
        let persist_info = self.persist.as_ref().map(|p| (p.dir.clone(), p.sync));
        let quotas = self.quotas;

        if let Some((dir, sync)) = persist_info {
            // Release all open WAL/mmap/lock handles before overwriting local files.
            *self = Self::empty();
            crate::persist::ensure_dir(&dir)?;
            if has_cold {
                let seg_root = dir.join(crate::disk::SEGMENTS_DIR);
                if seg_root.exists() {
                    let _ = std::fs::remove_dir_all(&seg_root);
                }
                std::fs::create_dir_all(&seg_root).map_err(crate::persist::io_err)?;
                std::fs::write(dir.join(crate::disk::MANIFEST_FILE), cold_manifest_json)
                    .map_err(crate::persist::io_err)?;
                std::fs::write(dir.join(crate::disk::TOMBSTONES_FILE), cold_tombstones)
                    .map_err(crate::persist::io_err)?;
                for (seg_id, lseg) in cold_segments {
                    crate::disk::unpack_segment_dir(
                        lseg,
                        &seg_root.join(format!("seg_{seg_id:08}")),
                    )?;
                }
            }
            crate::persist::write_snapshot(&dir, &snap)?;
            crate::persist::write_head(
                &dir,
                &crate::persist::Head {
                    r#gen: snap.r#gen,
                    catalog_hash: snap.catalog_hash.clone(),
                    embed_id: snap.embed_id.clone(),
                },
            )?;
            std::fs::write(dir.join(crate::persist::LOG_NAME), [])
                .map_err(crate::persist::io_err)?;
            let mut reopened = Self::open_follower_with(
                &dir,
                crate::exec::OpenOpts {
                    sync,
                    cold: has_cold,
                },
            )?;
            reopened.quotas = quotas;
            *self = reopened;
            return Ok(new_gen);
        }

        let mut catalog = crate::catalog::fixture();
        let store = if has_cold {
            let tmp_dir = std::env::temp_dir().join(format!(
                "lin_mem_follower_cold_{}_{}",
                std::process::id(),
                new_gen
            ));
            let seg_root = tmp_dir.join(crate::disk::SEGMENTS_DIR);
            if tmp_dir.exists() {
                let _ = std::fs::remove_dir_all(&tmp_dir);
            }
            std::fs::create_dir_all(&seg_root).map_err(crate::persist::io_err)?;
            std::fs::write(tmp_dir.join(crate::disk::MANIFEST_FILE), cold_manifest_json)
                .map_err(crate::persist::io_err)?;
            std::fs::write(tmp_dir.join(crate::disk::TOMBSTONES_FILE), cold_tombstones)
                .map_err(crate::persist::io_err)?;
            for (seg_id, lseg) in cold_segments {
                crate::disk::unpack_segment_dir(lseg, &seg_root.join(format!("seg_{seg_id:08}")))?;
            }
            let eng = DiskEngine::open(&tmp_dir, false)?;
            crate::store::Store::from_snapshot_with_disk_engine(&tmp_dir, snap, Some(eng))?
        } else {
            crate::store::Store::from_snapshot_with_disk_engine(Path::new(""), snap, None)?
        };
        store.merge_extras_into(&mut catalog);
        let mut rebuilt = Self::bare(catalog, store);
        rebuilt.follower = true;
        rebuilt.quotas = quotas;
        *self = rebuilt;
        Ok(new_gen)
    }

    pub(crate) fn hydrate_stmts_cold_tables(&mut self, stmts: &[Stmt]) -> Result<bool, Error> {
        let mut any = false;
        for stmt in stmts {
            match stmt {
                Stmt::Query(q) => {
                    if self.hydrate_query_cold_tables(q)? {
                        any = true;
                    }
                }
                Stmt::Let { query, .. } => {
                    if self.hydrate_query_cold_tables(query)? {
                        any = true;
                    }
                }
                _ => {}
            }
        }
        Ok(any)
    }
}
