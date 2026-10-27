use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use rustc_hash::FxHashMap;
#[cfg(feature = "parallel")]
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

use crate::ast::{CmpOp, Pred, Value};
use crate::catalog::{Catalog, IndexDef};
use crate::store::{Cell, Row};

/// Bloom filter for fast negative lookups during index seeks
/// Reduces false positives with minimal memory overhead
#[cfg(feature = "parallel")]
#[derive(Debug, Clone)]
struct BloomFilter {
    /// Bit array
    bits: Vec<u64>,
    num_hashes: usize,
    #[allow(dead_code)]
    capacity: usize,
}

#[cfg(feature = "parallel")]
impl BloomFilter {
    fn new(capacity: usize) -> Self {
        let num_bits = 8 * 1024 * 1024; // 1M bits = 128KB
        let num_hashes = 7;

        Self {
            bits: vec![0u64; num_bits / 64],
            num_hashes,
            capacity,
        }
    }

    fn hash<T: Hash + ?Sized>(&self, item: &T) -> u64 {
        let mut hasher = FxHasher::default();
        item.hash(&mut hasher);
        hasher.finish()
    }

    fn add<T: Hash + ?Sized>(&mut self, item: &T) {
        if self.bits.is_empty() {
            return;
        }

        let base_hash = self.hash(item);
        let delta = base_hash.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17) | 1;

        for i in 0..self.num_hashes {
            let h = base_hash.wrapping_add((i as u64).wrapping_mul(delta));
            let bit_pos = (h as usize) % (self.bits.len() * 64);
            let mask = 1u64 << (bit_pos % 64);
            self.bits[bit_pos / 64] |= mask;
        }
    }

    fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        if self.bits.is_empty() {
            return true;
        }

        let base_hash = self.hash(item);
        let delta = base_hash.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17) | 1;

        for i in 0..self.num_hashes {
            let h = base_hash.wrapping_add((i as u64).wrapping_mul(delta));
            let bit_pos = (h as usize) % (self.bits.len() * 64);
            let mask = 1u64 << (bit_pos % 64);

            if self.bits[bit_pos / 64] & mask == 0 {
                return false;
            }
        }
        true
    }
}

/// Composite bloom filter for multi-key index checks
#[cfg(feature = "parallel")]
#[allow(dead_code)]
pub struct MultiBloomFilter {
    filters: Vec<BloomFilter>,
}

#[cfg(feature = "parallel")]
#[allow(dead_code)]
impl MultiBloomFilter {
    pub fn new(num_filters: usize, capacity: usize) -> Self {
        Self {
            filters: (0..num_filters)
                .map(|_| BloomFilter::new(capacity))
                .collect(),
        }
    }

    pub fn add_keys(&mut self, keys: &[IndexKey]) {
        for key in keys {
            for filter in &mut self.filters {
                filter.add(key);
            }
        }
    }

    pub fn check_all(&self, target: &IndexKey) -> bool {
        self.filters.iter().all(|f| f.contains(target))
    }
}

#[derive(Debug, Clone)]
pub enum IndexPart {
    Min,
    Null,
    Bool(bool),
    Int(i64),
    /// Ordered f64 bucket, with an exact integer tie-breaker. The tie-breaker
    /// preserves distinct large integer keys for unique indexes.
    Num(u64, i128),
    Time(i64),
    Text(Arc<str>),
    /// Small String Optimization: inline storage for strings <= 22 bytes.
    SmallText(crate::store::InlineStr),
    Max,
}

#[inline]
fn part_rank(p: &IndexPart) -> u8 {
    match p {
        IndexPart::Min => 0,
        IndexPart::Null => 1,
        IndexPart::Bool(_) => 2,
        IndexPart::Int(_) => 3,
        IndexPart::Num(_, _) => 4,
        IndexPart::Time(_) => 5,
        IndexPart::Text(_) | IndexPart::SmallText(_) => 6,
        IndexPart::Max => 7,
    }
}

