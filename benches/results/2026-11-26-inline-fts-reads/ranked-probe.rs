use std::{hint::black_box, time::Instant};
fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let mut db = lin::Db::empty();
    for start in (0..n).step_by(1000) {
        let records = (start..(start + 1000).min(n))
            .map(|i| {
                let label = if i % 10 == 0 { "wal" } else { "plain" };
                format!(
                    r#"{{id:"d-{i}",uri:"bench://{i}",title:"doc {i} {label}",body:"body {i}"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        db.run(&format!("insert docs [{records}]")).unwrap();
    }
    for (case, term) in [
        ("rare", (n - 1).to_string()),
        ("common", "wal".into()),
        ("miss", "neverpresent".into()),
        ("prefix", "wa*".into()),
    ] {
        let src = format!("docs | search lex \"{term}\" | take 20");
        assert!(db.explain_as(&src, None).unwrap().contains("FtsSeek"));
        let q = db.prepare(&src).unwrap();
        let get_ids = |rows: &[lin::Row]| {
            rows.iter()
                .map(|r| r.get_str("id").unwrap().text().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        let ids = get_ids(&q.run(&mut db).unwrap().rows);
        match case {
            "rare" => assert_eq!(ids, vec![format!("d-{}", n - 1)]),
            "miss" => assert!(ids.is_empty()),
            _ => {
                assert_eq!(ids.len(), 20);
                assert!(
                    ids.iter()
                        .all(|id| id[2..].parse::<usize>().unwrap() % 10 == 0)
                );
            }
        }
        let start = Instant::now();
        let mut iters = 0;
        while start.elapsed().as_millis() < 40 {
            black_box(q.run(&mut db).unwrap());
            iters += 1;
        }
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        assert_eq!(get_ids(&q.run(&mut db).unwrap().rows), ids);
        println!(
            "{}",
            serde_json::json!({"case":case,"ids":ids,"ns":ns,"iterations":iters})
        );
    }
}
