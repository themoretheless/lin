//! Slotted row: field names live in a shared, interned [`Shape`]; a row owns only its cells.
//!
//! `Row` keeps the map-like API the engine was written against (sorted field order,
//! `get` / `insert` / `remove` / iteration yielding `(&String, &Cell)`), but cloning a row
//! clones one `Arc` and the cells instead of a tree of owned `String` keys, and rows with
//! the same set of fields share one shape, in whatever order the fields were inserted.

use std::cmp::Ordering;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, LazyLock, RwLock};

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;

use super::Cell;

/// Cells stored inline before spilling to the heap.
type Cells = SmallVec<[Cell; 4]>;

/// Shapes kept in the transition cache. Past this, new key sets still work but are not
/// shared (arbitrary user-supplied keys must not grow the cache without bound).
const MAX_INTERNED_SHAPES: usize = 65_536;

static INTERNED_SHAPES: AtomicUsize = AtomicUsize::new(0);

#[inline]
fn name_tag(s: &str) -> u16 {
    ((s.len().min(255) as u16) << 8) | (s.as_bytes().first().copied().unwrap_or(0) as u16)
}

#[inline]
fn build_tags(names: &[String]) -> Box<[u16]> {
    names.iter().map(|s| name_tag(s)).collect()
}

/// Sorted field names of a row, shared by every row with the same fields.
pub struct Shape {
    names: Arc<[String]>,
    tags: Box<[u16]>,
    /// Shapes reached by adding one field: `(position of the new field, shape)`.
    children: RwLock<Vec<(usize, Arc<Shape>)>>,
}

/// Every interned shape by its names, so that a set of names has one shape no matter
/// which transitions led to it. Consulted only when a transition is not cached yet.
static BY_NAMES: LazyLock<std::sync::Mutex<rustc_hash::FxHashMap<Arc<[String]>, Arc<Shape>>>> =
    LazyLock::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

static ROOT: LazyLock<Arc<Shape>> = LazyLock::new(|| {
    Arc::new(Shape {
        names: Arc::from([]),
        tags: Box::new([]),
        children: RwLock::new(Vec::new()),
    })
});

impl Shape {
    /// Field names in sorted order.
    #[inline]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Position of `key` in this shape, using a compact `(len, first_byte)` tag scan for
    /// shapes up to 24 fields (avoiding pointer-chasing `String` buffers on non-matches)
    /// and binary search for wider shapes.
    #[inline]
    pub fn position(&self, key: &str) -> Option<usize> {
        let names = &*self.names;
        if names.len() <= 24 {
            let key_tag = name_tag(key);
            for (i, &t) in self.tags.iter().enumerate() {
                if t == key_tag && names[i].as_str() == key {
                    return Some(i);
                }
            }
            None
        } else {
            names.binary_search_by(|n| n.as_str().cmp(key)).ok()
        }
    }

    #[inline]
    fn find(&self, key: &str) -> Result<usize, usize> {
        let names = &*self.names;
        if names.len() <= 8 {
            // Short rows: one pass, equality first (most lookups hit).
            let mut at = names.len();
            for (i, n) in names.iter().enumerate() {
                match n.as_str().cmp(key) {
                    Ordering::Equal => return Ok(i),
                    Ordering::Greater => {
                        at = i;
                        break;
                    }
                    Ordering::Less => {}
                }
            }
            Err(at)
        } else {
            names.binary_search_by(|n| n.as_str().cmp(key))
        }
    }

    /// Pre-resolve `key` against this shape: `Ok(existing_pos)` if already present, or
    /// `Err((insert_at, child_shape))` so callers updating uniform batches pay transition
    /// lookup once per batch instead of once per row.
    #[inline]
    pub(crate) fn prepare_insert(self: &Arc<Self>, key: &str) -> Result<usize, (usize, Arc<Shape>)> {
        match self.find(key) {
            Ok(pos) => Ok(pos),
            Err(at) => Err((at, self.with(at, key))),
        }
    }