impl Ord for IndexPart {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let r1 = part_rank(self);
        let r2 = part_rank(other);
        if r1 != r2 {
            return r1.cmp(&r2);
        }
        match (self, other) {
            (IndexPart::Min, IndexPart::Min) => std::cmp::Ordering::Equal,
            (IndexPart::Null, IndexPart::Null) => std::cmp::Ordering::Equal,
            (IndexPart::Bool(a), IndexPart::Bool(b)) => a.cmp(b),
            (IndexPart::Int(a), IndexPart::Int(b)) => a.cmp(b),
            (IndexPart::Num(b1, t1), IndexPart::Num(b2, t2)) => b1.cmp(b2).then_with(|| t1.cmp(t2)),
            (IndexPart::Time(a), IndexPart::Time(b)) => a.cmp(b),
            (IndexPart::Text(a), IndexPart::Text(b)) => a.cmp(b),
            (IndexPart::SmallText(a), IndexPart::SmallText(b)) => a.cmp(b),
            (IndexPart::Text(a), IndexPart::SmallText(b)) => a.as_ref().cmp(b.as_str()),
            (IndexPart::SmallText(a), IndexPart::Text(b)) => a.as_str().cmp(b.as_ref()),
            (IndexPart::Max, IndexPart::Max) => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl PartialOrd for IndexPart {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for IndexPart {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for IndexPart {}

impl Hash for IndexPart {
    fn hash<H: Hasher>(&self, state: &mut H) {
        part_rank(self).hash(state);
        match self {
            IndexPart::Min | IndexPart::Null | IndexPart::Max => {}
            IndexPart::Bool(b) => b.hash(state),
            IndexPart::Int(n) => n.hash(state),
            IndexPart::Num(b, t) => {
                b.hash(state);
                t.hash(state);
            }
            IndexPart::Time(t) => t.hash(state),
            IndexPart::Text(s) => s.as_ref().hash(state),
            IndexPart::SmallText(s) => s.as_str().hash(state),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexKey(pub smallvec::SmallVec<[IndexPart; 2]>);

/// Live secondary index: BTree over composite keys → row indices in the collection vec.
#[derive(Debug, Clone)]
pub struct LiveIndex {
    pub def: IndexDef,
    /// Either Vec<usize> (standard) or BitSet (for low-cardinality columns)
    pub forward: BTreeMap<IndexKey, PostingList>,
    pub reverse: FxHashMap<usize, IndexKey>,

    /// Bloom filter for fast negative lookups during seek operations
    #[cfg(feature = "parallel")]
    bloom_filter: Option<BloomFilter>,
}

#[derive(Debug, Clone)]
pub struct IndexUse {
    pub def: IndexDef,
    pub eqs: Vec<(String, Value)>,
    pub range: Option<(String, CmpOp, Value)>,
}

/// Posting list - either Vec<usize> (standard) or BitSet (for low-cardinality columns)
#[derive(Debug, Clone, PartialEq)]
pub enum PostingList {
    Standard(Vec<usize>),
    Bitmap(BitSet),
}

impl PostingList {
    pub fn new(is_bitmap: bool, capacity: usize) -> Self {
        if is_bitmap {
            PostingList::Bitmap(BitSet::new(capacity))
        } else {
            PostingList::Standard(Vec::new())
        }
    }

    #[inline]
    pub fn insert(&mut self, pos: usize) {
        match self {
            PostingList::Standard(vec) => {
                if vec.len() >= 32 {
                    let mut set = BitSet::new(pos + 1);
                    for &p in vec.iter() {
                        set.insert(p);
                    }
                    set.insert(pos);
                    *self = PostingList::Bitmap(set);
                } else {
                    vec.push(pos);
                }
            }
            PostingList::Bitmap(set) => set.insert(pos),
        }
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        match self {
            PostingList::Standard(vec) => {
                if let Some(p) = vec.iter().position(|&x| x == pos) {
                    vec.swap_remove(p);
                }
            }
            PostingList::Bitmap(set) => set.remove(pos),
        }
    }

    #[inline]
    pub fn push_back(&mut self, pos: usize) {
        self.insert(pos);
    }

    #[inline]
    pub fn push(&mut self, pos: usize) {
        self.push_back(pos);
    }

    pub fn sort_unstable(&mut self) {
        match self {
            PostingList::Standard(vec) => vec.sort_unstable(),
            PostingList::Bitmap(_) => { /* already sorted */ }
        }
    }

    pub fn dedup(&mut self) {
        match self {
            PostingList::Standard(vec) => vec.dedup(),
            PostingList::Bitmap(_) => { /* no duplicates in bitmap */ }
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.count()
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub fn is_bitmap(&self) -> bool {
        matches!(self, PostingList::Bitmap(_))
    }

    pub fn count(&self) -> usize {
        match self {
            PostingList::Standard(vec) => vec.len(),
            PostingList::Bitmap(set) => set.count(),
        }
    }

    #[inline]
    pub fn extend_into(&self, out: &mut Vec<usize>) {
        match self {
            PostingList::Standard(vec) => out.extend_from_slice(vec),
            PostingList::Bitmap(set) => set.extend_into(out),
        }
    }

    #[inline]
    pub fn has_other_than(&self, pos: usize) -> bool {
        match self {
            PostingList::Standard(vec) => vec.iter().any(|&x| x != pos),
            PostingList::Bitmap(set) => {
                let cnt = set.count();
                cnt > 1 || (cnt == 1 && !set.contains(pos))
            }
        }
    }

    pub fn to_indices(&self) -> Vec<usize> {
        match self {
            PostingList::Standard(vec) => vec.clone(),
            PostingList::Bitmap(set) => set.to_indices(),
        }
    }

    pub fn and_assign(&mut self, other: PostingList) {
        match (self, other) {
            (PostingList::Bitmap(self_set), PostingList::Bitmap(other_set)) => {
                self_set.and_assign(&other_set);
            }
            (PostingList::Standard(a), PostingList::Standard(mut b)) => {
                a.sort_unstable();
                a.dedup();
                b.sort_unstable();
                b.dedup();
                let mut result = Vec::with_capacity(a.len().min(b.len()));
                let (mut i, mut j) = (0, 0);
                while i < a.len() && j < b.len() {
                    match a[i].cmp(&b[j]) {
                        std::cmp::Ordering::Less => i += 1,
                        std::cmp::Ordering::Greater => j += 1,
                        std::cmp::Ordering::Equal => {
                            result.push(a[i]);
                            i += 1;
                            j += 1;
                        }
                    }
                }
                *a = result;
            }
            (this, other) => {
                let mut a = this.to_indices();
                let mut b = other.to_indices();
                a.sort_unstable();
                a.dedup();
                b.sort_unstable();
                b.dedup();
                let mut result = Vec::with_capacity(a.len().min(b.len()));
                let (mut i, mut j) = (0, 0);
                while i < a.len() && j < b.len() {
                    match a[i].cmp(&b[j]) {
                        std::cmp::Ordering::Less => i += 1,
                        std::cmp::Ordering::Greater => j += 1,
                        std::cmp::Ordering::Equal => {
                            result.push(a[i]);
                            i += 1;
                            j += 1;
                        }
                    }
                }
                *this = PostingList::Standard(result);
            }
        }
    }

    pub fn or_assign(&mut self, other: PostingList) {
        match (self, other) {
            (PostingList::Bitmap(self_set), PostingList::Bitmap(other_set)) => {
                self_set.or_assign(&other_set);
            }
            (PostingList::Standard(a), PostingList::Standard(b)) => {
                a.extend(b);
                a.sort_unstable();
                a.dedup();
            }
            (this, other) => {
                let mut a = this.to_indices();
                a.extend(other.to_indices());
                a.sort_unstable();
                a.dedup();
                *this = PostingList::Standard(a);
            }
        }
    }
}

impl LiveIndex {
    pub fn new(def: IndexDef) -> Self {
        Self {
            def,
            forward: BTreeMap::new(),
            reverse: FxHashMap::default(),
            #[cfg(feature = "parallel")]
            bloom_filter: None,
        }
    }

    /// Build bloom filter after initial index population (optimization)
    #[cfg(feature = "parallel")]
    pub fn build_bloom_filter(&mut self) {
        let num_keys = self.forward.len();
        if num_keys == 0 {
            return;
        }

        self.bloom_filter = Some(BloomFilter::new(num_keys));

        // Add all keys to bloom filter for fast negative lookups
        for key in self.forward.keys() {
            self.bloom_filter.as_mut().unwrap().add(key);
        }
    }

    /// Check if target key might exist using bloom filter (fast path before BTree lookup)
    #[inline]
    pub fn maybe_exists_fast(&self, _key: &IndexKey) -> bool {
        #[cfg(feature = "parallel")]
        if let Some(ref filter) = self.bloom_filter {
            return filter.contains(_key);
        }

        // Fallback: always assume present when no filter or feature not enabled
        true
    }

    pub fn key_of(&self, row: &Row) -> IndexKey {
        let mut parts = smallvec::SmallVec::with_capacity(self.def.fields.len());
        for f in &self.def.fields {
            parts.push(cell_part(row.get(f).unwrap_or(&Cell::Null)));
        }
        IndexKey(parts)
    }

    pub fn insert_at(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        // Partial index check: only insert if pred matches (or no pred)
        if let Some(ref pred) = self.def.pred
            && !crate::exec::eval_pred(pred, row, crate::store::now_ms())
        {
            // An update can move an indexed row out of the partial predicate.
            self.remove_at(idx);
            return Ok(());
        }

        let key = self.key_of(row);
        self.insert_key(idx, key)
    }

    /// Append-only bulk path: no reverse-key replace (fresh indices).
    pub fn insert_at_new(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        // Partial index check: only insert if pred matches (or no pred)
        if let Some(ref pred) = self.def.pred
            && !crate::exec::eval_pred(pred, row, crate::store::now_ms())
        {
            return Ok(()); // Skip indexing for non-matching rows
        }

        let key = self.key_of(row);
        use std::collections::btree_map::Entry;

        // Check if this is a bitmap index (for boolean fields)
        let is_bitmap = self
            .def
            .bitmap_card_field
            .as_ref()
            .is_some_and(|field| matches!(row.get(field), Some(Cell::Bool(_))));

        match self.forward.entry(key) {
            Entry::Vacant(v) => {
                self.reverse.insert(idx, v.key().clone());
                let mut posting = PostingList::new(is_bitmap, 1 << 16);
                posting.push(idx);
                v.insert(posting);
            }
            Entry::Occupied(mut o) => {
                if self.def.unique {
                    return Err(format!("unique index {}: duplicate key", self.def.label()));
                }
                self.reverse.insert(idx, o.key().clone());
                o.get_mut().push(idx);
            }
        }
        Ok(())
    }

    /// Determine if this index should use bitmap encoding based on cardinality estimation
    #[cfg(test)]
    fn should_use_bitmap(&self, _orig_idx: usize, row: &Row, rows: &[Row]) -> bool {
        // If bitmap_card_field is set, estimate cardinality from sample
        if let Some(field_name) = self.def.bitmap_card_field.as_ref() {
            // Quick check: if first value is boolean, always use bitmap
            if matches!(row.get(field_name), Some(Cell::Bool(_))) {
                return true;
            }

            // Sample up to 100 rows to estimate cardinality
            let sample_size = std::cmp::min(100, rows.len());
            let mut distinct_values = rustc_hash::FxHashSet::default();

            for row in rows.iter().take(sample_size) {
                if let Some(cell) = row.get(field_name) {
                    if matches!(cell, Cell::Bool(_)) {
                        return true;
                    }
                    distinct_values.insert(cell);
                }
            }

            // Use bitmap if cardinality <= 16 (or <= 32 for larger indexes)
            let threshold = if self.forward.len() > 10_000 { 32 } else { 16 };
            !distinct_values.is_empty() && distinct_values.len() <= threshold
        } else {
            false
        }
    }

    /// Batch-only insert for slab operations: compute all keys first, then bulk insert
    /// to reduce per-row HashMap overhead and key cloning.
    pub fn insert_slab_batch(&mut self, start: usize, rows: &[Row]) -> Result<(), String> {
        let now = crate::store::now_ms();
        // Partial index check: only insert matching rows
        let keyed_rows: Vec<(usize, &Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.def
                    .pred
                    .as_ref()
                    .is_none_or(|pred| crate::exec::eval_pred(pred, row, now))
            })
            .map(|(i, row)| (start + i, row))
            .collect();

        if keyed_rows.is_empty() {
            return Ok(());
        }

        // Pre-compute all keys in a single pass
        let keys: Vec<_> = keyed_rows.iter().map(|(_, r)| self.key_of(r)).collect();

        let bitmap_sample = self.def.bitmap_card_field.as_ref().map(|field| {
            let mut distinct = rustc_hash::FxHashSet::default();
            for row in rows.iter().take(100) {
                if let Some(cell) = row.get(field) {
                    if matches!(cell, Cell::Bool(_)) {
                        return (true, 0);
                    }
                    distinct.insert(cell);
                }
            }
            (false, distinct.len())
        });

        // Bulk insert using pre-computed keys
        for ((orig_idx, row), key) in keyed_rows.iter().zip(keys.into_iter()) {
            use std::collections::btree_map::Entry;

            // Auto-detect bitmap index based on field cardinality
            // If bitmap_card_field is set, estimate cardinality from sample
            let is_bitmap = self
                .def
                .bitmap_card_field
                .as_ref()
                .zip(bitmap_sample)
                .is_some_and(|(field, (sample_has_bool, count))| {
                    matches!(row.get(field), Some(Cell::Bool(_)))
                        || sample_has_bool
                        || (count > 0 && count <= if self.forward.len() > 10_000 { 32 } else { 16 })
                });

            match self.forward.entry(key) {
                Entry::Vacant(v) => {
                    self.reverse.insert(*orig_idx, v.key().clone());
                    let mut posting = PostingList::new(is_bitmap, 1 << 16);
                    posting.push(*orig_idx);
                    v.insert(posting);
                }
                Entry::Occupied(mut o) => {
                    if self.def.unique {
                        return Err(format!("unique index {}: duplicate key", self.def.label()));
                    }
                    self.reverse.insert(*orig_idx, o.key().clone());
                    o.get_mut().push(*orig_idx);
                }
            }
        }
        Ok(())
    }

    fn insert_key(&mut self, idx: usize, key: IndexKey) -> Result<(), String> {
        if self.def.unique
            && let Some(ids) = self.forward.get(&key)
            && ids.has_other_than(idx)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        if self.reverse.get(&idx) == Some(&key) {
            return Ok(());
        }

        // Check if this is a bitmap index (low cardinality column)
        let is_bitmap = self.def.bitmap_card_field.as_ref().is_some_and(|_field| {
            key.0
                .first()
                .is_some_and(|cell| matches!(cell, IndexPart::Bool(_)))
        });

        if let Some(old) = self.reverse.insert(idx, key.clone())
            && let Some(vec) = self.forward.get_mut(&old)
        {
            vec.remove(idx);
            if vec.count() == 0 {
                self.forward.remove(&old);
            }
        }

        // Use BitSet for boolean fields (≤2 distinct values)
        match self.forward.entry(key) {
            std::collections::btree_map::Entry::Vacant(v) => {
                self.reverse.insert(idx, v.key().clone());
                let mut posting = PostingList::new(is_bitmap, 1 << 16);
                posting.insert(idx);
                v.insert(posting);
            }
            std::collections::btree_map::Entry::Occupied(mut o) => {
                self.reverse.insert(idx, o.key().clone());
                o.get_mut().insert(idx);
            }
        }
        Ok(())
    }

    pub fn remove_at(&mut self, idx: usize) {
        if let Some(key) = self.reverse.remove(&idx)
            && let Some(vec) = self.forward.get_mut(&key)
        {
            vec.remove(idx);
            if vec.count() == 0 {
                self.forward.remove(&key);
            }
        }
    }

    pub fn move_at(&mut self, from: usize, to: usize) {
 let _phase_probe=crate::phase_probe::Span::new(13);

        if let Some(key) = self.reverse.remove(&from) {
            if let Some(vec) = self.forward.get_mut(&key) {
                vec.remove(from);
                vec.insert(to);
            }
            self.reverse.insert(to, key);
        }
    }

    fn bounds(&self, use_: &IndexUse, now: i64) -> (Bound<IndexKey>, Bound<IndexKey>) {
        let arity = self.def.fields.len();
        let mut lo: smallvec::SmallVec<[IndexPart; 2]> = smallvec::SmallVec::with_capacity(arity);
        let mut hi: smallvec::SmallVec<[IndexPart; 2]> = smallvec::SmallVec::with_capacity(arity);
        for (_, v) in &use_.eqs {
            let p = value_part(v, now);
            lo.push(numeric_low(&p));
            hi.push(numeric_high(&p));
        }
        while lo.len() < arity {
            lo.push(IndexPart::Min);
        }
        while hi.len() < arity {
            hi.push(IndexPart::Max);
        }
        if let Some((field, op, val)) = &use_.range
            && let Some(pos) = self.def.fields.iter().position(|f| f == field)
        {
            let p = value_part(val, now);
            match op {
                CmpOp::Gt => {
                    lo[pos] = numeric_high(&p);
                    lo[pos + 1..].fill(IndexPart::Max);
                }
                CmpOp::Ge => lo[pos] = numeric_low(&p),
                CmpOp::Lt => {
                    hi[pos] = numeric_low(&p);
                    hi[pos + 1..].fill(IndexPart::Min);
                    // Null has no numeric value and must not enter a covered range.
                    lo[pos] = match p {
                        IndexPart::Num(_, _) => IndexPart::Num(0, i128::MIN),
                        IndexPart::Time(_) => IndexPart::Time(i64::MIN),
                        _ => lo[pos].clone(),
                    };
                }
                CmpOp::Le => {
                    hi[pos] = numeric_high(&p);
                    lo[pos] = match p {
                        IndexPart::Num(_, _) => IndexPart::Num(0, i128::MIN),
                        IndexPart::Time(_) => IndexPart::Time(i64::MIN),
                        _ => lo[pos].clone(),
                    };
                }
                CmpOp::Eq | CmpOp::Ne => {}
            }
        }
        let start = match &use_.range {
            Some((_, CmpOp::Gt, _)) => Bound::Excluded(IndexKey(lo)),
            _ => Bound::Included(IndexKey(lo)),
        };
        let end = match &use_.range {
            Some((_, CmpOp::Lt, _)) => Bound::Excluded(IndexKey(hi)),
            _ => Bound::Included(IndexKey(hi)),
        };
        (start, end)
    }

    /// Count matching row indices without allocating an id/index list.
    /// Optional early termination if count exceeds threshold for performance.
    pub fn seek_count(&self, use_: &IndexUse, now: i64) -> usize {
        let (start, end) = self.bounds(use_, now);
        self.forward
            .range((start, end))
            .map(|(_, list)| list.count())
            .sum()
    }

    /// Count with early termination - returns None if exceeds threshold
    pub fn seek_count_with_early_termination(
        &self,
        use_: &IndexUse,
        now: i64,
        threshold: usize,
    ) -> Option<usize> {
        let (start, end) = self.bounds(use_, now);

        let mut count = 0;
        for (_, list) in self.forward.range((start, end)) {
            count += list.count();
            if count > threshold {
                return None; // Exceeded threshold
            }
        }
        Some(count)
    }

    /// Seek row indices directly calling closure for each index (zero allocations)
    pub fn seek_each(&self, use_: &IndexUse, now: i64, mut f: impl FnMut(usize)) {
        let (start, end) = self.bounds(use_, now);

        #[cfg(feature = "parallel")]
        if let (Bound::Included(lo), Bound::Included(hi)) = (&start, &end)
            && lo == hi
            && let Some(ref filter) = self.bloom_filter
            && !filter.contains(lo)
        {
            return;
        }

        for (_, list) in self.forward.range((start, end)) {
            match list {
                PostingList::Standard(vec) => {
                    for &pos in vec {
                        f(pos);
                    }
                }
                PostingList::Bitmap(set) => {
                    set.for_each(&mut f);
                }
            }
        }
    }

    /// Seek row indices by range bounds, appending directly into output vector
    pub fn seek_idxs_into(&self, use_: &IndexUse, now: i64, out: &mut Vec<usize>) {
        let (start, end) = self.bounds(use_, now);

        #[cfg(feature = "parallel")]
        if let (Bound::Included(lo), Bound::Included(hi)) = (&start, &end)
            && lo == hi
            && let Some(ref filter) = self.bloom_filter
            && !filter.contains(lo)
        {
            return;
        }

        for (_, list) in self.forward.range((start, end)) {
            list.extend_into(out);
        }
    }

    /// Seek row indices by range bounds: extend output vector from BTreeMap iteration
    pub fn seek_idxs(&self, use_: &IndexUse, now: i64) -> Vec<usize> {
        let mut out = Vec::new();
        self.seek_idxs_into(use_, now, &mut out);
        out
    }

    /// Parallel bulk seek across multiple index uses - each use is independent
    #[cfg(feature = "parallel")]
    pub fn seek_idxs_parallel(&self, uses: &[crate::index::IndexUse], now: i64) -> Vec<Vec<usize>> {
        if uses.is_empty() {
            return Vec::new();
        }

        // Each index seek runs independently, so we can parallelize
        use rayon::prelude::*;

        uses.par_iter()
            .map(|use_| {
                let (start, end) = self.bounds(use_, now);
                let mut out = Vec::new();
                for (_, list) in self.forward.range((start, end)) {
                    list.extend_into(&mut out);
                }
                out
            })
            .collect()
    }
}

fn numeric_low(p: &IndexPart) -> IndexPart {
    match p {
        IndexPart::Num(bits, _) => IndexPart::Num(*bits, i128::MIN),
        _ => p.clone(),
    }
}

fn numeric_high(p: &IndexPart) -> IndexPart {
    match p {
        IndexPart::Num(bits, _) => IndexPart::Num(*bits, i128::MAX),
        _ => p.clone(),
    }
}

fn numeric_part(n: f64, integer: Option<i64>) -> IndexPart {
    if n.is_nan() {
        return IndexPart::Null;
    }
    let n = if n == 0.0 { 0.0 } else { n };
    let bits = n.to_bits();
    let ordered = if bits >> 63 != 0 {
        !bits
    } else {
        bits ^ (1 << 63)
    };
    let integer = integer.or_else(|| {
        (n.fract() == 0.0 && n >= i64::MIN as f64 && n < -(i64::MIN as f64)).then_some(n as i64)
    });
    IndexPart::Num(ordered, integer.map(i128::from).unwrap_or(i128::MIN + 1))
}

fn cell_part(c: &Cell) -> IndexPart {
    match c {
        Cell::Null => IndexPart::Null,
        Cell::Bool(b) => IndexPart::Bool(*b),
        Cell::Int(n) => numeric_part(*n as f64, Some(*n)),
        Cell::Time(n) => IndexPart::Time(*n),
        Cell::Text(s) => IndexPart::Text(Arc::clone(s)),
        Cell::SmallText(s) => IndexPart::SmallText(*s),
        Cell::Float(n) => numeric_part(*n, None),
        Cell::Vec(_) => IndexPart::Null,
    }
}

fn value_part(v: &Value, now: i64) -> IndexPart {
    match v {
        Value::String(s) => {
            if let Some(inline) = crate::store::InlineStr::try_new(s.as_ref()) {
                IndexPart::SmallText(inline)
            } else {
                IndexPart::Text(Arc::clone(s))
            }
        }
        Value::Name(s) => {
            if let Some(inline) = crate::store::InlineStr::try_new(s.as_str()) {
                IndexPart::SmallText(inline)
            } else {
                IndexPart::Text(Arc::from(s.as_str()))
            }
        }
        Value::Int(n) => numeric_part(*n as f64, Some(*n)),
        Value::Float(n) => numeric_part(*n, None),
        Value::Bool(b) => IndexPart::Bool(*b),
        Value::Timestamp(millis) => IndexPart::Time(*millis),
        Value::Now => IndexPart::Time(now),
        Value::NowMinus(d) => IndexPart::Time(now - d.as_millis()),
        Value::Duration(d) => numeric_part(d.as_millis() as f64, Some(d.as_millis())),
    }
}

/// Simple bitset using Vec<u64> - no external dependency required
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitSet {
    bits: Vec<u64>,
    capacity: usize,
}

impl BitSet {
    pub fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            bits: vec![0u64; num_words],
            capacity,
        }
    }

    #[inline]
    pub fn insert(&mut self, pos: usize) {
        let word_idx = pos >> 6; // pos / 64
        let bit_idx = pos & 63; // pos % 64
        if word_idx >= self.bits.len() {
            let new_words = (word_idx + 1).next_power_of_two();
            self.bits.resize(new_words, 0);
            self.capacity = new_words * 64;
        } else if pos >= self.capacity {
            self.capacity = self.bits.len() * 64;
        }
        self.bits[word_idx] |= 1u64 << bit_idx;
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return;
        }
        let bit_idx = pos & 63;
        self.bits[word_idx] &= !(1u64 << bit_idx);
    }

    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return false;
        }
        let bit_idx = pos & 63;
        (self.bits[word_idx] & (1u64 << bit_idx)) != 0
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Append set positions directly into destination vector without intermediate allocations
    pub fn extend_into(&self, out: &mut Vec<usize>) {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                let pos = base_pos + bit_idx;
                if pos < self.capacity {
                    out.push(pos);
                }
                w &= w - 1;
            }
        }
    }

    /// Invoke closure for each set position without allocating vectors
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                let pos = base_pos + bit_idx;
                if pos < self.capacity {
                    f(pos);
                }
                w &= w - 1;
            }
        }
    }

    /// Convert to Vec<usize> of set positions
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count());
        self.extend_into(&mut out);
        out
    }

    /// Bitwise AND (intersection) - modifies self
    pub fn and_assign(&mut self, other: &BitSet) {
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= b;
        }
        if self.bits.len() > other.bits.len() {
            for a in &mut self.bits[other.bits.len()..] {
                *a = 0;
            }
        }
    }

    /// Bitwise OR (union) - modifies self
    pub fn or_assign(&mut self, other: &BitSet) {
        if other.bits.len() > self.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        if other.capacity > self.capacity {
            self.capacity = other.capacity;
        }
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a |= b;
        }
    }

    /// Check if any bits are set
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&w| w == 0)
    }

    /// Check if all bits would be within capacity
    pub fn can_hold(&self, pos: usize) -> bool {
        pos < self.capacity
    }
}

