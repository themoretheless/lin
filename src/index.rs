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
        let num_bits = (capacity.max(64) * 10).next_power_of_two().max(512);
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
        let total_bits = self.bits.len() * 64;

        for i in 0..self.num_hashes {
            let h = base_hash.wrapping_add((i as u64).wrapping_mul(delta));
            let bit_pos = (h as usize) & (total_bits - 1);
            let mask = 1u64 << (bit_pos & 63);
            self.bits[bit_pos >> 6] |= mask;
        }
    }

    fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        if self.bits.is_empty() {
            return true;
        }

        let base_hash = self.hash(item);
        let delta = base_hash.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17) | 1;
        let total_bits = self.bits.len() * 64;

        for i in 0..self.num_hashes {
            let h = base_hash.wrapping_add((i as u64).wrapping_mul(delta));
            let bit_pos = (h as usize) & (total_bits - 1);
            let mask = 1u64 << (bit_pos & 63);

            if self.bits[bit_pos >> 6] & mask == 0 {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub enum IndexPart {
    Min,
    Null,
    Bool(bool),
    Int(i64),
    /// Ordered f64 bucket, with a compact exact integer delta (`integer - (n as i128)`).
    /// Keeps `size_of::<IndexPart>() == 24` (8-byte aligned) while preserving exact
    /// distinctness and ordering across all 64-bit integers and floats.
    Num(u64, i32),
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

/// Live secondary index: Copy-on-Write BTree over composite keys → row indices in the collection vec.
#[derive(Debug, Clone)]
pub struct LiveIndex {
    pub def: IndexDef,
    slots: smallvec::SmallVec<[crate::store::row::OwnedFieldSlot; 2]>,
    /// Either Vec<usize> (standard) or BitSet (for low-cardinality columns), CoW-shared across ReadDb snapshots.
    pub forward: Arc<BTreeMap<IndexKey, PostingList>>,
    pub reverse: FxHashMap<usize, IndexKey>,
    pub use_reverse: bool,

    /// Bloom filter for fast negative lookups during seek operations
    #[cfg(feature = "parallel")]
    bloom_filter: Option<Arc<BloomFilter>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexUse {
    pub def: IndexDef,
    pub eqs: Vec<(String, Value)>,
    pub range: Option<(String, CmpOp, Value)>,
    pub opposite_bound: Option<(CmpOp, Value)>,
}

/// Fanout and cardinality distribution metrics for a [`LiveIndex`] (`LIN-0390`, `LIN-0431`).
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct IndexFanoutStats {
    pub distinct_keys: usize,
    pub total_postings: usize,
    pub max_posting_len: usize,
    pub avg_fanout: f64,
    pub p95_posting_len: usize,
    pub selectivity_estimate: f64,
}

/// Forward/reverse invariant inspection report for a [`LiveIndex`] (`LIN-0391`..`LIN-0396`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct IndexConsistencyReport {
    pub valid: bool,
    pub forward_postings: usize,
    pub reverse_entries: usize,
    pub unsorted_lists: usize,
    pub duplicate_positions: usize,
    pub forward_reverse_mismatches: usize,
    pub out_of_bounds_positions: usize,
}

/// Posting list - either SmallVec<[usize; 2]> (standard) or BitSet (for low-cardinality columns)
#[derive(Debug, Clone, PartialEq)]
pub enum PostingList {
    Standard(smallvec::SmallVec<[usize; 2]>),
    Bitmap(BitSet),
}

impl PostingList {
    pub fn new(is_bitmap: bool, capacity: usize) -> Self {
        if is_bitmap {
            PostingList::Bitmap(BitSet::new(capacity))
        } else {
            PostingList::Standard(smallvec::SmallVec::new())
        }
    }

    #[inline]
    pub fn insert(&mut self, pos: usize) {
        match self {
            PostingList::Standard(vec) => {
                if let Some(&last) = vec.last() {
                    if pos == last {
                        return;
                    }
                    if pos < last {
                        if let Err(idx) = vec.binary_search(&pos) {
                            vec.insert(idx, pos);
                        }
                        return;
                    }
                }
                // Checked at power-of-two lengths only: scanning for the maximum on every
                // insert made a sparse list quadratic to build.
                if vec.len() >= 32 && vec.len().is_power_of_two() {
                    let max_pos = vec.last().copied().unwrap_or(0).max(pos);
                    if max_pos <= vec.len().saturating_mul(64) {
                        let mut set = BitSet::from_positions(vec, max_pos);
                        set.insert(pos);
                        *self = PostingList::Bitmap(set);
                    } else {
                        vec.push(pos);
                    }
                } else {
                    vec.push(pos);
                }
            }
            PostingList::Bitmap(set) => set.insert(pos),
        }
    }

    #[inline]
    fn promote_to_bitmap(&mut self) -> Option<&mut BitSet> {
        if let PostingList::Standard(vec) = self
            && vec.len() >= 64
            && vec.len().is_power_of_two()
        {
            let max_pos = vec.iter().copied().max().unwrap_or(0);
            if max_pos <= vec.len().saturating_mul(256) {
                let set = BitSet::from_positions(vec, max_pos);
                *self = PostingList::Bitmap(set);
            }
        }
        match self {
            PostingList::Bitmap(set) => Some(set),
            PostingList::Standard(_) => None,
        }
    }

    #[inline]
    fn maybe_promote_batch(&mut self) {
        if let PostingList::Standard(vec) = self
            && vec.len() >= 64
        {
            let max_pos = vec.iter().copied().max().unwrap_or(0);
            if max_pos <= vec.len().saturating_mul(256) {
                let set = BitSet::from_positions(vec, max_pos);
                *self = PostingList::Bitmap(set);
            }
        }
    }

    #[inline]
    pub fn replace(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        if let Some(set) = self.promote_to_bitmap() {
            if set.contains(from) {
                set.replace(from, to);
            }
            return;
        }
        match self {
            PostingList::Standard(vec) => {
                let removed = if vec.last() == Some(&from) {
                    vec.pop();
                    true
                } else if let Ok(p) = vec.binary_search(&from) {
                    vec.remove(p);
                    true
                } else if let Some(p) = vec.iter().position(|&x| x == from) {
                    vec.remove(p);
                    true
                } else {
                    false
                };
                if removed {
                    if vec.last().is_none_or(|&last| to > last) {
                        vec.push(to);
                    } else if let Err(ins) = vec.binary_search(&to) {
                        vec.insert(ins, to);
                    }
                }
            }
            PostingList::Bitmap(set) => {
                if set.contains(from) {
                    set.replace(from, to);
                }
            }
        }
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        if let PostingList::Standard(vec) = self
            && vec.last() == Some(&pos)
        {
            vec.pop();
            return;
        }
        if let Some(set) = self.promote_to_bitmap() {
            set.remove(pos);
            return;
        }
        match self {
            PostingList::Standard(vec) => {
                if let Ok(p) = vec.binary_search(&pos) {
                    vec.remove(p);
                } else if let Some(p) = vec.iter().position(|&x| x == pos) {
                    vec.remove(p);
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

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            PostingList::Standard(vec) => vec.is_empty(),
            PostingList::Bitmap(set) => set.is_empty(),
        }
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
    pub fn for_each_ordered(&self, desc: bool, mut f: impl FnMut(usize) -> bool) -> bool {
        match self {
            PostingList::Standard(vec) => {
                if desc {
                    for &pos in vec.iter().rev() {
                        if !f(pos) {
                            return false;
                        }
                    }
                } else {
                    for &pos in vec.iter() {
                        if !f(pos) {
                            return false;
                        }
                    }
                }
                true
            }
            PostingList::Bitmap(set) => set.for_each_ordered(desc, f),
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
            PostingList::Standard(vec) => vec.to_vec(),
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
                let mut result = smallvec::SmallVec::with_capacity(a.len().min(b.len()));
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
                let mut result = smallvec::SmallVec::with_capacity(a.len().min(b.len()));
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
                *this = PostingList::Standard(smallvec::SmallVec::from_vec(a));
            }
        }
    }
}

impl LiveIndex {
    pub fn new(def: IndexDef) -> Self {
        let slots = def
            .fields
            .iter()
            .map(|f| crate::store::row::OwnedFieldSlot::new(f.as_str()))
            .collect();
        Self {
            def,
            slots,
            forward: Arc::new(BTreeMap::new()),
            reverse: FxHashMap::default(),
            use_reverse: true,
            #[cfg(feature = "parallel")]
            bloom_filter: None,
        }
    }

    /// Read-only snapshot clone: O(1) CoW share of the forward B-Tree and bloom filter, eliding the reverse map.
    pub fn clone_for_read(&self) -> Self {
        Self {
            def: self.def.clone(),
            slots: self.slots.clone(),
            forward: Arc::clone(&self.forward),
            reverse: FxHashMap::default(),
            use_reverse: false,
            #[cfg(feature = "parallel")]
            bloom_filter: self.bloom_filter.as_ref().map(Arc::clone),
        }
    }

    /// Build bloom filter after initial index population (optimization)
    #[cfg(feature = "parallel")]
    pub fn build_bloom_filter(&mut self) {
        let num_keys = self.forward.len();
        if num_keys == 0 {
            return;
        }

        let mut bf = BloomFilter::new(num_keys);
        for key in self.forward.keys() {
            bf.add(key);
        }
        self.bloom_filter = Some(Arc::new(bf));
    }

    #[inline]
    fn insert_forward_entry(
        &mut self,
        idx: usize,
        key: IndexKey,
        is_bitmap: bool,
        check_unique_occupied: bool,
        use_push: bool,
    ) -> Result<(), String> {
        use std::collections::btree_map::Entry;
        match Arc::make_mut(&mut self.forward).entry(key) {
            Entry::Vacant(v) => {
                #[cfg(feature = "parallel")]
                if let Some(ref mut bf) = self.bloom_filter {
                    Arc::make_mut(bf).add(v.key());
                }
                if self.use_reverse {
                    self.reverse.insert(idx, v.key().clone());
                }
                let mut posting = PostingList::new(is_bitmap, 64);
                if use_push {
                    posting.push(idx);
                } else {
                    posting.insert(idx);
                }
                v.insert(posting);
            }
            Entry::Occupied(mut o) => {
                if check_unique_occupied && self.def.unique {
                    return Err(format!("unique index {}: duplicate key", self.def.label()));
                }
                if self.use_reverse {
                    self.reverse.insert(idx, o.key().clone());
                }
                if use_push {
                    o.get_mut().push(idx);
                } else {
                    o.get_mut().insert(idx);
                }
            }
        }
        Ok(())
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

    #[inline]
    pub fn single_cell_key(cell: &Cell) -> IndexKey {
        IndexKey(smallvec::smallvec![cell_part(cell)])
    }

    #[inline]
    pub fn two_cell_key(c0: &Cell, c1: &Cell) -> IndexKey {
        IndexKey(smallvec::smallvec![cell_part(c0), cell_part(c1)])
    }

    #[inline]
    pub fn key_of(&self, row: &Row) -> IndexKey {
        match self.slots.as_slice() {
            [s0] if self.def.fields.len() == 1 => IndexKey(smallvec::smallvec![cell_part(
                s0.get_sync(row).unwrap_or(&Cell::Null)
            )]),
            [s0, s1] if self.def.fields.len() == 2 => IndexKey(smallvec::smallvec![
                cell_part(s0.get_sync(row).unwrap_or(&Cell::Null)),
                cell_part(s1.get_sync(row).unwrap_or(&Cell::Null)),
            ]),
            slots if slots.len() == self.def.fields.len() => {
                let mut parts = smallvec::SmallVec::with_capacity(slots.len());
                for s in slots {
                    parts.push(cell_part(s.get_sync(row).unwrap_or(&Cell::Null)));
                }
                IndexKey(parts)
            }
            _ => {
                let mut parts = smallvec::SmallVec::with_capacity(self.def.fields.len());
                for f in &self.def.fields {
                    parts.push(cell_part(row.get_str(f).unwrap_or(&Cell::Null)));
                }
                IndexKey(parts)
            }
        }
    }

    pub fn insert_at(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        // Partial index check: only insert if pred matches (or no pred)
        if let Some(ref pred) = self.def.pred
            && !crate::eval::eval_pred(pred, row, crate::store::now_ms())
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
            && !crate::eval::eval_pred(pred, row, crate::store::now_ms())
        {
            return Ok(()); // Skip indexing for non-matching rows
        }

        let key = self.key_of(row);

        // Check if this is a bitmap index (for boolean fields)
        let is_bitmap = self
            .def
            .bitmap_card_field
            .as_ref()
            .is_some_and(|field| matches!(row.get(field), Some(Cell::Bool(_))));

        self.insert_forward_entry(idx, key, is_bitmap, true, true)
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
    #[inline]
    fn insert_one_slab_key(
        &mut self,
        orig_idx: usize,
        row: &Row,
        key: IndexKey,
        bitmap_sample: Option<(bool, usize)>,
    ) -> Result<(), String> {
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

        self.insert_forward_entry(orig_idx, key, is_bitmap, true, true)
    }

    pub fn insert_slab_batch_no_reverse(&mut self, start: usize, rows: &[Row]) -> Result<(), String> {
        self.use_reverse = false;
        self.reverse.clear();
        self.insert_slab_batch(start, rows)
    }

    pub fn insert_slab_batch(&mut self, start: usize, rows: &[Row]) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }

        let bitmap_sample = self.def.bitmap_card_field.as_ref().map(|field| {
            let mut distinct = rustc_hash::FxHashSet::default();
            for row in rows.iter().take(100) {
                if let Some(cell) = row.get_str(field) {
                    if matches!(cell, Cell::Bool(_)) {
                        return (true, 0);
                    }
                    distinct.insert(cell);
                }
            }
            (false, distinct.len())
        });

        // Fast path for bulk building into an empty non-reverse index:
        // - Non-unique indexes group positions via a 4-slot MRU register/stack cache (matching
        //   raw `&Cell`s before constructing `IndexKey`) + overflow `FxHashMap`, then sort ONLY
        //   the `K` distinct keys instead of `N` `(IndexKey, usize)` pairs.
        // - Unique indexes extract flat `(IndexKey, usize)` pairs and sort once to detect
        //   duplicates and build `BTreeMap` in sorted order.
        if self.forward.is_empty()
            && !self.use_reverse
            && self.def.pred.is_none()
            && rows.len() >= 64
        {
            let sample_force_bitmap = bitmap_sample
                .is_some_and(|(has_bool, count)| has_bool || (count > 0 && count <= 16));
            let has_bitmap_card_field = self.def.bitmap_card_field.is_some();
            let first = &rows[0];
            let uniform = first.shape_arc().is_some_and(|fs| {
                rows[1..]
                    .iter()
                    .all(|r| r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, fs)))
            });

            if !self.def.unique {
                const NULL_CELL: &Cell = &Cell::Null;
                let build_posting =
                    |key: &IndexKey, sv: smallvec::SmallVec<[usize; 2]>| -> PostingList {
                        let key_has_bool = has_bitmap_card_field
                            && key
                                .0
                                .first()
                                .is_some_and(|p| matches!(p, IndexPart::Bool(_)));
                        let m = sv.len();
                        let max_pos = *sv.last().unwrap_or(&0);
                        if sample_force_bitmap
                            || key_has_bool
                            || (m >= 32 && max_pos <= m.saturating_mul(256))
                        {
                            PostingList::Bitmap(BitSet::from_sorted_unique(&sv))
                        } else {
                            PostingList::Standard(sv)
                        }
                    };

                let mut groups: FxHashMap<IndexKey, smallvec::SmallVec<[usize; 2]>> =
                    FxHashMap::default();
                let flush_group =
                    |groups: &mut FxHashMap<IndexKey, smallvec::SmallVec<[usize; 2]>>,
                     key: IndexKey,
                     sv: smallvec::SmallVec<[usize; 2]>| {
                        groups
                            .entry(key)
                            .and_modify(|v| v.extend_from_slice(&sv))
                            .or_insert(sv);
                    };

                let mut entries: Vec<(IndexKey, PostingList)> = if uniform {
                    let fshape = first.shape_arc().unwrap();
                    match self.def.fields.as_slice() {
                        [f0] => {
                            let p0 = fshape.position(f0);
                            let mut slots: [Option<(
                                &Cell,
                                IndexKey,
                                smallvec::SmallVec<[usize; 2]>,
                            )>; 4] = [None, None, None, None];
                            let mut order = [0usize, 1, 2, 3];
                            let mut cache_len = 0usize;

                            'row1: for (i, row) in rows.iter().enumerate() {
                                let pos = start + i;
                                let c0 = p0.map_or(NULL_CELL, |p| &row.cells_slice()[p]);
                                for r in 0..cache_len {
                                    let slot_idx = order[r];
                                    let (sc0, _, sv) = slots[slot_idx].as_mut().unwrap();
                                    if cells_eq_fast(sc0, c0) {
                                        sv.push(pos);
                                        if r > 0 {
                                            order.copy_within(0..r, 1);
                                            order[0] = slot_idx;
                                        }
                                        continue 'row1;
                                    }
                                }
                                let key = IndexKey(smallvec::smallvec![cell_part(c0)]);
                                for r in 0..cache_len {
                                    let slot_idx = order[r];
                                    let (sc0, sk, sv) = slots[slot_idx].as_mut().unwrap();
                                    if *sk == key {
                                        *sc0 = c0;
                                        sv.push(pos);
                                        if r > 0 {
                                            order.copy_within(0..r, 1);
                                            order[0] = slot_idx;
                                        }
                                        continue 'row1;
                                    }
                                }
                                if cache_len < 4 {
                                    let slot_idx = order[cache_len];
                                    slots[slot_idx] =
                                        Some((c0, key, smallvec::smallvec![pos]));
                                    order.copy_within(0..cache_len, 1);
                                    order[0] = slot_idx;
                                    cache_len += 1;
                                } else {
                                    let lru = order[3];
                                    let (_, ev_key, ev_sv) = slots[lru].take().unwrap();
                                    flush_group(&mut groups, ev_key, ev_sv);
                                    slots[lru] = Some((c0, key, smallvec::smallvec![pos]));
                                    order.copy_within(0..3, 1);
                                    order[0] = lru;
                                }
                            }

                            if groups.is_empty() {
                                let mut out = Vec::with_capacity(cache_len);
                                for slot in slots.into_iter().flatten() {
                                    let (_, key, sv) = slot;
                                    let posting = build_posting(&key, sv);
                                    out.push((key, posting));
                                }
                                out
                            } else {
                                for slot in slots.into_iter().flatten() {
                                    flush_group(&mut groups, slot.1, slot.2);
                                }
                                groups
                                    .into_iter()
                                    .map(|(k, sv)| {
                                        let p = build_posting(&k, sv);
                                        (k, p)
                                    })
                                    .collect()
                            }
                        }
                        [f0, f1] => {
                            let p0 = fshape.position(f0);
                            let p1 = fshape.position(f1);
                            let mut slots: [Option<(
                                &Cell,
                                &Cell,
                                IndexKey,
                                smallvec::SmallVec<[usize; 2]>,
                            )>; 4] = [None, None, None, None];
                            let mut order = [0usize, 1, 2, 3];
                            let mut cache_len = 0usize;

                            'row2: for (i, row) in rows.iter().enumerate() {
                                let pos = start + i;
                                let cells = row.cells_slice();
                                let c0 = p0.map_or(NULL_CELL, |p| &cells[p]);
                                let c1 = p1.map_or(NULL_CELL, |p| &cells[p]);
                                for r in 0..cache_len {
                                    let slot_idx = order[r];
                                    let (sc0, sc1, _, sv) = slots[slot_idx].as_mut().unwrap();
                                    if cells_eq_fast(sc0, c0) && cells_eq_fast(sc1, c1) {
                                        sv.push(pos);
                                        if r > 0 {
                                            order.copy_within(0..r, 1);
                                            order[0] = slot_idx;
                                        }
                                        continue 'row2;
                                    }
                                }
                                let key = IndexKey(smallvec::smallvec![
                                    cell_part(c0),
                                    cell_part(c1)
                                ]);
                                for r in 0..cache_len {
                                    let slot_idx = order[r];
                                    let (sc0, sc1, sk, sv) = slots[slot_idx].as_mut().unwrap();
                                    if *sk == key {
                                        *sc0 = c0;
                                        *sc1 = c1;
                                        sv.push(pos);
                                        if r > 0 {
                                            order.copy_within(0..r, 1);
                                            order[0] = slot_idx;
                                        }
                                        continue 'row2;
                                    }
                                }
                                if cache_len < 4 {
                                    let slot_idx = order[cache_len];
                                    slots[slot_idx] =
                                        Some((c0, c1, key, smallvec::smallvec![pos]));
                                    order.copy_within(0..cache_len, 1);
                                    order[0] = slot_idx;
                                    cache_len += 1;
                                } else {
                                    let lru = order[3];
                                    let (_, _, ev_key, ev_sv) = slots[lru].take().unwrap();
                                    flush_group(&mut groups, ev_key, ev_sv);
                                    slots[lru] =
                                        Some((c0, c1, key, smallvec::smallvec![pos]));
                                    order.copy_within(0..3, 1);
                                    order[0] = lru;
                                }
                            }

                            if groups.is_empty() {
                                let mut out = Vec::with_capacity(cache_len);
                                for slot in slots.into_iter().flatten() {
                                    let (_, _, key, sv) = slot;
                                    let posting = build_posting(&key, sv);
                                    out.push((key, posting));
                                }
                                out
                            } else {
                                for slot in slots.into_iter().flatten() {
                                    flush_group(&mut groups, slot.2, slot.3);
                                }
                                groups
                                    .into_iter()
                                    .map(|(k, sv)| {
                                        let p = build_posting(&k, sv);
                                        (k, p)
                                    })
                                    .collect()
                            }
                        }
                        fields => {
                            let positions: smallvec::SmallVec<[Option<usize>; 4]> =
                                fields.iter().map(|f| fshape.position(f)).collect();
                            let mut slots: [Option<(
                                IndexKey,
                                smallvec::SmallVec<[usize; 2]>,
                            )>; 4] = [None, None, None, None];
                            let mut order = [0usize, 1, 2, 3];
                            let mut cache_len = 0usize;

                            'row_n: for (i, row) in rows.iter().enumerate() {
                                let pos = start + i;
                                let cells = row.cells_slice();
                                let mut parts =
                                    smallvec::SmallVec::with_capacity(positions.len());
                                for &p_opt in &positions {
                                    parts.push(cell_part(p_opt.map_or(NULL_CELL, |p| &cells[p])));
                                }
                                let key = IndexKey(parts);
                                for r in 0..cache_len {
                                    let slot_idx = order[r];
                                    let (sk, sv) = slots[slot_idx].as_mut().unwrap();
                                    if *sk == key {
                                        sv.push(pos);
                                        if r > 0 {
                                            order.copy_within(0..r, 1);
                                            order[0] = slot_idx;
                                        }
                                        continue 'row_n;
                                    }
                                }
                                if cache_len < 4 {
                                    let slot_idx = order[cache_len];
                                    slots[slot_idx] = Some((key, smallvec::smallvec![pos]));
                                    order.copy_within(0..cache_len, 1);
                                    order[0] = slot_idx;
                                    cache_len += 1;
                                } else {
                                    let lru = order[3];
                                    let (ev_key, ev_sv) = slots[lru].take().unwrap();
                                    flush_group(&mut groups, ev_key, ev_sv);
                                    slots[lru] = Some((key, smallvec::smallvec![pos]));
                                    order.copy_within(0..3, 1);
                                    order[0] = lru;
                                }
                            }

                            if groups.is_empty() {
                                let mut out = Vec::with_capacity(cache_len);
                                for slot in slots.into_iter().flatten() {
                                    let (key, sv) = slot;
                                    let posting = build_posting(&key, sv);
                                    out.push((key, posting));
                                }
                                out
                            } else {
                                for slot in slots.into_iter().flatten() {
                                    flush_group(&mut groups, slot.0, slot.1);
                                }
                                groups
                                    .into_iter()
                                    .map(|(k, sv)| {
                                        let p = build_posting(&k, sv);
                                        (k, p)
                                    })
                                    .collect()
                            }
                        }
                    }
                } else {
                    let mut slots: [Option<(IndexKey, smallvec::SmallVec<[usize; 2]>)>; 4] =
                        [None, None, None, None];
                    let mut order = [0usize, 1, 2, 3];
                    let mut cache_len = 0usize;

                    'row_dyn: for (i, row) in rows.iter().enumerate() {
                        let pos = start + i;
                        let key = self.key_of(row);
                        for r in 0..cache_len {
                            let slot_idx = order[r];
                            let (sk, sv) = slots[slot_idx].as_mut().unwrap();
                            if *sk == key {
                                sv.push(pos);
                                if r > 0 {
                                    order.copy_within(0..r, 1);
                                    order[0] = slot_idx;
                                }
                                continue 'row_dyn;
                            }
                        }
                        if cache_len < 4 {
                            let slot_idx = order[cache_len];
                            slots[slot_idx] = Some((key, smallvec::smallvec![pos]));
                            order.copy_within(0..cache_len, 1);
                            order[0] = slot_idx;
                            cache_len += 1;
                        } else {
                            let lru = order[3];
                            let (ev_key, ev_sv) = slots[lru].take().unwrap();
                            flush_group(&mut groups, ev_key, ev_sv);
                            slots[lru] = Some((key, smallvec::smallvec![pos]));
                            order.copy_within(0..3, 1);
                            order[0] = lru;
                        }
                    }

                    if groups.is_empty() {
                        let mut out = Vec::with_capacity(cache_len);
                        for slot in slots.into_iter().flatten() {
                            let (key, sv) = slot;
                            let posting = build_posting(&key, sv);
                            out.push((key, posting));
                        }
                        out
                    } else {
                        for slot in slots.into_iter().flatten() {
                            flush_group(&mut groups, slot.0, slot.1);
                        }
                        groups
                            .into_iter()
                            .map(|(k, sv)| {
                                let p = build_posting(&k, sv);
                                (k, p)
                            })
                            .collect()
                    }
                };

                entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                self.forward = Arc::new(BTreeMap::from_iter(entries));
                #[cfg(feature = "parallel")]
                if let Some(ref mut bf) = self.bloom_filter {
                    let bf_mut = Arc::make_mut(bf);
                    for k in self.forward.keys() {
                        bf_mut.add(k);
                    }
                }
                return Ok(());
            }

            let mut pairs: Vec<(IndexKey, usize)> = Vec::with_capacity(rows.len());
            if uniform {
                let fshape = first.shape_arc().unwrap();
                match self.def.fields.as_slice() {
                    [f0] => {
                        let p0 = fshape.position(f0);
                        for (i, row) in rows.iter().enumerate() {
                            let c0 = p0.map_or(&Cell::Null, |p| &row.cells_slice()[p]);
                            pairs.push((IndexKey(smallvec::smallvec![cell_part(c0)]), start + i));
                        }
                    }
                    [f0, f1] => {
                        let p0 = fshape.position(f0);
                        let p1 = fshape.position(f1);
                        for (i, row) in rows.iter().enumerate() {
                            let cells = row.cells_slice();
                            let c0 = p0.map_or(&Cell::Null, |p| &cells[p]);
                            let c1 = p1.map_or(&Cell::Null, |p| &cells[p]);
                            pairs.push((
                                IndexKey(smallvec::smallvec![cell_part(c0), cell_part(c1)]),
                                start + i,
                            ));
                        }
                    }
                    fields => {
                        let positions: smallvec::SmallVec<[Option<usize>; 4]> =
                            fields.iter().map(|f| fshape.position(f)).collect();
                        for (i, row) in rows.iter().enumerate() {
                            let cells = row.cells_slice();
                            let mut parts = smallvec::SmallVec::with_capacity(positions.len());
                            for &pos in &positions {
                                parts.push(cell_part(pos.map_or(&Cell::Null, |p| &cells[p])));
                            }
                            pairs.push((IndexKey(parts), start + i));
                        }
                    }
                }
            } else {
                for (i, row) in rows.iter().enumerate() {
                    pairs.push((self.key_of(row), start + i));
                }
            }

            pairs.sort_unstable();

            if pairs.windows(2).any(|w| w[0].0 == w[1].0) {
                for (i, row) in rows.iter().enumerate() {
                    let key = self.key_of(row);
                    self.insert_one_slab_key(start + i, row, key, bitmap_sample)?;
                }
                return Ok(());
            }

            let mut entries: Vec<(IndexKey, PostingList)> = Vec::with_capacity(pairs.len());
            for (key, first_pos) in pairs {
                let key_has_bool = has_bitmap_card_field
                    && key
                        .0
                        .first()
                        .is_some_and(|p| matches!(p, IndexPart::Bool(_)));
                let posting = if sample_force_bitmap || key_has_bool {
                    PostingList::Bitmap(BitSet::from_sorted_unique(&[first_pos]))
                } else {
                    PostingList::Standard(smallvec::smallvec![first_pos])
                };
                entries.push((key, posting));
            }
            self.forward = Arc::new(BTreeMap::from_iter(entries));
            #[cfg(feature = "parallel")]
            if let Some(ref mut bf) = self.bloom_filter {
                let bf_mut = Arc::make_mut(bf);
                for k in self.forward.keys() {
                    bf_mut.add(k);
                }
            }
            return Ok(());
        }

        if let Some(pred) = &self.def.pred {
            let now = crate::store::now_ms();
            let keyed_rows: Vec<(usize, &Row)> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| crate::eval::eval_pred(pred, row, now))
                .map(|(i, row)| (start + i, row))
                .collect();

            if keyed_rows.is_empty() {
                return Ok(());
            }

            let keys: Vec<_> = keyed_rows.iter().map(|(_, r)| self.key_of(r)).collect();
            if self.use_reverse {
                self.reverse.reserve(keyed_rows.len());
            }

            for ((orig_idx, row), key) in keyed_rows.iter().zip(keys.into_iter()) {
                self.insert_one_slab_key(*orig_idx, row, key, bitmap_sample)?;
            }
        } else {
            if self.use_reverse {
                self.reverse.reserve(rows.len());
            }
            let first = &rows[0];
            let uniform = first.shape_arc().is_some_and(|fs| {
                rows[1..]
                    .iter()
                    .all(|r| r.shape_arc().is_some_and(|s| Arc::ptr_eq(s, fs)))
            });
            if uniform {
                let fshape = first.shape_arc().unwrap();
                let positions: smallvec::SmallVec<[Option<usize>; 4]> = self
                    .def
                    .fields
                    .iter()
                    .map(|f| fshape.position(f))
                    .collect();
                for (i, row) in rows.iter().enumerate() {
                    let orig_idx = start + i;
                    let cells = row.cells_slice();
                    let key = match positions.as_slice() {
                        [p0] => IndexKey(smallvec::smallvec![cell_part(
                            p0.map_or(&Cell::Null, |p| &cells[p])
                        )]),
                        [p0, p1] => IndexKey(smallvec::smallvec![
                            cell_part(p0.map_or(&Cell::Null, |p| &cells[p])),
                            cell_part(p1.map_or(&Cell::Null, |p| &cells[p])),
                        ]),
                        pos_slice => {
                            let mut parts = smallvec::SmallVec::with_capacity(pos_slice.len());
                            for &pos in pos_slice {
                                parts.push(cell_part(pos.map_or(&Cell::Null, |p| &cells[p])));
                            }
                            IndexKey(parts)
                        }
                    };
                    self.insert_one_slab_key(orig_idx, row, key, bitmap_sample)?;
                }
            } else {
                for (i, row) in rows.iter().enumerate() {
                    let orig_idx = start + i;
                    let key = self.key_of(row);
                    self.insert_one_slab_key(orig_idx, row, key, bitmap_sample)?;
                }
            }
        }

        if rows.len() >= 256 && self.forward.len().saturating_mul(16) <= start + rows.len() {
            for list in Arc::make_mut(&mut self.forward).values_mut() {
                list.maybe_promote_batch();
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
        if self.use_reverse && self.reverse.get(&idx) == Some(&key) {
            return Ok(());
        }

        // Check if this is a bitmap index (low cardinality column)
        let is_bitmap = self.def.bitmap_card_field.as_ref().is_some_and(|_field| {
            key.0
                .first()
                .is_some_and(|cell| matches!(cell, IndexPart::Bool(_)))
        });

        if self.use_reverse
            && let Some(old) = self.reverse.remove(&idx)
            && let std::collections::btree_map::Entry::Occupied(mut entry) =
                Arc::make_mut(&mut self.forward).entry(old)
        {
            entry.get_mut().remove(idx);
            if entry.get().is_empty() {
                entry.remove_entry();
            }
        }

        // Use BitSet for boolean fields (≤2 distinct values)
        self.insert_forward_entry(idx, key, is_bitmap, false, false)
    }

    pub fn update_row(&mut self, idx: usize, before: &Row, after: &Row) -> Result<(), String> {
        if self.def.pred.is_none()
            && (!self.use_reverse || self.reverse.contains_key(&idx))
            && self.slots.len() == self.def.fields.len()
            && self.slots.iter().all(|s| s.get_sync(before) == s.get_sync(after))
        {
            return Ok(());
        }
        if self.use_reverse {
            return self.insert_at(idx, after);
        }
        if let Some(ref pred) = self.def.pred {
            let now = crate::store::now_ms();
            let old_key = crate::eval::eval_pred(pred, before, now).then(|| self.key_of(before));
            let new_key = crate::eval::eval_pred(pred, after, now).then(|| self.key_of(after));
            self.update_keys(idx, old_key, new_key)
        } else {
            let old_key = Some(self.key_of(before));
            let new_key = Some(self.key_of(after));
            self.update_keys(idx, old_key, new_key)
        }
    }

    pub fn update_keys(
        &mut self,
        idx: usize,
        old_key: Option<IndexKey>,
        new_key: Option<IndexKey>,
    ) -> Result<(), String> {
        if old_key == new_key {
            return Ok(());
        }
        if let Some(ref nk) = new_key
            && self.def.unique
            && let Some(ids) = self.forward.get(nk)
            && ids.has_other_than(idx)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        if let Some(ref ok) = old_key {
            let fwd = Arc::make_mut(&mut self.forward);
            let empty = if let Some(list) = fwd.get_mut(ok) {
                list.remove(idx);
                list.is_empty()
            } else {
                false
            };
            if empty {
                fwd.remove(ok);
            }
        }
        if let Some(nk) = new_key {
            let is_bitmap = self.def.bitmap_card_field.as_ref().is_some_and(|_field| {
                nk.0.first()
                    .is_some_and(|cell| matches!(cell, IndexPart::Bool(_)))
            });
            self.insert_forward_entry(idx, nk, is_bitmap, false, false)?;
        } else if self.use_reverse {
            self.reverse.remove(&idx);
        }
        Ok(())
    }

    pub fn remove_row(&mut self, idx: usize, doomed: &Row) {
        if self.use_reverse {
            self.remove_at(idx);
            return;
        }
        if let Some(ref pred) = self.def.pred {
            let now = crate::store::now_ms();
            if !crate::eval::eval_pred(pred, doomed, now) {
                return;
            }
        }
        let kd = self.key_of(doomed);
        let fwd = Arc::make_mut(&mut self.forward);
        let empty = if let Some(list) = fwd.get_mut(&kd) {
            list.remove(idx);
            list.is_empty()
        } else {
            false
        };
        if empty {
            fwd.remove(&kd);
        }
    }

    pub fn swap_remove_keys(&mut self, p: usize, kd: &IndexKey, last: usize, kt: &IndexKey) {
        if self.use_reverse {
            self.swap_remove_at(p, last);
            return;
        }
        let fwd = Arc::make_mut(&mut self.forward);
        if kd == kt {
            let empty = if let Some(list) = fwd.get_mut(kt) {
                list.remove(last);
                list.is_empty()
            } else {
                false
            };
            if empty {
                fwd.remove(kt);
            }
        } else {
            if let Some(list) = fwd.get_mut(kt) {
                list.replace(last, p);
            }
            let empty = if let Some(list) = fwd.get_mut(kd) {
                list.remove(p);
                list.is_empty()
            } else {
                false
            };
            if empty {
                fwd.remove(kd);
            }
        }
    }

    pub fn swap_remove_rows(&mut self, p: usize, doomed: &Row, last: usize, tail: &Row) {
        if self.use_reverse {
            self.swap_remove_at(p, last);
            return;
        }
        let (d_in, t_in) = if let Some(pred) = &self.def.pred {
            let now = crate::store::now_ms();
            (
                crate::eval::eval_pred(pred, doomed, now),
                crate::eval::eval_pred(pred, tail, now),
            )
        } else {
            (true, true)
        };
        match (d_in, t_in) {
            (true, true) => {
                let kd = self.key_of(doomed);
                let kt = self.key_of(tail);
                self.swap_remove_keys(p, &kd, last, &kt);
            }
            (false, true) => {
                let kt = self.key_of(tail);
                if let Some(list) = Arc::make_mut(&mut self.forward).get_mut(&kt) {
                    list.replace(last, p);
                }
            }
            (true, false) => {
                let kd = self.key_of(doomed);
                if let std::collections::btree_map::Entry::Occupied(mut entry) =
                    Arc::make_mut(&mut self.forward).entry(kd)
                {
                    entry.get_mut().remove(p);
                    if entry.get().is_empty() {
                        entry.remove_entry();
                    }
                }
            }
            (false, false) => {}
        }
    }

    pub fn remove_at(&mut self, idx: usize) {
        if let Some(key) = self.reverse.remove(&idx) {
            if let std::collections::btree_map::Entry::Occupied(mut entry) =
                Arc::make_mut(&mut self.forward).entry(key)
            {
                entry.get_mut().remove(idx);
                if entry.get().is_empty() {
                    entry.remove_entry();
                }
            }
        }
    }

    pub fn move_at(&mut self, from: usize, to: usize) {
        if let Some(key) = self.reverse.remove(&from) {
            if let Some(vec) = Arc::make_mut(&mut self.forward).get_mut(&key) {
                vec.replace(from, to);
            }
            self.reverse.insert(to, key);
        }
    }

    pub fn swap_remove_at(&mut self, p: usize, last: usize) {
        let key_tail = self.reverse.remove(&last);
        if let Some(kt) = key_tail {
            let fwd = Arc::make_mut(&mut self.forward);
            if let Some(kd_slot) = self.reverse.get_mut(&p) {
                if *kd_slot == kt {
                    if let std::collections::btree_map::Entry::Occupied(mut entry) = fwd.entry(kt) {
                        entry.get_mut().remove(last);
                        if entry.get().is_empty() {
                            entry.remove_entry();
                        }
                    }
                } else {
                    if let Some(list) = fwd.get_mut(&kt) {
                        list.replace(last, p);
                    }
                    let kd = std::mem::replace(kd_slot, kt);
                    if let std::collections::btree_map::Entry::Occupied(mut entry) = fwd.entry(kd) {
                        entry.get_mut().remove(p);
                        if entry.get().is_empty() {
                            entry.remove_entry();
                        }
                    }
                }
            } else {
                if let Some(list) = fwd.get_mut(&kt) {
                    list.replace(last, p);
                }
                self.reverse.insert(p, kt);
            }
        } else if let Some(kd) = self.reverse.remove(&p)
            && let std::collections::btree_map::Entry::Occupied(mut entry) =
                Arc::make_mut(&mut self.forward).entry(kd)
        {
            entry.get_mut().remove(p);
            if entry.get().is_empty() {
                entry.remove_entry();
            }
        }
    }

    fn bounds(&self, use_: &IndexUse, now: i64) -> (Bound<IndexKey>, Bound<IndexKey>) {
        let arity = self.def.fields.len();
        let mut lo: smallvec::SmallVec<[IndexPart; 2]> = smallvec::SmallVec::with_capacity(arity);
        let mut hi: smallvec::SmallVec<[IndexPart; 2]> = smallvec::SmallVec::with_capacity(arity);
        for (_, v) in &use_.eqs {
            // Keys order exactly (an integer outside f64 precision keeps its exact
            // offset from its f64 bucket), as row comparison does, so the bounds are the
            // literal's own key and not its whole f64 bucket.
            let p = value_part(v, now);
            lo.push(p.clone());
            hi.push(p);
        }
        while lo.len() < arity {
            lo.push(IndexPart::Min);
        }
        while hi.len() < arity {
            hi.push(IndexPart::Max);
        }
        let mut exclude_lo = false;
        let mut exclude_hi = false;
        if let Some((field, op, val)) = &use_.range
            && let Some(pos) = self.def.fields.iter().position(|f| f == field)
        {
            let lower = matches!(op, CmpOp::Gt | CmpOp::Ge);
            let p = value_part(val, now);
            if !lower {
                // Null has no numeric value and must not enter a covered range.
                lo[pos] = match p {
                    IndexPart::Num(_, _) => IndexPart::Num(0, i32::MIN),
                    IndexPart::Time(_) => IndexPart::Time(i64::MIN),
                    _ => lo[pos].clone(),
                };
            }
            for (op, value) in std::iter::once((*op, val))
                .chain(use_.opposite_bound.iter().map(|(op, value)| (*op, value)))
            {
                match op {
                    CmpOp::Gt | CmpOp::Ge => {
                        lo[pos] = value_part(value, now);
                        exclude_lo = op == CmpOp::Gt;
                        if exclude_lo {
                            lo[pos + 1..].fill(IndexPart::Max);
                        }
                    }
                    CmpOp::Lt | CmpOp::Le => {
                        hi[pos] = value_part(value, now);
                        exclude_hi = op == CmpOp::Lt;
                        if exclude_hi {
                            hi[pos + 1..].fill(IndexPart::Min);
                        }
                    }
                    CmpOp::Eq | CmpOp::Ne => {}
                }
            }
        }
        // BTreeMap::range panics on reversed bounds and on equal excluded bounds.
        if lo > hi || (lo == hi && (exclude_lo || exclude_hi)) {
            let empty = IndexKey(smallvec::smallvec![IndexPart::Max]);
            return (Bound::Included(empty.clone()), Bound::Included(empty));
        }
        let start = if exclude_lo {
            Bound::Excluded(IndexKey(lo))
        } else {
            Bound::Included(IndexKey(lo))
        };
        let end = if exclude_hi {
            Bound::Excluded(IndexKey(hi))
        } else {
            Bound::Included(IndexKey(hi))
        };
        (start, end)
    }

    /// Fast path for full-key equality seeks (`use_.range.is_none() && use_.eqs.len() == arity`),
    /// building a single `IndexKey` for `BTreeMap::get` instead of two `Bound` keys + `range`.
    #[inline]
    fn exact_eq_key(&self, use_: &IndexUse, now: i64) -> Option<IndexKey> {
        if use_.range.is_some() || use_.eqs.len() != self.def.fields.len() {
            return None;
        }
        let key = match use_.eqs.as_slice() {
            [(_, v0)] => IndexKey(smallvec::smallvec![value_part(v0, now)]),
            [(_, v0), (_, v1)] => {
                IndexKey(smallvec::smallvec![value_part(v0, now), value_part(v1, now)])
            }
            eqs => {
                let mut parts = smallvec::SmallVec::with_capacity(eqs.len());
                for (_, v) in eqs {
                    parts.push(value_part(v, now));
                }
                IndexKey(parts)
            }
        };
        Some(key)
    }

    #[inline]
    fn lookup_exact<'a>(&'a self, key: &IndexKey) -> Option<&'a PostingList> {
        #[cfg(feature = "parallel")]
        if let Some(ref filter) = self.bloom_filter
            && !filter.contains(key)
        {
            return None;
        }
        self.forward.get(key)
    }

    /// Count matching row indices without allocating an id/index list.
    /// Optional early termination if count exceeds threshold for performance.
    pub fn seek_count(&self, use_: &IndexUse, now: i64) -> usize {
        if let Some(key) = self.exact_eq_key(use_, now) {
            return self.lookup_exact(&key).map_or(0, PostingList::count);
        }
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
        if let Some(key) = self.exact_eq_key(use_, now) {
            let count = self.lookup_exact(&key).map_or(0, PostingList::count);
            return (count <= threshold).then_some(count);
        }
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
        if let Some(key) = self.exact_eq_key(use_, now) {
            if let Some(list) = self.lookup_exact(&key) {
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
            return;
        }
        let (start, end) = self.bounds(use_, now);

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
        if let Some(key) = self.exact_eq_key(use_, now) {
            if let Some(list) = self.lookup_exact(&key) {
                list.extend_into(out);
            }
            return;
        }
        let (start, end) = self.bounds(use_, now);

        for (_, list) in self.forward.range((start, end)) {
            list.extend_into(out);
        }
    }

    /// Seek row indices by range bounds: extend output vector from BTreeMap iteration
    pub fn seek_idxs(&self, use_: &IndexUse, now: i64) -> Vec<usize> {
        if let Some(key) = self.exact_eq_key(use_, now) {
            return match self.lookup_exact(&key) {
                Some(list) => list.to_indices(),
                None => Vec::new(),
            };
        }
        let mut out = Vec::new();
        self.seek_idxs_into(use_, now, &mut out);
        out
    }

    /// Iterate matching row indices in index key order (`desc = false` ascending, `desc = true` descending),
    /// stopping early as soon as `f(row_idx)` returns `false`.
    pub fn seek_ordered_limit(&self, use_: &IndexUse, now: i64, desc: bool, mut f: impl FnMut(usize) -> bool) {
        if let Some(key) = self.exact_eq_key(use_, now) {
            if let Some(list) = self.lookup_exact(&key) {
                let _ = list.for_each_ordered(false, &mut f);
            }
            return;
        }
        let (start, end) = self.bounds(use_, now);
        if desc {
            for (_, list) in self.forward.range((start, end)).rev() {
                if !list.for_each_ordered(false, &mut f) {
                    break;
                }
            }
        } else {
            for (_, list) in self.forward.range((start, end)) {
                if !list.for_each_ordered(false, &mut f) {
                    break;
                }
            }
        }
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
            .map(|use_| self.seek_idxs(use_, now))
            .collect()
    }

    /// Compute fanout and posting-length distribution statistics (`LIN-0390`, `LIN-0431`).
    pub fn fanout_stats(&self) -> IndexFanoutStats {
        let distinct_keys = self.forward.len();
        if distinct_keys == 0 {
            return IndexFanoutStats::default();
        }
        let mut lens: Vec<usize> = Vec::with_capacity(distinct_keys);
        let mut total_postings = 0usize;
        let mut max_posting_len = 0usize;
        for list in self.forward.values() {
            let c = list.count();
            lens.push(c);
            total_postings += c;
            if c > max_posting_len {
                max_posting_len = c;
            }
        }
        lens.sort_unstable();
        let p95_idx = (distinct_keys.saturating_mul(95).div_ceil(100))
            .saturating_sub(1)
            .min(distinct_keys - 1);
        let p95_posting_len = lens[p95_idx];
        let avg_fanout = total_postings as f64 / distinct_keys as f64;
        let selectivity_estimate = if total_postings > 0 {
            avg_fanout / total_postings as f64
        } else {
            0.0
        };
        IndexFanoutStats {
            distinct_keys,
            total_postings,
            max_posting_len,
            avg_fanout,
            p95_posting_len,
            selectivity_estimate,
        }
    }

    /// Inspect forward B-Tree posting lists and reverse map invariants (`LIN-0391`..`LIN-0396`).
    pub fn inspect_consistency(&self, collection_len: usize) -> IndexConsistencyReport {
        let mut forward_postings = 0usize;
        let reverse_entries = self.reverse.len();
        let mut unsorted_lists = 0usize;
        let mut duplicate_positions = 0usize;
        let mut forward_reverse_mismatches = 0usize;
        let mut out_of_bounds_positions = 0usize;
        let mut empty_lists = 0usize;
        let mut unique_violations = 0usize;

        let mut seen_global = rustc_hash::FxHashSet::with_capacity_and_hasher(
            reverse_entries.max(self.forward.len()),
            Default::default(),
        );

        for (key, list) in self.forward.iter() {
            let c = list.count();
            if c == 0 {
                empty_lists += 1;
            }
            if self.def.unique && c > 1 {
                unique_violations += c - 1;
            }
            match list {
                PostingList::Standard(vec) => {
                    forward_postings += vec.len();
                    if vec.windows(2).any(|w| w[0] > w[1]) {
                        unsorted_lists += 1;
                    }
                    let mut seen_local =
                        rustc_hash::FxHashSet::with_capacity_and_hasher(vec.len(), Default::default());
                    for &pos in vec.iter() {
                        if pos >= collection_len {
                            out_of_bounds_positions += 1;
                        }
                        if !seen_local.insert(pos) || !seen_global.insert(pos) {
                            duplicate_positions += 1;
                        }
                        if self.use_reverse && self.reverse.get(&pos) != Some(key) {
                            forward_reverse_mismatches += 1;
                        }
                    }
                }
                PostingList::Bitmap(set) => {
                    let actual_ones: usize =
                        set.bits.iter().map(|w| w.count_ones() as usize).sum();
                    if actual_ones != set.len {
                        forward_reverse_mismatches += 1;
                    }
                    forward_postings += actual_ones;
                    set.for_each(|pos| {
                        if pos >= collection_len {
                            out_of_bounds_positions += 1;
                        }
                        if !seen_global.insert(pos) {
                            duplicate_positions += 1;
                        }
                        if self.use_reverse && self.reverse.get(&pos) != Some(key) {
                            forward_reverse_mismatches += 1;
                        }
                    });
                }
            }
        }

        if self.use_reverse {
            for (&pos, rkey) in &self.reverse {
                if pos >= collection_len && !seen_global.contains(&pos) {
                    out_of_bounds_positions += 1;
                }
                let in_forward = match self.forward.get(rkey) {
                    Some(PostingList::Standard(vec)) => vec.contains(&pos),
                    Some(PostingList::Bitmap(set)) => set.contains(pos),
                    None => false,
                };
                if !in_forward {
                    forward_reverse_mismatches += 1;
                }
            }
        } else if !self.reverse.is_empty() {
            forward_reverse_mismatches += self.reverse.len();
        }

        let valid = unsorted_lists == 0
            && duplicate_positions == 0
            && forward_reverse_mismatches == 0
            && out_of_bounds_positions == 0
            && empty_lists == 0
            && unique_violations == 0;

        IndexConsistencyReport {
            valid,
            forward_postings,
            reverse_entries,
            unsorted_lists,
            duplicate_positions,
            forward_reverse_mismatches,
            out_of_bounds_positions,
        }
    }

    /// Rebuild a [`LiveIndex`] from `rows` with progress reporting and cooperative cancellation (`LIN-0393`..`LIN-0396`).
    pub fn rebuild_with_progress(
        def: &IndexDef,
        rows: &[Row],
        on_progress: &mut dyn FnMut(usize, usize),
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, crate::error::Error> {
        if is_cancelled() {
            return Err(crate::error::Error::canceled("index rebuild canceled"));
        }
        let total = rows.len();
        let mut idx = Self::new(def.clone());
        if total == 0 {
            on_progress(0, 0);
            return Ok(idx);
        }
        idx.reverse.reserve(total);
        let step = (total / 16).clamp(1, 256);
        let mut done = 0usize;
        while done < total {
            if is_cancelled() {
                return Err(crate::error::Error::canceled("index rebuild canceled"));
            }
            let end = (done + step).min(total);
            for (i, row) in rows[done..end].iter().enumerate() {
                idx.insert_at_new(done + i, row).map_err(|msg| {
                    crate::error::Error::runtime(msg)
                        .with_code(crate::error::ErrorCode::UniqueViolation)
                })?;
            }
            done = end;
            on_progress(done, total);
        }
        if is_cancelled() {
            return Err(crate::error::Error::canceled("index rebuild canceled"));
        }
        #[cfg(feature = "parallel")]
        idx.build_bloom_filter();
        Ok(idx)
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
    let tie = if let Some(orig) = integer {
        let approx = n as i128;
        (i128::from(orig) - approx).clamp(i128::from(i32::MIN) + 2, i128::from(i32::MAX) - 1) as i32
    } else {
        0
    };
    IndexPart::Num(ordered, tie)
}

#[inline(always)]
fn cells_eq_fast(a: &Cell, b: &Cell) -> bool {
    match (a, b) {
        (Cell::SmallText(x), Cell::SmallText(y)) => x == y,
        (Cell::Int(x), Cell::Int(y)) => *x == *y,
        (Cell::Time(x), Cell::Time(y)) => *x == *y,
        (Cell::Bool(x), Cell::Bool(y)) => *x == *y,
        (Cell::Text(x), Cell::Text(y)) => Arc::ptr_eq(x, y) || x == y,
        (Cell::Null, Cell::Null) => true,
        (Cell::Float(x), Cell::Float(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

#[inline(always)]
fn cell_part(c: &Cell) -> IndexPart {
    match c {
        Cell::Null => IndexPart::Null,
        Cell::Bool(b) => IndexPart::Bool(*b),
        Cell::Int(n) => numeric_part(*n as f64, Some(*n)),
        Cell::Time(n) => IndexPart::Time(*n),
        Cell::Text(s) => {
            if let Some(inline) = crate::store::InlineStr::try_new(s.as_ref()) {
                IndexPart::SmallText(inline)
            } else {
                IndexPart::Text(Arc::clone(s))
            }
        }
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
        Value::Param(_) => IndexPart::Null,
    }
}

/// Simple bitset using Vec<u64> - no external dependency required.
///
/// Maintains exact `len` in `O(1)` across all single-element and bulk operations so
/// `count()` and `is_empty()` are 1-instruction `O(1)`.
#[derive(Debug, Clone)]
pub struct BitSet {
    bits: Vec<u64>,
    capacity: usize,
    len: usize,
}

impl PartialEq for BitSet {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }
        let common = self.bits.len().min(other.bits.len());
        if self.bits[..common] != other.bits[..common] {
            return false;
        }
        self.bits[common..].iter().all(|&w| w == 0)
            && other.bits[common..].iter().all(|&w| w == 0)
    }
}

impl Eq for BitSet {}

impl BitSet {
    pub fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            bits: vec![0u64; num_words],
            capacity,
            len: 0,
        }
    }

    /// Build a `BitSet` in one word-coalesced pass from strictly ascending, unique positions.
    #[inline]
    pub fn from_sorted_unique(positions: &[usize]) -> Self {
        let Some(&max_pos) = positions.last() else {
            return Self::new(0);
        };
        let capacity = (max_pos + 1).max(64);
        let num_words = capacity.div_ceil(64);
        let mut bits = vec![0u64; num_words];
        let mut cur_word_idx = positions[0] >> 6;
        let mut cur_word = 0u64;
        for &pos in positions {
            let w = pos >> 6;
            if w != cur_word_idx {
                bits[cur_word_idx] = cur_word;
                cur_word_idx = w;
                cur_word = 0;
            }
            cur_word |= 1u64 << (pos & 63);
        }
        bits[cur_word_idx] = cur_word;
        Self {
            bits,
            capacity,
            len: positions.len(),
        }
    }

    /// Build a `BitSet` from arbitrary positions bounded by `max_pos` (`pos <= max_pos`).
    #[inline]
    pub fn from_positions(positions: &[usize], max_pos: usize) -> Self {
        if positions.is_empty() {
            return Self::new(0);
        }
        let capacity = (max_pos + 1).max(64);
        let num_words = capacity.div_ceil(64);
        let mut bits = vec![0u64; num_words];
        let mut len = 0usize;
        for &pos in positions {
            let w = pos >> 6;
            let mask = 1u64 << (pos & 63);
            if (bits[w] & mask) == 0 {
                bits[w] |= mask;
                len += 1;
            }
        }
        Self {
            bits,
            capacity,
            len,
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
        let mask = 1u64 << bit_idx;
        if (self.bits[word_idx] & mask) == 0 {
            self.bits[word_idx] |= mask;
            self.len += 1;
        }
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return;
        }
        let bit_idx = pos & 63;
        let mask = 1u64 << bit_idx;
        if (self.bits[word_idx] & mask) != 0 {
            self.bits[word_idx] &= !mask;
            self.len -= 1;
        }
    }

    #[inline]
    pub fn replace(&mut self, from: usize, to: usize) {
        self.remove(from);
        self.insert(to);
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

    #[inline(always)]
    pub fn count(&self) -> usize {
        self.len
    }

    /// Append set positions directly into destination vector without intermediate allocations
    pub fn extend_into(&self, out: &mut Vec<usize>) {
        if self.len == 0 {
            return;
        }
        out.reserve(self.len);
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                out.push(base_pos + bit_idx);
                w &= w - 1;
            }
        }
    }

    /// Invoke closure for each set position without allocating vectors
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        if self.len == 0 {
            return;
        }
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                f(base_pos + bit_idx);
                w &= w - 1;
            }
        }
    }

    /// Invoke closure in ascending or descending position order, stopping early when `f` returns `false`.
    pub fn for_each_ordered(&self, desc: bool, mut f: impl FnMut(usize) -> bool) -> bool {
        if self.len == 0 {
            return true;
        }
        if desc {
            for (word_idx, &word) in self.bits.iter().enumerate().rev() {
                if word == 0 {
                    continue;
                }
                let base_pos = word_idx << 6;
                let mut w = word;
                while w != 0 {
                    let bit_idx = 63 - (w.leading_zeros() as usize);
                    if !f(base_pos + bit_idx) {
                        return false;
                    }
                    w &= !(1u64 << bit_idx);
                }
            }
        } else {
            for (word_idx, &word) in self.bits.iter().enumerate() {
                if word == 0 {
                    continue;
                }
                let base_pos = word_idx << 6;
                let mut w = word;
                while w != 0 {
                    let bit_idx = w.trailing_zeros() as usize;
                    if !f(base_pos + bit_idx) {
                        return false;
                    }
                    w &= w - 1;
                }
            }
        }
        true
    }

    /// Convert to Vec<usize> of set positions
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.len);
        self.extend_into(&mut out);
        out
    }

    /// Bitwise AND (intersection) - modifies self
    pub fn and_assign(&mut self, other: &BitSet) {
        if self.len == 0 {
            return;
        }
        let common = self.bits.len().min(other.bits.len());
        let mut new_len = 0usize;
        for (a, &b) in self.bits[..common].iter_mut().zip(&other.bits[..common]) {
            let w = *a & b;
            *a = w;
            new_len += w.count_ones() as usize;
        }
        if self.len > new_len && self.bits.len() > common {
            self.bits[common..].fill(0);
        }
        self.len = new_len;
    }

    /// Bitwise OR (union) - modifies self
    pub fn or_assign(&mut self, other: &BitSet) {
        if other.len == 0 {
            return;
        }
        if other.bits.len() > self.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        if other.capacity > self.capacity {
            self.capacity = other.capacity;
        }
        let mut delta = 0usize;
        for (a, &b) in self.bits.iter_mut().zip(other.bits.iter()) {
            if b != 0 {
                let added = b & !*a;
                *a |= b;
                delta += added.count_ones() as usize;
            }
        }
        self.len += delta;
    }

    /// Check if any bits are set
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Check if all bits would be within capacity
    pub fn can_hold(&self, pos: usize) -> bool {
        pos < self.capacity
    }
}

