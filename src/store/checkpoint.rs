//! Durable store open, WAL replay orchestration, checkpointing, snapshots, and DiskEngine tiering.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast::{Decl, TypeExpr};
use crate::catalog::{Catalog, catalog_hash};
use crate::disk::{
    DiskColKind, DiskColSchema, DiskEngine, DiskTableSchema, MANIFEST_FILE, SEGMENTS_DIR,
    TOMBSTONES_FILE, WAL_FILE, infer_schema_from_rows, is_disk_dir,
};
use crate::error::Error;
use crate::persist::{
    self, ColSnap, Head, IndexSnap, LogRecord, OpenMemOpts, Pack, Persist, Snapshot, SnapshotRef,
};

use super::columnar::DictColumn;
use super::value::next_docs_epoch;
use super::{COLD_MIN_ROWS, DiskColLazy, Store, StoreDiskTier};

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

/// True when `dir` is a standalone `DiskEngine` directory (no legacy `head` or `snapshot` files).
pub(crate) fn is_pure_disk_dir(dir: &Path) -> bool {
    is_disk_dir(dir) && !persist::head_path(dir).exists() && !persist::snapshot_path(dir).exists()
}

fn cleanup_disk_files(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(MANIFEST_FILE));
    let _ = std::fs::remove_file(dir.join(WAL_FILE));
    let _ = std::fs::remove_file(dir.join(TOMBSTONES_FILE));
    let _ = std::fs::remove_dir_all(dir.join(SEGMENTS_DIR));
}

fn populate_store_from_native_engine(
    eng: DiskEngine,
    cat: &Catalog,
    writer_lock: Option<Arc<std::fs::File>>,
) -> Store {
    let mut store = Store::empty(eng.manifest.embed_id.clone());
    store.r#gen = eng.manifest.generation;

    let mut tables = BTreeMap::new();
    let mut max_numeric_id = 0u64;

    for (tname, schema) in &eng.manifest.tables {
        let nrows = eng.count_table(tname);
        store.collections.entry(tname.clone()).or_default();
        store.col_counts.insert(tname.clone(), nrows);

        let fields: Vec<(String, String)> = schema
            .columns
            .iter()
            .map(|c| {
                let ty = if c.name == "id" {
                    "id"
                } else {
                    match c.kind {
                        DiskColKind::Int64 => "i64",
                        DiskColKind::Float64 => "f64",
                        DiskColKind::Bool => "bool",
                        DiskColKind::Time64 => "time",
                        DiskColKind::TextZstd | DiskColKind::TextDict => "var",
                        DiskColKind::VecF32Sq8 { .. } => "vec",
                    }
                };
                (c.name.clone(), ty.to_string())
            })
            .collect();

        let needs_extra_col = match cat.collections.get(tname) {
            None => true,
            Some(cdef) => fields
                .iter()
                .any(|(fname, _)| !cdef.fields.contains_key(fname)),
        };
        if needs_extra_col {
            store.extra_collections.insert(
                tname.clone(),
                ColSnap {
                    name: tname.clone(),
                    append_only: false,
                    fields,
                },
            );
        }

        for idx_field in &schema.indexed_fields {
            if idx_field == "id" {
                continue;
            }
            let label = format!("{tname}[{idx_field}]");
            if !cat.indexes.contains_key(&label) {
                store.extra_indexes.insert(
                    label,
                    IndexSnap {
                        collection: tname.clone(),
                        unique: false,
                        fields: vec![idx_field.clone()],
                        pred_src: None,
                    },
                );
            }
        }

        let _ = eng.for_each_memtable_row(tname, |_, r| {
            if let Some(id_str) = r.get("id").and_then(|c| c.text())
                && let Some(num_s) = id_str.strip_prefix("id-")
                && let Ok(n) = num_s.parse::<u64>()
            {
                max_numeric_id = max_numeric_id.max(n);
            }
            Ok(true)
        });

        tables.insert(
            tname.clone(),
            DiskColLazy {
                row_count: nrows,
                materialized: OnceLock::new(),
            },
        );
    }

    store.next_id = max_numeric_id
        .saturating_add(1)
        .max(eng.manifest.generation.saturating_add(1))
        .max(1);

    store.disk = Some(StoreDiskTier {
        engine: Arc::new(RwLock::new(eng)),
        tables,
        native_disk: true,
        writer_lock,
    });
    store.rebuild_indexes();
    store
}

