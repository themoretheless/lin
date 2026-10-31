//! Full-only alternative f32 wire encoding. Public Cell serialization stays canonical.
use super::{LogRecord, Pack};
use crate::store::{Cell, Row};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq, SerializeStruct},
};
use std::cell::Cell as Counter;

pub(super) fn encode(
    buf: &mut Vec<u8>,
    record: &LogRecord,
) -> Result<usize, rmp_serde::encode::Error> {
    let extra = Counter::new(0usize);
    rmp_serde::encode::write_named(buf, &RecordWire(record, &extra))?;
    Ok(extra.get())
}
struct RecordWire<'a>(&'a LogRecord, &'a Counter<usize>);
impl Serialize for RecordWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("LogRecord", 3)?;
        out.serialize_field("gen", &self.0.r#gen)?;
        out.serialize_field("next_id", &self.0.next_id)?;
        out.serialize_field("pack", &PackWire(&self.0.pack, self.1, 0))?;
        out.end()
    }
}
struct PackWire<'a>(&'a Pack, &'a Counter<usize>, usize);
impl Serialize for PackWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.2 >= 16 {
            return self.0.serialize(serializer);
        }
        match self.0 {
            Pack::Update { collection, rows } | Pack::Delete { collection, rows } => {
                let kind = if matches!(self.0, Pack::Update { .. }) {
                    "update"
                } else {
                    "delete"
                };
                let mut out = serializer.serialize_struct("Pack", 3)?;
                out.serialize_field("type", kind)?;
                out.serialize_field("collection", collection)?;
                out.serialize_field("rows", &RowsWire(rows, self.1))?;
                out.end()
            }
            Pack::Batch { packs } => {
                let mut out = serializer.serialize_struct("Pack", 2)?;
                out.serialize_field("type", "batch")?;
                out.serialize_field("packs", &PacksWire(packs, self.1, self.2 + 1))?;
                out.end()
            }
            _ => self.0.serialize(serializer),
        }
    }
}
struct PacksWire<'a>(&'a [Pack], &'a Counter<usize>, usize);
impl Serialize for PacksWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_seq(Some(self.0.len()))?;
        for pack in self.0 {
            out.serialize_element(&PackWire(pack, self.1, self.2))?;
        }
        out.end()
    }
}
struct RowsWire<'a>(&'a [Row], &'a Counter<usize>);
impl Serialize for RowsWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_seq(Some(self.0.len()))?;
        for row in self.0 {
            out.serialize_element(&RowWire(row, self.1))?;
        }
        out.end()
    }
}
struct RowWire<'a>(&'a Row, &'a Counter<usize>);
impl Serialize for RowWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            out.serialize_entry(key, &CellWire(value, self.1))?;
        }
        out.end()
    }
}
struct CellWire<'a>(&'a Cell, &'a Counter<usize>);
#[derive(Serialize)]
#[serde(tag = "t", content = "v")]
enum VectorCell<'a> {
    Vec(VectorWire<'a>),
}
impl Serialize for CellWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Cell::Vec(values) => VectorCell::Vec(VectorWire(values, self.1)).serialize(serializer),
            other => other.serialize(serializer),
        }
    }
}
struct VectorWire<'a>(&'a [f32], &'a Counter<usize>);
impl Serialize for VectorWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_seq(Some(self.0.len()))?;
        for &value in self.0 {
            out.serialize_element(&FloatWire(value, self.1))?;
        }
        out.end()
    }
}
struct FloatWire<'a>(f32, &'a Counter<usize>);
impl Serialize for FloatWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0 == 0.0 {
            // Canonical MessagePack f32 uses five bytes; positive fixint zero uses one.
            // Saturation makes the caller reject an impossible overflowing canonical size.
            self.1.set(self.1.get().saturating_add(4));
            serializer.serialize_u8(0)
        } else {
            serializer.serialize_f32(self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MAX_RECORD, SyncMode, append_record, for_each_wal_frame};
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct TempLog {
        file: File,
        path: PathBuf,
    }
    impl TempLog {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lin-positive-zero-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            Self { file, path }
        }
    }
    impl Drop for TempLog {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    fn row(values: Vec<f32>) -> Row {
        Row::from([
            ("id".into(), Cell::text_arc("d")),
            ("title".into(), Cell::text_arc("ЁЖ 🦔 \\\" Unicode")),
            ("embedding".into(), Cell::Vec(Arc::from(values))),
            ("negative_zero".into(), Cell::Float(-0.0)),
            ("null".into(), Cell::Null),
            ("flag".into(), Cell::Bool(true)),
            ("integer".into(), Cell::Int(-42)),
            ("time".into(), Cell::Time(1234567)),
        ])
    }
    fn record(pack: Pack) -> LogRecord {
        LogRecord {
            r#gen: 41,
            next_id: 99,
            pack,
        }
    }
    fn payload(record: &LogRecord) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        let extra = encode(&mut out, record).unwrap();
        (out, extra)
    }
    #[test]
    fn positive_zero_record_shapes_preserve_canonical_bits_and_size() {
        let patterns = [
            0u32, 0x80000000, 1, 0x80000001, 0x00800000, 0x80800000, 0x3f800000, 0xbf800000,
            0x7f800000, 0xff800000, 0x7fc01234, 0x7fa00001, 0xffc05678,
        ];
        for n in [0usize, 1, 15, 16, 31, 768, 1024] {
            for values in [
                vec![0.0; n],
                vec![-0.0; n],
                (0..n)
                    .map(|i| f32::from_bits(patterns[i % patterns.len()]))
                    .collect(),
            ] {
                let rows = vec![row(values)];
                let update = Pack::Update {
                    collection: "docs".into(),
                    rows: rows.clone(),
                };
                let delete = Pack::Delete {
                    collection: "docs".into(),
                    rows: rows.clone(),
                };
                let insert = Pack::InsertBulk {
                    collection: "docs".into(),
                    rows,
                    edges: vec![],
                };
                let mut shapes = vec![
                    update.clone(),
                    delete.clone(),
                    insert.clone(),
                    Pack::Batch {
                        packs: vec![update.clone(), delete, insert],
                    },
                ];
                for depth in [15, 16, 17, 32] {
                    let mut pack = update.clone();
                    for _ in 0..depth {
                        pack = Pack::Batch { packs: vec![pack] };
                    }
                    shapes.push(pack);
                }
                for pack in shapes {
                    let original = record(pack);
                    let canonical = rmp_serde::to_vec_named(&original).unwrap();
                    let (encoded, extra) = payload(&original);
                    assert_eq!(encoded.len() + extra, canonical.len());
                    let decoded: LogRecord = rmp_serde::from_slice(&encoded).unwrap();
                    // Re-encoding with the untouched serializer verifies gen/next_id,
                    // every Cell tag/value and IEEE bits, including NaN payloads.
                    assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), canonical);
                    if extra == 0 {
                        assert_eq!(encoded, canonical);
                    }
                }
            }
        }
    }
    fn framed(original: &LogRecord, sync: SyncMode) -> Vec<u8> {
        let mut log = TempLog::new();
        let mut bytes = 0;
        append_record(&mut log.file, original, &mut Vec::new(), sync, &mut bytes).unwrap();
        let frame = std::fs::read(&log.path).unwrap();
        assert_eq!(bytes, frame.len() as u64);
        frame
    }
    #[test]
    fn positive_zero_full_frames_replay_and_normal_bytes_stay_canonical() {
        let original = record(Pack::Batch {
            packs: vec![Pack::Update {
                collection: "docs".into(),
                rows: vec![row(vec![0.0, -0.0, 1.0, f32::from_bits(0x7fc01234)])],
            }],
        });
        let canonical = rmp_serde::to_vec_named(&original).unwrap();
        let normal = framed(&original, SyncMode::Normal);
        let full = framed(&original, SyncMode::Full);
        let mut expected = Vec::new();
        expected.extend_from_slice(&super::super::LOG_MAGIC_CHECKSUM);
        expected.extend_from_slice(&((canonical.len() + 5) as u32).to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&canonical);
        let mut crc = crate::crc::Hasher::new();
        crc.update(&[1]);
        crc.update(&canonical);
        expected.extend_from_slice(&crc.finalize().to_le_bytes());
        assert_eq!(normal, expected);
        assert_eq!(full.len() + 4, normal.len());
        let mut count = 0;
        for_each_wal_frame(&full, |decoded| {
            assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), canonical);
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 1);
        let mut corrupt = full;
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(for_each_wal_frame(&corrupt, |_| Ok(())).is_err());
        let no_positive_zero = record(Pack::Delete {
            collection: "docs".into(),
            rows: vec![row(vec![-0.0, 1.0])],
        });
        assert_eq!(
            framed(&no_positive_zero, SyncMode::Full),
            framed(&no_positive_zero, SyncMode::Normal)
        );
    }
    fn reject_without_write(original: &LogRecord) {
        let mut log = TempLog::new();
        log.file.write_all(b"sentinel").unwrap();
        let mut bytes = 8;
        let error = append_record(
            &mut log.file,
            original,
            &mut Vec::new(),
            SyncMode::Full,
            &mut bytes,
        )
        .unwrap_err();
        assert!(error.to_string().contains("16MiB"));
        assert_eq!(bytes, 8);
        assert_eq!(std::fs::read(&log.path).unwrap(), b"sentinel");
    }
    #[test]
    fn positive_zero_canonical_payload_and_envelope_limits_reject_before_write() {
        let oversized = record(Pack::Update {
            collection: "docs".into(),
            rows: vec![row(vec![0.0; MAX_RECORD as usize / 5 + 1])],
        });
        let canonical = rmp_serde::to_vec_named(&oversized).unwrap();
        let (encoded, _) = payload(&oversized);
        assert!(canonical.len() > MAX_RECORD as usize && encoded.len() < MAX_RECORD as usize);
        reject_without_write(&oversized);
        let mut boundary_row = row(vec![0.0]);
        let mut padding = MAX_RECORD as usize - 256;
        for _ in 0..3 {
            boundary_row.insert("padding".into(), Cell::text_arc("x".repeat(padding)));
            let rec = record(Pack::Delete {
                collection: "docs".into(),
                rows: vec![boundary_row.clone()],
            });
            let len = rmp_serde::to_vec_named(&rec).unwrap().len();
            let target = MAX_RECORD as usize - 2;
            if len == target {
                let (encoded, extra) = payload(&rec);
                assert_eq!(extra, 4);
                assert!(encoded.len() + 5 <= MAX_RECORD as usize);
                reject_without_write(&rec);
                return;
            }
            padding = (padding as isize + target as isize - len as isize) as usize;
        }
        panic!("failed to construct canonical envelope boundary");
    }
}
