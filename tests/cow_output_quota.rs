use lin::{Db, Quotas};

#[test]
fn shared_rows_charge_full_payload_against_output_quota() {
    for promote in [false, true] {
        let mut db = Db::empty();
        let body = "q".repeat(8192);
        for id in ["a", "b"] {
            db.run(&format!(
                r#"insert docs {{id:"{id}",body:"{body}",wing:"rag"}}"#,
            ))
            .unwrap();
            if promote {
                let hash = db.store.get_by_id("docs", id).unwrap()["hash"]
                    .text()
                    .unwrap()
                    .to_owned();
                db.run(&format!(
                    r#"update docs[id=="{id}"] cas "{hash}" {{wing:"sys"}}"#,
                ))
                .unwrap();
            }
        }
        let generation = db.stats().r#gen;
        let mut db = db.with_quotas(Quotas {
            max_output_bytes: 16384,
            ..Default::default()
        });
        for prepared in [false, true] {
            let one = if prepared {
                let query = db.prepare("docs | take 1").unwrap();
                db.run_prepared(&query).unwrap()
            } else {
                db.run("docs | take 1").unwrap()
            };
            assert_eq!(one.rows.len(), 1);
            assert_eq!(one.rows[0]["body"].text(), Some(body.as_str()));
            let error = if prepared {
                let query = db.prepare("docs | take 2").unwrap();
                db.run_prepared(&query).unwrap_err()
            } else {
                db.run("docs | take 2").unwrap_err()
            };
            assert!(error.to_string().contains("max_output_bytes"), "{error}");
            assert_eq!(db.stats().r#gen, generation);
            for id in ["a", "b"] {
                assert_eq!(
                    db.store.get_by_id("docs", id).unwrap()["body"].text(),
                    Some(body.as_str())
                );
            }
        }
    }
}