    /// The shape with `key` added at sorted position `at`.
    fn with(self: &Arc<Self>, at: usize, key: &str) -> Arc<Shape> {
        if let Ok(children) = self.children.read() {
            for (pos, child) in children.iter() {
                if *pos == at && child.names[at] == key {
                    return Arc::clone(child);
                }
            }
        }
        let mut names: Vec<String> = Vec::with_capacity(self.names.len() + 1);
        names.extend(self.names[..at].iter().cloned());
        names.push(key.to_owned());
        names.extend(self.names[at..].iter().cloned());
        let names: Arc<[String]> = Arc::from(names);
        if INTERNED_SHAPES.load(AtomicOrdering::Relaxed) >= MAX_INTERNED_SHAPES {
            // Cache full: the key set still works, it is just not shared.
            let tags = build_tags(&names);
            return Arc::new(Shape {
                names,
                tags,
                children: RwLock::new(Vec::new()),
            });
        }
        // One shape per set of names, whatever order the fields were inserted in: the
        // transition found here may lead to a shape another insertion order already made.
        let child = {
            let mut by_names = match BY_NAMES.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            match by_names.get(&names) {
                Some(existing) => Arc::clone(existing),
                None => {
                    let tags = build_tags(&names);
                    let made = Arc::new(Shape {
                        names: Arc::clone(&names),
                        tags,
                        children: RwLock::new(Vec::new()),
                    });
                    by_names.insert(names, Arc::clone(&made));
                    INTERNED_SHAPES.fetch_add(1, AtomicOrdering::Relaxed);
                    made
                }
            }
        };
        if let Ok(mut children) = self.children.write()
            && !children
                .iter()
                .any(|(pos, existing)| *pos == at && existing.names[at] == key)
        {
            children.push((at, Arc::clone(&child)));
        }
        child
    }

    /// Interned shape for a sorted, duplicate-free list of names.
    fn of_sorted<'a>(names: impl Iterator<Item = &'a str>) -> Arc<Shape> {
        let mut shape = Arc::clone(&ROOT);
        for (i, name) in names.enumerate() {
            shape = shape.with(i, name);
        }
        shape
    }
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names.iter()).finish()
    }
}

/// Remembers where one field sits in the shapes seen so far, so reading that field from
/// many rows of the same shape is a pointer comparison and an index instead of a search
/// by name. A few shapes are kept, because rows of one collection differ in which
/// optional fields they carry.
pub struct FieldSlot<'k> {
    key: &'k str,
    /// `(shape, position of the field in it)`, most recently added last.
    seen: SmallVec<[(Arc<Shape>, Option<usize>); 4]>,
}

impl<'k> FieldSlot<'k> {
    const KEPT_SHAPES: usize = 4;

    #[inline]
    pub fn new(key: &'k str) -> Self {
        FieldSlot {
            key,
            seen: SmallVec::new(),
        }
    }

    #[inline]
    pub fn key(&self) -> &'k str {
        self.key
    }

    /// The field's value in `row`; same result as `row.get(key)`.
    #[inline]
    pub fn get<'r>(&mut self, row: &'r Row) -> Option<&'r Cell> {
        let shape = row.shape.as_ref()?;
        for (seen, at) in self.seen.iter().rev() {
            if Arc::ptr_eq(seen, shape) {
                return at.map(|i| &row.cells[i]);
            }
        }
        let at = shape.position(self.key);
        if self.seen.len() == Self::KEPT_SHAPES {
            self.seen.remove(0);
        }
        // Holding the `Arc` keeps the address from being reused by another shape.
        self.seen.push((Arc::clone(shape), at));
        at.map(|i| &row.cells[i])
    }
}

/// Owned, `'static`-free counterpart of [`FieldSlot`], suitable for storing inside
/// [`crate::Prepared`] plans and [`crate::QueryCursor`] states.
pub struct OwnedFieldSlot {
    key: Arc<str>,
    seen: SmallVec<[(Arc<Shape>, Option<usize>); 4]>,
    sync_seen: std::sync::OnceLock<(Arc<Shape>, Option<usize>)>,
}

impl Clone for OwnedFieldSlot {
    fn clone(&self) -> Self {
        let sync_seen = std::sync::OnceLock::new();
        if let Some(cached) = self.sync_seen.get() {
            let _ = sync_seen.set(cached.clone());
        }
        Self {
            key: Arc::clone(&self.key),
            seen: self.seen.clone(),
            sync_seen,
        }
    }
}

