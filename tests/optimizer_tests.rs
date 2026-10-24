use lin::Db;

#[test]
fn test_constant_folding_lin_0521() {
    let mut db = Db::empty();
    db.run(r#"insert docs [{ id: "d1", title: "Doc 1" }, { id: "d2", title: "Doc 2" }]"#)
        .unwrap();

    // Constant comparison in `in` list deduplicated
    let res = db.run(r#"docs | id in ["d1", "d1"]"#).unwrap();
    assert_eq!(res.rows.len(), 1);

    // Negated comparison fold: not (id != "d1") -> id == "d1"
    let res2 = db.run(r#"docs | not (id != "d1")"#).unwrap();
    assert_eq!(res2.rows.len(), 1);
}

#[test]
fn test_predicate_simplification_lin_0522() {
    let mut db = Db::empty();
    db.run(r#"insert docs [{ id: "d1", title: "Doc 1" }, { id: "d2", title: "Doc 2" }]"#)
        .unwrap();

    // a AND a -> a
    let res_and = db.run(r#"docs | (id == "d1") and (id == "d1")"#).unwrap();
    assert_eq!(res_and.rows.len(), 1);

    // a OR a -> a
    let res_or = db.run(r#"docs | (id == "d1") or (id == "d1")"#).unwrap();
    assert_eq!(res_or.rows.len(), 1);
}

#[test]
fn test_projection_pushdown_equivalence_lin_0523() {
    let mut db = Db::empty();
    db.run(r#"insert docs [{ id: "d1", title: "Doc 1", body: "Large body", uri: "u://1" }]"#)
        .unwrap();

    // Projected scan vs full scan
    let full = db.run("docs").unwrap();
    assert!(full.rows[0].contains_key("body"));

    let projected = db.run("docs | { id, title }").unwrap();
    assert_eq!(projected.rows[0].get("id").unwrap().text(), Some("d1"));
    assert_eq!(
        projected.rows[0].get("title").unwrap().text(),
        Some("Doc 1")
    );
    assert!(!projected.rows[0].contains_key("body"));
}

#[test]
fn test_limit_pushdown_equivalence_lin_0524() {
    let mut db = Db::empty();
    for i in 0..100 {
        db.run(&format!(
            r#"insert docs {{ id: "d{i}", title: "Doc {i}" }}"#
        ))
        .unwrap();
    }

    let res_take = db.run("docs | take 5").unwrap();
    assert_eq!(res_take.rows.len(), 5);

    let res_offset_take = db.run("docs | offset 10 | take 5").unwrap();
    assert_eq!(res_offset_take.rows.len(), 5);
}
