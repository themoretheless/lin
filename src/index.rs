use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};

use crate::ast::{CmpOp, Pred, Value};
use crate::catalog::{Catalog, IndexDef};
use crate::store::{Cell, Row};

/// Bloom filter for fast negative lookups during index seeks
/// Reduces false positives with minimal memory overhead
#[cfg(feature = "parallel")]
struct BloomFilter {
    /// Bit array
    bits: Vec<u64>,
    num_hashes: usize,
    capacity: usize,
}

#[cfg(feature = "parallel")]
impl BloomFilter {
    fn new(capacity: usize) -> Self {
        // Calculate bit array size (16MB max for reasonable memory footprint)
        let num_bits = 8 * 1024 * 1024; // 1M bits = 128KB
        let num_hashes = 7; // Optimal number of hash functions for low false positive rate
        
        Self {
            bits: vec![0u64; num_bits / 64],
            num_hashes,
            capacity,
        }
    }

    fn hash(&self, item: impl Hash) -> u64 {
        let mut hasher = FxHasher::default();
        item.hash(&mut hasher);
        hasher.finish()
    }

    fn add(&mut self, item: impl Hash) {
        if self.bits.is_empty() {
            return;
        }
        
        let base_hash = self.hash(item);
        let delta = base_hash.wrapping_mul(0x9e3779b97f4a7c15); // Golden ratio
        
        for i in 0..self.num_hashes {
            let h = if i == 0 {
                base_hash
            } else {
                delta ^= base_hash
            };
            
            let bit_pos = (h as usize) % (self.bits.len() * 64);
            let mask = 1u64 << (bit_pos % 64);
            self.bits[bit_pos / 64] |= mask;
        }
    }

    fn contains(&self, item: impl Hash) -> bool {
        if self.bits.is_empty() {
            return true; // Assume present when empty
        }
        
        let base_hash = self.hash(item);
        let delta = base_hash.wrapping_mul(0x9e3779b97f4a7c15);
        
        for i in 0..self.num_hashes {
            let h = if i == 0 {
                base_hash
            } else {
                delta ^= base_hash
            };
            
            let bit_pos = (h as usize) % (self.bits.len() * 64);
            let mask = 1u64 << (bit_pos % 64);
            
            if self.bits[bit_pos / 64] & mask == 0 {
                return false; // Definitely not present
            }
        }
        true // Probably present
    }
}

/// Composite bloom filter for multi-key index checks
#[cfg(feature = "parallel")]
pub struct MultiBloomFilter {
    filters: Vec<BloomFilter>,
}

#[cfg(feature = "parallel")]
impl MultiBloomFilter {
    fn new(num_filters: usize, capacity: usize) -> Self {
        Self {
            filters: (0..num_filters).map(|_| BloomFilter::new(capacity)).collect(),
        }
    }

    fn add_keys(&mut self, keys: &[IndexKey]) {
        for key in keys {
            for filter in &mut self.filters {
                filter.add(key);
            }
        }
    }