impl fmt::Debug for OwnedFieldSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedFieldSlot")
            .field("key", &self.key)
            .finish()
    }
}

impl OwnedFieldSlot {
    const KEPT_SHAPES: usize = 4;

    #[inline]
    pub fn new(key: impl Into<Arc<str>>) -> Self {
        Self {
            key: key.into(),
            seen: SmallVec::new(),
            sync_seen: std::sync::OnceLock::new(),
        }
    }

    #[inline]
    pub fn key(&self) -> &str {
        &self.key
    }

    #[inline]
    pub fn get<'r>(&mut self, row: &'r Row) -> Option<&'r Cell> {
        let shape = row.shape.as_ref()?;
        for (seen, at) in self.seen.iter().rev() {
            if Arc::ptr_eq(seen, shape) {
                return at.map(|i| &row.cells[i]);
            }
        }
        let at = shape.position(&self.key);
        if self.seen.len() == Self::KEPT_SHAPES {
            self.seen.remove(0);
        }
        self.seen.push((Arc::clone(shape), at));
        at.map(|i| &row.cells[i])
    }

    /// Lock-free `&self` lookup for shared `Prepared` plans (`PointGetPlan`, `AggPlan`):
    /// caches the collection's primary `Shape` on first call and falls back to `Shape::position`
    /// only if a row has a different shape.
    #[inline]
    pub fn get_sync<'r>(&self, row: &'r Row) -> Option<&'r Cell> {
        let shape = row.shape.as_ref()?;
        let (cached_shape, cached_at) = self
            .sync_seen
            .get_or_init(|| (Arc::clone(shape), shape.position(&self.key)));
        if Arc::ptr_eq(cached_shape, shape) {
            return cached_at.map(|i| &row.cells[i]);
        }
        shape.position(&self.key).map(|i| &row.cells[i])
    }
}

/// One record: field name -> [`Cell`], iterated in field-name order.
#[derive(Default)]
pub struct Row {
    /// `None` is the empty row (no allocation, no refcount).
    shape: Option<Arc<Shape>>,
    cells: Cells,
}

pub static WIDE_CLONES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl Clone for Row {
    fn clone(&self) -> Self {
        let cells = if self.cells.len() > self.cells.inline_size() {
            WIDE_CLONES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Cells::from_vec(self.cells.as_slice().to_vec())
        } else {
            self.cells.clone()
        };
        Self {
            shape: self.shape.clone(),
            cells,
        }
    }
}

