#[cfg(test)]
mod direct_batch_mixed_budget {
    use super::*;
    fn update() -> Pack {
        Pack::Update {
            collection: "docs".into(),
            rows: vec![Row::from([
                ("embedding".into(), crate::Cell::Vec(vec![0.0; 768].into())),
                ("signed".into(), crate::Cell::Float(-0.0)),
            ])],
        }
    }
    fn fallback() -> Pack {
        Pack::AppendEdge {
            rel: "r".into(),
            from: "a".into(),
            to: String::new(),
        }
    }
    #[test]
    fn mixed_siblings_share_exact_canonical_budget() {
        for reverse in [false, true] {
            let mut packs = vec![update(), fallback()];
            if reverse {
                packs.reverse();
            }
            let fallback_index = if reverse { 0 } else { 1 };
            let mut rec = LogRecord {
                r#gen: 42,
                next_id: 64,
                pack: Pack::Batch { packs },
            };
            let size = rmp_serde::to_vec_named(&rec).unwrap().len();
            // A long str header takes four additional bytes relative to fixstr.
            let padding = MAX_RECORD as usize - 5 - size - 4;
            if let Pack::Batch { packs } = &mut rec.pack {
                if let Pack::AppendEdge { to, .. } = &mut packs[fallback_index] {
                    *to = "x".repeat(padding);
                }
            }
            let canonical = rmp_serde::to_vec_named(&rec).unwrap();
            assert_eq!(canonical.len() + 5, MAX_RECORD as usize);
            let mut compact = Vec::new();
            wal_direct_zero::encode(&mut compact, &rec).unwrap();
            assert!(compact.len() + 5 < MAX_RECORD as usize);
            let decoded: LogRecord = rmp_serde::from_slice(&compact).unwrap();
            assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), canonical);
            if let Pack::Batch { packs } = &mut rec.pack {
                if let Pack::AppendEdge { to, .. } = &mut packs[fallback_index] {
                    to.push('x');
                }
            }
            assert_eq!(
                rmp_serde::to_vec_named(&rec).unwrap().len() + 5,
                MAX_RECORD as usize + 1
            );
            compact.clear();
            assert!(wal_direct_zero::encode(&mut compact, &rec).is_err());
            assert!(compact.len() + 5 < MAX_RECORD as usize);
        }
    }
    #[test]
    fn mixed_batch_array_headers_and_fallback_depth_preserve_values() {
        for count in [0, 1, 15, 16, 255, 256] {
            for depth in [0, 17] {
                let mut pack = Pack::Batch {
                    packs: (0..count)
                        .map(|i| if i % 2 == 0 { update() } else { fallback() })
                        .collect(),
                };
                for _ in 0..depth {
                    pack = Pack::Batch { packs: vec![pack] };
                }
                let rec = LogRecord {
                    r#gen: u64::MAX,
                    next_id: 0,
                    pack,
                };
                let canonical = rmp_serde::to_vec_named(&rec).unwrap();
                let mut compact = Vec::new();
                wal_direct_zero::encode(&mut compact, &rec).unwrap();
                let decoded: LogRecord = rmp_serde::from_slice(&compact).unwrap();
                assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), canonical);
            }
        }
    }
}
