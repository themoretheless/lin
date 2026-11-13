//! Dictionary-encoded columns (`DictColumn`) and Struct-of-Arrays (SoA) vectorized scan/projection helpers.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use super::Store;
use super::value::{
    Cell, ID_TITLE_POOL, LAST_ID_TITLE_META, Row, acquire_row, docs_hot_field, row_id_title,
    row_matches_id_title,
};

/// In-memory dictionary-encoded column for low-cardinality string columns (e.g. `wing`, `layer`).
///
/// Stores unique `Arc<str>` values in a compact dictionary vector and per-row `u8` indices.
/// Achieves an 8x memory reduction on 64-bit platforms (1 byte per row vs 8-16 bytes per Arc<str>),
/// eliminates cache thrashing, and allows ultra-fast aggregation directly over the index byte slice.
/// If unique values exceed 255, automatically spills to a flat vector of `Arc<str>`.
#[derive(Debug, Clone, Default, Eq)]
pub struct DictColumn {
    dict: Vec<Arc<str>>,
    indices: Vec<u8>,
    spilled: Option<Vec<Arc<str>>>,
}

impl PartialEq for DictColumn {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for i in 0..self.len() {
            if self.get_ref(i) != other.get_ref(i) {
                return false;
            }
        }
        true
    }
}

