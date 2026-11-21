//! Row storage per collection with a write epoch.
//!
//! Every mutable access takes a fresh, process-wide unique epoch, so a cache built
//! from the rows (a declared column mirror) is valid exactly while the epoch it
//! recorded is still current. Shared access goes through `Deref`.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::Row;

static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_epoch() -> u64 {
    NEXT_EPOCH.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub struct RowMap {
    map: Arc<BTreeMap<String, Vec<Row>>>,
    epoch: u64,
    rewrite_epoch: u64,
}

impl RowMap {
    pub fn new() -> Self {
        Self::from(BTreeMap::new())
    }

    /// Changes whenever any collection may have been mutated (including appends).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Changes whenever any collection may have had non-append mutations (updates, deletes,
    /// truncates, or rollbacks). Pure tail appends via [`Self::append_only_mut`] advance
    /// [`Self::epoch`] while keeping `rewrite_epoch` stable.
    pub fn rewrite_epoch(&self) -> u64 {
        self.rewrite_epoch
    }

    /// O(1) copy-on-write snapshot of the underlying `BTreeMap` `Arc`.
    #[inline]
    pub(crate) fn share_arc(&self) -> Arc<BTreeMap<String, Vec<Row>>> {
        Arc::clone(&self.map)
    }

    /// Restore from a shared `Arc` snapshot with a fresh write epoch.
    #[inline]
    pub(crate) fn from_arc(map: Arc<BTreeMap<String, Vec<Row>>>) -> Self {
        let ep = next_epoch();
        Self {
            map,
            epoch: ep,
            rewrite_epoch: ep,
        }
    }

    fn touch(&mut self) {
        let next = next_epoch();
        self.epoch = next;
        self.rewrite_epoch = next;
    }

    fn touch_append_only(&mut self) {
        self.epoch = next_epoch();
    }

    /// Mutable access restricted by caller contract to reserving or appending at the end
    /// (`push` / `extend` / `reserve`), keeping existing prefix `[0..old_len]` intact.
    pub(crate) fn append_only_mut(&mut self, key: &str) -> &mut Vec<Row> {
        self.touch_append_only();
        let map = Arc::make_mut(&mut self.map);
        if map.contains_key(key) {
            map.get_mut(key).expect("existing collection")
        } else {
            map.entry(key.to_string()).or_default()
        }
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut Vec<Row>>
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.touch();
        Arc::make_mut(&mut self.map).get_mut(key)
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, Vec<Row>> {
        self.touch();
        Arc::make_mut(&mut self.map).entry(key)
    }

    pub fn insert(&mut self, key: String, rows: Vec<Row>) -> Option<Vec<Row>> {
        self.touch();
        Arc::make_mut(&mut self.map).insert(key, rows)
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<Vec<Row>>
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.touch();
        Arc::make_mut(&mut self.map).remove(key)
    }

    pub fn iter_mut(&mut self) -> std::collections::btree_map::IterMut<'_, String, Vec<Row>> {
        self.touch();
        Arc::make_mut(&mut self.map).iter_mut()
    }

    pub fn values_mut(&mut self) -> std::collections::btree_map::ValuesMut<'_, String, Vec<Row>> {
        self.touch();
        Arc::make_mut(&mut self.map).values_mut()
    }

    /// Unrestricted mutable access; counts as a write.
    pub fn map_mut(&mut self) -> &mut BTreeMap<String, Vec<Row>> {
        self.touch();
        Arc::make_mut(&mut self.map)
    }

    pub fn into_inner(self) -> BTreeMap<String, Vec<Row>> {
        Arc::unwrap_or_clone(self.map)
    }
}

impl Default for RowMap {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RowMap {
    fn clone(&self) -> Self {
        Self {
            map: Arc::clone(&self.map),
            epoch: self.epoch,
            rewrite_epoch: self.rewrite_epoch,
        }
    }
}

impl Deref for RowMap {
    type Target = BTreeMap<String, Vec<Row>>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl From<BTreeMap<String, Vec<Row>>> for RowMap {
    fn from(map: BTreeMap<String, Vec<Row>>) -> Self {
        let ep = next_epoch();
        Self {
            map: Arc::new(map),
            epoch: ep,
            rewrite_epoch: ep,
        }
    }
}

impl<'a> IntoIterator for &'a RowMap {
    type Item = (&'a String, &'a Vec<Row>);
    type IntoIter = std::collections::btree_map::Iter<'a, String, Vec<Row>>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}
