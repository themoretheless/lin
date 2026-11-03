use lin::{Cell, Db, Quotas};

#[test]
fn grouped_sum_preserves_order_bits_and_owned_result_keys() {
    let mut db = Db::empty();
    db.run("col items { k: text, v: f64 }").unwrap();
    db.run(
        r#"insert items [
        { id: "a", k: "group-alpha-with-long-text", v: 10000000000000000.0 },
        { id: "b", k: "group-alpha-with-long-text", v: -10000000000000000.0 },
        { id: "c", k: "group-alpha-with-long-text", v: 1.0 },
        { id: "d", k: "group-beta-with-long-text", v: -0.0 },
        { id: "e", v: 7.0 }
    ]"#,
    )
    .unwrap();
    for source in [
        "items | sum v by k",
        "items | v > -20000000000000000.0 | sum v by k",
    ] {
        let prepared = db.prepare(source).unwrap();
        let actual = prepared.run(&mut db).unwrap().rows;
        // Projection forces the independent generic aggregate implementation.
        let reference = db.run("items | {k, v} | sum v by k").unwrap().rows;
        assert_eq!(actual, reference);
        let alpha = actual
            .iter()
            .find(|r| r["k"].text() == Some("group-alpha-with-long-text"))
            .unwrap();
        let Cell::Float(sum) = alpha["v"] else {
            panic!("float sum required")
        };
        assert_eq!(sum.to_bits(), 1.0f64.to_bits());
        let beta = actual
            .iter()
            .find(|r| r["k"].text() == Some("group-beta-with-long-text"))
            .unwrap();
        let Cell::Float(sum) = beta["v"] else {
            panic!("float sum required")
        };
        assert_eq!(sum.to_bits(), 0.0f64.to_bits());
    }
    let retained = db
        .prepare("items | sum v by k")
        .unwrap()
        .run(&mut db)
        .unwrap()
        .rows;
    drop(db);
    assert!(
        retained
            .iter()
            .any(|r| r["k"].text() == Some("group-alpha-with-long-text"))
    );
}

#[test]
fn grouped_sum_charges_distinct_keys_at_the_same_quota_boundary() {
    let alpha = "group-alpha-with-long-text";
    let beta = "group-beta-with-long-text";
    let required = alpha.len() + beta.len() + 2 * (32 + 64);
    for limit in [required - 1, required] {
        let mut db = Db::empty().with_quotas(Quotas {
            max_aggregate_bytes: limit,
            ..Default::default()
        });
        db.run("col items { k: text, v: f64 }").unwrap();
        db.run(&format!(
            r#"insert items [
            {{ id: "a", k: "{alpha}", v: 1.0 }},
            {{ id: "b", k: "{alpha}", v: 2.0 }},
            {{ id: "c", k: "{beta}", v: 3.0 }}
        ]"#
        ))
        .unwrap();
        let prepared = db.prepare("items | sum v by k").unwrap().run(&mut db);
        let reference = db.run("items | {k, v} | sum v by k");
        if limit < required {
            let expected = format!(
                "aggregate state bytes {required} exceeds max_aggregate_bytes quota {limit}"
            );
            assert!(prepared.unwrap_err().to_string().contains(&expected));
            assert!(reference.unwrap_err().to_string().contains(&expected));
        } else {
            assert_eq!(prepared.unwrap().rows, reference.unwrap().rows);
        }
    }
}
