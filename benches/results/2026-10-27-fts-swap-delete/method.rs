    /// Delete a row while the last row moves into its slot. Common terms retain
    /// their destination membership and only lose the old tail position.
    pub(crate) fn swap_remove_row(&mut self, to: usize, from: usize, removed: &Row, moved: &Row) {
        debug_assert_ne!(to, from);
        let old = row_tokens(removed, &self.fields);
        let mut retained = old.capacity().saturating_mul(std::mem::size_of::<std::borrow::Cow<'_, str>>());
        if retained <= 64 * 1024 {
            for token in &old {
                if let std::borrow::Cow::Owned(text) = token {
                    retained = retained.saturating_add(text.capacity());
                    if retained > 64 * 1024 { break; }
                }
            }
        }
        // Bound extra peak token memory versus the sequential implementation.
        if retained > 64 * 1024 {
            for token in &old { self.note_del(token, to); }
            drop(old);
            self.move_row(from, to, moved);
            return;
        }
        let new = row_tokens(moved, &self.fields);
        let (mut i, mut j) = (0, 0);
        while i < old.len() && j < new.len() {
            match old[i].cmp(&new[j]) {
                std::cmp::Ordering::Less => { self.note_del(&old[i], to); i += 1; }
                std::cmp::Ordering::Greater => {
                    self.note_del(&new[j], from); self.note_add(&new[j], to); j += 1;
                }
                std::cmp::Ordering::Equal => { self.note_del(&new[j], from); i += 1; j += 1; }
            }
        }
        for token in &old[i..] { self.note_del(token, to); }
        for token in &new[j..] { self.note_del(token, from); self.note_add(token, to); }
    }