impl Store {
    /// Load snapshot+log into memory (no writer lock / no Persist).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = path.as_ref();
        let cat = crate::catalog::fixture();
        if is_pure_disk_dir(dir) {
            return Ok(Self::open_native_disk(dir, &cat, false)?.0);
        }
        Ok(Self::open_with(dir, &cat, OpenMemOpts::default())?.0)
    }

    pub(crate) fn open_native_disk(
        dir: &Path,
        cat: &Catalog,
        sync_full: bool,
    ) -> Result<(Self, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let t0 = Instant::now();
        persist::validate_data_dir(dir)?;
        let lock = persist::acquire_writer_lock(dir)?;
        phases.setup_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t1 = Instant::now();
        let eng = DiskEngine::open(dir, sync_full)?;
        phases.snapshot_ms = t1.elapsed().as_secs_f64() * 1000.0;

        let t2 = Instant::now();
        let store = populate_store_from_native_engine(eng, cat, Some(Arc::new(lock)));
        phases.indexes_ms = t2.elapsed().as_secs_f64() * 1000.0;
        Ok((store, phases))
    }

    pub(crate) fn open_native_disk_read(
        dir: &Path,
        cat: &Catalog,
    ) -> Result<(Self, std::fs::File, StoreOpenPhases), Error> {
        let mut phases = StoreOpenPhases::default();
        let t0 = Instant::now();
        persist::validate_data_dir(dir)?;
        let lock = persist::acquire_reader_lock(dir)?;
        phases.setup_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t1 = Instant::now();
        let eng = DiskEngine::open_read_only(dir)?;
        phases.snapshot_ms = t1.elapsed().as_secs_f64() * 1000.0;

        let t2 = Instant::now();
        let store = populate_store_from_native_engine(eng, cat, None);
        phases.indexes_ms = t2.elapsed().as_secs_f64() * 1000.0;
        Ok((store, lock, phases))
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
        let dir = path.as_ref();
        if is_pure_disk_dir(dir) {
            return Self::open_native_disk_read(dir, cat);
        }
        let mut phases = StoreOpenPhases::default();
        let phase = Instant::now();
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
            disk: None,
            dirty_fts_disk: BTreeMap::new(),
            overlay_savepoint_fts: None,
            disk_cluster_keys: BTreeMap::new(),
            disk_compact_requests: Vec::new(),
        };
        for name in store.extra_collections.keys() {
            store.collections.entry(name.clone()).or_default();
        }
        if !s.cold_collections.is_empty() && is_disk_dir(dir) {
            let eng = DiskEngine::open_read_only(dir)?;
            let mut tables = BTreeMap::new();
            for name in &s.cold_collections {
                if eng.has_table(name) {
                    let nrows = eng.count_table(name);
                    store.col_counts.insert(name.clone(), nrows);
                    store.collections.insert(name.clone(), Vec::new());
                    tables.insert(
                        name.clone(),
                        DiskColLazy {
                            row_count: nrows,
                            materialized: OnceLock::new(),
                        },
                    );
                }
            }
            if !tables.is_empty() {
                store.disk = Some(StoreDiskTier {
                    engine: Arc::new(RwLock::new(eng)),
                    tables,
                    native_disk: false,
                    writer_lock: None,
                });
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
        let disk_names: Vec<String> = self
            .disk
            .as_ref()
            .map(|d| d.tables.keys().cloned().collect())
            .unwrap_or_default();
        for name in disk_names {
            self.promote_disk_col(&name);
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
            if let Some(existing) = cat.collections.get_mut(&c.name) {
                for (fname, tname) in &c.fields {
                    if !existing.fields.contains_key(fname) {
                        let clean_t = if tname.starts_with("vec@") {
                            "vec"
                        } else {
                            tname.as_str()
                        };
                        if let Some(ty) = crate::catalog::Type::from_name(clean_t) {
                            existing.fields.insert(
                                fname.clone(),
                                crate::catalog::FieldInfo {
                                    ty,
                                    fts: ty == crate::catalog::Type::Text,
                                    unique: fname == "id" || fname == "uri",
                                },
                            );
                        }
                    }
                }
                continue;
            }
            let fields = c
                .fields
                .iter()
                .map(|(n, t)| {
                    let clean_t = if t.starts_with("vec@") {
                        "vec".to_string()
                    } else {
                        t.clone()
                    };
                    (n.clone(), TypeExpr::Named(clean_t))
                })
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
        persist::durable_sync(&persist.log)
            .or_else(|_| persist.log.sync_all())
            .map_err(persist::io_err)?;
        self.ensure_facts_rows();

        let mut cold_names = Vec::new();
        if !persist.cold {
            let disk_names: Vec<String> = self
                .disk
                .as_ref()
                .map(|d| d.tables.keys().cloned().collect())
                .unwrap_or_default();
            for name in disk_names {
                self.promote_disk_col(&name);
            }
            self.disk = None;
            if is_disk_dir(&persist.dir) {
                cleanup_disk_files(&persist.dir);
            }
        } else {
            // Preserve already-unmodified (or overlay-dirty) disk-backed tables without re-materializing them!
            let untouched_disk: FxHashSet<String> = self
                .disk
                .as_ref()
                .map(|d| {
                    d.tables
                        .iter()
                        .filter(|(_, lazy)| lazy.row_count >= COLD_MIN_ROWS)
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();

            // Promote any tiny (< COLD_MIN_ROWS) disk-backed table back into inline snapshot.
            let tiny_disk: Vec<String> = self
                .disk
                .as_ref()
                .map(|d| {
                    d.tables
                        .iter()
                        .filter(|(_, lazy)| lazy.row_count < COLD_MIN_ROWS)
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();
            for name in &tiny_disk {
                self.promote_disk_col(name);
            }

            // Drain any cold overlay mutations (new/updated memtable rows + tombstones) before releasing `ro`.
            let (mut dirty_mem_rows, dirty_tombstones) = if let Some(d) = self.disk.as_ref()
                && let Ok(mut eng) = d.engine.write()
                && eng.has_dirty_overlay()
            {
                let (mr, ts) = eng.drain_dirty_overlays();
                (mr, Some(ts))
            } else {
                (BTreeMap::new(), None)
            };
            for name in &tiny_disk {
                dirty_mem_rows.remove(name);
            }

            // Close read-only mmap handles before opening DiskEngine writer to rewrite segments.
            self.disk = None;

            for (name, rows) in &self.collections {
                if untouched_disk.contains(name) || rows.len() >= COLD_MIN_ROWS {
                    cold_names.push(name.clone());
                }
            }

            if cold_names.is_empty() {
                if is_disk_dir(&persist.dir) {
                    cleanup_disk_files(&persist.dir);
                }
            } else {
                let mut eng = DiskEngine::open(&persist.dir, false)?;
                eng.manifest.embed_id = self.embed_id.clone();

                // Cluster keys already in the manifest must survive a table being dropped
                // and spilled again below; keys declared this session win. An empty string
                // records an explicit "no key".
                for (t, schema) in &eng.manifest.tables {
                    if let Some(k) = &schema.cluster_by {
                        self.disk_cluster_keys
                            .entry(t.clone())
                            .or_insert_with(|| k.clone());
                    }
                }
                let cluster_keys = self.disk_cluster_keys.clone();
                let apply_cluster_keys = |eng: &mut DiskEngine| {
                    for (t, key) in &cluster_keys {
                        if let Some(schema) = eng.manifest.tables.get_mut(t) {
                            schema.cluster_by = (!key.is_empty()
                                && schema.columns.iter().any(|c| &c.name == key))
                            .then(|| key.clone());
                        }
                    }
                };
                apply_cluster_keys(&mut eng);

                // Drop any stale table from DiskEngine that is no longer cold or was promoted & modified in RAM.
                let existing_tables: Vec<String> = eng.manifest.tables.keys().cloned().collect();
                for t in existing_tables {
                    if !untouched_disk.contains(&t) {
                        eng.drop_table(&t)?;
                    }
                }

                // Flush incremental cold overlay mutations (delta segments + tombstones) for untouched disk tables.
                if let Some(ts) = dirty_tombstones {
                    let filtered_rows: BTreeMap<String, _> = dirty_mem_rows
                        .into_iter()
                        .filter(|(k, _)| untouched_disk.contains(k))
                        .collect();
                    eng.merge_overlay_and_flush(filtered_rows, &ts)?;
                }

                // Spill newly hot/modified large collections into columnar mmap segments.
                for name in &cold_names {
                    if untouched_disk.contains(name) {
                        continue;
                    }
                    let Some(rows) = self.collections.get(name) else {
                        continue;
                    };
                    if rows.is_empty() {
                        continue;
                    }
                    let mut schema = infer_schema_from_rows(name, rows);
                    if name == "docs" && !schema.indexed_fields.iter().any(|f| f == "uri") {
                        schema.indexed_fields.push("uri".to_string());
                    }
                    if name == "orders" && !schema.indexed_fields.iter().any(|f| f == "user_id") {
                        schema.indexed_fields.push("user_id".to_string());
                    }
                    for idx_snap in self.extra_indexes.values() {
                        if idx_snap.collection == *name {
                            for f in &idx_snap.fields {
                                if f != "id" && !schema.indexed_fields.contains(f) {
                                    schema.indexed_fields.push(f.clone());
                                }
                            }
                        }
                    }
                    let cluster_key = cluster_keys
                        .get(name)
                        .filter(|k| !k.is_empty() && schema.columns.iter().any(|c| &c.name == *k))
                        .cloned();
                    schema.cluster_by = cluster_key.clone();
                    eng.manifest.tables.insert(name.clone(), schema);
                    match cluster_key {
                        // Spill a clustered collection in key order (the in-memory copy
                        // keeps its own order).
                        Some(key) => {
                            let mut sorted = rows.clone();
                            crate::disk::sort_rows_by_cluster_key(&mut sorted, &key);
                            for chunk in sorted.chunks(65_536) {
                                eng.write_bulk_segment(name, chunk)?;
                            }
                        }
                        None => {
                            for chunk in rows.chunks(65_536) {
                                eng.write_bulk_segment(name, chunk)?;
                            }
                        }
                    }
                }
                apply_cluster_keys(&mut eng);

                // Requested compactions: reclaim dead rows and merge segments; a clustered
                // table comes out globally ordered by its key.
                for t in std::mem::take(&mut self.disk_compact_requests) {
                    let key = eng.manifest.tables.get(&t).and_then(|s| s.cluster_by.clone());
                    if eng.manifest.tables.contains_key(&t) {
                        match key {
                            Some(key) => eng.compact_table_clustered(&t, &key)?,
                            None => eng.compact_table(&t, true)?,
                        };
                    }
                }

                eng.manifest.generation = self.r#gen;
                eng.save_tombstones_and_manifest()?;
                persist::sync_dir_checked(&persist.dir.join(SEGMENTS_DIR))?;
                drop(eng);

                // Re-attach read-only mmap view for any previously untouched disk tables!
                if !untouched_disk.is_empty() {
                    let ro = DiskEngine::open_read_only(&persist.dir)?;
                    let mut tables = BTreeMap::new();
                    for name in &untouched_disk {
                        let nrows = ro.count_table(name);
                        self.col_counts.insert(name.clone(), nrows);
                        tables.insert(
                            name.clone(),
                            DiskColLazy {
                                row_count: nrows,
                                materialized: OnceLock::new(),
                            },
                        );
                    }
                    let dirty_fts = std::mem::take(&mut self.dirty_fts_disk);
                    for (name, fields) in dirty_fts {
                        if !untouched_disk.contains(&name) {
                            continue;
                        }
                        let mut proj: Vec<&str> = Vec::with_capacity(fields.len() + 1);
                        proj.push("id");
                        for f in &fields {
                            if f != "id" {
                                proj.push(f.as_str());
                            }
                        }
                        let mut rows = Vec::new();
                        ro.scan_projected(&name, Some(&proj), |row| {
                            rows.push(row.clone());
                            Ok(true)
                        })?;
                        let idx = crate::fts::FtsIndex::build(&rows, &fields);
                        self.fts.insert(name, idx);
                    }
                    self.disk = Some(StoreDiskTier {
                        engine: Arc::new(RwLock::new(ro)),
                        tables,
                        native_disk: false,
                        writer_lock: None,
                    });
                }
            }
        }

        // Write FTS files after all promoted/dirty tables have rebuilt indexes and accurate counts.
        self.write_fts_files(&persist.dir)?;

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

    pub(crate) fn commit_pack_to_native_disk(&mut self, pack: &Pack) -> Result<(), Error> {
        let mut ops = Vec::new();
        pack_to_disk_wal_ops(pack, &mut ops);
        if ops.is_empty() {
            return Ok(());
        }
        let Some(d) = self.disk.as_mut() else {
            return Ok(());
        };
        if !d.native_disk || d.writer_lock.is_none() {
            return Ok(());
        }
        // Invalidate any stale OnceLock materializations on still-lazy tables.
        for op in &ops {
            match op {
                crate::disk::DiskWalOp::EnsureTable { schema } => {
                    if let Some(lazy) = d.tables.get_mut(&schema.name) {
                        lazy.materialized = OnceLock::new();
                    }
                }
                crate::disk::DiskWalOp::UpsertBatch { table, .. }
                | crate::disk::DiskWalOp::DeleteIds { table, .. }
                | crate::disk::DiskWalOp::EnsureIndex { table, .. } => {
                    if let Some(lazy) = d.tables.get_mut(table) {
                        lazy.materialized = OnceLock::new();
                    }
                }
            }
        }
        let mut eng = d
            .engine
            .write()
            .map_err(|_| Error::runtime("disk engine lock poisoned"))?;
        eng.commit_ops(ops)?;
        let table_names: Vec<String> = d.tables.keys().cloned().collect();
        for tname in table_names {
            let nrows = eng.count_table(&tname);
            if let Some(lazy) = d.tables.get_mut(&tname) {
                lazy.row_count = nrows;
            }
            self.col_counts.insert(tname, nrows);
        }
        Ok(())
    }
}

pub(crate) fn pack_to_disk_wal_ops(pack: &Pack, out: &mut Vec<crate::disk::DiskWalOp>) {
    use crate::disk::DiskWalOp;
    match pack {
        Pack::Insert {
            collection, row, ..
        } => {
            out.push(DiskWalOp::UpsertBatch {
                table: collection.clone(),
                rows: vec![row.clone()],
            });
        }
        Pack::InsertBulk {
            collection, rows, ..
        } => {
            if !rows.is_empty() {
                out.push(DiskWalOp::UpsertBatch {
                    table: collection.clone(),
                    rows: rows.clone(),
                });
            }
        }
        Pack::InsertCols {
            collection,
            fields,
            cols,
            n,
            ..
        } => {
            if *n > 0 {
                let rows = persist::cols_to_rows(fields, cols, *n as usize);
                out.push(DiskWalOp::UpsertBatch {
                    table: collection.clone(),
                    rows,
                });
            }
        }
        Pack::Update { collection, rows } => {
            if !rows.is_empty() {
                out.push(DiskWalOp::UpsertBatch {
                    table: collection.clone(),
                    rows: rows.clone(),
                });
            }
        }
        Pack::Delete { collection, rows } => {
            let ids: Vec<String> = rows
                .iter()
                .filter_map(|r| r.get("id").and_then(|c| c.text()).map(str::to_string))
                .collect();
            if !ids.is_empty() {
                out.push(DiskWalOp::DeleteIds {
                    table: collection.clone(),
                    ids,
                });
            }
        }
        Pack::SchemaCol { name, fields, .. } => {
            let mut columns = Vec::with_capacity(fields.len() + 1);
            if !fields.iter().any(|(n, _)| n == "id") {
                columns.push(DiskColSchema {
                    name: "id".to_string(),
                    kind: DiskColKind::TextZstd,
                });
            }
            for (fname, tname) in fields {
                let kind = match tname.as_str() {
                    "i64" | "dur" => DiskColKind::Int64,
                    "f64" => DiskColKind::Float64,
                    "bool" => DiskColKind::Bool,
                    "time" => DiskColKind::Time64,
                    t if t == "vec" || t.starts_with("vec@") => DiskColKind::VecF32Sq8 { dim: 0 },
                    _ => DiskColKind::TextZstd,
                };
                columns.push(DiskColSchema {
                    name: fname.clone(),
                    kind,
                });
            }
            out.push(DiskWalOp::EnsureTable {
                schema: DiskTableSchema {
                    name: name.clone(),
                    columns,
                    indexed_fields: Vec::new(),
                    cluster_by: None,
                },
            });
        }
        Pack::SchemaIndex {
            collection, fields, ..
        } => {
            if fields.len() == 1 && fields[0] != "id" {
                out.push(DiskWalOp::EnsureIndex {
                    table: collection.clone(),
                    field: fields[0].clone(),
                });
            }
        }
        Pack::Batch { packs } => {
            for p in packs {
                pack_to_disk_wal_ops(p, out);
            }
        }
        Pack::AppendFact { .. }
        | Pack::AppendFactsBulk { .. }
        | Pack::AppendEdge { .. }
        | Pack::AppendEdgesBulk { .. }
        | Pack::DeleteEdge { .. }
        | Pack::Reembed
        | Pack::SchemaRel { .. }
        | Pack::SchemaFilter { .. }
        | Pack::SchemaOwned { .. } => {}
    }
}