/// Pick one seek per DNF branch. `or` → union of seeks when every branch is indexable.
pub fn pick_index(cat: &Catalog, collection: &str, pred: &Pred) -> Option<Vec<IndexUse>> {
    let branches = dnf(pred)?;
    let mut uses = Vec::with_capacity(branches.len());
    for branch in &branches {
        uses.push(pick_conjunct(cat, collection, branch)?);
    }
    Some(uses)
}

fn pick_conjunct(cat: &Catalog, collection: &str, pred: &Pred) -> Option<IndexUse> {
    let atoms = flatten_and(pred);
    let mut best: Option<IndexUse> = None;
    let mut best_n = 0usize;
    for def in cat.indexes_on(collection) {
        if let Some(ref index_pred) = def.pred {
            let req = flatten_and(index_pred);
            if !req.iter().all(|r| atoms.contains(r)) {
                continue;
            }
        }
        let mut eqs = Vec::new();
        let mut range = None;
        let mut n = 0usize;
        for field in &def.fields {
            if let Some(v) = find_eq(&atoms, field) {
                eqs.push((field.clone(), v.clone()));
                n += 1;
                continue;
            }
            if let Some((op, v)) = find_range(&atoms, field) {
                range = Some((field.clone(), op, v.clone()));
                n += 1;
            }
            break;
        }
        if n == 0 {
            continue;
        }
        if eqs.is_empty() && def.fields.len() > 1 {
            continue;
        }
        if n > best_n {
            best_n = n;
            best = Some(IndexUse {
                def: def.clone(),
                eqs,
                range,
            });
        }
    }
    best
}