impl Row {
    pub fn wide_clone_count() -> usize {
        WIDE_CLONES.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[inline]
    pub const fn new() -> Self {
        Row {
            shape: None,
            cells: SmallVec::new_const(),
        }
    }

    #[inline]
    pub fn from_single(shape: Arc<Shape>, c0: Cell) -> Self {
        debug_assert_eq!(shape.names.len(), 1);
        Row {
            shape: Some(shape),
            cells: smallvec::smallvec![c0],
        }
    }

    #[inline]
    pub fn from_pair(shape: Arc<Shape>, c0: Cell, c1: Cell) -> Self {
        debug_assert_eq!(shape.names.len(), 2);
        Row {
            shape: Some(shape),
            cells: smallvec::smallvec![c0, c1],
        }
    }

    #[inline]
    pub(crate) fn insert_at_new_shape(&mut self, at: usize, new_shape: Arc<Shape>, value: Cell) {
        self.shape = Some(new_shape);
        self.cells.insert(at, value);
    }

    /// A row with the given shape and cells (`cells.len()` must equal the shape's width).
    #[inline]
    pub fn from_parts(shape: Arc<Shape>, cells: impl IntoIterator<Item = Cell>) -> Self {
        let cells: Cells = cells.into_iter().collect();
        debug_assert_eq!(cells.len(), shape.names.len());
        Row {
            shape: Some(shape),
            cells,
        }
    }

    #[inline]
    pub fn from_vec_parts(shape: Arc<Shape>, vec: Vec<Cell>) -> Self {
        debug_assert_eq!(vec.len(), shape.names.len());
        Row {
            shape: Some(shape),
            cells: SmallVec::from_vec(vec),
        }
    }

    #[inline]
    pub fn shape_arc(&self) -> Option<&Arc<Shape>> {
        self.shape.as_ref()
    }

    #[inline]
    pub fn cells_slice(&self) -> &[Cell] {
        &self.cells
    }

    /// Replace this row's contents in place from a precomputed [`Shape`] and cells in
    /// sorted field-name order, reusing the row's inline buffer.
    #[inline]
    pub fn reset_from_parts(&mut self, shape: Arc<Shape>, cells: impl IntoIterator<Item = Cell>) {
        self.cells.clear();
        self.cells.extend(cells);
        debug_assert_eq!(self.cells.len(), shape.names.len());
        self.shape = Some(shape);
    }

    /// Replace a 2-column row's shape and cells in place without atomic shape refcount traffic
    /// when `shape` already matches `self.shape`.
    #[inline]
    pub fn set_pair_with_shape(&mut self, shape: &Arc<Shape>, c0: Cell, c1: Cell) {
        if !self.shape.as_ref().is_some_and(|s| Arc::ptr_eq(s, shape)) {
            self.shape = Some(Arc::clone(shape));
        }
        debug_assert_eq!(shape.names.len(), 2);
        if self.cells.len() == 2 {
            self.cells[0] = c0;
            self.cells[1] = c1;
        } else {
            self.cells.clear();
            self.cells.push(c0);
            self.cells.push(c1);
        }
    }

    /// Replace this row's contents in place using a shared shape reference without atomic
    /// refcount churn when the shape pointer already matches.
    #[inline]
    pub fn reset_with_shape_ref(
        &mut self,
        shape: &Arc<Shape>,
        cells: impl IntoIterator<Item = Cell>,
    ) {
        if !self.shape.as_ref().is_some_and(|s| Arc::ptr_eq(s, shape)) {
            self.shape = Some(Arc::clone(shape));
        }
        self.cells.clear();
        self.cells.extend(cells);
        debug_assert_eq!(self.cells.len(), shape.names.len());
    }

    /// A row from names in strictly ascending order and their cells, without a search
    /// per field.
    pub fn from_sorted<'a>(
        names: impl Iterator<Item = &'a str>,
        cells: impl IntoIterator<Item = Cell>,
    ) -> Self {
        let cells: Cells = cells.into_iter().collect();
        if cells.is_empty() {
            return Row::new();
        }
        let shape = Shape::of_sorted(names);
        debug_assert_eq!(cells.len(), shape.names.len());
        debug_assert!(shape.names.windows(2).all(|w| w[0] < w[1]));
        Row {
            shape: Some(shape),
            cells,
        }
    }

    /// Shape for a set of field names (any order, duplicates ignored); pair with
    /// [`Row::from_parts`] to build many rows without per-row name handling.
    pub fn shape_of<S: AsRef<str>>(names: &[S]) -> Arc<Shape> {
        let mut sorted: Vec<&str> = names.iter().map(AsRef::as_ref).collect();
        sorted.sort_unstable();
        sorted.dedup();
        Shape::of_sorted(sorted.into_iter())
    }

    /// This row's shape (`None` for the empty row).
    #[inline]
    pub fn shape(&self) -> Option<&Arc<Shape>> {
        self.shape.as_ref()
    }

