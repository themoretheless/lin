// Persistence-only views. Public Cell/Row serde and complete logical Packs stay unchanged.
mod wal_positive_zero {
    use super::{LogRecord, Pack};
    use crate::store::{Cell, Row};
    use serde::{Serialize, Serializer};
    use serde::ser::{SerializeMap, SerializeSeq, SerializeStruct};
    use std::cell::Cell as Counter;

    #[derive(Serialize)]
    struct Record<'a> {
        r#gen: u64,
        next_id: u64,
        pack: PackView<'a>,
    }
    struct PackView<'a>(&'a Pack, &'a Counter<usize>);
    struct PacksView<'a>(&'a [Pack], &'a Counter<usize>);
    struct RowsView<'a>(&'a [Row], &'a Counter<usize>);
    struct RowView<'a>(&'a Row, &'a Counter<usize>);
    struct CellView<'a>(&'a Cell, &'a Counter<usize>);
    struct VectorView<'a>(&'a [f32], &'a Counter<usize>);

    impl Serialize for PackView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self.0 {
                Pack::Update { collection, rows } | Pack::Delete { collection, rows } => {
                    let mut map = serializer.serialize_struct("Pack", 3)?;
                    map.serialize_field("type", if matches!(self.0, Pack::Update { .. }) { "update" } else { "delete" })?;
                    map.serialize_field("collection", collection)?;
                    map.serialize_field("rows", &RowsView(rows, self.1))?;
                    map.end()
                }
                Pack::Batch { packs } => {
                    let mut map = serializer.serialize_struct("Pack", 2)?;
                    map.serialize_field("type", "batch")?;
                    map.serialize_field("packs", &PacksView(packs, self.1))?;
                    map.end()
                }
                other => other.serialize(serializer),
            }
        }
    }
    impl Serialize for PacksView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            for pack in self.0 { seq.serialize_element(&PackView(pack, self.1))?; }
            seq.end()
        }
    }
    impl Serialize for RowsView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            for row in self.0 { seq.serialize_element(&RowView(row, self.1))?; }
            seq.end()
        }
    }
    impl Serialize for RowView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(self.0.len()))?;
            for (key, cell) in self.0 { map.serialize_entry(key, &CellView(cell, self.1))?; }
            map.end()
        }
    }
    impl Serialize for CellView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self.0 {
                Cell::Vec(vector) => {
                    let mut map = serializer.serialize_struct("Cell", 2)?;
                    map.serialize_field("t", "Vec")?;
                    map.serialize_field("v", &VectorView(vector, self.1))?;
                    map.end()
                }
                other => other.serialize(serializer),
            }
        }
    }
    impl Serialize for VectorView<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            let mut savings = 0usize;
            for value in self.0 {
                if value.to_bits() == 0 {
                    seq.serialize_element(&0u8)?;
                    savings = savings.saturating_add(4);
                } else {
                    seq.serialize_element(value)?;
                }
            }
            self.1.set(self.1.get().saturating_add(savings));
            seq.end()
        }
    }
    // Bounded hint only: missed zeros use canonical encoding with identical semantics.
    fn zero_hint(pack: &Pack, depth: u8) -> bool {
        match pack {
            Pack::Update { rows, .. } | Pack::Delete { rows, .. } => rows.first().is_some_and(|row| {
                row.values().take(4).find_map(|cell| {
                    if let Cell::Vec(values) = cell { Some(values) } else { None }
                }).is_some_and(|values| values.iter().take(8).any(|value| value.to_bits() == 0))
            }),
            Pack::Batch { packs } if depth < 2 => packs.first().is_some_and(|pack| zero_hint(pack, depth + 1)),
            _ => false,
        }
    }
    pub(super) fn encode(buf: &mut Vec<u8>, rec: &LogRecord) -> Result<(), crate::Error> {
        if !zero_hint(&rec.pack, 0) {
            rmp_serde::encode::write_named(&mut *buf, rec).map_err(super::io_err)?;
            if buf.len().checked_add(5).is_none_or(|n| n > super::MAX_RECORD as usize) {
                return Err(super::io_err("log record exceeds 16MiB"));
            }
            return Ok(());
        }
        encode_compact(buf, rec)
    }
    #[inline(never)]
    fn encode_compact(buf: &mut Vec<u8>, rec: &LogRecord) -> Result<(), crate::Error> {
        let savings = Counter::new(0);
        let record = Record { r#gen: rec.r#gen, next_id: rec.next_id, pack: PackView(&rec.pack, &savings) };
        rmp_serde::encode::write_named(&mut *buf, &record).map_err(super::io_err)?;
        // Preserve the pre-existing canonical MessagePack frame-size rejection boundary.
        let original_frame = buf.len().checked_add(savings.get()).and_then(|n| n.checked_add(5));
        if original_frame.is_none_or(|n| n > super::MAX_RECORD as usize) {
            return Err(super::io_err("log record exceeds 16MiB"));
        }
        Ok(())
    }
}
