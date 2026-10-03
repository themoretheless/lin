use lin::{Cell, Db};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for n in [1000usize, 10000] {
        let records = (0..n).map(|i| format!(
            r#"{{id:"d-{i}",uri:"bench://{i}",wing:"{}",title:"doc {i} {}",body:"body {i}",layer:"wiki",ts:timestamp(1700000000000)}}"#,
            if i % 2 == 0 { "rag" } else { "sys" },
            if i % 10 == 0 { "wal note" } else { "plain" }
        )).collect::<Vec<_>>();
        let source = format!("insert docs [{}]", records.join(","));
        for _ in 0..3 {
            let mut db = Db::empty();
            db.run("index docs [wing, ts]")?;
            let prepared = db.prepare(&source)?;
            assert_eq!(prepared.run(&mut db)?.done.n, n);
            let rows = db.store.collection("docs");
            assert_eq!(rows.len(), n);
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row["id"].text(), Some(format!("d-{i}").as_str()));
                assert_eq!(row["uri"].text(), Some(format!("bench://{i}").as_str()));
                assert_eq!(
                    row["wing"].text(),
                    Some(if i % 2 == 0 { "rag" } else { "sys" })
                );
                assert_eq!(
                    row["title"].text(),
                    Some(
                        format!("doc {i} {}", if i % 10 == 0 { "wal note" } else { "plain" })
                            .as_str()
                    )
                );
                assert_eq!(row["body"].text(), Some(format!("body {i}").as_str()));
                assert_eq!(row["ts"], Cell::Time(1700000000000));
                assert_eq!(row["embedding"].as_vec().unwrap().len(), 768);
            }
        }
    }
    Ok(())
}