    #[inline]
    fn names(&self) -> &[String] {
        match &self.shape {
            Some(s) => &s.names,
            None => &[],
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    #[inline]
    fn find(&self, key: &str) -> Result<usize, usize> {
        match &self.shape {
            Some(s) => s.find(key),
            None => Err(0),
        }
    }

    #[inline]
    fn position(&self, key: &str) -> Option<usize> {
        self.shape.as_ref()?.position(key)
    }

    #[inline]
    pub fn get_str(&self, key: &str) -> Option<&Cell> {
        self.position(key).map(|i| &self.cells[i])
    }

    #[inline]
    pub fn get<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> Option<&Cell> {
        self.get_str(key.as_ref())
    }

    #[inline]
    pub fn get_mut_str(&mut self, key: &str) -> Option<&mut Cell> {
        self.position(key).map(|i| &mut self.cells[i])
    }

    #[inline]
    pub fn get_mut<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<&mut Cell> {
        self.get_mut_str(key.as_ref())
    }

    #[inline]
    pub fn get_key_value_str(&self, key: &str) -> Option<(&String, &Cell)> {
        let i = self.position(key)?;
        Some((&self.names()[i], &self.cells[i]))
    }

    #[inline]
    pub fn get_key_value<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> Option<(&String, &Cell)> {
        self.get_key_value_str(key.as_ref())
    }

    #[inline]
    pub fn contains_key_str(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    #[inline]
    pub fn contains_key<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> bool {
        self.contains_key_str(key.as_ref())
    }

    /// Set `key` to `value`; returns the previous value, if any.
    pub fn insert_str(&mut self, key: &str, value: Cell) -> Option<Cell> {
        // Rows are usually built in field-name order: a key past the last name appends
        // without searching.
        let found = match self.names().last() {
            Some(last) if last.as_str() < key => Err(self.cells.len()),
            _ => self.find(key),
        };
        match found {
            Ok(i) => Some(std::mem::replace(&mut self.cells[i], value)),
            Err(at) => {
                let shape = self.shape.as_ref().unwrap_or(&ROOT).with(at, key);
                self.shape = Some(shape);
                self.cells.insert(at, value);
                None
            }
        }
    }

    /// Set `key` to `value`; returns the previous value, if any.
    #[inline]
    pub fn insert(&mut self, key: impl AsRef<str>, value: Cell) -> Option<Cell> {
        self.insert_str(key.as_ref(), value)
    }

    /// Mutable slot for `key`, inserting `default()` when absent.
    pub fn get_or_insert_with(&mut self, key: &str, default: impl FnOnce() -> Cell) -> &mut Cell {
        let i = match self.find(key) {
            Ok(i) => i,
            Err(at) => {
                let shape = self.shape.as_ref().unwrap_or(&ROOT).with(at, key);
                self.shape = Some(shape);
                self.cells.insert(at, default());
                at
            }
        };
        &mut self.cells[i]
    }

    pub fn remove_str(&mut self, key: &str) -> Option<Cell> {
        let i = self.position(key)?;
        let cell = self.cells.remove(i);
        self.reshape_without(|pos, _| pos != i);
        Some(cell)
    }

    #[inline]
    pub fn remove<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<Cell> {
        self.remove_str(key.as_ref())
    }

    pub fn remove_entry_str(&mut self, key: &str) -> Option<(String, Cell)> {
        let i = self.position(key)?;
        let name = self.names()[i].clone();
        let cell = self.cells.remove(i);
        self.reshape_without(|pos, _| pos != i);
        Some((name, cell))
    }

    #[inline]
    pub fn remove_entry<Q: AsRef<str> + ?Sized>(&mut self, key: &Q) -> Option<(String, Cell)> {
        self.remove_entry_str(key.as_ref())
    }

    /// Replace the shape by the one keeping only names for which `keep(position, name)`.
    /// The cells must already match.
    fn reshape_without(&mut self, keep: impl Fn(usize, &str) -> bool) {
        let shape = match &self.shape {
            Some(s) => Shape::of_sorted(
                s.names
                    .iter()
                    .enumerate()
                    .filter(|(i, n)| keep(*i, n))
                    .map(|(_, n)| n.as_str()),
            ),
            None => return,
        };
        self.shape = (!shape.names.is_empty()).then_some(shape);
    }

    /// Keep only the fields for which `f` returns true.
    pub fn retain(&mut self, mut f: impl FnMut(&String, &mut Cell) -> bool) {
        let Some(shape) = self.shape.clone() else {
            return;
        };
        let mut keep: SmallVec<[bool; 16]> = SmallVec::with_capacity(self.cells.len());
        for (name, cell) in shape.names.iter().zip(self.cells.iter_mut()) {
            keep.push(f(name, cell));
        }
        if keep.iter().all(|&k| k) {
            return;
        }
        let mut i = 0;
        self.cells.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
        self.reshape_without(|pos, _| keep[pos]);
    }

    pub fn clear(&mut self) {
        self.shape = None;
        self.cells.clear();
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.names().iter().zip(self.cells.iter()),
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        let names: &[String] = match &self.shape {
            Some(s) => &s.names,
            None => &[],
        };
        IterMut {
            inner: names.iter().zip(self.cells.iter_mut()),
        }
    }

    #[inline]
    pub fn keys(&self) -> std::slice::Iter<'_, String> {
        self.names().iter()
    }

    #[inline]
    pub fn values(&self) -> std::slice::Iter<'_, Cell> {
        self.cells.iter()
    }

    #[inline]
    pub fn values_mut(&mut self) -> std::slice::IterMut<'_, Cell> {
        self.cells.iter_mut()
    }

