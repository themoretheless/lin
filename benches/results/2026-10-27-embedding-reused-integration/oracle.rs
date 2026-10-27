#[cfg(test)]
mod reused_row_batch_oracle {
    use super::*;
    use crate::store::{Cell, Row};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn hashing_row_batch_matches_all_vector_bits_and_keeps_existing() {
        let parts = [
            None,
            Some(""),
            Some("x"),
            Some("ЁЖ ΟΣ İ\u{b}note\u{a0}body"),
            Some("longer payload"),
        ];
        let fields = ["title", "body", "snippet"];
        let mut original = Vec::new();
        for a in parts {
            for b in parts {
                for c in parts {
                    original.push(
                        fields
                            .into_iter()
                            .zip([a, b, c])
                            .filter_map(|(k, v)| v.map(|s| (k.to_string(), Cell::text_arc(s))))
                            .collect::<Row>(),
                    );
                }
            }
        }
        original.push(Row::from([(
            "body".into(),
            Cell::text_arc("wal note ".repeat(4096)),
        )]));
        original.push(Row::from([
            ("title".into(), Cell::text_arc("keep existing")),
            ("embedding".into(), Cell::Vec(Arc::from([0.25f32; 8]))),
        ]));
        for dim in [8, 9, 128, 768, 1536] {
            let model = HashingEmbedder::new("test", dim);
            let mut expected = original.clone();
            for row in &mut expected {
                if row.get("embedding").and_then(Cell::as_vec).is_some() {
                    continue;
                }
                let text = row_embed_text(row);
                if !text.is_empty() {
                    let vector = model.embed(&text);
                    row.insert("embedding".into(), Cell::Vec(vector));
                }
            }
            let mut actual = original.clone();
            model.embed_rows(&mut actual);
            assert_eq!(actual.len(), expected.len());
            for (a, b) in actual.iter().zip(&expected) {
                assert_eq!(a, b);
                if let (Some(a), Some(b)) = (
                    a.get("embedding").and_then(Cell::as_vec),
                    b.get("embedding").and_then(Cell::as_vec),
                ) {
                    assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
                }
            }
        }
    }
    struct Backend {
        bad: bool,
        batch: AtomicUsize,
        single: AtomicUsize,
    }
    impl Embedder for Backend {
        fn id(&self) -> &str {
            "test"
        }
        fn dim(&self) -> usize {
            8
        }
        fn embed(&self, text: &str) -> Arc<[f32]> {
            assert_eq!(text, "title body ");
            self.single.fetch_add(1, Ordering::Relaxed);
            Arc::from([0.5; 8])
        }
        fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
            assert_eq!(texts, ["title body "]);
            self.batch.fetch_add(1, Ordering::Relaxed);
            if self.bad {
                Vec::new()
            } else {
                vec![Arc::from([0.25; 8])]
            }
        }
    }
    #[test]
    fn custom_batch_override_and_wrong_length_fallback_preserved() {
        for bad in [false, true] {
            let backend = Backend {
                bad,
                batch: AtomicUsize::new(0),
                single: AtomicUsize::new(0),
            };
            let keep: Arc<[f32]> = Arc::from([0.75; 8]);
            let mut rows = vec![
                Row::new(),
                Row::from([
                    ("title".into(), Cell::text_arc("ignored")),
                    ("embedding".into(), Cell::Vec(keep.clone())),
                ]),
                Row::from([
                    ("title".into(), Cell::text_arc("title")),
                    ("body".into(), Cell::text_arc("body")),
                    ("snippet".into(), Cell::text_arc("")),
                ]),
            ];
            backend.embed_rows(&mut rows);
            assert!(rows[0].get("embedding").is_none());
            assert_eq!(rows[1]["embedding"], Cell::Vec(keep));
            assert_eq!(
                rows[2]["embedding"].as_vec().unwrap(),
                &[if bad { 0.5 } else { 0.25 }; 8]
            );
            assert_eq!(backend.batch.load(Ordering::Relaxed), 1);
            assert_eq!(backend.single.load(Ordering::Relaxed), usize::from(bad));
        }
    }
}

#[cfg(test)]
mod reused_batch_empty_resource_oracle {
    use super::*;
    use crate::store::{Cell, Row};
    #[test]
    fn no_eligible_rows_does_not_resize_scratch() {
        let before = EMBED_SCRATCH.with(|cell| cell.borrow().0.v.len());
        let model = HashingEmbedder::new("test", before + 17);
        let mut rows = vec![
            Row::new(),
            Row::from([("title".into(), Cell::text_arc(""))]),
            Row::from([
                ("body".into(), Cell::text_arc("already embedded")),
                ("embedding".into(), Cell::Vec(Arc::from([0.25; 8]))),
            ]),
        ];
        let original = rows.clone();
        model.embed_rows(&mut rows);
        assert_eq!(rows, original);
        EMBED_SCRATCH.with(|cell| assert_eq!(cell.borrow().0.v.len(), before));
    }
}
