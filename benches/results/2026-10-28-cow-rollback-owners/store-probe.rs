#[cfg(test)]
impl Store {
    pub(crate) fn probe_row_owner_count(&self, id: &str) -> usize {
        match &self.get_by_id("docs", id).unwrap().0 {
            RowStorage::Inline(_) => 0,
            RowStorage::Shared(map) => Arc::strong_count(map),
        }
    }
}