    /// Cells in field-name order.
    #[inline]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Mutable cells in field-name order (the shape is fixed).
    #[inline]
    pub fn cells_mut(&mut self) -> &mut [Cell] {
        &mut self.cells
    }

    #[inline]
    pub fn into_values(self) -> smallvec::IntoIter<[Cell; 4]> {
        self.cells.into_iter()
    }

    #[inline]
    pub fn first_key_value(&self) -> Option<(&String, &Cell)> {
        self.iter().next()
    }

    #[inline]
    pub fn last_key_value(&self) -> Option<(&String, &Cell)> {
        self.iter().next_back()
    }

    /// Move every field of `other` into `self` (values in `other` win), leaving it empty.
    pub fn append(&mut self, other: &mut Row) {
        let taken = std::mem::take(other);
        self.extend(taken);
    }
}

pub struct Iter<'a> {
    inner: std::iter::Zip<std::slice::Iter<'a, String>, std::slice::Iter<'a, Cell>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a String, &'a Cell);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl ExactSizeIterator for Iter<'_> {}

pub struct IterMut<'a> {
    inner: std::iter::Zip<std::slice::Iter<'a, String>, std::slice::IterMut<'a, Cell>>,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = (&'a String, &'a mut Cell);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for IterMut<'_> {}

/// Owning iterator: field names are cloned out of the shared shape.
pub struct IntoIter {
    shape: Option<Arc<Shape>>,
    pos: usize,
    cells: smallvec::IntoIter<[Cell; 4]>,
}

impl Iterator for IntoIter {
    type Item = (String, Cell);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let cell = self.cells.next()?;
        let name = self.shape.as_ref()?.names[self.pos].clone();
        self.pos += 1;
        Some((name, cell))
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.cells.size_hint()
    }
}

impl ExactSizeIterator for IntoIter {}

impl IntoIterator for Row {
    type Item = (String, Cell);
    type IntoIter = IntoIter;
    #[inline]
    fn into_iter(self) -> IntoIter {
        IntoIter {
            shape: self.shape,
            pos: 0,
            cells: self.cells.into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a Row {
    type Item = (&'a String, &'a Cell);
    type IntoIter = Iter<'a>;
    #[inline]
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Row {
    type Item = (&'a String, &'a mut Cell);
    type IntoIter = IterMut<'a>;
    #[inline]
    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

impl<K: AsRef<str>> Extend<(K, Cell)> for Row {
    fn extend<I: IntoIterator<Item = (K, Cell)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<K: AsRef<str>> FromIterator<(K, Cell)> for Row {
    fn from_iter<I: IntoIterator<Item = (K, Cell)>>(iter: I) -> Self {
        let mut row = Row::new();
        row.extend(iter);
        row
    }
}

impl<K: AsRef<str>, const N: usize> From<[(K, Cell); N]> for Row {
    fn from(pairs: [(K, Cell); N]) -> Self {
        pairs.into_iter().collect()
    }
}

impl From<std::collections::BTreeMap<String, Cell>> for Row {
    fn from(map: std::collections::BTreeMap<String, Cell>) -> Self {
        let shape = Shape::of_sorted(map.keys().map(String::as_str));
        if shape.names.is_empty() {
            return Row::new();
        }
        Row {
            shape: Some(shape),
            cells: map.into_values().collect(),
        }
    }
}

impl From<Row> for std::collections::BTreeMap<String, Cell> {
    fn from(row: Row) -> Self {
        row.into_iter().collect()
    }
}

impl<Q: AsRef<str> + ?Sized> std::ops::Index<&Q> for Row {
    type Output = Cell;
    fn index(&self, key: &Q) -> &Cell {
        self.get(key).expect("no entry found for key")
    }
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        if self.cells.len() != other.cells.len() {
            return false;
        }
        let same_shape = match (&self.shape, &other.shape) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a.names == b.names,
            _ => false,
        };
        same_shape && self.cells == other.cells
    }
}

impl Eq for Row {}

impl std::hash::Hash for Row {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.len());
        for (k, v) in self {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl fmt::Debug for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl Serialize for Row {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        enum KeyRef<'de> {
            Borrowed(&'de str),
            Buffered,
        }

        struct KeySeed<'s>(&'s mut String);

        impl<'de> serde::de::DeserializeSeed<'de> for KeySeed<'_> {
            type Value = KeyRef<'de>;

            #[inline]
            fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
                struct KeyVisitor<'s>(&'s mut String);

                impl<'de> Visitor<'de> for KeyVisitor<'_> {
                    type Value = KeyRef<'de>;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a field name string")
                    }

                    #[inline]
                    fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
                        Ok(KeyRef::Borrowed(v))
                    }

                    #[inline]
                    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        self.0.clear();
                        self.0.push_str(v);
                        Ok(KeyRef::Buffered)
                    }

