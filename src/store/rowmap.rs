//! Row storage per collection with a write epoch.
//!
//! Every mutable access takes a fresh, process-wide unique epoch, so a cache built
//! from the rows (a declared column mirror) is valid exactly while the epoch it
//! recorded is still current. Shared access goes through `Deref`.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};

use super::Row;

static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_epoch() -> u64 {
    NEXT_EPOCH.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub struct RowMap {
    map: BTreeMap<String, Vec<Row>>,
    epoch: u64,
}

impl RowMap {
    pub fn new() -> Self {
        Self::from(BTreeMap::new())
    }

    /// Changes whenever any collection may have been mutated.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Build a copy backed by `map` while preserving `self.epoch` (used by `Store::clone_mem`
    /// so read snapshots share mirrors built against the same immutable row version).
    pub(crate) fn clone_with_map(&self, map: BTreeMap<String, Vec<Row>>) -> Self {
        Self {
            map,
            epoch: self.epoch,
        }
    }

    fn touch(&mut self) {
        self.epoch = next_epoch();
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut Vec<Row>>
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.touch();
        self.map.get_mut(key)
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, Vec<Row>> {
        self.touch();
        self.map.entry(key)
    }

    pub fn insert(&mut self, key: String, rows: Vec<Row>) -> Option<Vec<Row>> {
        self.touch();
        self.map.insert(key, rows)
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<Vec<Row>>
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.touch();
        self.map.remove(key)
    }

    pub fn iter_mut(&mut self) -> std::collections::btree_map::IterMut<'_, String, Vec<Row>> {
        self.touch();
        self.map.iter_mut()
    }

    pub fn values_mut(&mut self) -> std::collections::btree_map::ValuesMut<'_, String, Vec<Row>> {
        self.touch();
        self.map.values_mut()
    }

    /// Unrestricted mutable access; counts as a write.
    pub fn map_mut(&mut self) -> &mut BTreeMap<String, Vec<Row>> {
        self.touch();
        &mut self.map
    }

    pub fn into_inner(self) -> BTreeMap<String, Vec<Row>> {
        self.map
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
            map: self.map.clone(),
            epoch: self.epoch,
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
        Self {
            map,
            epoch: next_epoch(),
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
