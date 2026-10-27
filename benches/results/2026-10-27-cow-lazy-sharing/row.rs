#[derive(Clone)]
enum RowStorage {
    Inline(BTreeMap<String, Cell>),
    Shared(Arc<BTreeMap<String, Cell>>),
}
#[derive(Clone)]
pub struct Row(RowStorage);
impl Row {
    pub fn new() -> Self { Self(RowStorage::Inline(BTreeMap::new())) }
    fn as_map(&self) -> &BTreeMap<String, Cell> {
        match &self.0 { RowStorage::Inline(map) => map, RowStorage::Shared(map) => map }
    }
    fn into_map(self) -> BTreeMap<String, Cell> {
        match self.0 { RowStorage::Inline(map) => map, RowStorage::Shared(map) => Arc::unwrap_or_clone(map) }
    }
    pub(crate) fn ensure_shared(&mut self) {
        if matches!(&self.0, RowStorage::Shared(_)) { return; }
        let RowStorage::Inline(map) = std::mem::replace(&mut self.0, RowStorage::Inline(BTreeMap::new())) else { unreachable!() };
        self.0 = RowStorage::Shared(Arc::new(map));
    }
    pub fn into_keys(self) -> std::collections::btree_map::IntoKeys<String, Cell> { self.into_map().into_keys() }
    pub fn into_values(self) -> std::collections::btree_map::IntoValues<String, Cell> { self.into_map().into_values() }
}
impl Default for Row { fn default() -> Self { Self::new() } }
impl std::fmt::Debug for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_tuple("Row").field(self.as_map()).finish() }
}
impl PartialEq for Row { fn eq(&self, other: &Self) -> bool { self.as_map() == other.as_map() } }
impl Eq for Row {}
impl std::hash::Hash for Row {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) { std::hash::Hash::hash(self.as_map(), state); }
}
impl Serialize for Row {
    fn serialize<S:serde::Serializer>(&self, serializer:S) -> Result<S::Ok,S::Error> { self.as_map().serialize(serializer) }
}
impl<'de> Deserialize<'de> for Row {
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D) -> Result<Self,D::Error> { BTreeMap::<String,Cell>::deserialize(deserializer).map(Self::from) }
}
impl std::ops::Deref for Row {
    type Target=BTreeMap<String,Cell>;
    fn deref(&self) -> &Self::Target { self.as_map() }
}
impl std::ops::DerefMut for Row {
    fn deref_mut(&mut self) -> &mut Self::Target { match &mut self.0 { RowStorage::Inline(map) => map, RowStorage::Shared(map) => Arc::make_mut(map) } }
}
impl FromIterator<(String,Cell)> for Row {
    fn from_iter<T:IntoIterator<Item=(String,Cell)>>(iter:T) -> Self { Self::from(iter.into_iter().collect::<BTreeMap<_,_>>()) }
}
impl<const N:usize> From<[(String,Cell);N]> for Row {
    fn from(value:[(String,Cell);N]) -> Self { value.into_iter().collect() }
}
impl From<BTreeMap<String,Cell>> for Row { fn from(map:BTreeMap<String,Cell>) -> Self { Self(RowStorage::Inline(map)) } }
impl From<Row> for BTreeMap<String,Cell> { fn from(row:Row) -> Self { row.into_map() } }
impl Extend<(String,Cell)> for Row {
    fn extend<T:IntoIterator<Item=(String,Cell)>>(&mut self, iter:T) { std::ops::DerefMut::deref_mut(self).extend(iter); }
}
impl IntoIterator for Row {
    type Item=(String,Cell);
    type IntoIter=std::collections::btree_map::IntoIter<String,Cell>;
    fn into_iter(self) -> Self::IntoIter { self.into_map().into_iter() }
}
impl<'a> IntoIterator for &'a Row {
    type Item=(&'a String,&'a Cell);
    type IntoIter=std::collections::btree_map::Iter<'a,String,Cell>;
    fn into_iter(self) -> Self::IntoIter { self.as_map().iter() }
}
impl<'a> IntoIterator for &'a mut Row {
    type Item=(&'a String,&'a mut Cell);
    type IntoIter=std::collections::btree_map::IterMut<'a,String,Cell>;
    fn into_iter(self) -> Self::IntoIter { std::ops::DerefMut::deref_mut(self).iter_mut() }
}