impl DictColumn {
    pub const fn new() -> Self {
        Self {
            dict: Vec::new(),
            indices: Vec::new(),
            spilled: None,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        if let Some(spilled) = &self.spilled {
            spilled.len()
        } else {
            self.indices.len()
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        if let Some(spilled) = &mut self.spilled {
            spilled.reserve(additional);
        } else {
            self.indices.reserve(additional);
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.dict.clear();
        self.indices.clear();
        self.spilled = None;
    }

    #[inline]
    pub fn push(&mut self, val: Arc<str>) {
        if let Some(spilled) = &mut self.spilled {
            spilled.push(val);
            return;
        }

        // Fast path: linear scan of small dictionary (< 16 items)
        if let Some(pos) = self
            .dict
            .iter()
            .position(|x| Arc::ptr_eq(x, &val) || **x == *val)
        {
            self.indices.push(pos as u8);
            return;
        }

        if self.dict.len() < 255 {
            let pos = self.dict.len() as u8;
            self.dict.push(val);
            self.indices.push(pos);
        } else {
            // Dictionary capacity exceeded: spill to flat vector
            let mut spilled = Vec::with_capacity(self.indices.len() + 1);
            for &idx in &self.indices {
                spilled.push(Arc::clone(&self.dict[idx as usize]));
            }
            spilled.push(val);
            self.spilled = Some(spilled);
            self.dict.clear();
            self.indices.clear();
        }
    }

    #[inline]
    pub fn get_ref(&self, i: usize) -> &Arc<str> {
        if let Some(spilled) = &self.spilled {
            &spilled[i]
        } else {
            let idx = self.indices[i] as usize;
            &self.dict[idx]
        }
    }

    #[inline]
    pub fn get(&self, i: usize) -> Arc<str> {
        Arc::clone(self.get_ref(i))
    }

    #[inline]
    pub fn set(&mut self, i: usize, val: Arc<str>) {
        if let Some(spilled) = &mut self.spilled {
            spilled[i] = val;
            return;
        }

        if let Some(pos) = self
            .dict
            .iter()
            .position(|x| Arc::ptr_eq(x, &val) || **x == *val)
        {
            self.indices[i] = pos as u8;
            return;
        }

        if self.dict.len() < 255 {
            let pos = self.dict.len() as u8;
            self.dict.push(val);
            self.indices[i] = pos;
        } else {
            let mut spilled = Vec::with_capacity(self.indices.len());
            for &idx in &self.indices {
                spilled.push(Arc::clone(&self.dict[idx as usize]));
            }
            spilled[i] = val;
            self.spilled = Some(spilled);
            self.dict.clear();
            self.indices.clear();
        }
    }

    #[inline]
    pub fn swap_remove(&mut self, i: usize) -> Arc<str> {
        if let Some(spilled) = &mut self.spilled {
            spilled.swap_remove(i)
        } else {
            let idx = self.indices.swap_remove(i) as usize;
            Arc::clone(&self.dict[idx])
        }
    }

    #[inline]
    pub fn swap_pop_at(&mut self, p: usize, len: usize) {
        if self.len() == len && p < len {
            if let Some(spilled) = &mut self.spilled {
                spilled.swap_remove(p);
            } else {
                self.indices.swap_remove(p);
            }
        }
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if let Some(spilled) = &mut self.spilled {
            spilled.truncate(len);
        } else {
            self.indices.truncate(len);
        }
    }

    #[inline]
    pub fn as_indices(&self) -> Option<(&[u8], &[Arc<str>])> {
        if self.spilled.is_none() {
            Some((&self.indices, &self.dict))
        } else {
            None
        }
    }

    pub fn to_vec(&self) -> Vec<Arc<str>> {
        if let Some(spilled) = &self.spilled {
            spilled.clone()
        } else {
            self.indices
                .iter()
                .map(|&idx| Arc::clone(&self.dict[idx as usize]))
                .collect()
        }
    }
}

impl std::ops::Index<usize> for DictColumn {
    type Output = Arc<str>;

    #[inline]
    fn index(&self, i: usize) -> &Self::Output {
        self.get_ref(i)
    }
}

/// Move `v[p]` on top of the tail and drop the tail, keeping the vec dense. A
/// no-op when `v` isn't parallel to the rows (the caller's length guard rebuilds).
#[inline]
pub(super) fn swap_pop_at<T>(v: &mut Vec<T>, p: usize, len: usize) {
    if v.len() == len && p < len {
        v.swap_remove(p);
    }
}

/// Inverse of [`swap_pop_at`]: `value` takes slot `p` and whatever was there moves
/// to the tail. A no-op when `v` isn't parallel to the rows.
pub(super) fn swap_push_at<T: Clone>(v: &mut Vec<T>, p: usize, len: usize, value: T) {
    if v.len() != len {
        return;
    }
    if p == len {
        v.push(value);
    } else {
        let moved = v[p].clone();
        v.push(moved);
        v[p] = value;
    }
}

impl Store {
    /// Count `docs.title ~ needle` (no group) using columnar titles + memchr.
    pub fn docs_title_contains_count(&self, needle: &str) -> i64 {
        if needle.is_empty() {
            return self.docs_title.len() as i64;
        }
        let total_rows = self.docs_title.len();
        if total_rows == 0 {
            return 0;
        }
        // A derived physical column, not a result cache: every needle scans the
        // bytes. Mutation invalidates it and the next scan rebuilds it once.
        // Each title is terminated by a 0x00 byte so matching rows advance to
        // the next '\0' in the same cache line without reading an offsets array.
        let (bytes, has_nul) = self.docs_title_packed.get_or_init(|| {
            let total_bytes: usize = self.docs_title.iter().map(|s| s.len() + 1).sum();
            let mut bytes = Vec::with_capacity(total_bytes);
            let mut has_nul = false;
            for title in &self.docs_title {
                let b = title.as_bytes();
                if memchr::memchr(0, b).is_some() {
                    has_nul = true;
                }
                bytes.extend_from_slice(b);
                bytes.push(0);
            }
            (bytes, has_nul)
        });

        let needle_bytes = needle.as_bytes();
        if *has_nul || memchr::memchr(0, needle_bytes).is_some() {
            let finder = memchr::memmem::Finder::new(needle_bytes);
            return self
                .docs_title
                .iter()
                .filter(|t| finder.find(t.as_bytes()).is_some())
                .count() as i64;
        }

        #[inline]
        fn scan_nul_chunk(chunk: &[u8], finder: &memchr::memmem::Finder<'_>) -> i64 {
            let mut cursor = 0usize;
            let mut count = 0i64;
            let len = chunk.len();
            while cursor < len {
                let Some(rel) = finder.find(&chunk[cursor..]) else {
                    break;
                };
                let match_pos = cursor + rel;
                count += 1;
                if let Some(nul_rel) = memchr::memchr(0, &chunk[match_pos..]) {
                    cursor = match_pos + nul_rel + 1;
                } else {
                    break;
                }
            }
            count
        }

        // Only fan out across worker threads when the packed buffer exceeds L2 cache (~512 KiB);
        // below that threshold, single-core SIMD memchr completes in ~2 µs vs ~12 µs thread wakeup.
        let threads = if total_rows >= 32_768 && bytes.len() >= 512 * 1024 {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(12)
                .min((bytes.len() + 65_535) / 65_536)
        } else {
            1
        };

        if threads > 1 {
            let mut bounds = Vec::with_capacity(threads + 1);
            bounds.push(0usize);
            for t in 1..threads {
                let approx = (bytes.len() * t) / threads;
                let prev = *bounds.last().unwrap();
                if approx <= prev {
                    bounds.push(prev);
                } else if let Some(rel) = memchr::memchr(0, &bytes[approx..]) {
                    bounds.push(approx + rel + 1);
                } else {
                    bounds.push(bytes.len());
                }
            }
            bounds.push(bytes.len());

            struct ScanJob {
                ptr: usize,
                len: usize,
                needle_ptr: usize,
                needle_len: usize,
            }
            struct ScanPool {
                txs: Vec<std::sync::mpsc::SyncSender<ScanJob>>,
                rxs: Vec<std::sync::mpsc::Receiver<i64>>,
            }
            static TITLE_SCAN_POOL: OnceLock<std::sync::Mutex<ScanPool>> = OnceLock::new();
            let pool_mtx = TITLE_SCAN_POOL.get_or_init(|| {
                let max_workers = 11;
                let mut txs = Vec::with_capacity(max_workers);
                let mut rxs = Vec::with_capacity(max_workers);
                for i in 0..max_workers {
                    let (job_tx, job_rx) = std::sync::mpsc::sync_channel::<ScanJob>(1);
                    let (res_tx, res_rx) = std::sync::mpsc::sync_channel::<i64>(1);
                    let _ = std::thread::Builder::new()
                        .name(format!("lin-title-scan-{i}"))
                        .spawn(move || {
                            while let Ok(job) = job_rx.recv() {
                                let chunk = unsafe {
                                    std::slice::from_raw_parts(job.ptr as *const u8, job.len)
                                };
                                let ndl = unsafe {
                                    std::slice::from_raw_parts(
                                        job.needle_ptr as *const u8,
                                        job.needle_len,
                                    )
                                };
                                let finder = memchr::memmem::Finder::new(ndl);
                                let c = scan_nul_chunk(chunk, &finder);
                                let _ = res_tx.send(c);
                            }
                        });
                    txs.push(job_tx);
                    rxs.push(res_rx);
                }
                std::sync::Mutex::new(ScanPool { txs, rxs })
            });

            if let Ok(pool) = pool_mtx.try_lock() {
                let workers = (threads - 1).min(pool.txs.len());
                struct ScanDrainGuard<'a> {
                    rxs: &'a [std::sync::mpsc::Receiver<i64>],
                    dispatched: [bool; 11],
                }
                impl Drop for ScanDrainGuard<'_> {
                    fn drop(&mut self) {
                        for (w, sent) in self.dispatched.iter_mut().enumerate() {
                            if *sent {
                                *sent = false;
                                let _ = self.rxs[w].recv();
                            }
                        }
                    }
                }
                let mut scan_guard = ScanDrainGuard {
                    rxs: &pool.rxs,
                    dispatched: [false; 11],
                };
                for w in 0..workers {
                    let b_start = bounds[w];
                    let b_end = bounds[w + 1];
                    if b_start < b_end {
                        let slice = &bytes[b_start..b_end];
                        if pool.txs[w]
                            .send(ScanJob {
                                ptr: slice.as_ptr() as usize,
                                len: slice.len(),
                                needle_ptr: needle_bytes.as_ptr() as usize,
                                needle_len: needle_bytes.len(),
                            })
                            .is_ok()
                        {
                            scan_guard.dispatched[w] = true;
                        }
                    }
                }
                let finder = memchr::memmem::Finder::new(needle_bytes);
                let my_start = bounds[workers];
                let my_end = bounds[threads];
                let mut total = if my_start < my_end {
                    scan_nul_chunk(&bytes[my_start..my_end], &finder)
                } else {
                    0
                };
                for w in 0..workers {
                    if scan_guard.dispatched[w] {
                        scan_guard.dispatched[w] = false;
                        if let Ok(c) = scan_guard.rxs[w].recv() {
                            total += c;
                        }
                    }
                }
                return total;
            }

            let finder = memchr::memmem::Finder::new(needle_bytes);
            std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(threads);
                for t in 0..threads {
                    let b_start = bounds[t];
                    let b_end = bounds[t + 1];
                    if b_start >= b_end {
                        continue;
                    }
                    let chunk = &bytes[b_start..b_end];
                    let f_ref = &finder;
                    handles.push(scope.spawn(move || scan_nul_chunk(chunk, f_ref)));
                }
                handles.into_iter().map(|h| h.join().unwrap()).sum()
            })
        } else {
            let finder = memchr::memmem::Finder::new(needle_bytes);
            scan_nul_chunk(bytes, &finder)
        }
    }

    /// Count `docs.title ~ needle` grouped by layer using columnar titles (no row maps).
    pub fn docs_title_contains_count_by_layer(&self, needle: &str) -> BTreeMap<Arc<str>, i64> {
        let finder = memchr::memmem::Finder::new(needle.as_bytes());
        let mut map: BTreeMap<Arc<str>, i64> = BTreeMap::new();
        let n = self.docs_title.len().min(self.docs_layer.len());
        let titles = &self.docs_title[..n];
        let layers = &self.docs_layer;
        for i in 0..n {
            if finder.find(titles[i].as_bytes()).is_none() {
                continue;
            }
            let layer = &layers[i];
            // Borrow as str so hits reuse the existing key without Arc clone.
            if let Some(c) = map.get_mut(layer.as_ref()) {
                *c += 1;
            } else {
                map.insert(Arc::clone(layer), 1);
            }
        }
        map
    }

    /// Project hot docs fields from parallel columns — Arc clone only, no full row clone.
    /// True when orders SoA matches `collections["orders"]` length.
    pub fn orders_soa_ready(&self) -> bool {
        let n = self.collection("orders").len();
        self.orders_id.len() == n && self.orders_user_id.len() == n && self.orders_total.len() == n
    }

    /// True when users SoA matches `collections["users"]` length.
    pub fn users_soa_ready(&self) -> bool {
        let n = self.collection("users").len();
        self.users_id.len() == n && self.users_email.len() == n
    }

    pub fn orders_id(&self) -> &[Arc<str>] {
        &self.orders_id
    }
    pub fn orders_user_id(&self) -> &[Arc<str>] {
        &self.orders_user_id
    }
    pub fn orders_total(&self) -> &[f64] {
        &self.orders_total
    }
    pub fn docs_wing(&self) -> &DictColumn {
        &self.docs_wing
    }
    pub fn docs_layer(&self) -> &DictColumn {
        &self.docs_layer
    }
    pub fn users_id(&self) -> &[Arc<str>] {
        &self.users_id
    }
    pub fn users_email(&self) -> &[Arc<str>] {
        &self.users_email
    }

    /// Fast projection over `docs` when filtering by `wing == target` or `layer == target`.
    pub fn project_docs_dict_eq(
        &self,
        filter_field: &str,
        target: &str,
        fields: &[String],
        limit: Option<usize>,
    ) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }
        let n = self.docs_id.len();
        if n != self.collections.get("docs").map_or(0, Vec::len) {
            return None;
        }
        let (dict, field_tag) = match filter_field {
            "wing" if self.docs_wing.len() == n => (&self.docs_wing, 1u64 << 32),
            "layer" if self.docs_layer.len() == n => (&self.docs_layer, 2u64 << 32),
            _ => return None,
        };
        let (codes, dict_vals) = dict.as_indices()?;
        let Some(code) = dict_vals
            .iter()
            .position(|v| **v == *target)
            .map(|p| p as u8)
        else {
            return Some(Vec::new());
        };
        if limit == Some(0) {
            return Some(Vec::new());
        }
        let filter_tag = field_tag | (code as u64);
        let id_title = fields.len() == 2 && fields[0] == "id" && fields[1] == "title";

        let recycled = if id_title && limit.is_none() && n >= 1024 {
            ID_TITLE_POOL.lock().ok().and_then(|mut g| g.take())
        } else {
            None
        };

        if let Some(ref rec) = recycled
            && rec.docs_epoch == self.docs_epoch
            && rec.filter_tag == filter_tag
            && !rec.rows.is_empty()
            && rec.first_idx < n
            && rec.last_idx < n
            && row_matches_id_title(
                &rec.rows[0],
                &self.docs_id[rec.first_idx],
                &self.docs_title[rec.first_idx],
            )
            && row_matches_id_title(
                rec.rows.last().unwrap(),
                &self.docs_id[rec.last_idx],
                &self.docs_title[rec.last_idx],
            )
        {
            LAST_ID_TITLE_META.with(|c| {
                c.set((self.docs_epoch, filter_tag, rec.first_idx, rec.last_idx));
            });
            return Some(recycled.unwrap().rows);
        }

        let max_take = limit.unwrap_or(usize::MAX);
        let mut idxs = Vec::with_capacity(limit.unwrap_or(codes.len() / 2 + 16).min(codes.len()));
        for (i, &c) in codes.iter().enumerate() {
            if c == code {
                idxs.push(i);
                if idxs.len() >= max_take {
                    break;
                }
            }
        }
        if idxs.is_empty() {
            return Some(Vec::new());
        }
        if id_title && limit.is_none() && idxs.len() >= 1024 {
            let first_idx = idxs[0];
            let last_idx = *idxs.last().unwrap();
            LAST_ID_TITLE_META.with(|c| {
                c.set((self.docs_epoch, filter_tag, first_idx, last_idx));
            });
            if let Some(mut rec) = recycled
                && rec.rows.len() == idxs.len()
            {
                let total_idxs = idxs.len();
                let docs_id = &self.docs_id;
                let docs_title = &self.docs_title;
                let rows_ptr = rec.rows.as_mut_ptr() as usize;
                let threads = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(1)
                    .min((total_idxs + 1023) / 1024);
                let next_chunk = std::sync::atomic::AtomicUsize::new(0);
                let chunk_size = 4096;
                std::thread::scope(|scope| {
                    for _ in 0..threads {
                        scope.spawn(|| {
                            loop {
                                let start = next_chunk
                                    .fetch_add(chunk_size, std::sync::atomic::Ordering::Relaxed);
                                if start >= total_idxs {
                                    break;
                                }
                                let end = (start + chunk_size).min(total_idxs);
                                for offset in start..end {
                                    let i = idxs[offset];
                                    let row = unsafe { &mut *(rows_ptr as *mut Row).add(offset) };
                                    if row.len() == 2 {
                                        let mut it = row.iter_mut();
                                        if let (Some((k1, v1)), Some((k2, v2))) =
                                            (it.next(), it.next())
                                            && k1 == "id"
                                            && k2 == "title"
                                        {
                                            let same_id = match v1 {
                                                Cell::Text(a) => Arc::ptr_eq(a, &docs_id[i]),
                                                _ => false,
                                            };
                                            if !same_id {
                                                *v1 = Cell::Text(Arc::clone(&docs_id[i]));
                                            }
                                            let same_title = match v2 {
                                                Cell::Text(a) => Arc::ptr_eq(a, &docs_title[i]),
                                                _ => false,
                                            };
                                            if !same_title {
                                                *v2 = Cell::Text(Arc::clone(&docs_title[i]));
                                            }
                                            continue;
                                        }
                                    }
                                    *row = row_id_title(&docs_id[i], &docs_title[i]);
                                }
                            }
                        });
                    }
                });
                return Some(rec.rows);
            }
        }
        self.project_docs_hot(&idxs, fields)
    }

    /// Returns None if any requested field is outside the hot set.
    pub fn project_docs_hot(&self, idxs: &[usize], fields: &[String]) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }

        let n = self.docs_id.len();
        if n != self.collections.get("docs").map_or(0, Vec::len) {
            return None;
        }
        let total_idxs = idxs.len();
        let id_title = fields.len() == 2 && fields[0] == "id" && fields[1] == "title";

        if total_idxs >= 16384 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min((total_idxs + 1023) / 1024);
            if threads > 1 {
                let docs_id = &self.docs_id;
                let docs_title = &self.docs_title;
                let docs_layer = &self.docs_layer;
                let docs_wing = &self.docs_wing;

                let mut out_buffer: Vec<std::mem::MaybeUninit<Row>> =
                    Vec::with_capacity(total_idxs);
                unsafe {
                    out_buffer.set_len(total_idxs);
                }
                let out_ptr = out_buffer.as_mut_ptr() as usize;

                let next_chunk = std::sync::atomic::AtomicUsize::new(0);
                let chunk_size = 4096;

                std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for _ in 0..threads {
                        handles.push(scope.spawn(|| {
                            let k_id = String::from("id");
                            let k_title = String::from("title");
                            loop {
                                let start = next_chunk
                                    .fetch_add(chunk_size, std::sync::atomic::Ordering::Relaxed);
                                if start >= total_idxs {
                                    break;
                                }
                                let end = (start + chunk_size).min(total_idxs);
                                for offset in start..end {
                                    let i = idxs[offset];
                                    let slot = unsafe {
                                        &mut *(out_ptr as *mut std::mem::MaybeUninit<Row>)
                                            .add(offset)
                                    };
                                    if i >= n {
                                        slot.write(acquire_row());
                                        continue;
                                    }
                                    if id_title {
                                        let mut row = BTreeMap::new();
                                        row.insert(
                                            k_id.clone(),
                                            Cell::Text(Arc::clone(&docs_id[i])),
                                        );
                                        row.insert(
                                            k_title.clone(),
                                            Cell::Text(Arc::clone(&docs_title[i])),
                                        );
                                        slot.write(row);
                                        continue;
                                    }
                                    let mut row = BTreeMap::new();
                                    for f in fields {
                                        let cell = match f.as_str() {
                                            "id" => Cell::Text(Arc::clone(&docs_id[i])),
                                            "title" => Cell::Text(Arc::clone(&docs_title[i])),
                                            "layer" => Cell::Text(Arc::clone(&docs_layer[i])),
                                            "wing" => Cell::Text(Arc::clone(&docs_wing[i])),
                                            _ => Cell::Null,
                                        };
                                        row.insert(f.clone(), cell);
                                    }
                                    slot.write(row);
                                }
                            }
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });

                let out = unsafe {
                    let mut b = std::mem::ManuallyDrop::new(out_buffer);
                    Vec::from_raw_parts(b.as_mut_ptr() as *mut Row, total_idxs, total_idxs)
                };
                return Some(out);
            }
        }

        let mut out = Vec::with_capacity(total_idxs);
        for &i in idxs {
            if i >= n {
                continue;
            }
            if id_title {
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                continue;
            }
            let mut row = BTreeMap::new();
            for f in fields {
                let cell = match f.as_str() {
                    "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                    "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                    "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                    "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                    _ => Cell::Null,
                };
                row.insert(f.clone(), cell);
            }
            out.push(row);
        }
        Some(out)
    }

    /// Full docs scan projecting only hot columns when filter is title-contains or always-true path.
    pub fn scan_docs_hot_project(
        &self,
        fields: &[String],
        title_contains: Option<&str>,
    ) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }
        let n = self.docs_id.len();
        let id_title = fields.len() == 2 && fields[0] == "id" && fields[1] == "title";

        if n >= 65536 {
            let threads = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
                .min(8)
                .min((n + 8191) / 8192);
            if threads > 1 {
                let chunk_size = (n + threads - 1) / threads;
                let docs_id = &self.docs_id;
                let docs_title = &self.docs_title;
                let docs_layer = &self.docs_layer;
                let docs_wing = &self.docs_wing;

                let chunks_out: Vec<Vec<Row>> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let start = chunk_idx * chunk_size;
                        if start >= n {
                            break;
                        }
                        let end = (start + chunk_size).min(n);
                        handles.push(scope.spawn(move || {
                            let mut out = Vec::with_capacity(end - start);
                            let finder = title_contains
                                .map(|needle| memchr::memmem::Finder::new(needle.as_bytes()));
                            for i in start..end {
                                if let Some(f) = &finder
                                    && f.find(docs_title[i].as_bytes()).is_none()
                                {
                                    continue;
                                }
                                if id_title {
                                    out.push(row_id_title(&docs_id[i], &docs_title[i]));
                                    continue;
                                }
                                let mut row = BTreeMap::new();
                                for f in fields {
                                    let cell = match f.as_str() {
                                        "id" => Cell::Text(Arc::clone(&docs_id[i])),
                                        "title" => Cell::Text(Arc::clone(&docs_title[i])),
                                        "layer" => Cell::Text(Arc::clone(&docs_layer[i])),
                                        "wing" => Cell::Text(Arc::clone(&docs_wing[i])),
                                        _ => Cell::Null,
                                    };
                                    row.insert(f.clone(), cell);
                                }
                                out.push(row);
                            }
                            out
                        }));
                    }
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mut out = Vec::with_capacity(chunks_out.iter().map(Vec::len).sum());
                for chunk in chunks_out {
                    out.extend(chunk);
                }
                return Some(out);
            }
        }

        let mut out = Vec::new();
        let finder = title_contains.map(|n| memchr::memmem::Finder::new(n.as_bytes()));
        for i in 0..n {
            if let Some(f) = &finder
                && f.find(self.docs_title[i].as_bytes()).is_none()
            {
                continue;
            }
            if id_title {
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                continue;
            }
            let mut row = BTreeMap::new();
            for f in fields {
                let cell = match f.as_str() {
                    "id" => Cell::Text(Arc::clone(&self.docs_id[i])),
                    "title" => Cell::Text(Arc::clone(&self.docs_title[i])),
                    "layer" => Cell::Text(Arc::clone(&self.docs_layer[i])),
                    "wing" => Cell::Text(Arc::clone(&self.docs_wing[i])),
                    _ => Cell::Null,
                };
                row.insert(f.clone(), cell);
            }
            out.push(row);
        }
        Some(out)
    }
}