/// True when every atomic predicate is enforced by the index seek(s) (no residual filter).
pub fn index_covers_pred(pred: &Pred, uses: &[IndexUse]) -> bool {
    let Some(branches) = dnf(pred) else {
        return false;
    };
    if branches.len() != uses.len() {
        return false;
    }
    branches
        .iter()
        .zip(uses.iter())
        .all(|(branch, use_)| index_covers_conjunct(branch, use_))
}

fn index_covers_conjunct(pred: &Pred, use_: &IndexUse) -> bool {
    let pred_atoms = use_.def.pred.as_ref().map(flatten_and).unwrap_or_default();
    for atom in flatten_and(pred) {
        if pred_atoms.contains(&atom) {
            continue;
        }
        match atom {
            Pred::Cmp {
                field,
                op: CmpOp::Eq,
                value,
            } => {
                // Equal f64 buckets can contain distinct large integer values.
                // Recheck against the row's original cell in that precision range.
                if matches!(value, Value::Int(n) if n.unsigned_abs() >= (1 << 53))
                    || matches!(value, Value::Float(n) if !n.is_finite() || n.abs() >= (1u64 << 53) as f64)
                {
                    return false;
                }
                if !use_
                    .eqs
                    .iter()
                    .any(|(f, v)| f == &field.as_str() && v == value)
                {
                    return false;
                }
            }
            Pred::Cmp { field, op, value }
                if matches!(op, CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le) =>
            {
                let Some((rf, rop, rv)) = &use_.range else {
                    return false;
                };
                if rf != &field.as_str() || rop != op || rv != value {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Disjunctive normal form: list of AND-trees (no top-level `or` inside a branch).
fn dnf(pred: &Pred) -> Option<Vec<Pred>> {
    // Bound both input size and expansion. Falling back to a scan keeps the
    // original predicate as a residual filter and preserves query semantics.
    let mut pending = vec![pred];
    let mut nodes = 0;
    while let Some(p) = pending.pop() {
        nodes += 1;
        if nodes > 1024 {
            return None;
        }
        match p {
            Pred::And(a, b) | Pred::Or(a, b) => {
                pending.extend([a.as_ref(), b.as_ref()]);
            }
            Pred::Not(inner) => {
                pending.push(inner.as_ref());
            }
            _ => {}
        }
    }
    dnf_bounded(pred)
}

fn dnf_bounded(pred: &Pred) -> Option<Vec<Pred>> {
    const MAX_BRANCHES: usize = 256;
    match pred {
        Pred::In { field, values } => {
            if values.is_empty() || values.len() > MAX_BRANCHES {
                return None;
            }
            Some(
                values
                    .iter()
                    .map(|v| Pred::Cmp {
                        field: field.clone(),
                        op: CmpOp::Eq,
                        value: v.clone(),
                    })
                    .collect(),
            )
        }
        Pred::Or(a, b) => {
            let mut out = dnf_bounded(a)?;
            let right = dnf_bounded(b)?;
            if out.len().checked_add(right.len())? > MAX_BRANCHES {
                return None;
            }
            out.extend(right);
            Some(out)
        }
        Pred::And(a, b) => {
            let left = dnf_bounded(a)?;
            let right = dnf_bounded(b)?;
            let len = left.len().checked_mul(right.len())?;
            if len > MAX_BRANCHES {
                return None;
            }
            let mut out = Vec::with_capacity(len);
            for l in &left {
                for r in &right {
                    out.push(Pred::And(Box::new(l.clone()), Box::new(r.clone())));
                }
            }
            Some(out)
        }
        other => Some(vec![other.clone()]),
    }
}

fn flatten_and(pred: &Pred) -> Vec<&Pred> {
    let mut v = Vec::with_capacity(4);
    flatten_and_into(pred, &mut v);
    v
}

fn flatten_and_into<'a>(pred: &'a Pred, out: &mut Vec<&'a Pred>) {
    match pred {
        Pred::And(a, b) => {
            flatten_and_into(a, out);
            flatten_and_into(b, out);
        }
        other => out.push(other),
    }
}

fn find_eq<'a>(atoms: &[&'a Pred], field: &str) -> Option<&'a Value> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value,
        } = a
            && f.as_str() == field
        {
            return Some(value);
        }
    }
    None
}

