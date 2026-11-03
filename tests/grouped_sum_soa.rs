use lin::{Cell, Db, Quotas};

#[test]
fn orders_soa_grouped_sum_enforces_the_generic_quota() {
    for source in [
        "orders | sum total by user_id",
        "orders | total > 0 | sum total by user_id",
    ] {
        let mut db = Db::empty().with_quotas(Quotas {
            max_aggregate_bytes: 195,
            ..Default::default()
        });
        db.run(
            r#"insert users [{id:"u1",email:"u1@bench.local"},{id:"u2",email:"u2@bench.local"}]"#,
        )
        .unwrap();
        db.run(r#"insert orders [{id:"a",user_id:"u1",total:1},{id:"b",user_id:"u2",total:2}]"#)
            .unwrap();
        assert!(db.store.orders_soa_ready());
        let err = db.prepare(source).unwrap().run(&mut db).unwrap_err();
        assert!(
            err.to_string()
                .contains("aggregate state bytes 196 exceeds max_aggregate_bytes quota 195"),
            "{err}"
        );
    }
}

#[test]
fn orders_soa_grouped_sum_preserves_source_order_float_bits() {
    let mut db = Db::empty();
    db.run(r#"insert users [{id:"u1",email:"u1@bench.local"}]"#)
        .unwrap();
    let records = (0..8192)
        .map(|i| {
            let value = match i {
                0 => "10000000000000000.0",
                4096 => "-10000000000000000.0",
                4097 => "1.0",
                _ => "0.0",
            };
            format!(r#"{{id:"o{i}",user_id:"u1",total:{value}}}"#)
        })
        .collect::<Vec<_>>();
    for chunk in records.chunks(512) {
        db.run(&format!("insert orders [{}]", chunk.join(",")))
            .unwrap();
    }
    assert!(db.store.orders_soa_ready());
    let actual = db
        .prepare("orders | sum total by user_id")
        .unwrap()
        .run(&mut db)
        .unwrap()
        .rows;
    let reference = db
        .run("orders | {user_id,total} | sum total by user_id")
        .unwrap()
        .rows;
    let Cell::Float(sum) = reference[0]["total"] else {
        panic!("float sum required")
    };
    assert_eq!(sum.to_bits(), 1.0f64.to_bits());
    assert_eq!(actual, reference);
}

#[test]
fn orders_soa_filter_retains_integer_precision_above_two_to_fifty_three() {
    let mut db = Db::empty();
    db.run(r#"insert users [{id:"u1",email:"u1@bench.local"}]"#)
        .unwrap();
    db.run(
        r#"insert orders [
        {id:"a",user_id:"u1",total:9007199254740993},
        {id:"b",user_id:"u1",total:9007199254740992}
    ]"#,
    )
    .unwrap();
    assert!(db.store.orders_soa_ready());
    let actual = db
        .prepare("orders | total > 9007199254740992 | sum total by user_id")
        .unwrap()
        .run(&mut db)
        .unwrap()
        .rows;
    let reference = db
        .run("orders | total > 9007199254740992 | {user_id,total} | sum total by user_id")
        .unwrap()
        .rows;
    assert_eq!(reference.len(), 1);
    assert_eq!(actual, reference);
}

#[test]
fn cached_scalar_sum_is_invalidated_by_writes_and_rollback() {
    let mut db = Db::empty();
    db.run(r#"insert users [{id:"u1",email:"u1@bench.local"}]"#)
        .unwrap();
    db.run(r#"insert orders [{id:"a",user_id:"u1",total:1},{id:"b",user_id:"u1",total:2}]"#)
        .unwrap();
    fn sum(db: &mut Db) -> f64 {
        db.run("orders | sum total").unwrap().rows[0]["total"]
            .as_f64()
            .unwrap()
    }
    assert_eq!(sum(&mut db), 3.0);
    assert_eq!(sum(&mut db), 3.0);
    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"update orders[id == "a"] cas each {total:9}"#).unwrap();
        assert_eq!(
            tx.run("orders | sum total").unwrap().rows[0]["total"].as_f64(),
            Some(11.0)
        );
        tx.rollback().unwrap();
    }
    assert_eq!(sum(&mut db), 3.0);
    db.run(r#"update orders[id == "a"] cas each {total:4}"#).unwrap();
    assert_eq!(sum(&mut db), 6.0);
    db.run(r#"insert orders {id:"c",user_id:"u1",total:8}"#)
        .unwrap();
    assert_eq!(sum(&mut db), 14.0);
    db.run(r#"delete orders[id == "b"] cas each"#).unwrap();
    assert_eq!(sum(&mut db), 12.0);
    {
        let mut tx = db.transaction().unwrap();
        tx.run(r#"update orders[id == "a"] cas each {total:-100}"#).unwrap();
        assert_eq!(
            tx.run("orders | sum total").unwrap().rows[0]["total"].as_f64(),
            Some(-92.0)
        );
    }
    assert_eq!(sum(&mut db), 12.0);
}