/// Pick one seek per DNF branch. `or` → union of seeks when every branch is indexable.
pub fn pick_index(cat: &Catalog, collection: &str, pred: &Pred) -> Option<Vec<IndexUse>> {
    pick_index_with_coverage(cat, collection, pred).map(|(u, _)| u)
}

/// Pick one seek per DNF branch, also returning whether every atomic predicate is covered
/// by the chosen index seek(s) in a single pass without re-evaluating DNF.
pub fn pick_index_with_coverage(
    cat: &Catalog,
    collection: &str,
    pred: &Pred,
) -> Option<(Vec<IndexUse>, bool)> {
    if !cat.has_indexes_on(collection) {
        return None;
    }
    pick_from_indexes(&cat.indexes_on(collection), pred)
}

pub(crate) fn pick_from_indexes(
    indexes: &[&IndexDef],
    pred: &Pred,
) -> Option<(Vec<IndexUse>, bool)> {
    let top_conjuncts = flatten_and(pred);
    let mut indexable = Vec::with_capacity(top_conjuncts.len());
    let mut has_unindexed = false;
    for c in top_conjuncts {
        if pred_mentions_indexed_field(c, indexes) {
            indexable.push(c);
        } else {
            has_unindexed = true;
        }
    }
    if indexable.is_empty() {
        return None;
    }

    let branches = if indexable.len() == 1 {
        dnf_atoms(indexable[0])?
    } else {
        dnf_atoms_conjuncts(&indexable)?
    };

    let mut uses: Vec<IndexUse> = Vec::with_capacity(branches.len());
    let mut all_covered = !has_unindexed;
    for branch in &branches {
        let u = pick_conjunct_atoms(indexes, branch)?;
        if all_covered && !index_covers_atoms(branch, &u) {
            all_covered = false;
        }
        if !uses.contains(&u) {
            uses.push(u);
        }
    }
    Some((uses, all_covered))
}