fn find_range<'a>(atoms: &[&'a Pred], field: &str) -> Option<(CmpOp, &'a Value)> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op,
            value,
        } = a
            && f.as_str() == field
            && matches!(op, CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le)
        {
            return Some((*op, value));
        }
    }
    None
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use crate::store::Cell;

    #[test]
    fn index_part_small_text_semantics_and_zero_alloc() {
        use std::collections::hash_map::DefaultHasher;

        // Size check: IndexPart doesn't blow up
        assert_eq!(std::mem::size_of::<IndexPart>(), 32);

        let inline = crate::store::InlineStr::try_new("user_101").unwrap();
        let p_small = IndexPart::SmallText(inline);
        let p_text = IndexPart::Text(Arc::from("user_101"));

        // Equality between SmallText and Text
        assert_eq!(p_small, p_text);
        assert_eq!(p_text, p_small);

        // Hashing consistency
        let mut h1 = DefaultHasher::new();
        p_small.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        p_text.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Lexicographical ordering
        let p_smaller = IndexPart::SmallText(crate::store::InlineStr::try_new("user_099").unwrap());
        let p_larger = IndexPart::Text(Arc::from("user_200"));
        assert!(p_smaller < p_small);
        assert!(p_smaller < p_text);
        assert!(p_small < p_larger);
        assert!(p_text < p_larger);

        // Cell::SmallText lowers to IndexPart::SmallText
        let cell_small = Cell::SmallText(inline);
        let part_from_cell = cell_part(&cell_small);
        assert!(matches!(part_from_cell, IndexPart::SmallText(_)));

        // Value::String lowers to IndexPart::SmallText when short
        let val_short = Value::String(Arc::from("short_key"));
        let part_from_val = value_part(&val_short, 0);
        assert!(matches!(part_from_val, IndexPart::SmallText(_)));
    }

    #[test]
    fn test_bitmap_auto_detection() {
        let index_def = IndexDef {
            collection: "tasks".into(),
            unique: false,
            fields: vec!["status".into()],
            pred: None,
            bitmap_card_field: Some("status".into()),
        };

        let index = LiveIndex {
            def: index_def,
            forward: BTreeMap::new(),
            reverse: FxHashMap::default(),
            #[cfg(feature = "parallel")]
            bloom_filter: None,
        };

        // Test with boolean field - should always use bitmap
        let row_pending = Row::from([("status".into(), Cell::Bool(false))]);
        let row_active = Row::from([("status".into(), Cell::Bool(true))]);
        let sample = [row_pending.clone(), row_active.clone()];

        assert!(index.should_use_bitmap(0, &row_pending, &sample));
        assert!(index.should_use_bitmap(1, &row_active, &sample));
    }

    #[test]
    fn test_low_cardinality_detection() {
        let index_def = IndexDef {
            collection: "orders".into(),
            unique: false,
            fields: vec!["priority".into()],
            pred: None,
            bitmap_card_field: Some("priority".into()),
        };

        let index = LiveIndex {
            def: index_def,
            forward: BTreeMap::new(),
            reverse: FxHashMap::default(),
            #[cfg(feature = "parallel")]
            bloom_filter: None,
        };

        // Create sample data with low cardinality (only 3 distinct values)
        let rows: Vec<Row> = (0..100)
            .map(|i| Row::from([("priority".into(), Cell::Int((i % 3) as i64))]))
            .collect();

        // With only 3 distinct values out of 100, should use bitmap
        let row = &rows[0];
        assert!(index.should_use_bitmap(0, row, &rows));
    }

    #[test]
    fn test_high_cardinality_no_bitmap() {
        let index_def = IndexDef {
            collection: "users".into(),
            unique: false,
            fields: vec!["id".into()],
            pred: None,
            bitmap_card_field: Some("id".into()),
        };

        let index = LiveIndex {
            def: index_def,
            forward: BTreeMap::new(),
            reverse: FxHashMap::default(),
            #[cfg(feature = "parallel")]
            bloom_filter: None,
        };

        // Create sample data with high cardinality (100 distinct values in sample)
        let rows: Vec<Row> = (0..100)
            .map(|i| Row::from([("id".into(), Cell::Int(i as i64))]))
            .collect();

        // With 100 distinct values, should NOT use bitmap (> 16 threshold)
        let row = &rows[0];
        assert!(!index.should_use_bitmap(0, row, &rows));
    }

    #[test]
    fn unchanged_and_conflicting_keys_preserve_index_state() {
        for unique in [false, true] {
            let mut index = LiveIndex::new(IndexDef {
                collection: "test".into(),
                fields: vec!["key".into()],
                unique,
                pred: None,
                bitmap_card_field: None,
            });
            let a = Row::from([("key".into(), Cell::Int(1))]);
            let b = Row::from([("key".into(), Cell::Int(2))]);
            index.insert_at_new(0, &a).unwrap();
            index
                .insert_at_new(1, if unique { &b } else { &a })
                .unwrap();
            for _ in 0..10 {
                index.insert_at(0, &a).unwrap();
            }
            let mut actual = index.forward[&index.key_of(&a)].clone();
            actual.sort_unstable();
            assert_eq!(
                actual.to_indices(),
                if unique { vec![0] } else { vec![0, 1] }
            );
            if unique {
                let forward = index.forward.clone();
                let reverse = index.reverse.clone();
                assert!(index.insert_at(0, &b).is_err());
                assert!(index.insert_at_new(2, &a).is_err());
                assert_eq!(index.forward, forward);
                assert_eq!(index.reverse, reverse);
            }
            index
                .insert_at(0, &Row::from([("key".into(), Cell::Int(3))]))
                .unwrap();
            assert_eq!(
                index.forward[&index.reverse[&0]],
                PostingList::Standard(vec![0])
            );
            index.remove_at(0);
            assert!(!index.reverse.contains_key(&0));
        }
    }

    #[test]
    #[ignore = "manual paired performance measurement"]
    fn index_optimization_measurement() {
        use std::{hint::black_box, time::Instant};
        for n in [1000, 10000, 100000] {
            let mut index = LiveIndex::new(IndexDef {
                collection: "test".into(),
                fields: vec!["key".into()],
                unique: false,
                pred: None,
                bitmap_card_field: None,
            });
            let row = Row::from([("key".into(), Cell::Int(1))]);
            for i in 0..n {
                index.insert_at_new(i, &row).unwrap();
            }
            let start = Instant::now();
            for _ in 0..1000 {
                index.insert_at(black_box(n - 1), black_box(&row)).unwrap();
            }
            println!("unchanged {n}: {} ns", start.elapsed().as_nanos() / 1000);
            assert_eq!(index.forward[&index.key_of(&row)].len(), n);
            assert_eq!(index.reverse.len(), n);
        }
        let rows = (0..10000)
            .map(|i| Row::from([("key".into(), Cell::Int(i))]))
            .collect::<Vec<_>>();
        let start = Instant::now();
        for _ in 0..20 {
            let mut index = LiveIndex::new(IndexDef {
                collection: "test".into(),
                fields: vec!["key".into()],
                unique: true,
                pred: None,
                bitmap_card_field: None,
            });
            for (i, row) in rows.iter().enumerate() {
                index.insert_at_new(i, black_box(row)).unwrap();
            }
            assert_eq!(index.reverse.len(), rows.len());
            black_box(index);
        }
        println!("unique 10000: {} ns", start.elapsed().as_nanos() / 20);
    }

    #[test]
    fn inline_and_spilled_keys_keep_lexicographic_order_and_reverse_updates() {
        let variants = vec![
            vec![],
            vec![IndexPart::Null],
            vec![IndexPart::Min],
            vec![IndexPart::Text(Arc::from("é"))],
            vec![IndexPart::Text(Arc::from("a")), IndexPart::Num(1, 2)],
            vec![
                IndexPart::Text(Arc::from("a")),
                IndexPart::Num(1, 2),
                IndexPart::Max,
            ],
        ];
        for a in &variants {
            for b in &variants {
                let ka = IndexKey(a.iter().cloned().collect());
                let kb = IndexKey(b.iter().cloned().collect());
                assert_eq!(ka.cmp(&kb), a.cmp(b));
                assert_eq!(ka.clone(), ka);
            }
        }
        for arity in [1, 2, 3, 5] {
            let fields = (0..arity).map(|n| format!("f{n}")).collect::<Vec<_>>();
            let mut index = LiveIndex::new(IndexDef {
                collection: "test".into(),
                fields: fields.clone(),
                unique: true,
                pred: None,
                bitmap_card_field: None,
            });
            let row = fields
                .iter()
                .enumerate()
                .map(|(i, f)| (f.clone(), Cell::Int(i as i64)))
                .collect::<Row>();
            index.insert_at_new(7, &row).unwrap();
            assert!(index.insert_at_new(8, &row).is_err());
            let mut updated = row.clone();
            updated.insert("f0".into(), Cell::Int(99));
            index.insert_at(7, &updated).unwrap();
            index.insert_at_new(8, &row).unwrap();
            index.remove_at(7);
            assert_eq!(index.forward.len(), 1);
            assert_eq!(index.reverse.len(), 1);
            assert_eq!(
                index.forward.get(&index.key_of(&row)),
                Some(&PostingList::Standard(vec![8]))
            );
        }
    }
}

