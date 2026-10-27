    // Own both versions during inverse maintenance; avoid cloning a BTreeMap.
    #[inline(never)]
    pub(crate) fn restore_row_incremental_owned(&mut self, collection: &str, idx: usize, row: Row) {
        let Some(slot) = self.collections.get_mut(collection).and_then(|rows| rows.get_mut(idx)) else { return; };
        let current = std::mem::take(slot);
        // Existing reverse keys let insert_at replace only changed memberships.
        let _ = self.index_insert_row(collection, idx, &row);
        if let Some(fts) = self.fts.get_mut(collection) {
            fts.sync_row(idx, &current, &row);
        }
        let id = row_text(&row, "id");
        let identity_moved = row_text(&current, "id") != id || row_text(&current, "uri") != row_text(&row, "uri");
        let stale_slot = id.is_some_and(|key| self.row_index(collection, key) != Some(idx));
        let short_columns = match collection {
            "docs" => idx >= self.docs_title.len() || idx >= self.docs_layer.len() || idx >= self.docs_wing.len(),
            "orders" => idx >= self.orders_user_id.len() || idx >= self.orders_total.len(),
            "users" => idx >= self.users_email.len(),
            _ => false,
        };
        if identity_moved || stale_slot || short_columns {
            self.collection_mut(collection)[idx] = row;
            if collection == "docs" { self.docs_title_packed.take(); }
            self.rebuild_row_maps_collection(collection);
        } else {
            // Conditions above guarantee this call uses its direct column path,
            // so it cannot inspect the temporarily empty Store slot via rebuild.
            self.row_maps_sync_updated(collection, idx, &current, &row);
            self.collection_mut(collection)[idx] = row;
        }
    }

