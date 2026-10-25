use lin::row::{FromCell, ToCell};
use lin::{Db, QueryCursor, Stmt};

#[test]
fn test_ast_parser_roundtrip_cases_lin_1641() {
    let queries = [
        r#"docs | wing == "rag" | { id, title } | take 5"#,
        r#"docs | { id, title } | sort id | take 10"#,
        r#"docs | count"#,
        r#"insert docs [{ id: "x", title: "test" }]"#,
        r#"delete docs[id == "x"]"#,
    ];

    for src in queries {
        let stmt1 = lin::parse(src).unwrap();
        // Stmt matches expected kind
        match &stmt1 {
            Stmt::Query(_) | Stmt::Insert { .. } | Stmt::Delete { .. } => {}
            _ => panic!("unexpected statement: {stmt1:?}"),
        }
    }
}

#[test]
fn test_typed_untyped_roundtrip_cases_lin_1642() {
    // Int
    let int_val: i64 = 42;
    let cell_int = int_val.to_cell();
    assert_eq!(i64::from_cell(&cell_int).unwrap(), 42);

    // Float
    let float_val: f64 = 3.14159;
    let cell_float = float_val.to_cell();
    assert_eq!(f64::from_cell(&cell_float).unwrap(), 3.14159);

    // Bool
    let bool_val: bool = true;
    let cell_bool = bool_val.to_cell();
    assert_eq!(bool::from_cell(&cell_bool).unwrap(), true);

    // Text
    let text_val = "hello lin database";
    let cell_text = text_val.to_cell();
    assert_eq!(String::from_cell(&cell_text).unwrap(), "hello lin database");
}

#[test]
fn test_row_batch_equivalence_cases_lin_1643() {
    let mut db = Db::empty();
    for i in 0..25 {
        db.run(&format!(
            r#"insert docs {{ id: "d{i}", title: "Title {i}", wing: "rag" }}"#
        ))
        .unwrap();
    }

    let prep = db
        .prepare(r#"docs | wing == "rag" | { id, title } | take 20"#)
        .unwrap();

    let row_handle = prep.run(&mut db).unwrap();
    let batch = prep.run_batch(&mut db).unwrap();

    assert_eq!(row_handle.rows.len(), batch.n());
    assert!(!row_handle.rows.is_empty());

    // Verify cell-by-cell equivalence between row materialization and columnar batch
    for row_idx in 0..batch.n() {
        for (col_idx, name) in batch.names.iter().enumerate() {
            let row_cell = row_handle.rows[row_idx].get(name).unwrap();
            let batch_cell = &batch.cols[col_idx][row_idx];
            assert_eq!(row_cell, batch_cell, "mismatch at row {row_idx} col {name}");
        }
    }
}

#[test]
fn test_cursor_materialized_equivalence_cases_lin_1644() {
    let mut db = Db::empty();
    for i in 0..30 {
        db.run(&format!(
            r#"insert docs {{ id: "doc_{i:02}", title: "Item {i}" }}"#
        ))
        .unwrap();
    }

    let query_src = "docs | { id, title } | take 15";
    let materialized = db.run(query_src).unwrap();

    let stmt = lin::parse(query_src).unwrap();
    let q = match stmt {
        Stmt::Query(q) => q,
        _ => unreachable!(),
    };

    let cursor = QueryCursor::open_db(&db, &q).unwrap();
    let cursor_rows: Result<Vec<_>, _> = cursor.collect();
    let cursor_rows = cursor_rows.unwrap();

    assert_eq!(materialized.rows.len(), cursor_rows.len());
    for (i, (m, c)) in materialized.rows.iter().zip(&cursor_rows).enumerate() {
        assert_eq!(m, c, "mismatch at row {i}");
    }
}