#[cfg(test)]
impl LiveIndex {
    fn insert_slab_batch_reference(&mut self, start: usize, rows: &[Row]) -> Result<(), String> {
        let now = crate::store::now_ms();
        // Partial index check: only insert matching rows
        let keyed_rows: Vec<(usize, &Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.def
                    .pred
                    .as_ref()
                    .is_none_or(|pred| crate::exec::eval_pred(pred, row, now))
            })
            .map(|(i, row)| (start + i, row))
            .collect();

        if keyed_rows.is_empty() {
            return Ok(());
        }

        // Pre-compute all keys in a single pass
        let keys: Vec<_> = keyed_rows.iter().map(|(_, r)| self.key_of(r)).collect();

        // Bulk insert using pre-computed keys
        for ((orig_idx, row), key) in keyed_rows.iter().zip(keys.into_iter()) {
            use std::collections::btree_map::Entry;

            // Auto-detect bitmap index based on field cardinality
            // If bitmap_card_field is set, estimate cardinality from sample
            let is_bitmap = self.should_use_bitmap(*orig_idx, row, rows);

            match self.forward.entry(key) {
                Entry::Vacant(v) => {
                    self.reverse.insert(*orig_idx, v.key().clone());
                    let mut posting = PostingList::new(is_bitmap, 1 << 16);
                    posting.push(*orig_idx);
                    v.insert(posting);
                }
                Entry::Occupied(mut o) => {
                    if self.def.unique {
                        return Err(format!("unique index {}: duplicate key", self.def.label()));
                    }
                    self.reverse.insert(*orig_idx, o.key().clone());
                    o.get_mut().push(*orig_idx);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod bitmap_sample_once_tests {
    use super::*;
    fn compare(rows: &[Row], unique: bool, partial: bool, populated: usize) {
        let def = IndexDef {
            collection: "docs".into(),
            unique,
            fields: vec!["key".into()],
            pred: if partial {
                Some(Pred::Cmp {
                    field: crate::ast::Field {
                        parts: vec!["keep".into()],
                    },
                    op: CmpOp::Eq,
                    value: Value::Bool(true),
                })
            } else {
                None
            },
            bitmap_card_field: Some("sample".into()),
        };
        let mut expected = LiveIndex::new(def.clone());
        if populated > 0 {
            expected.def.bitmap_card_field = None;
            for i in 0..populated {
                let row = Row::from([("key".into(), Cell::Int(-1 - i as i64))]);
                expected.insert_at_new(i as usize, &row).unwrap();
            }
            expected.def.bitmap_card_field = def.bitmap_card_field;
        }
        let mut actual = expected.clone();
        let a = expected.insert_slab_batch_reference(20_000, rows);
        let b = actual.insert_slab_batch(20_000, rows);
        assert_eq!(a, b);
        assert_eq!(expected.reverse, actual.reverse);
        let a: Vec<_> = expected
            .forward
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    matches!(v, PostingList::Bitmap(_)),
                    v.to_indices(),
                )
            })
            .collect();
        let b: Vec<_> = actual
            .forward
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    matches!(v, PostingList::Bitmap(_)),
                    v.to_indices(),
                )
            })
            .collect();
        assert_eq!(a, b);
    }
    #[test]
    fn sample_once_matches_per_row_reference() {
        for n in [0, 1, 2, 99, 100, 101, 128] {
            for cardinality in [0, 1, 16, 17, 32, 33] {
                let rows: Vec<Row> = (0..n)
                    .map(|i| {
                        let mut r = Row::from([
                            ("key".into(), Cell::Int((i % 37) as i64)),
                            ("keep".into(), Cell::Bool(i % 3 != 0)),
                        ]);
                        if cardinality > 0 {
                            r.insert("sample".into(), Cell::Int((i % cardinality) as i64));
                        }
                        r
                    })
                    .collect();
                for boolean_at in [None, Some(0), Some(100)] {
                    let mut case_rows = rows.clone();
                    if let Some(i) = boolean_at {
                        if let Some(row) = case_rows.get_mut(i) {
                            row.insert("sample".into(), Cell::Bool(true));
                        }
                    }
                    for unique in [false, true] {
                        for partial in [false, true] {
                            compare(&case_rows, unique, partial, 0);
                        }
                    }
                }
            }
        }
        let rows: Vec<Row> = (0..128)
            .map(|i| {
                Row::from([
                    ("key".into(), Cell::Int(i)),
                    ("sample".into(), Cell::Int(i % 24)),
                    ("keep".into(), Cell::Bool(true)),
                ])
            })
            .collect();
        for population in [9_999, 10_000, 10_001] {
            compare(&rows, false, false, population);
        }
    }
}