                    #[inline]
                    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                        *self.0 = v;
                        Ok(KeyRef::Buffered)
                    }

                    #[inline]
                    fn visit_borrowed_bytes<E: serde::de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
                        std::str::from_utf8(v).map(KeyRef::Borrowed).map_err(E::custom)
                    }

                    #[inline]
                    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                        let s = std::str::from_utf8(v).map_err(E::custom)?;
                        self.0.clear();
                        self.0.push_str(s);
                        Ok(KeyRef::Buffered)
                    }
                }

                deserializer.deserialize_str(KeyVisitor(self.0))
            }
        }

        struct RowVisitor;
        impl<'de> Visitor<'de> for RowVisitor {
            type Value = Row;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of field names to cells")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Row, A::Error> {
                let mut row = Row::new();
                let mut key_buf = String::new();
                while let Some(k_ref) = access.next_key_seed(KeySeed(&mut key_buf))? {
                    let v: Cell = access.next_value()?;
                    let k = match k_ref {
                        KeyRef::Borrowed(s) => s,
                        KeyRef::Buffered => key_buf.as_str(),
                    };
                    row.insert_str(k, v);
                }
                Ok(row)
            }
        }
        deserializer.deserialize_map(RowVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Cell {
        Cell::Text(Arc::from(s))
    }

    #[test]
    fn behaves_like_a_sorted_map() {
        let mut row = Row::new();
        assert!(row.is_empty());
        assert_eq!(row.insert("b", Cell::Int(2)), None);
        assert_eq!(row.insert("a".to_string(), Cell::Int(1)), None);
        assert_eq!(row.insert("c", text("x")), None);
        assert_eq!(row.insert("b", Cell::Int(20)), Some(Cell::Int(2)));
        let keys: Vec<&str> = row.keys().map(String::as_str).collect();
        assert_eq!(keys, ["a", "b", "c"]);
        assert_eq!(row.get("b"), Some(&Cell::Int(20)));
        assert_eq!(row.get(&"c".to_string()), Some(&text("x")));
        assert_eq!(row.get("zz"), None);
        *row.get_mut("a").unwrap() = Cell::Bool(true);
        assert_eq!(row.remove("b"), Some(Cell::Int(20)));
        assert_eq!(row.remove("b"), None);
        assert_eq!(row.len(), 2);
        let owned: Vec<(String, Cell)> = row.clone().into_iter().collect();
        assert_eq!(owned, vec![("a".into(), Cell::Bool(true)), ("c".into(), text("x"))]);
        row.retain(|k, _| k != "a");
        assert_eq!(row.keys().count(), 1);
        row.remove("c");
        assert!(row.is_empty());
        assert_eq!(row, Row::new());
    }

    #[test]
    fn rows_built_the_same_way_share_a_shape() {
        let build = |n: i64| {
            let mut row = Row::new();
            row.insert("id", Cell::Int(n));
            row.insert("title", text("t"));
            row
        };
        let (a, b) = (build(1), build(2));
        assert!(Arc::ptr_eq(a.shape().unwrap(), b.shape().unwrap()));
        // Insert order does not change equality or iteration order.
        let mut c = Row::new();
        c.insert("title", text("t"));
        c.insert("id", Cell::Int(1));
        assert_eq!(a, c);
        assert_ne!(a, b);
        let shape = Row::shape_of(&["title", "id"]);
        let d = Row::from_parts(shape, [Cell::Int(1), text("t")]);
        assert_eq!(a, d);
    }

    #[test]
    fn wide_rows_use_binary_search() {
        let mut row = Row::new();
        for i in (0..40).rev() {
            row.insert(format!("f{i:02}"), Cell::Int(i));
        }
        for i in 0..40 {
            assert_eq!(row.get(&format!("f{i:02}")), Some(&Cell::Int(i)));
        }
        assert_eq!(row.get("f99"), None);
        let keys: Vec<&String> = row.keys().collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn serializes_as_a_map() {
        let row: Row = [("b", Cell::Int(2)), ("a", Cell::Null)].into();
        let map: std::collections::BTreeMap<String, Cell> = row.clone().into();
        assert_eq!(
            serde_json::to_string(&row).unwrap(),
            serde_json::to_string(&map).unwrap()
        );
        let back: Row = serde_json::from_str(&serde_json::to_string(&row).unwrap()).unwrap();
        assert_eq!(back, row);
    }

    #[test]
    fn field_slot_matches_get_across_shapes() {
        // More shapes than the slot keeps, visited in an interleaved order.
        let shapes: Vec<Vec<&str>> = vec![
            vec!["a", "k", "z"],
            vec!["k"],
            vec!["a", "b", "c", "d", "e", "f", "g", "h", "k", "z"],
            vec!["a", "z"],
            vec![],
            vec!["j", "k", "l"],
            vec!["k", "kk"],
        ];
        let rows: Vec<Row> = shapes
            .iter()
            .enumerate()
            .map(|(n, names)| {
                names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| (*name, Cell::Int((n * 100 + i) as i64)))
                    .collect()
            })
            .collect();
        for key in ["k", "a", "z", "missing", "kk"] {
            let mut slot = FieldSlot::new(key);
            for round in 0..5 {
                for step in 0..rows.len() {
                    let row = &rows[(step * 3 + round) % rows.len()];
                    assert_eq!(slot.get(row), row.get(key), "{key} in {row:?}");
                }
            }
        }
    }

    #[test]
    fn from_sorted_and_append_equal_inserts() {
        let by_insert = |pairs: &[(&str, i64)]| -> Row {
            let mut row = Row::new();
            for (k, v) in pairs.iter().rev() {
                row.insert(*k, Cell::Int(*v));
            }
            row
        };
        let pairs = [("a", 1), ("b", 2), ("d", 4), ("z", 26)];
        let sorted = Row::from_sorted(
            pairs.iter().map(|(k, _)| *k),
            pairs.iter().map(|(_, v)| Cell::Int(*v)),
        );
        // Equal as rows, and one shape whatever order the fields went in.
        assert_eq!(sorted, by_insert(&pairs));
        assert!(Arc::ptr_eq(sorted.shape().unwrap(), by_insert(&pairs).shape().unwrap()));
        assert_eq!(Row::from_sorted(std::iter::empty(), std::iter::empty()), Row::new());

        // Appending in name order takes the fast path and lands on the same shape.
        let mut appended = Row::new();
        for (k, v) in pairs {
            assert_eq!(appended.insert(k, Cell::Int(v)), None);
        }
        assert_eq!(appended, sorted);
        assert!(Arc::ptr_eq(appended.shape().unwrap(), sorted.shape().unwrap()));
        // Overwriting the last key and inserting before it still work.
        assert_eq!(appended.insert("z", Cell::Int(0)), Some(Cell::Int(26)));
        assert_eq!(appended.insert("c", Cell::Int(3)), None);
        assert_eq!(
            appended.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "b", "c", "d", "z"]
        );
    }
}