fn pred_mentions_indexed_field(pred: &Pred, indexes: &[&IndexDef]) -> bool {
    let field_indexed = |name: &str| {
        indexes.iter().any(|def| {
            def.fields.iter().any(|f| f == name)
                || def
                    .pred
                    .as_ref()
                    .is_some_and(|ip| pred_mentions_field_name(ip, name))
        })
    };
    match pred {
        Pred::Cmp {
            field,
            op: CmpOp::Eq | CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le,
            ..
        } => field_indexed(field.as_cow_str().as_ref()),
        Pred::In { field, .. } => field_indexed(field.as_cow_str().as_ref()),
        Pred::Cmp { field, op: CmpOp::Ne, .. }
        | Pred::IsNull { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => {
            let name = field.as_cow_str();
            indexes.iter().any(|def| {
                def.pred
                    .as_ref()
                    .is_some_and(|ip| pred_mentions_field_name(ip, name.as_ref()))
            })
        }
        Pred::Not(inner) => pred_mentions_indexed_field(inner, indexes),
        Pred::And(a, b) | Pred::Or(a, b) => {
            pred_mentions_indexed_field(a, indexes) || pred_mentions_indexed_field(b, indexes)
        }
    }
}

fn pred_mentions_field_name(pred: &Pred, target: &str) -> bool {
    match pred {
        Pred::Cmp { field, .. }
        | Pred::IsNull { field, .. }
        | Pred::In { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. } => field.as_cow_str().as_ref() == target,
        Pred::And(a, b) | Pred::Or(a, b) => {
            pred_mentions_field_name(a, target) || pred_mentions_field_name(b, target)
        }
        Pred::Not(inner) => pred_mentions_field_name(inner, target),
    }
}

fn dnf_atoms_conjuncts<'a>(
    conjuncts: &[&'a Pred],
) -> Option<Vec<Vec<std::borrow::Cow<'a, Pred>>>> {
    const MAX_BRANCHES: usize = 256;
    let mut nodes = 0usize;
    let mut pending: Vec<&Pred> = conjuncts.to_vec();
    while let Some(p) = pending.pop() {
        nodes += 1;
        if nodes > 1024 {
            return None;
        }
        match p {
            Pred::And(a, b) | Pred::Or(a, b) => pending.extend([a.as_ref(), b.as_ref()]),
            Pred::Not(inner) => pending.push(inner.as_ref()),
            _ => {}
        }
    }
    let mut est = 1usize;
    for c in conjuncts {
        est = est.saturating_mul(dnf_branch_count(c, MAX_BRANCHES));
        if est > MAX_BRANCHES {
            return None;
        }
    }
    let mut acc = dnf_atoms_bounded(conjuncts[0])?;
    for &c in &conjuncts[1..] {
        let right = dnf_atoms_bounded(c)?;
        let len = acc.len().checked_mul(right.len())?;
        if len > MAX_BRANCHES {
            return None;
        }
        let mut next = Vec::with_capacity(len);
        for l in &acc {
            for r in &right {
                let mut b = Vec::with_capacity(l.len() + r.len());
                b.extend_from_slice(l);
                b.extend_from_slice(r);
                next.push(b);
            }
        }
        acc = next;
    }
    Some(acc)
}

