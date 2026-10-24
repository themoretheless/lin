use lin::{Db, Quotas};

#[test]
fn test_query_bytes_limit_lin_1761() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_query_bytes: 100,
        ..Default::default()
    });

    let small_query = "docs | count";
    assert!(db.run(small_query).is_ok());

    let long_query = format!("docs | [id == \"{}\"]", "a".repeat(200));
    let err = db.run(&long_query).unwrap_err();
    assert!(
        err.to_string().contains("max_query_bytes") || err.to_string().contains("exceeds"),
        "{err}"
    );
}

#[test]
fn test_ast_recursion_depth_lin_1762() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_ast_depth: 32,
        ..Default::default()
    });

    // Deeply nested parentheses: (((...id == "x"...)))
    let mut deep_query = String::new();
    deep_query.push_str("docs | ");
    for _ in 0..100 {
        deep_query.push('(');
    }
    deep_query.push_str("id == \"x\"");
    for _ in 0..100 {
        deep_query.push(')');
    }

    let err = db.run(&deep_query).unwrap_err();
    assert!(
        err.to_string().contains("depth limit") || err.to_string().contains("recursion"),
        "{err}"
    );
}

#[test]
fn test_output_bytes_limit_lin_1763() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_output_bytes: 3500, // small cap: permits single insert, fails full scan
        ..Default::default()
    });

    db.run(
        r#"insert docs { id: "d1", title: "A large title that takes plenty of bytes in output" }"#,
    )
    .unwrap();
    db.run(r#"insert docs { id: "d2", title: "Another large title that takes plenty of bytes in output" }"#)
        .unwrap();

    let err = db.run("docs").unwrap_err();
    assert!(err.to_string().contains("max_output_bytes"), "{err}");
}

#[test]
fn test_execution_work_limit_lin_1764() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_execution_work: 5, // very small work cap
        ..Default::default()
    });

    for i in 0..10 {
        db.run(&format!(
            r#"insert docs {{ id: "d{i}", title: "Title {i}" }}"#
        ))
        .unwrap();
    }

    let err = db.run("docs | title == \"Title 9\"").unwrap_err();
    assert!(
        err.to_string().contains("max_execution_work") || err.to_string().contains("work"),
        "{err}"
    );
}
