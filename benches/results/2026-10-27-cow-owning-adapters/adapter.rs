impl Row {
    pub fn into_keys(self) -> std::collections::btree_map::IntoKeys<String, Cell> {
        BTreeMap::from(self).into_keys()
    }
    pub fn into_values(self) -> std::collections::btree_map::IntoValues<String, Cell> {
        BTreeMap::from(self).into_values()
    }
}
impl From<Row> for BTreeMap<String, Cell> {
    fn from(row: Row) -> Self {
        Arc::unwrap_or_clone(row.0)
    }
}

