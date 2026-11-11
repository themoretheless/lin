//! Durable store open, WAL replay orchestration, checkpointing, snapshots, and cold promotion.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast::{Decl, TypeExpr};
use crate::catalog::{Catalog, catalog_hash};
use crate::cold;
use crate::error::Error;
use crate::persist::{
    self, Head, LogRecord, OpenMemOpts, Pack, Persist, Snapshot, SnapshotRef,
};

use super::Store;
use super::columnar::DictColumn;
use super::value::next_docs_epoch;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StoreOpenPhases {
    pub setup_ms: f64,
    pub snapshot_ms: f64,
    pub wal_ms: f64,
    pub metadata_ms: f64,
    pub indexes_ms: f64,
    pub row_maps_ms: f64,
    pub fts_ms: f64,
}


impl Store {
    /// Load snapshot+log into memory (no writer lock / no Persist).
    /// Does not truncate the log or rewrite head — safe alongside a writer
    /// only if the caller coordinates; prefer [`crate::exec::Db::reader`] for
    /// in-process shared snapshots.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let cat = crate::catalog::fixture();
        Ok(Self::open_with(path, &cat, OpenMemOpts::default())?.0)
    }

    /// Open durable data dir: memory image + exclusive Persist for writes.
    pub fn open_with(
        path: impl AsRef<Path>,
        cat: &Catalog,
        opts: OpenMemOpts,
    ) -> Result<(Self, Persist), Error> {
        let (store, persist, _) = Self::open_with_profiled(path, cat, opts)?;
        Ok((store, persist))
    }

    pub(crate) fn open_with_profiled(
        path: impl AsRef<Path>,
        cat: &Catalog,
        opts: OpenMemOpts,
    ) -> Result<(Self, Persist, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let phase = Instant::now();
        let dir = path.as_ref();
        persist::ensure_dir(dir)?;
        let lock = persist::acquire_writer_lock(dir)?;
        let fixture_hash = catalog_hash(cat);
        phases.setup_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let snap = persist::read_snapshot(dir)?;
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let snap_catalog_hash = snap.as_ref().map(|s| s.catalog_hash.clone());
        let snap_has_extras = snap
            .as_ref()
            .map(|s| {
                !s.extra_collections.is_empty()
                    || !s.extra_rels.is_empty()
                    || !s.extra_indexes.is_empty()
                    || !s.extra_filters.is_empty()
                    || !s.extra_owned.is_empty()
            })
            .unwrap_or(false);

        let mut store = match snap {
            Some(s) => Self::from_snapshot_unindexed(dir, s)?,
            None => {
                let embed = persist::read_head(dir)?
                    .map(|h| h.embed_id)
                    .unwrap_or_else(|| cat.embed_id.clone());
                Self::empty(embed)
            }
        };
        phases.snapshot_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        store.rebuild_indexes();
        phases.indexes_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_row_maps();
        phases.row_maps_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let mut fts_cat = cat.clone();
        store.merge_extras_into(&mut fts_cat);
        let fts_t0 = Instant::now();
        store.try_load_fts(dir, &fts_cat);
        let mut fts_ms = fts_t0.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let mut log = persist::open_log(dir)?;
        let min_gen = store.r#gen;
        let mut wal_index = Vec::new();
        let end = persist::replay_log_indexed(
            &mut log,
            min_gen,
            start,
            Some(&mut wal_index),
            &mut |rec| {
                store.apply_record(&rec);
                Ok(())
            },
        )?;
        persist::truncate_log(&mut log, end)?;
        phases.wal_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let phase = Instant::now();
        let mut live = cat.clone();
        store.merge_extras_into(&mut live);
        let live_hash = catalog_hash(&live);

        let head_disk = persist::read_head(dir)?;
        if let Some(ref head) = head_disk
            && head.catalog_hash != live_hash
        {
            return Err(Error::runtime(format!(
                "catalog_hash mismatch: store={} catalog={}",
                head.catalog_hash, live_hash
            )));
        }
        if let Some(snap_hash) = snap_catalog_hash
            && snap_hash != live_hash
            && !snap_has_extras
            && snap_hash != fixture_hash
        {
            return Err(Error::runtime(format!(
                "catalog_hash mismatch: snapshot={} catalog={}",
                snap_hash, live_hash
            )));
        }

        let head_needs_write = match &head_disk {
            Some(head) => {
                head.r#gen != store.r#gen
                    || head.catalog_hash != live_hash
                    || head.embed_id != store.embed_id
            }
            None => true,
        };
        if head_needs_write {
            persist::write_head(
                dir,
                &Head {
                    r#gen: store.r#gen,
                    catalog_hash: live_hash.clone(),
                    embed_id: store.embed_id.clone(),
                },
            )?;
        }
        phases.metadata_ms = phase.elapsed().as_secs_f64() * 1000.0;

        let fts_t1 = Instant::now();
        store.ensure_fts(&live);
        fts_ms += fts_t1.elapsed().as_secs_f64() * 1000.0;
        phases.fts_ms = fts_ms;

        let persist = Persist {
            dir: dir.to_path_buf(),
            log,
            lock,
            lock_owner_pid: std::process::id(),
            catalog_hash: live_hash,
            writes_since_snapshot: 0,
            encode_buf: Vec::with_capacity(64 * 1024),
            encoder: persist::acquire_encoder(),
            sync: opts.sync,
            cold: opts.cold,
            log_bytes: end,
            wal_index,
        };
        Ok((store, persist, phases))
    }

    /// Read-only open: replay into memory, do not truncate log or write head.
    /// Returns `(store, shared_lock)`.
    pub fn open_read_with(
        path: impl AsRef<Path>,
        cat: &Catalog,
    ) -> Result<(Self, std::fs::File), Error> {
        let (store, lock, _) = Self::open_read_with_profiled(path, cat)?;
        Ok((store, lock))
    }

    pub(crate) fn open_read_with_profiled(
        path: impl AsRef<Path>,
        cat: &Catalog,
    ) -> Result<(Self, std::fs::File, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let phase = Instant::now();
        let dir = path.as_ref();
        persist::validate_data_dir(dir)?;
        if !dir.exists() {
            return Err(Error::runtime(format!(
                "data dir not found: {}",
                dir.display()
            )));
        }
        let lock = persist::acquire_reader_lock(dir)?;
        phases.setup_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let snap = persist::read_snapshot(dir)?;
        let start = snap.as_ref().map(|s| s.log_offset).unwrap_or(0);
        let mut store = match snap {
            Some(s) => Self::from_snapshot_unindexed(dir, s)?,
            None => {
                let embed = persist::read_head(dir)?
                    .map(|h| h.embed_id)
                    .unwrap_or_else(|| cat.embed_id.clone());
                Self::empty(embed)
            }
        };
        phases.snapshot_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_indexes();
        phases.indexes_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        store.rebuild_row_maps();
        phases.row_maps_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let mut fts_cat = cat.clone();
        store.merge_extras_into(&mut fts_cat);
        let fts_t0 = Instant::now();
        store.try_load_fts(dir, &fts_cat);
        let mut fts_ms = fts_t0.elapsed().as_secs_f64() * 1000.0;
        let phase = Instant::now();
        let mut log = persist::open_log_read(dir)?;
        let min_gen = store.r#gen;
        if let Some(log) = log.as_mut() {
            let _ = persist::replay_log(log, min_gen, start, |rec| {
                store.apply_record(&rec);
                Ok(())
            })?;
        }
        phases.wal_ms = phase.elapsed().as_secs_f64() * 1000.0;
        let mut live = cat.clone();
        store.merge_extras_into(&mut live);
        let fts_t1 = Instant::now();
        store.ensure_fts(&live);
        fts_ms += fts_t1.elapsed().as_secs_f64() * 1000.0;
        phases.fts_ms = fts_ms;
        Ok((store, lock, phases))
    }

    pub(crate) fn from_snapshot(dir: &Path, s: Snapshot) -> Result<Self, Error> {
        let mut store = Self::from_snapshot_unindexed(dir, s)?;
        store.rebuild_indexes();
        store.rebuild_row_maps();
        Ok(store)
    }

    pub(super) fn from_snapshot_unindexed(dir: &Path, mut s: Snapshot) -> Result<Self, Error> {
        let col_counts = {
            let mut counts = FxHashMap::default();
            for (name, rows) in &s.collections {
                counts.insert(name.clone(), rows.len());
            }
            counts
        };
        let mut store = Self {
            r#gen: s.r#gen,
            embed_id: std::mem::take(&mut s.embed_id),
            next_id: s.next_id,
            collections: std::mem::take(&mut s.collections),
            edges: std::mem::take(&mut s.edges),
            extra_collections: std::mem::take(&mut s.extra_collections),
            extra_rels: std::mem::take(&mut s.extra_rels),
            extra_indexes: std::mem::take(&mut s.extra_indexes),
            extra_filters: std::mem::take(&mut s.extra_filters),
            extra_owned: std::mem::take(&mut s.extra_owned),
            indexes: BTreeMap::new(),
            fts: BTreeMap::new(),
            hnsw: BTreeMap::new(),
            by_id: BTreeMap::new(),
            docs_by_uri: FxHashMap::default(),
            facts_by_spo: FxHashMap::default(),
            edge_keys: FxHashSet::default(),
            edges_out: FxHashMap::default(),
            edges_in: FxHashMap::default(),
            docs_id: Vec::new(),
            docs_title: Vec::new(),
            docs_title_packed: OnceLock::new(),
            docs_epoch: next_docs_epoch(),
            docs_layer: DictColumn::new(),
            docs_wing: DictColumn::new(),
            last_doc_id_idx: std::sync::atomic::AtomicUsize::new(usize::MAX),
            docs_moved_ids: smallvec::SmallVec::new(),
            docs_moved_idxs: smallvec::SmallVec::new(),
            retired_doc_ids: smallvec::SmallVec::new(),
            retired_arcs: smallvec::SmallVec::new(),
            fts_deferred_doc_swaps: smallvec::SmallVec::new(),
            retired_rows: smallvec::SmallVec::new(),
            retired_cells: smallvec::SmallVec::new(),
            orders_id: Vec::new(),
            orders_user_id: Vec::new(),
            orders_total: Vec::new(),
            users_id: Vec::new(),
            users_email: Vec::new(),
            facts_s: Vec::new(),
            facts_p: Vec::new(),
            facts_o: Vec::new(),
            facts_rows_stale: false,
            col_counts,
            cold: BTreeMap::new(),
        };
        for name in store.extra_collections.keys() {
            store.collections.entry(name.clone()).or_default();
        }
        for name in &s.cold_collections {
            if crate::cold::cold_path(dir, name).exists() {
                // Lazy: mmap only; rows page-in on first collection() access.
                let col = cold::map_cold(dir, name)?;
                let nrows = col.row_count();
                store.col_counts.insert(name.clone(), nrows);
                store.collections.insert(name.clone(), Vec::new());
                store.cold.insert(name.clone(), col);
            }
        }
        Ok(store)
    }

    /// Capture current memory image (for backup / checkpoint payload).
    pub fn capture_snapshot(&mut self, catalog_hash: impl Into<String>) -> Snapshot {
        self.capture_snapshot_at(catalog_hash, 0)
    }

    pub fn capture_snapshot_at(
        &mut self,
        catalog_hash: impl Into<String>,
        log_offset: u64,
    ) -> Snapshot {
        self.ensure_facts_rows();
        let cold_names: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names {
            self.promote_cold(&name);
        }
        Snapshot {
            r#gen: self.r#gen,
            embed_id: self.embed_id.clone(),
            next_id: self.next_id,
            catalog_hash: catalog_hash.into(),
            log_offset,
            collections: self.collections.clone(),
            edges: self.edges.clone(),
            extra_collections: self.extra_collections.clone(),
            extra_rels: self.extra_rels.clone(),
            extra_indexes: self.extra_indexes.clone(),
            extra_filters: self.extra_filters.clone(),
            extra_owned: self.extra_owned.clone(),
            cold_collections: Vec::new(),
        }
    }

    pub fn merge_extras_into(&self, cat: &mut Catalog) {
        for (name, fields) in &self.extra_owned {
            let fields = fields
                .iter()
                .map(|(n, t)| (n.clone(), TypeExpr::Named(t.clone())))
                .collect();
            let _ = cat.apply_decl(&Decl::Owned {
                name: name.clone(),
                fields,
            });
        }
        for c in self.extra_collections.values() {
            let fields = c
                .fields
                .iter()
                .map(|(n, t)| (n.clone(), TypeExpr::Named(t.clone())))
                .collect();
            let _ = cat.apply_decl(&Decl::Col {
                name: c.name.clone(),
                append: c.append_only,
                fields,
            });
        }
        for r in self.extra_rels.values() {
            let decl = if let Some(of) = &r.reverse_of {
                Decl::RelReverse {
                    name: r.name.clone(),
                    of: of.clone(),
                }
            } else {
                Decl::Rel {
                    name: r.name.clone(),
                    stub: r.stub,
                }
            };
            let _ = cat.apply_decl(&decl);
        }
        for idx in self.extra_indexes.values() {
            let pred = idx
                .pred_src
                .as_deref()
                .and_then(|src| crate::parse::parse_pred_src(src).ok());
            let _ = cat.apply_decl(&Decl::Index {
                collection: idx.collection.clone(),
                unique: idx.unique,
                fields: idx.fields.clone(),
                pred,
            });
        }
        for (collection, src) in &self.extra_filters {
            if let Ok(pred) = crate::parse::parse_pred_src(src) {
                let _ = cat.apply_decl(&Decl::Filter {
                    collection: collection.clone(),
                    pred: Some(pred),
                    src: src.clone(),
                });
            }
        }
        if cat.embed_id != self.embed_id {
            cat.set_embed_id(self.embed_id.clone());
        }
    }

    /// Flush one committed pack to the log (data sync). Head is updated durably
    /// on checkpoint/close only (soft head skipped on the hot path).
    pub fn durable_commit(&self, persist: &mut Persist, pack: Pack) -> Result<(), Error> {
        self.durable_commit_prepared(persist, pack, None)
    }

    pub(crate) fn durable_commit_prepared(
        &self,
        persist: &mut Persist,
        pack: Pack,
        prepared: Option<crate::persist::PreparedEncoding>,
    ) -> Result<(), Error> {
        let rec = LogRecord {
            r#gen: self.r#gen + 1,
            next_id: self.next_id,
            pack,
        };
        let sync = persist.sync;
        let prev_log_bytes = persist.log_bytes;
        let rec_gen = rec.r#gen;
        {
            let Persist {
                log,
                encode_buf,
                log_bytes,
                ..
            } = persist;
            match prepared {
                Some(persist::PreparedEncoding::Direct {
                    generation,
                    next_id,
                    collection,
                    fields,
                    rows,
                    magic,
                }) => {
                    let valid = rec.r#gen == generation
                        && rec.next_id == next_id
                        && matches!(&rec.pack, Pack::InsertCols { collection: c, fields: f, n, edges, .. }
                            if c == &collection && f == &fields && *n == rows && edges.is_empty());
                    if valid {
                        persist::write_frame(log, &magic, encode_buf, sync, log_bytes)?;
                    } else {
                        persist::append_record(log, &rec, encode_buf, sync, log_bytes)?;
                    }
                }
                Some(persist::PreparedEncoding::Ready(encoded)) => {
                    persist::append_prepared_record(
                        log, &rec, encoded, encode_buf, sync, log_bytes,
                    )?;
                }
                Some(persist::PreparedEncoding::Failed(error)) => return Err(error),
                Some(persist::PreparedEncoding::Panicked(payload)) => {
                    std::panic::resume_unwind(payload)
                }
                None => persist::append_record(log, &rec, encode_buf, sync, log_bytes)?,
            }
        }
        persist.wal_index.push((rec_gen, prev_log_bytes));
        persist.writes_since_snapshot += 1;
        Ok(())
    }

    pub fn maybe_checkpoint(&mut self, persist: &mut Persist) -> Result<(), Error> {
        if (persist.writes_since_snapshot >= persist::SNAPSHOT_EVERY
            && persist.log_bytes >= 4 * 1024 * 1024)
            || persist.writes_since_snapshot >= 4096
        {
            self.write_snapshot(persist)?;
        }
        Ok(())
    }

    /// Force a durable checkpoint (snapshot + head fsync).
    pub fn checkpoint(&mut self, persist: &mut Persist) -> Result<(), Error> {
        self.write_snapshot(persist)
    }

    pub fn write_snapshot(&mut self, persist: &mut Persist) -> Result<(), Error> {
        // Exclude open_read processes while publishing snapshot + compacting log.
        let _fence = persist::acquire_fence_exclusive(&persist.dir)?;
        // Ensure log content is fully durable before publishing snapshot/head.
        // Normal mode: this is the durability point for prior commits.
        persist::durable_sync(&persist.log)
            .or_else(|_| persist.log.sync_all())
            .map_err(persist::io_err)?;
        self.ensure_facts_rows();
        let cold_names_to_promote: Vec<String> = self.cold.keys().cloned().collect();
        for name in cold_names_to_promote {
            self.promote_cold(&name);
        }
        let mut cold_names = Vec::new();
        if persist.cold {
            // Cold bins are a decode cache; snapshot stays self-contained (rows kept).
            for (name, rows) in &self.collections {
                if rows.len() >= cold::COLD_MIN_ROWS {
                    cold::write_cold_ext(&persist.dir, name, rows, false)?;
                    cold_names.push(name.clone());
                }
            }
            cold::prune_cold(&persist.dir, &cold_names)?;
            if !cold_names.is_empty() {
                persist::sync_dir_checked(&cold::cold_dir(&persist.dir))?;
            }
        }
        let head = Head {
            r#gen: self.r#gen,
            catalog_hash: persist.catalog_hash.clone(),
            embed_id: self.embed_id.clone(),
        };
        let stubbed_collections;
        let collections_ref = if !cold_names.is_empty() {
            stubbed_collections = self
                .collections
                .iter()
                .map(|(k, v)| {
                    if cold_names.contains(k) {
                        (k.clone(), Vec::new())
                    } else {
                        (k.clone(), v.clone())
                    }
                })
                .collect::<BTreeMap<_, _>>();
            &stubbed_collections
        } else {
            &self.collections
        };

        let snap_ref = SnapshotRef {
            r#gen: self.r#gen,
            embed_id: &self.embed_id,
            next_id: self.next_id,
            catalog_hash: &persist.catalog_hash,
            log_offset: 0,
            collections: collections_ref,
            edges: &self.edges,
            extra_collections: &self.extra_collections,
            extra_rels: &self.extra_rels,
            extra_indexes: &self.extra_indexes,
            extra_filters: &self.extra_filters,
            extra_owned: &self.extra_owned,
            cold_collections: &cold_names,
        };
        persist::write_snapshot_ref_to_file_ext(
            &persist::snapshot_path(&persist.dir),
            &snap_ref,
            false,
        )?;
        persist::write_head_no_dir_sync(&persist.dir, &head)?;
        self.write_fts_files(&persist.dir)?;
        persist::sync_dir_checked(&persist.dir)?;
        persist::compact_log(&mut persist.log)?;
        persist.writes_since_snapshot = 0;
        persist.log_bytes = 0;
        persist.wal_index.clear();
        Ok(())
    }

    pub(super) fn apply_record(&mut self, rec: &LogRecord) {
        self.apply_pack(&rec.pack);
        self.next_id = rec.next_id;
        self.r#gen = rec.r#gen;
    }


    /// Promote a cold mmap collection into `collections` (idempotent).
    pub fn promote_cold(&mut self, name: &str) {
        let Some(col) = self.cold.remove(name) else {
            return;
        };
        if let Ok(rows) = col.into_rows() {
            let nrows = rows.len();
            self.collections.insert(name.to_string(), rows);
            self.col_counts.insert(name.to_string(), nrows);
            self.rebuild_row_maps_collection(name);
            self.rebuild_indexes_collection(name);
        }
    }

}
