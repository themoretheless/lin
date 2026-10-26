use lin::{Cell, Db, Quotas, Row};

fn fixture() -> Db {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    db.run(
        r#"insert docs [
        {id:"a",title:"A",body:"hit",wing:"rag",ts:timestamp(10)},
        {id:"b",title:"B",body:"miss",wing:"rag",ts:timestamp(20)},
        {id:"c",title:"B",body:"hit",wing:"rag",ts:timestamp(30)},
        {id:"d",title:"D",body:"miss",wing:"ops",ts:timestamp(40)}
    ]"#,
    )
    .unwrap();
    db
}
fn groups(rows: &[Row], key: &str) -> Vec<(String, i64)> {
    rows.iter()
        .map(|row| {
            assert_eq!(row.len(), 2);
            let count = match row["hits"] {
                Cell::Int(n) => n,
                _ => panic!("count must be Int"),
            };
            (row[key].text().unwrap().to_string(), count)
        })
        .collect()
}
#[test]
fn indexed_prepared_group_counts_match_explicit_expected_values() {
    let mut db = fixture();
    for (query, key, expected) in [
        (
            r#"docs | wing == "rag" | count by wing"#,
            "wing",
            vec![("rag", 3)],
        ),
        (
            r#"docs | wing == "rag" and ts > timestamp(15) | count by wing"#,
            "wing",
            vec![("rag", 2)],
        ),
        (r#"docs | wing == "absent" | count by wing"#, "wing", vec![]),
        (
            r#"docs | wing == "rag" and wing == "ops" | count by wing"#,
            "wing",
            vec![],
        ),
        (
            r#"docs | wing == "rag" and body ~ "hit" | count by wing"#,
            "wing",
            vec![("rag", 2)],
        ),
        (
            r#"docs | wing == "rag" | count by title"#,
            "title",
            vec![("A", 1), ("B", 2)],
        ),
        (
            r#"docs | wing == "rag" or wing == "ops" | count by wing"#,
            "wing",
            vec![("ops", 1), ("rag", 3)],
        ),
    ] {
        let prepared = db.prepare(query).unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(s, n)| (s.to_owned(), n))
            .collect();
        assert_eq!(
            groups(&prepared.run(&mut db).unwrap().rows, key),
            expected,
            "{query}"
        );
        assert_eq!(
            groups(&db.reader().run_prepared(&prepared).unwrap().rows, key),
            expected,
            "readonly {query}"
        );
    }
}
#[test]
fn indexed_prepared_group_counts_follow_mutations_and_rollback() {
    let mut db = fixture();
    let prepared = db
        .prepare(r#"docs | wing == "rag" | count by wing"#)
        .unwrap();
    db.run(r#"update docs[id == "a"] cas each {wing:"ops"}"#)
        .unwrap();
    assert_eq!(
        groups(&prepared.run(&mut db).unwrap().rows, "wing"),
        vec![("rag".into(), 2)]
    );
    db.run(r#"delete docs[id == "b"] cas each"#).unwrap();
    assert_eq!(
        groups(&prepared.run(&mut db).unwrap().rows, "wing"),
        vec![("rag".into(), 1)]
    );
    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"update docs[id == "c"] cas each {wing:"ops"}"#)
            .unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(
        groups(&prepared.run(&mut db).unwrap().rows, "wing"),
        vec![("rag".into(), 1)]
    );
}
#[test]
fn fixed_group_index_path_preserves_exact_aggregate_quota() {
    for limit in [0, 98, 99, 100] {
        for query in [
            r#"docs | wing == "rag" | count by wing"#,
            // A residual forces the existing generic grouping path.
            r#"docs | wing == "rag" and title ~ "" | count by wing"#,
        ] {
            let mut db = fixture().with_quotas(Quotas {
                max_aggregate_bytes: limit,
                ..Default::default()
            });
            let prepared = db.prepare(query).unwrap();
            let result = prepared.run(&mut db);
            if limit < 99 {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("aggregate state bytes 99"),
                    "{query}: {error}"
                );
                assert!(error.contains("max_aggregate_bytes"));
            } else {
                assert_eq!(
                    groups(&result.unwrap().rows, "wing"),
                    vec![("rag".into(), 3)]
                );
            }
        }
    }
    let mut db = fixture().with_quotas(Quotas {
        max_aggregate_bytes: 0,
        ..Default::default()
    });
    let prepared = db
        .prepare(r#"docs | wing == "absent" | count by wing"#)
        .unwrap();
    assert!(prepared.run(&mut db).unwrap().rows.is_empty());
}

#[test]
fn fixed_group_quota_counts_utf8_bytes_and_preserves_empty_keys() {
    for (key, bytes) in [("ёж", 4), ("", 0)] {
        for delta in [0, 1] {
            let mut db = Db::empty();
            db.run("index docs [wing, ts]").unwrap();
            db.run(&format!(
                r#"insert docs {{id:"u",title:"Unicode",wing:"{key}",ts:timestamp(1)}}"#
            ))
            .unwrap();
            let limit = bytes + 96 - delta;
            let mut db = db.with_quotas(Quotas {
                max_aggregate_bytes: limit,
                ..Default::default()
            });
            let prepared = db
                .prepare(&format!(r#"docs | wing == "{key}" | count by wing"#))
                .unwrap();
            let result = prepared.run(&mut db);
            if delta == 0 {
                assert_eq!(
                    groups(&result.unwrap().rows, "wing"),
                    vec![(key.to_owned(), 1)]
                );
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("max_aggregate_bytes")
                );
            }
        }
    }
}

#[test]
fn benchmark_shaped_group_counts_are_exact_at_ten_thousand_rows() {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    for start in (0..10_000).step_by(500) {
        let records = (start..start + 500).map(|i| {
            let (wing, age) = if i % 2 == 0 { ("rag", 1) } else { ("sys", 30) };
            let ts = now - age * 86_400_000;
            format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"doc {i}",body:"body {i}",wing:"{wing}",ts:timestamp({ts})}}"#)
        }).collect::<Vec<_>>().join(",");
        db.run(&format!("insert docs [{records}]")).unwrap();
    }
    for query in [
        r#"docs | wing == "rag" | count by wing"#,
        r#"docs | wing == "rag" and ts > ago 7d | count by wing"#,
    ] {
        let prepared = db.prepare(query).unwrap();
        let expected = vec![("rag".to_owned(), 5_000)];
        assert_eq!(
            groups(&prepared.run(&mut db).unwrap().rows, "wing"),
            expected
        );
        assert_eq!(
            groups(&db.reader().run_prepared(&prepared).unwrap().rows, "wing"),
            expected
        );
    }
}
