use lin::{Cell, Db, Row};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    for width in [1usize, 2, 3, 8] {
        let fields = ["wing", "title", "layer", "ts", "uri", "id", "hash", "body"];
        let mut db = Db::empty();
        db.run(&format!("index docs [{}]", fields[..width].join(","))).unwrap();
        let rows = (0..1000).map(|i| format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"title {i}",layer:"wiki",wing:"rag",body:"body {i}",ts:timestamp({i})}}"#)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert docs [{rows}]")).unwrap();
        let original: Vec<Row> = db.store.collection("docs").to_vec();
        let mut a = original[0].clone();
        let mut b = original[0].clone();
        a.insert("wing".into(), Cell::from_text("alternating long index text key alpha"));
        b.insert("wing".into(), Cell::from_text("alternating long index text key bravo"));
        let index = db.store.indexes.values_mut().next().unwrap();
        let baseline_index = index.clone();
        for changed in [false, true] {
            *index = baseline_index.clone();
            index.insert_at(0, &a).unwrap();
            for sample in 0..8 {
                let timer = Instant::now();
                for _ in 0..10000 {
                    index.insert_at(black_box(0), black_box(&a)).unwrap();
                    index.insert_at(black_box(0), black_box(if changed { &b } else { &a })).unwrap();
                }
                let elapsed = timer.elapsed().as_nanos() as f64 / 20000.0;
                let expected = if changed { &b } else { &a };
                assert_eq!(index.reverse.len(), 1000);
                for (position, row) in original.iter().enumerate() {
                    let key = index.key_of(if position == 0 { expected } else { row });
                    assert_eq!(index.reverse.get(&position), Some(&key));
                }
                let mut seen = std::collections::BTreeSet::new();
                for (key, posting) in &index.forward {
                    let positions = posting.to_indices();
                    assert!(!positions.is_empty());
                    for position in positions {
                        assert!(seen.insert(position));
                        assert_eq!(index.reverse.get(&position), Some(key));
                    }
                }
                assert_eq!(seen.len(), 1000);
                println!("{}", serde_json::json!({"width":width,"changed":changed,"sample":sample,"operations":20000,"ns_per_update":elapsed,"complete_forward_reverse_oracle":true}));
            }
        }
    }
}