fn pick_conjunct_atoms(indexes: &[&IndexDef], atoms: &[std::borrow::Cow<'_, Pred>]) -> Option<IndexUse> {
    let mut best: Option<IndexUse> = None;
    let mut best_n = 0usize;
    for def in indexes {
        if let Some(ref index_pred) = def.pred {
            let req = flatten_and(index_pred);
            if !req.iter().all(|r| atoms.iter().any(|a| a.as_ref() == *r)) {
                continue;
            }
        }
        let mut eqs = Vec::new();
        let mut range = None;
        let mut opposite_bound = None;
        let mut n = 0usize;
        for field in &def.fields {
            if let Some(v) = find_eq_atoms(atoms, field) {
                eqs.push((field.clone(), v.clone()));
                n += 1;
                continue;
            }
            if let Some((op, v)) = find_range_atoms(atoms, field) {
                range = Some((field.clone(), op, v.clone()));
                opposite_bound = atoms.iter().find_map(|atom| match atom.as_ref() {
                    Pred::Cmp {
                        field: other,
                        op: other_op,
                        value,
                    } if other.as_cow_str().as_ref() == field
                        && matches!(op, CmpOp::Gt | CmpOp::Ge)
                            != matches!(other_op, CmpOp::Gt | CmpOp::Ge)
                        && matches!(other_op, CmpOp::Gt | CmpOp::Ge | CmpOp::Lt | CmpOp::Le)
                        && matches!(
                            (v, value),
                            (Value::Int(_), Value::Int(_))
                                | (Value::Timestamp(_), Value::Timestamp(_))
                        ) =>
                    {
                        Some((*other_op, value.clone()))
                    }
                    _ => None,
                });

                n += 1;
            }
            break;
        }
        if n == 0
            || (eqs.is_empty()
                && def.fields.len() > 1
                && !matches!(range, Some((_, _, Value::Int(_) | Value::Timestamp(_)))))
        {
            continue;
        }
        if n > best_n {
            best_n = n;
            best = Some(IndexUse {
                def: (*def).clone(),
                eqs,
                range,
                opposite_bound,
            });
        }
    }
    best
}

/// True when every atomic predicate is enforced by the index seek(s) (no residual filter).
#[allow(dead_code)]
pub fn index_covers_pred(pred: &Pred, uses: &[IndexUse]) -> bool {
    let Some(branches) = dnf_atoms(pred) else {
        return false;
    };
    if branches.len() != uses.len() {
        return false;
    }
    branches
        .iter()
        .zip(uses.iter())
        .all(|(branch, use_)| index_covers_atoms(branch, use_))
}

fn index_covers_atoms(atoms: &[std::borrow::Cow<'_, Pred>], use_: &IndexUse) -> bool {
    let pred_atoms = use_.def.pred.as_ref().map(flatten_and).unwrap_or_default();
    for atom in atoms {
        let atom = atom.as_ref();
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
                let f_cow = field.as_cow_str();
                if !use_
                    .eqs
                    .iter()
                    .any(|(f, v)| f.as_str() == f_cow.as_ref() && v == value)
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
                if rf.as_str() != field.as_cow_str().as_ref()
                    || ((rop, rv) != (op, value)
                        && !use_
                            .opposite_bound
                            .as_ref()
                            .is_some_and(|(o, v)| (o, v) == (op, value)))
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn dnf_branch_count(pred: &Pred, limit: usize) -> usize {
    match pred {
        Pred::In { values, .. } => values.len().max(1),
        Pred::Or(a, b) => {
            let left = dnf_branch_count(a, limit);
            if left > limit {
                return left;
            }
            left.saturating_add(dnf_branch_count(b, limit))
        }
        Pred::And(a, b) => {
            let left = dnf_branch_count(a, limit);
            if left > limit {
                return left;
            }
            left.saturating_mul(dnf_branch_count(b, limit))
        }
        _ => 1,
    }
}

/// Disjunctive normal form: list of atomic conjuncts per branch.
fn dnf_atoms(pred: &Pred) -> Option<Vec<Vec<std::borrow::Cow<'_, Pred>>>> {
    // Bound both input size and expansion. Falling back to a scan keeps the
    // original predicate as a residual filter and preserves query semantics.
    const MAX_BRANCHES: usize = 256;
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
    if dnf_branch_count(pred, MAX_BRANCHES) > MAX_BRANCHES {
        return None;
    }
    dnf_atoms_bounded(pred)
}

fn dnf_atoms_bounded(pred: &Pred) -> Option<Vec<Vec<std::borrow::Cow<'_, Pred>>>> {
    const MAX_BRANCHES: usize = 256;
    match pred {
        Pred::In { field, values } => {
            if values.is_empty() || values.len() > MAX_BRANCHES {
                return None;
            }
            Some(
                values
                    .iter()
                    .map(|v| {
                        vec![std::borrow::Cow::Owned(Pred::Cmp {
                            field: field.clone(),
                            op: CmpOp::Eq,
                            value: v.clone(),
                        })]
                    })
                    .collect(),
            )
        }
        Pred::Or(a, b) => {
            let mut out = dnf_atoms_bounded(a)?;
            let right = dnf_atoms_bounded(b)?;
            if out.len().checked_add(right.len())? > MAX_BRANCHES {
                return None;
            }
            out.extend(right);
            Some(out)
        }
        Pred::And(a, b) => {
            let left = dnf_atoms_bounded(a)?;
            let right = dnf_atoms_bounded(b)?;
            let len = left.len().checked_mul(right.len())?;
            if len > MAX_BRANCHES {
                return None;
            }
            let mut out = Vec::with_capacity(len);
            for l in &left {
                for r in &right {
                    let mut b = Vec::with_capacity(l.len() + r.len());
                    b.extend_from_slice(l);
                    b.extend_from_slice(r);
                    out.push(b);
                }
            }
            Some(out)
        }
        other => Some(vec![vec![std::borrow::Cow::Borrowed(other)]]),
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

fn find_eq_atoms<'a>(atoms: &'a [std::borrow::Cow<'_, Pred>], field: &str) -> Option<&'a Value> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op: CmpOp::Eq,
            value,
        } = a.as_ref()
            && f.as_cow_str().as_ref() == field
        {
            return Some(value);
        }
    }
    None
}

fn find_range_atoms<'a>(atoms: &'a [std::borrow::Cow<'_, Pred>], field: &str) -> Option<(CmpOp, &'a Value)> {
    for a in atoms {
        if let Pred::Cmp {
            field: f,
            op,
            value,
        } = a.as_ref()
            && f.as_cow_str().as_ref() == field
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

        // Size check: IndexPart shrinks to 24 bytes (align 8)
        assert_eq!(std::mem::size_of::<IndexPart>(), 24);

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

        let index = LiveIndex::new(index_def);

        // Test with boolean field - should always use bitmap
        let row_pending = Row::from([("status", Cell::Bool(false))]);
        let row_active = Row::from([("status", Cell::Bool(true))]);
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

        let index = LiveIndex::new(index_def);

        // Create sample data with low cardinality (only 3 distinct values)
        let rows: Vec<Row> = (0..100)
            .map(|i| Row::from([("priority", Cell::Int((i % 3) as i64))]))
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

        let index = LiveIndex::new(index_def);

        // Create sample data with high cardinality (100 distinct values in sample)
        let rows: Vec<Row> = (0..100)
            .map(|i| Row::from([("id", Cell::Int(i as i64))]))
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
            let a = Row::from([("key", Cell::Int(1))]);
            let b = Row::from([("key", Cell::Int(2))]);
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
                .insert_at(0, &Row::from([("key", Cell::Int(3))]))
                .unwrap();
            assert_eq!(
                index.forward[&index.reverse[&0]],
                PostingList::Standard(smallvec::smallvec![0])
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
            let row = Row::from([("key", Cell::Int(1))]);
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
            .map(|i| Row::from([("key", Cell::Int(i))]))
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
            updated.insert("f0", Cell::Int(99));
            index.insert_at(7, &updated).unwrap();
            index.insert_at_new(8, &row).unwrap();
            index.remove_at(7);
            assert_eq!(index.forward.len(), 1);
            assert_eq!(index.reverse.len(), 1);
            assert_eq!(
                index.forward.get(&index.key_of(&row)),
                Some(&PostingList::Standard(smallvec::smallvec![8]))
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
                    .is_none_or(|pred| crate::eval::eval_pred(pred, row, now))
            })
            .map(|(i, row)| (start + i, row))
            .collect();

        if keyed_rows.is_empty() {
            return Ok(());
        }

        // Pre-compute all keys in a single pass
        let keys: Vec<_> = keyed_rows.iter().map(|(_, r)| self.key_of(r)).collect();
        self.reverse.reserve(keyed_rows.len());

        // Bulk insert using pre-computed keys
        for ((orig_idx, row), key) in keyed_rows.iter().zip(keys.into_iter()) {
            use std::collections::btree_map::Entry;

            // Auto-detect bitmap index based on field cardinality
            // If bitmap_card_field is set, estimate cardinality from sample
            let is_bitmap = self.should_use_bitmap(*orig_idx, row, rows);

            match Arc::make_mut(&mut self.forward).entry(key) {
                Entry::Vacant(v) => {
                    self.reverse.insert(*orig_idx, v.key().clone());
                    let mut posting = PostingList::new(is_bitmap, 64);
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
                let row = Row::from([("key", Cell::Int(-1 - i as i64))]);
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
                            ("key", Cell::Int((i % 37) as i64)),
                            ("keep".into(), Cell::Bool(i % 3 != 0)),
                        ]);
                        if cardinality > 0 {
                            r.insert("sample", Cell::Int((i % cardinality) as i64));
                        }
                        r
                    })
                    .collect();
                for boolean_at in [None, Some(0), Some(100)] {
                    let mut case_rows = rows.clone();
                    if let Some(i) = boolean_at {
                        if let Some(row) = case_rows.get_mut(i) {
                            row.insert("sample", Cell::Bool(true));
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
                    ("key", Cell::Int(i)),
                    ("sample".into(), Cell::Int(i % 24)),
                    ("keep".into(), Cell::Bool(true)),
                ])
            })
            .collect();
        for population in [9_999, 10_000, 10_001] {
            compare(&rows, false, false, population);
        }
    }

    #[test]
    fn bulk_slab_promotes_dense_keys_to_bitmap_for_o1_swap_remove() {
        let def = IndexDef {
            collection: "docs".into(),
            unique: false,
            fields: vec!["wing".into()],
            pred: None,
            bitmap_card_field: None,
        };
        let rows: Vec<Row> = (0..10_000)
            .map(|i| {
                Row::from([(
                    "wing",
                    Cell::from_text(match i % 100 {
                        0..=49 => "rag",
                        50..=79 => "sys",
                        _ => "core",
                    }),
                )])
            })
            .collect();
        let mut index = LiveIndex::new(def);
        index.insert_slab_batch_no_reverse(0, &rows).unwrap();
        for list in index.forward.values() {
            assert!(list.is_bitmap(), "expected Bitmap for dense bulk key");
        }
        let use_ = IndexUse {
            def: index.def.clone(),
            eqs: vec![("wing".into(), Value::String(Arc::from("rag")))],
            range: None,
            opposite_bound: None,
        };
        assert_eq!(index.seek_count(&use_, 0), 5_000);
        index.swap_remove_rows(0, &rows[0], 9_999, &rows[9_999]);
        assert_eq!(index.seek_count(&use_, 0), 4_999);
    }

    #[test]
    fn bulk_slab_no_reverse_mru_cache_matches_reference_across_cardinalities_and_arities() {
        for arity in [1usize, 2, 3] {
            let fields: Vec<String> = match arity {
                1 => vec!["wing".into()],
                2 => vec!["wing".into(), "ts".into()],
                _ => vec!["wing".into(), "ts".into(), "tier".into()],
            };
            for card in [1usize, 2, 3, 4, 8, 64, 500] {
                let rows: Vec<Row> = (0..1_000)
                    .map(|i| {
                        let k = i % card;
                        Row::from([
                            ("wing", Cell::from_text(format!("w{}", k % 5))),
                            ("ts", Cell::Time(1_700_000_000 + (k as i64) * 10)),
                            ("tier", Cell::Int((k / 2) as i64)),
                        ])
                    })
                    .collect();

                let def = IndexDef {
                    collection: "docs".into(),
                    unique: false,
                    fields: fields.clone(),
                    pred: None,
                    bitmap_card_field: None,
                };
                let mut fast = LiveIndex::new(def.clone());
                fast.insert_slab_batch_no_reverse(10, &rows).unwrap();

                let mut reference = LiveIndex::new(def);
                reference.insert_slab_batch_reference(10, &rows).unwrap();

                assert_eq!(fast.forward.len(), reference.forward.len());
                for ((ka, la), (kb, lb)) in fast.forward.iter().zip(reference.forward.iter()) {
                    assert_eq!(ka, kb);
                    assert_eq!(la.to_indices(), lb.to_indices());
                }
            }
        }

        // Cross-variant numeric key equivalence (`Cell::Int(5)` vs `Cell::Float(5.0)`).
        let mixed_rows: Vec<Row> = (0..128)
            .map(|i| {
                if i % 2 == 0 {
                    Row::from([("v", Cell::Int((i % 3) as i64))])
                } else {
                    Row::from([("v", Cell::Float((i % 3) as f64))])
                }
            })
            .collect();
        let def = IndexDef {
            collection: "nums".into(),
            unique: false,
            fields: vec!["v".into()],
            pred: None,
            bitmap_card_field: None,
        };
        let mut fast = LiveIndex::new(def.clone());
        fast.insert_slab_batch_no_reverse(0, &mixed_rows).unwrap();
        let mut reference = LiveIndex::new(def);
        reference.insert_slab_batch_reference(0, &mixed_rows).unwrap();
        assert_eq!(fast.forward.len(), 3);
        for ((ka, la), (kb, lb)) in fast.forward.iter().zip(reference.forward.iter()) {
            assert_eq!(ka, kb);
            assert_eq!(la.to_indices(), lb.to_indices());
        }
    }

    #[test]
    fn bitset_o1_len_and_trailing_zero_equality() {
        let mut a = BitSet::new(64);
        let mut b = BitSet::new(4096);
        assert!(a.is_empty());
        assert!(b.is_empty());
        assert_eq!(a, b);

        a.insert(5);
        a.insert(63);
        b.insert(5);
        b.insert(63);
        assert_eq!(a.count(), 2);
        assert_eq!(b.count(), 2);
        assert_eq!(a, b);

        b.insert(2048);
        assert_ne!(a, b);
        b.remove(2048);
        assert_eq!(b.count(), 2);
        assert_eq!(a, b);

        let c = BitSet::from_sorted_unique(&[5, 63, 130, 500]);
        assert_eq!(c.count(), 4);
        assert_eq!(c.to_indices(), vec![5, 63, 130, 500]);

        let mut u = a.clone();
        u.or_assign(&c);
        assert_eq!(u.count(), 4);
        assert_eq!(u, c);

        u.and_assign(&a);
        assert_eq!(u.count(), 2);
        assert_eq!(u, a);
    }
}
