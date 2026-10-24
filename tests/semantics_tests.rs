use lin::{Db, cosine};

#[test]
fn test_cosine_dimensional_mismatch_lin_1081() {
    let a = [1.0f32, 2.0, 3.0];
    let b = [1.0f32, 2.0, 3.0, 4.0];
    assert_eq!(cosine(&a, &b), 0.0);
}

#[test]
fn test_cosine_zero_norms_lin_1082() {
    let a = [0.0f32, 0.0, 0.0];
    let b = [1.0f32, 2.0, 3.0];
    assert_eq!(cosine(&a, &b), 0.0);
    assert_eq!(cosine(&a, &a), 0.0);
}

#[test]
fn test_cosine_nan_handling_lin_1083() {
    let a = [f32::NAN, 1.0, 2.0];
    let b = [1.0f32, 2.0, 3.0];
    let res = cosine(&a, &b);
    assert!(!res.is_nan(), "cosine returned NaN!");
    assert_eq!(res, 0.0);
}

#[test]
fn test_cosine_infinity_handling_lin_1084() {
    let a = [f32::INFINITY, 1.0, 2.0];
    let b = [1.0f32, 2.0, 3.0];
    let res = cosine(&a, &b);
    assert!(!res.is_infinite());
    assert_eq!(res, 0.0);

    let c = [f32::NEG_INFINITY, 1.0, 2.0];
    let res2 = cosine(&c, &b);
    assert_eq!(res2, 0.0);
}

#[test]
fn test_int_above_2_pow_53_lin_0401() {
    let mut db = Db::empty();
    db.run("col items { id: text, amount: i64 }").unwrap();

    let val1: i64 = 9_007_199_254_740_993; // 2^53 + 1
    let val2: i64 = 9_007_199_254_740_992; // 2^53

    db.run(&format!(r#"insert items {{ id: "d1", amount: {val1} }}"#))
        .unwrap();
    db.run(&format!(r#"insert items {{ id: "d2", amount: {val2} }}"#))
        .unwrap();

    let res1 = db.run(&format!(r#"items | amount == {val1}"#)).unwrap();
    assert_eq!(res1.rows.len(), 1);
    assert_eq!(res1.rows[0].get("id").unwrap().text(), Some("d1"));

    let res2 = db.run(&format!(r#"items | amount == {val2}"#)).unwrap();
    assert_eq!(res2.rows.len(), 1);
    assert_eq!(res2.rows[0].get("id").unwrap().text(), Some("d2"));

    let res_gt = db.run(&format!(r#"items | amount > {val2}"#)).unwrap();
    assert_eq!(res_gt.rows.len(), 1);
    assert_eq!(res_gt.rows[0].get("id").unwrap().text(), Some("d1"));
}

#[test]
fn test_float_integer_tie_breaker_lin_0402() {
    let mut db = Db::empty();
    db.run("col items { id: text, val: f64 }").unwrap();

    db.run(r#"insert items [{ id: "d1", val: 10.0 }, { id: "d2", val: 10.5 }]"#)
        .unwrap();

    let res = db.run("items | val == 10").unwrap();
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0].get("id").unwrap().text(), Some("d1"));

    let res_gt = db.run("items | val > 10").unwrap();
    assert_eq!(res_gt.rows.len(), 1);
    assert_eq!(res_gt.rows[0].get("id").unwrap().text(), Some("d2"));
}

#[test]
fn test_negative_infinity_ordering_lin_0403() {
    let mut db = Db::empty();
    db.run("col items { id: text, val: f64 }").unwrap();

    db.run(r#"insert items [{ id: "d1", val: -1000000.0 }]"#)
        .unwrap();

    let res = db.run("items | val > -9999999999.0").unwrap();
    assert_eq!(res.rows.len(), 1);
}

#[test]
fn test_positive_infinity_ordering_lin_0404() {
    let mut db = Db::empty();
    db.run("col items { id: text, val: f64 }").unwrap();

    db.run(r#"insert items [{ id: "d1", val: 1000000.0 }]"#)
        .unwrap();

    let res = db.run("items | val < 9999999999.0").unwrap();
    assert_eq!(res.rows.len(), 1);
}

#[test]
fn test_null_predicate_semantics_lin_0481() {
    let mut db = Db::empty();
    db.run("col items { id: text, val: i64 }").unwrap();

    db.run(r#"insert items [{ id: "d1" }, { id: "d2", val: 42 }]"#)
        .unwrap();

    let res_is_null = db.run("items | val is null").unwrap();
    assert_eq!(res_is_null.rows.len(), 1);
    assert_eq!(res_is_null.rows[0].get("id").unwrap().text(), Some("d1"));

    let res_is_not_null = db.run("items | val is not null").unwrap();
    assert_eq!(res_is_not_null.rows.len(), 1);
    assert_eq!(
        res_is_not_null.rows[0].get("id").unwrap().text(),
        Some("d2")
    );

    // In 3VL, null == 42 is false
    let res_eq = db.run("items | val == 42").unwrap();
    assert_eq!(res_eq.rows.len(), 1);
    assert_eq!(res_eq.rows[0].get("id").unwrap().text(), Some("d2"));
}

#[test]
fn test_absent_field_semantics_lin_0482() {
    let mut db = Db::empty();
    db.run("col items { id: text, extra: text }").unwrap();

    db.run(r#"insert items [{ id: "d1" }, { id: "d2", extra: "present" }]"#)
        .unwrap();

    let res_absent = db.run("items | extra is null").unwrap();
    assert_eq!(res_absent.rows.len(), 1);
    assert_eq!(res_absent.rows[0].get("id").unwrap().text(), Some("d1"));
}

#[test]
fn test_mixed_numeric_coercions_lin_0483() {
    let mut db = Db::empty();
    db.run("col items { id: text, score: f64 }").unwrap();

    db.run(r#"insert items [{ id: "d1", score: 100.0 }, { id: "d2", score: 100.0 }]"#)
        .unwrap();

    // Int 100 equals Float 100.0
    let res1 = db.run("items | score == 100.0").unwrap();
    assert_eq!(res1.rows.len(), 2);

    let res2 = db.run("items | score == 100").unwrap();
    assert_eq!(res2.rows.len(), 2);
}

#[test]
fn test_boolean_coercion_policy_lin_0484() {
    let mut db = Db::empty();
    db.run("col items { id: text, active: bool }").unwrap();

    db.run(r#"insert items [{ id: "d1", active: true }, { id: "d2", active: false }]"#)
        .unwrap();

    let res = db.run("items | active == true").unwrap();
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0].get("id").unwrap().text(), Some("d1"));
}