    fn check_all(&self, target: &IndexKey) -> bool {
        self.filters.iter().all(|f| f.contains(target))
    }
}


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
            PostingList::Standard(vec) => vec.push(pos),
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
        match self {
            PostingList::Standard(vec) => vec.push(pos),
            PostingList::Bitmap(set) => set.insert(pos),
        }
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

    pub fn to_indices(&self) -> Vec<usize> {
        match self {
            PostingList::Standard(vec) => vec.clone(),
            PostingList::Bitmap(set) => set.to_indices(),
        }
    }

    pub fn and_assign(&mut self, other: PostingList) {
        // Both bitmaps: do bitwise AND
        if let PostingList::Bitmap(self_set) = self {
            if let PostingList::Bitmap(other_set) = &other {
                let mut new_set = (*self_set).clone();
                new_set.and_assign(other_set);
                *self = PostingList::Bitmap(new_set);
                return;
            }
        }
        
        // Otherwise: standard intersection
        let self_indices = self.to_indices();
        let other_indices = other.to_indices();
        let result = self_indices.into_iter().filter(|i| other_indices.contains(i)).collect();
        *self = PostingList::Standard(result);
    }

    pub fn or_assign(&mut self, other: PostingList) {
        // Both bitmaps: do bitwise OR
        if let PostingList::Bitmap(self_set) = self {
            if let PostingList::Bitmap(other_set) = &other {
                let mut new_set = (*self_set).clone();
                new_set.or_assign(other_set);
                *self = PostingList::Bitmap(new_set);
                return;
            }
        }
        
        // Otherwise: standard union
        let mut self_indices = self.to_indices();
        let other_indices = other.to_indices();
        for i in other_indices {
            if !self_indices.contains(&i) {
                self_indices.push(i);
            }
        }
        self_indices.sort_unstable();
        self_indices.dedup();
        *self = PostingList::Standard(self_indices);
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
    pub fn maybe_exists_fast(&self, key: &IndexKey) -> bool {
        #[cfg(feature = "parallel")]
        if let Some(ref filter) = self.bloom_filter {
            return filter.contains(key);
        }
        
        // Fallback: always assume present when no filter or feature not enabled
        true
    }

    pub fn key_of(&self, row: &Row) -> IndexKey {
        let parts = self
            .def
            .fields
            .iter()
            .map(|f| cell_part(row.get(f).unwrap_or(&Cell::Null)))
            .collect();
        IndexKey(parts)
    }

    pub fn insert_at(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        // Partial index check: only insert if pred matches (or no pred)
        if let Some(ref pred) = self.def.pred {
            if !crate::exec::eval_pred(pred, row, 0) {
                return Ok(()); // Skip indexing for non-matching rows
            }
        }
        
        let key = self.key_of(row);
        self.insert_key(idx, key)
    }

    /// Append-only bulk path: no reverse-key replace (fresh indices).
    pub fn insert_at_new(&mut self, idx: usize, row: &Row) -> Result<(), String> {
        // Partial index check: only insert if pred matches (or no pred)
        if let Some(ref pred) = self.def.pred {
            if !crate::exec::eval_pred(pred, row, 0) {
                return Ok(()); // Skip indexing for non-matching rows
            }
        }
        
        let key = self.key_of(row);
        if self.def.unique && self.forward.contains_key(&key) {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        use std::collections::btree_map::Entry;
        
        // Check if this is a bitmap index (for boolean fields)
        let is_bitmap = self.def.bitmap_card_field.as_ref().map_or(false, |field| {
            if let Some(cell) = row.get(field) {
                matches!(cell, Cell::Bool(_))
            } else {
                false
            }
        });
        
        match self.forward.entry(key) {
            Entry::Vacant(v) => {
                self.reverse.insert(idx, v.key().clone());
                let mut posting = PostingList::new(is_bitmap, 1 << 16);
                posting.push(idx);
                v.insert(posting);
            }
            Entry::Occupied(mut o) => {
                self.reverse.insert(idx, o.key().clone());
                o.get_mut().push(idx);
            }
        }
        Ok(())
    }

    /// Batch-only insert for slab operations: compute all keys first, then bulk insert
    /// to reduce per-row HashMap overhead and key cloning.
    pub fn insert_slab_batch(
        &mut self,
        start: usize,
        rows: &[Row],
    ) -> Result<(), String> {
        // Partial index check: only insert matching rows
        let keyed_rows: Vec<(usize, Row)> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.def.pred.as_ref().map_or(true, |pred| {
                    crate::exec::eval_pred(pred, row, 0)
                })
            })
            .map(|(i, row)| (start + i, (*row).clone()))
            .collect();
        
        if keyed_rows.is_empty() {
            return Ok(());
        }
        
        // Pre-compute all keys in a single pass
        let keys: Vec<_> = keyed_rows.iter().map(|(_, r)| self.key_of(r)).collect();
        
        // Bulk insert using pre-computed keys
        for ((orig_idx, row), key) in keyed_rows.iter().zip(keys.into_iter()) {
            if self.def.unique && self.forward.contains_key(&key) {
                return Err(format!("unique index {}: duplicate key", self.def.label()));
            }
            use std::collections::btree_map::Entry;
            
            // Check if this is a bitmap index (for boolean fields)
            let is_bitmap = self.def.bitmap_card_field.as_ref().map_or(false, |field| {
                if let Some(cell) = row.get(field) {
                    matches!(cell, Cell::Bool(_))
                } else {
                    false
                }
            });
            
            match self.forward.entry(key) {
                Entry::Vacant(v) => {
                    self.reverse.insert(*orig_idx, v.key().clone());
                    let mut posting = PostingList::new(is_bitmap, 1 << 16);
                    posting.push(*orig_idx);
                    v.insert(posting);
                }
                Entry::Occupied(mut o) => {
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
            && ids.to_indices().iter().any(|&x| x != idx)
        {
            return Err(format!("unique index {}: duplicate key", self.def.label()));
        }
        if self.reverse.get(&idx) == Some(&key) {
            return Ok(());
        }
        
        // Check if this is a bitmap index (low cardinality column)
        let is_bitmap = self.def.bitmap_card_field.as_ref().map_or(false, |field| {
            if let Some(cell) = self.reverse.get(&idx).and_then(|k| k.0.first()) {
                matches!(cell, IndexPart::Bool(_))
            } else {
                false
            }
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
                let mut posting = PostingList::new(is_bitmap, 1 << 16); // Support up to 65K rows
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

    fn bounds(&self, use_: &IndexUse, now: i64) -> (Bound<IndexKey>, Bound<IndexKey>) {
        let arity = self.def.fields.len();
        let eqs: smallvec::SmallVec<[IndexPart; 2]> =
            use_.eqs.iter().map(|(_, v)| value_part(v, now)).collect();
        let mut lo: smallvec::SmallVec<[IndexPart; 2]> = eqs.iter().map(numeric_low).collect();
        let mut hi: smallvec::SmallVec<[IndexPart; 2]> = eqs.iter().map(numeric_high).collect();
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

    /// Seek row indices by range bounds: extend output vector from BTreeMap iteration
    pub fn seek_idxs(&self, use_: &IndexUse, now: i64) -> Vec<usize> {
        let (start, end) = self.bounds(use_, now);
        
        // Use bloom filter for fast early termination if available
        #[cfg(feature = "parallel")]
        {
            if let Some(ref filter) = self.bloom_filter {
                // Quick check: if target key not in bloom filter, skip this range
                // Note: This is a heuristic - we still need full BTree scan for correctness
                // The bloom filter helps identify when index has no matches at all
                let potential_key = start.clone();
                if !filter.contains(&potential_key) {
                    return Vec::new();
                }
            }
        }
        
        let mut out = Vec::new();
        for (_, list) in self.forward.range((start, end)) {
            out.extend_from_slice(&list.to_indices());
        }
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
                    out.extend_from_slice(&list.to_indices());
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
        Cell::Float(n) => numeric_part(*n, None),
        Cell::Vec(_) => IndexPart::Null,
    }
}

fn value_part(v: &Value, now: i64) -> IndexPart {
    match v {
        Value::String(s) => IndexPart::Text(Arc::clone(s)),
        Value::Name(s) => IndexPart::Text(Arc::from(s.as_str())),
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
        let num_words = (capacity + 63) / 64;
        Self {
            bits: vec![0u64; num_words],
            capacity,
        }
    }

    #[inline]
    pub fn insert(&mut self, pos: usize) {
        if pos >= self.capacity {
            return;
        }
        let word_idx = pos >> 6; // pos / 64
        let bit_idx = pos & 63;   // pos % 64
        self.bits[word_idx] |= 1u64 << bit_idx;
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        if pos >= self.capacity {
            return;
        }
        let word_idx = pos >> 6;
        let bit_idx = pos & 63;
        self.bits[word_idx] &= !(1u64 << bit_idx);
    }

    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        if pos >= self.capacity {
            return false;
        }
        let word_idx = pos >> 6;
        let bit_idx = pos & 63;
        (self.bits[word_idx] & (1u64 << bit_idx)) != 0
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Convert to Vec<usize> of set positions
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count());
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            for bit_idx in 0..64 {
                if (word & (1u64 << bit_idx)) != 0 {
                    let pos = base_pos + bit_idx as usize;
                    if pos < self.capacity {
                        out.push(pos);
                    }
                }
            }
        }
        out
    }

    /// Bitwise AND (intersection) - modifies self
    pub fn and_assign(&mut self, other: &BitSet) {
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= b;
        }
    }

    /// Bitwise OR (union) - modifies self
    pub fn or_assign(&mut self, other: &BitSet) {
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
    for atom in flatten_and(pred) {
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
        if let Pred::And(a, b) | Pred::Or(a, b) = p {
            pending.extend([a.as_ref(), b.as_ref()]);
        }
    }
    dnf_bounded(pred)
}

fn dnf_bounded(pred: &Pred) -> Option<Vec<Pred>> {
    const MAX_BRANCHES: usize = 256;
    match pred {
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
    match pred {
        Pred::And(a, b) => {
            let mut v = flatten_and(a);
            v.extend(flatten_and(b));
            v
        }
        other => vec![other],
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
            assert_eq!(actual.to_indices(), if unique { vec![0] } else { vec![0, 1] });
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
            assert_eq!(index.forward[&index.reverse[&0]], PostingList::Standard(vec![0]));
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
            assert_eq!(index.forward.get(&index.key_of(&row)), Some(&PostingList::Standard(vec![8])));
        }
    }
}
