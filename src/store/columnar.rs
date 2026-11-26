//! Dictionary-encoded columns (`DictColumn`) and Struct-of-Arrays (SoA) vectorized scan/projection helpers.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use super::Store;
use super::value::{
    Cell, Row, acquire_row, docs_hot_field, empty_arc_str, intern_wing, intern_wing_str,
    row_id_title, shape_id, shape_id_title, take_recycled_id_title_vec,
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
        if self.spilled.is_none()
            && other.spilled.is_none()
            && self.dict.len() == other.dict.len()
            && self
                .dict
                .iter()
                .zip(&other.dict)
                .all(|(a, b)| Arc::ptr_eq(a, b) || a == b)
        {
            return self.indices == other.indices;
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
    fn push_new_dict_val(&mut self, val: Arc<str>) {
        if self.dict.len() < 255 {
            let pos = self.dict.len() as u8;
            self.dict.push(val);
            self.indices.push(pos);
        } else {
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

        self.push_new_dict_val(val);
    }

    /// Push from an optional [`Cell`] without allocating an `Arc<str>` when the string already
    /// resides in `self.dict` (eliminating `SmallText -> Arc<str>` heap churn on bulk inserts).
    #[inline]
    pub fn push_cell(&mut self, cell: Option<&Cell>) {
        if let Some(spilled) = &mut self.spilled {
            let val = match cell {
                Some(Cell::Text(arc)) => intern_wing(Arc::clone(arc)),
                Some(Cell::SmallText(inl)) => intern_wing_str(inl.as_str()),
                _ => empty_arc_str(),
            };
            spilled.push(val);
            return;
        }
        match cell {
            Some(Cell::Text(arc)) => {
                if let Some(pos) = self
                    .dict
                    .iter()
                    .position(|x| Arc::ptr_eq(x, arc) || **x == **arc)
                {
                    self.indices.push(pos as u8);
                    return;
                }
                self.push_new_dict_val(intern_wing(Arc::clone(arc)));
            }
            Some(Cell::SmallText(inl)) => {
                let s = inl.as_str();
                if let Some(pos) = self.dict.iter().position(|x| x.as_ref() == s) {
                    self.indices.push(pos as u8);
                    return;
                }
                self.push_new_dict_val(intern_wing_str(s));
            }
            _ => {
                if let Some(pos) = self.dict.iter().position(|x| x.is_empty()) {
                    self.indices.push(pos as u8);
                    return;
                }
                self.push_new_dict_val(empty_arc_str());
            }
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
    pub fn set_str(&mut self, i: usize, val: &str) {
        if self.spilled.is_none()
            && let Some(pos) = self.dict.iter().position(|x| x.as_ref() == val)
        {
            self.indices[i] = pos as u8;
            return;
        }
        self.set(i, intern_wing_str(val));
    }

    /// Update slot `i` from an optional [`Cell`], looking up in `self.dict` before allocating.
    #[inline]
    pub fn set_cell(&mut self, i: usize, cell: Option<&Cell>) {
        if let Some(spilled) = &mut self.spilled {
            spilled[i] = match cell {
                Some(Cell::Text(arc)) => intern_wing(Arc::clone(arc)),
                Some(Cell::SmallText(inl)) => intern_wing_str(inl.as_str()),
                _ => empty_arc_str(),
            };
            return;
        }
        match cell {
            Some(Cell::Text(arc)) => {
                if let Some(pos) = self
                    .dict
                    .iter()
                    .position(|x| Arc::ptr_eq(x, arc) || **x == **arc)
                {
                    self.indices[i] = pos as u8;
                    return;
                }
                self.set(i, intern_wing(Arc::clone(arc)));
            }
            Some(Cell::SmallText(inl)) => {
                let s = inl.as_str();
                if let Some(pos) = self.dict.iter().position(|x| x.as_ref() == s) {
                    self.indices[i] = pos as u8;
                    return;
                }
                self.set(i, intern_wing_str(s));
            }
            _ => {
                if let Some(pos) = self.dict.iter().position(|x| x.is_empty()) {
                    self.indices[i] = pos as u8;
                    return;
                }
                self.set(i, empty_arc_str());
            }
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

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn as_spilled(&self) -> Option<&[Arc<str>]> {
        self.spilled.as_deref()
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
        let (bytes, has_nul) = self
            .docs_title_packed
            .get_or_init(|| {
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
                Arc::new((bytes, has_nul))
            })
            .as_ref();

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
        fn scan_nul_chunk(
            chunk: &[u8],
            finder: &memchr::memmem::Finder<'_>,
            needle_len: usize,
        ) -> i64 {
            let mut cursor = 0usize;
            let mut count = 0i64;
            let len = chunk.len();
            while cursor < len {
                let Some(rel) = finder.find(&chunk[cursor..]) else {
                    break;
                };
                let match_pos = cursor + rel;
                count += 1;
                let mut p = match_pos + needle_len;
                let short_end = (p + 24).min(len);
                while p < short_end {
                    if chunk[p] == 0 {
                        break;
                    }
                    p += 1;
                }
                if p < short_end {
                    cursor = p + 1;
                } else if let Some(nul_rel) = memchr::memchr(0, &chunk[p..]) {
                    cursor = p + nul_rel + 1;
                } else {
                    break;
                }
            }
            count
        }

        let needle_len = needle_bytes.len();
        // Only fan out across worker threads when > 32k rows AND the packed buffer exceeds L2
        // cache (~512 KiB); below that threshold, 3-way persistent steal lanes cover medium
        // buffers (>= 128 KiB and >= 8k rows) without pool locking.
        let threads = if total_rows > 32_768 && bytes.len() >= 512 * 1024 {
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
                                let c = scan_nul_chunk(chunk, &finder, job.needle_len);
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
                let mut orphaned = [false; 11];
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
                                needle_len,
                            })
                            .is_ok()
                        {
                            scan_guard.dispatched[w] = true;
                        } else {
                            // Worker is gone: its chunk is scanned here, not dropped.
                            orphaned[w] = true;
                        }
                    }
                }
                let finder = memchr::memmem::Finder::new(needle_bytes);
                let my_start = bounds[workers];
                let my_end = bounds[threads];
                let mut total = if my_start < my_end {
                    scan_nul_chunk(&bytes[my_start..my_end], &finder, needle_len)
                } else {
                    0
                };
                for w in 0..workers {
                    if scan_guard.dispatched[w] {
                        scan_guard.dispatched[w] = false;
                        match scan_guard.rxs[w].recv() {
                            Ok(c) => total += c,
                            Err(_) => orphaned[w] = true,
                        }
                    }
                    if orphaned[w] {
                        total +=
                            scan_nul_chunk(&bytes[bounds[w]..bounds[w + 1]], &finder, needle_len);
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
                    handles.push(scope.spawn(move || scan_nul_chunk(chunk, f_ref, needle_len)));
                }
                handles.into_iter().map(|h| h.join().unwrap()).sum()
            })
        } else if bytes.len() >= 128 * 1024 && total_rows >= 8_192 {
            let len = bytes.len();
            let a1 = len / 3;
            let b1 = match memchr::memchr(0, &bytes[a1..]) {
                Some(rel) => a1 + rel + 1,
                None => len,
            };
            let a2 = ((len * 2) / 3).max(b1);
            let b2 = if a2 >= len {
                len
            } else {
                match memchr::memchr(0, &bytes[a2..]) {
                    Some(rel) => a2 + rel + 1,
                    None => len,
                }
            };
            let finder = memchr::memmem::Finder::new(needle_bytes);
            let f_ref = &finder;
            let c0 = &bytes[..b1];
            let c1 = &bytes[b1..b2];
            let c2 = &bytes[b2..];
            let (n0, n1, n2) = crate::work_steal::steal3(
                crate::work_steal::StealLane::MaintFts,
                !c0.is_empty(),
                move || scan_nul_chunk(c0, f_ref, needle_len),
                crate::work_steal::StealLane::MaintIdx,
                move || {
                    if c1.is_empty() {
                        0
                    } else {
                        scan_nul_chunk(c1, f_ref, needle_len)
                    }
                },
                || {
                    if c2.is_empty() {
                        0
                    } else {
                        scan_nul_chunk(c2, f_ref, needle_len)
                    }
                },
            );
            n0 + n1 + n2
        } else {
            let finder = memchr::memmem::Finder::new(needle_bytes);
            scan_nul_chunk(bytes, &finder, needle_len)
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
        if self.is_disk_backed("orders") {
            return false;
        }
        let n = self.collections.get("orders").map_or(0, Vec::len);
        self.orders_id.len() == n
            && self.orders_user_id.len() == n
            && self.orders_total.len() == n
            && self.orders_total_tag.len() == n
    }

    /// True when users SoA matches `collections["users"]` length.
    pub fn users_soa_ready(&self) -> bool {
        if self.is_disk_backed("users") {
            return false;
        }
        let n = self.collections.get("users").map_or(0, Vec::len);
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
    pub fn orders_total_tag(&self) -> &[u8] {
        &self.orders_total_tag
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
        let dict = match filter_field {
            "wing" if self.docs_wing.len() == n => &self.docs_wing,
            "layer" if self.docs_layer.len() == n => &self.docs_layer,
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
        let id_title = fields.len() == 2
            && ((fields[0] == "id" && fields[1] == "title")
                || (fields[0] == "title" && fields[1] == "id"));

        let recycled = if id_title && limit.is_none() && n >= 1024 {
            take_recycled_id_title_vec()
        } else {
            None
        };

        // The pooled vector is only a buffer: it may hold another query's rows or rows the
        // caller edited, so every row is checked against the columns below before returning.
        let max_take = limit.unwrap_or(usize::MAX);
        let mut idxs = Vec::with_capacity(limit.unwrap_or(codes.len() / 2 + 16).min(codes.len()));
        for i in memchr::memchr_iter(code, codes) {
            idxs.push(i);
            if idxs.len() >= max_take {
                break;
            }
        }
        if idxs.is_empty() {
            return Some(Vec::new());
        }
        if id_title && limit.is_none() && idxs.len() >= 1024 {
            if let Some(mut rows) = recycled
                && rows.len() == idxs.len()
            {
                let total_idxs = idxs.len();
                let docs_id = &self.docs_id;
                let docs_title = &self.docs_title;
                let rows_ptr = rows.as_mut_ptr() as usize;
                let id_title_shape = super::value::shape_id_title();
                // Checking a row is two pointer compares: below this many rows, spawning
                // workers costs far more than the pass itself.
                const PAR_REFILL_MIN_ROWS: usize = 65_536;
                let threads = if total_idxs < PAR_REFILL_MIN_ROWS {
                    1
                } else {
                    std::thread::available_parallelism()
                        .map(|p| p.get())
                        .unwrap_or(1)
                        .min((total_idxs + 1023) / 1024)
                };
                let next_chunk = std::sync::atomic::AtomicUsize::new(0);
                let chunk_size = 4096;
                {
                    {
                        let work = || {
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
                                    // Shapes are interned: one pointer compare stands
                                    // for "exactly the fields id and title".
                                    if row.shape().is_some_and(|s| Arc::ptr_eq(s, id_title_shape))
                                        && let [v1, v2] = row.cells_mut()
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
                                    *row = row_id_title(&docs_id[i], &docs_title[i]);
                                }
                            }
                        };
                        // The caller is one of the workers.
                        std::thread::scope(|scope| {
                            for _ in 1..threads {
                                scope.spawn(&work);
                            }
                            work();
                        });
                    }
                }
                return Some(rows);
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
        let id_title = fields.len() == 2
            && ((fields[0] == "id" && fields[1] == "title")
                || (fields[0] == "title" && fields[1] == "id"));
        let plan = (!id_title).then(|| DocHotPlan::new(fields));

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
                let plan_ref = plan.as_ref();

                let mut out_buffer: Vec<std::mem::MaybeUninit<Row>> =
                    Vec::with_capacity(total_idxs);
                unsafe {
                    out_buffer.set_len(total_idxs);
                }
                let out_ptr = out_buffer.as_mut_ptr() as usize;

                let next_chunk = std::sync::atomic::AtomicUsize::new(0);
                let chunk_size = 4096;

                let work = || {
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
                                &mut *(out_ptr as *mut std::mem::MaybeUninit<Row>).add(offset)
                            };
                            if i >= n {
                                slot.write(acquire_row());
                                continue;
                            }
                            if id_title {
                                slot.write(row_id_title(&docs_id[i], &docs_title[i]));
                            } else if let Some(p) = plan_ref {
                                slot.write(p.build_row(i, docs_id, docs_title, docs_layer, docs_wing));
                            }
                        }
                    }
                };
                std::thread::scope(|scope| {
                    for _ in 1..threads {
                        scope.spawn(&work);
                    }
                    work();
                });

                let out = unsafe {
                    let mut b = std::mem::ManuallyDrop::new(out_buffer);
                    Vec::from_raw_parts(b.as_mut_ptr() as *mut Row, total_idxs, total_idxs)
                };
                return Some(out);
            }
        }

        let mut out = Vec::with_capacity(total_idxs);
        if id_title {
            for &i in idxs {
                if i < n {
                    out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                }
            }
        } else if let Some(ref p) = plan {
            for &i in idxs {
                if i < n {
                    out.push(p.build_row(
                        i,
                        &self.docs_id,
                        &self.docs_title,
                        &self.docs_layer,
                        &self.docs_wing,
                    ));
                }
            }
        }
        Some(out)
    }

    /// Project a `PostingList` (`Standard` or `Bitmap`) of `docs` row positions directly
    /// into `Vec<Row>` using hot SoA columns without allocating an intermediate `Vec<usize>`.
    #[allow(dead_code)]
    pub(crate) fn project_docs_hot_posting(
        &self,
        list: &crate::index::PostingList,
        fields: &[String],
        limit: Option<usize>,
    ) -> Option<Vec<Row>> {
        if fields.is_empty() || !fields.iter().all(|f| docs_hot_field(f)) {
            return None;
        }
        if self.is_disk_backed("docs") {
            return None;
        }
        let n = self.docs_id.len();
        if n != self.collections.get("docs").map_or(0, Vec::len) || self.docs_title.len() != n {
            return None;
        }
        if fields.iter().any(|f| f == "layer") && self.docs_layer.len() != n {
            return None;
        }
        if fields.iter().any(|f| f == "wing") && self.docs_wing.len() != n {
            return None;
        }
        if limit == Some(0) || list.is_empty() {
            return Some(Vec::new());
        }
        let max_take = limit.unwrap_or(usize::MAX);
        let want = list.len().min(max_take);
        let id_title = fields.len() == 2
            && ((fields[0] == "id" && fields[1] == "title")
                || (fields[0] == "title" && fields[1] == "id"));

        let mut recycled_buf = None;
        if id_title && limit.is_none() && want >= 1024 {
            if let Some(mut rows) = take_recycled_id_title_vec() {
                if rows.len() == want {
                    let id_title_shape = shape_id_title();
                    let docs_id = &self.docs_id;
                    let docs_title = &self.docs_title;
                    let mut offset = 0usize;
                    list.for_each_ordered(false, |i| {
                        if i >= n {
                            return true;
                        }
                        let row = &mut rows[offset];
                        offset += 1;
                        if row.shape().is_some_and(|s| Arc::ptr_eq(s, id_title_shape))
                            && let [v1, v2] = row.cells_mut()
                        {
                            if !matches!(v1, Cell::Text(a) if Arc::ptr_eq(a, &docs_id[i])) {
                                *v1 = Cell::Text(Arc::clone(&docs_id[i]));
                            }
                            if !matches!(v2, Cell::Text(a) if Arc::ptr_eq(a, &docs_title[i])) {
                                *v2 = Cell::Text(Arc::clone(&docs_title[i]));
                            }
                        } else {
                            *row = row_id_title(&docs_id[i], &docs_title[i]);
                        }
                        offset < max_take
                    });
                    rows.truncate(offset);
                    return Some(rows);
                }
                rows.clear();
                rows.reserve(want);
                recycled_buf = Some(rows);
            }
        }

        let mut out = recycled_buf.unwrap_or_else(|| Vec::with_capacity(want));
        if id_title {
            list.for_each_ordered(false, |i| {
                if i < n {
                    out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
                    if out.len() >= max_take {
                        return false;
                    }
                }
                true
            });
        } else {
            let plan = DocHotPlan::new(fields);
            list.for_each_ordered(false, |i| {
                if i < n {
                    out.push(plan.build_row(
                        i,
                        &self.docs_id,
                        &self.docs_title,
                        &self.docs_layer,
                        &self.docs_wing,
                    ));
                    if out.len() >= max_take {
                        return false;
                    }
                }
                true
            });
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
        let id_title = fields.len() == 2
            && ((fields[0] == "id" && fields[1] == "title")
                || (fields[0] == "title" && fields[1] == "id"));
        let plan = (!id_title).then(|| DocHotPlan::new(fields));

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
                let plan_ref = plan.as_ref();

                let chunks_out: Vec<Vec<Row>> = std::thread::scope(|scope| {
                    let mut handles = Vec::with_capacity(threads);
                    for chunk_idx in 0..threads {
                        let start = chunk_idx * chunk_size;
                        if start >= n {
                            break;
                        }
                        let end = (start + chunk_size).min(n);
                        handles.push(scope.spawn(move || {
                            let cap = match title_contains {
                                None | Some("") => end - start,
                                Some(_) => ((end - start) / 8).clamp(16, 4096).min(end - start),
                            };
                            let mut out = Vec::with_capacity(cap);
                            let finder = title_contains
                                .filter(|s| !s.is_empty())
                                .map(|needle| memchr::memmem::Finder::new(needle.as_bytes()));
                            for i in start..end {
                                if let Some(f) = &finder
                                    && f.find(docs_title[i].as_bytes()).is_none()
                                {
                                    continue;
                                }
                                if id_title {
                                    out.push(row_id_title(&docs_id[i], &docs_title[i]));
                                } else if let Some(p) = plan_ref {
                                    out.push(p.build_row(
                                        i,
                                        docs_id,
                                        docs_title,
                                        docs_layer,
                                        docs_wing,
                                    ));
                                }
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

        let init_cap = match title_contains {
            None | Some("") => n,
            Some(_) => (n / 8).clamp(16, 4096).min(n),
        };
        let mut out = Vec::with_capacity(init_cap);
        let finder = title_contains
            .filter(|s| !s.is_empty())
            .map(|ndl| memchr::memmem::Finder::new(ndl.as_bytes()));
        for i in 0..n {
            if let Some(f) = &finder
                && f.find(self.docs_title[i].as_bytes()).is_none()
            {
                continue;
            }
            if id_title {
                out.push(row_id_title(&self.docs_id[i], &self.docs_title[i]));
            } else if let Some(ref p) = plan {
                out.push(p.build_row(
                    i,
                    &self.docs_id,
                    &self.docs_title,
                    &self.docs_layer,
                    &self.docs_wing,
                ));
            }
        }
        Some(out)
    }
}

#[derive(Copy, Clone)]
pub(super) enum DocHotCol {
    Id,
    Title,
    Layer,
    Wing,
}

pub(super) struct DocHotPlan {
    shape: Arc<super::RowShape>,
    cols: smallvec::SmallVec<[DocHotCol; 4]>,
}

impl DocHotPlan {
    #[inline]
    pub(super) fn new(fields: &[String]) -> Self {
        let shape = if fields.len() == 1 && fields[0] == "id" {
            Arc::clone(shape_id())
        } else if fields.len() == 2
            && ((fields[0] == "id" && fields[1] == "title")
                || (fields[0] == "title" && fields[1] == "id"))
        {
            Arc::clone(shape_id_title())
        } else {
            Row::shape_of(fields)
        };
        let cols = shape
            .names()
            .iter()
            .map(|name| match name.as_str() {
                "id" => DocHotCol::Id,
                "title" => DocHotCol::Title,
                "layer" => DocHotCol::Layer,
                _ => DocHotCol::Wing,
            })
            .collect();
        Self { shape, cols }
    }

    #[inline]
    pub(super) fn build_row(
        &self,
        i: usize,
        docs_id: &[Arc<str>],
        docs_title: &[Arc<str>],
        docs_layer: &DictColumn,
        docs_wing: &DictColumn,
    ) -> Row {
        #[inline]
        fn col_cell(
            col: DocHotCol,
            i: usize,
            docs_id: &[Arc<str>],
            docs_title: &[Arc<str>],
            docs_layer: &DictColumn,
            docs_wing: &DictColumn,
        ) -> Cell {
            match col {
                DocHotCol::Id => Cell::Text(Arc::clone(&docs_id[i])),
                DocHotCol::Title => Cell::Text(Arc::clone(&docs_title[i])),
                DocHotCol::Layer => Cell::Text(Arc::clone(&docs_layer[i])),
                DocHotCol::Wing => Cell::Text(Arc::clone(&docs_wing[i])),
            }
        }
        match self.cols.as_slice() {
            [c0] => Row::from_single(
                Arc::clone(&self.shape),
                col_cell(*c0, i, docs_id, docs_title, docs_layer, docs_wing),
            ),
            [c0, c1] => Row::from_pair(
                Arc::clone(&self.shape),
                col_cell(*c0, i, docs_id, docs_title, docs_layer, docs_wing),
                col_cell(*c1, i, docs_id, docs_title, docs_layer, docs_wing),
            ),
            cols => Row::from_parts(
                Arc::clone(&self.shape),
                cols.iter()
                    .map(|&c| col_cell(c, i, docs_id, docs_title, docs_layer, docs_wing)),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::PostingList;

    fn seed_docs_store(n: usize) -> Store {
        let mut store = Store::empty("test/8");
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            let title = if i % 17 == 0 {
                // >= 24 bytes after "wal" to exercise the memchr fallback path too
                format!("doc {i} wal extended_suffix_0123456789_abcdefghijklmnop")
            } else if i % 5 == 0 {
                // needle right at the end before NUL + duplicate needle in same title
                format!("doc {i} wal and trailing wal")
            } else {
                format!("doc {i} ordinary note")
            };
            rows.push(Row::from([
                ("id", Cell::from_text(format!("d{i}"))),
                ("uri", Cell::from_text(format!("u{i}"))),
                ("title", Cell::from_text(title)),
                ("wing", Cell::from_text(if i % 2 == 0 { "rag" } else { "sys" })),
                ("layer", Cell::from_text(if i % 3 == 0 { "raw" } else { "wiki" })),
            ]));
        }
        store.collections.insert("docs".into(), rows);
        store.rebuild_row_maps();
        store
    }

    #[test]
    fn title_contains_count_covers_inline_nul_steal3_and_pool() {
        for n in [128usize, 10_000, 36_000] {
            let store = seed_docs_store(n);
            let expected = (0..n).filter(|i| i % 17 == 0 || i % 5 == 0).count() as i64;
            assert_eq!(store.docs_title_contains_count("wal"), expected, "n={n}");
            assert_eq!(
                store.docs_title_contains_count("trailing wal"),
                (0..n).filter(|i| i % 17 != 0 && i % 5 == 0).count() as i64
            );
            assert_eq!(store.docs_title_contains_count(""), n as i64);
            assert_eq!(store.docs_title_contains_count("never-present-token"), 0);
        }
    }

    #[test]
    fn project_docs_hot_posting_handles_standard_and_bitmap() {
        let store = seed_docs_store(256);
        let mut std_pl = PostingList::new(false, 256);
        let mut bmp_pl = PostingList::new(true, 256);
        for i in (0..256).step_by(4) {
            std_pl.insert(i);
            bmp_pl.insert(i);
        }
        let f_id_title = vec!["title".to_string(), "id".to_string()];
        let f_multi = vec!["id".to_string(), "wing".to_string(), "layer".to_string()];

        let r_std = store.project_docs_hot_posting(&std_pl, &f_id_title, None).unwrap();
        let r_bmp = store.project_docs_hot_posting(&bmp_pl, &f_id_title, None).unwrap();
        assert_eq!(r_std, r_bmp);
        assert_eq!(r_std.len(), 64);
        assert_eq!(r_std[0].get("id").and_then(Cell::text), Some("d0"));
        assert_eq!(r_std[1].get("id").and_then(Cell::text), Some("d4"));

        let r_lim = store.project_docs_hot_posting(&bmp_pl, &f_multi, Some(5)).unwrap();
        assert_eq!(r_lim.len(), 5);
        assert_eq!(r_lim[4].get("id").and_then(Cell::text), Some("d16"));
        assert_eq!(r_lim[4].get("wing").and_then(Cell::text), Some("rag"));
        assert_eq!(store.project_docs_hot_posting(&bmp_pl, &f_multi, Some(0)).unwrap().len(), 0);
        assert!(store.project_docs_hot_posting(&bmp_pl, &["body".to_string()], None).is_none());
    }
}
