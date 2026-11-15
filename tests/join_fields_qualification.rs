//! Joined rows keep both sides' values when field names collide: a field present on both
//! sides is reachable from each side by its qualified name, the left side keeps its
//! unqualified name, and projections and filters on either name see the right values.

use lin::Db;

fn seed() -> Db {
    let mut db = Db::empty();
    db.run("col p { id: text, label: text, n: i64 }").unwrap();
    db.run("col c { id: text, pid: text, label: text, n: i64 }").unwrap();
    db.run("fk c.pid -> p.id").unwrap();
    db.run(r#"insert p [{ id: "p1", label: "parent-one", n: 100 }, { id: "p2", label: "parent-two", n: 200 }]"#).unwrap();
    db.run(r#"insert c [{ id: "c1", pid: "p1", label: "child-one", n: 1 }, { id: "c2", pid: "p2", label: "child-two", n: 2 }, { id: "c3", pid: "p1", label: "child-three", n: 3 }]"#).unwrap();
    db
}

fn text(r: &lin::Row, k: &str) -> Option<String> {
    r.get(k).and_then(|c| c.text()).map(str::to_string)
}

#[test]
fn colliding_field_names_keep_both_sides() {
    let mut db = seed();
    let rows = db.run("c | join p on pid | sort id | take all").unwrap().rows.clone();
    assert_eq!(rows.len(), 3);
    for r in &rows {
        let id = text(r, "id").unwrap();
        let (want_child, want_parent, n, pn) = match id.as_str() {
            "c1" => ("child-one", "parent-one", 1.0, 100.0),
            "c2" => ("child-two", "parent-two", 2.0, 200.0),
            "c3" => ("child-three", "parent-one", 3.0, 100.0),
            other => panic!("unexpected row {other}"),
        };
        assert_eq!(text(r, "label").as_deref(), Some(want_child), "{r:?}");
        assert_eq!(text(r, "p.label").as_deref(), Some(want_parent), "{r:?}");
        assert_eq!(r.get("n").and_then(|c| c.as_f64()), Some(n), "{r:?}");
        assert_eq!(r.get("p.n").and_then(|c| c.as_f64()), Some(pn), "{r:?}");
    }
    // Projection by each name.
    let proj = db.run("c | join p on pid | { id, label, p.label } | sort id | take all").unwrap().rows.clone();
    let got: Vec<(String, String)> = proj.iter().map(|r| (text(r, "label").unwrap(), text(r, "p.label").unwrap())).collect();
    assert_eq!(got, [("child-one".into(), "parent-one".into()), ("child-two".into(), "parent-two".into()), ("child-three".into(), "parent-one".into())]);
    // Filters on the qualified and unqualified colliding names select different rows.
    let ids = |db: &mut Db, q: &str| -> Vec<String> {
        let mut v: Vec<String> = db.run(q).unwrap().rows.iter().map(|r| text(r, "id").unwrap()).collect();
        v.sort();
        v
    };
    assert_eq!(ids(&mut db, "c | join p on pid | n > 1 | take all"), ["c2", "c3"]);
    assert_eq!(ids(&mut db, "c | join p on pid | p.n > 150 | take all"), ["c2"]);
    assert_eq!(ids(&mut db, r#"c | join p on pid | p.label == "parent-one" | take all"#), ["c1", "c3"]);
}
