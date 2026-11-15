//! Path multiplicity: a variable-length `match` returns one row per distinct path (over
//! distinct edges, so a repeated physical edge does not double a path), while `hop`
//! returns each reachable node once. Checked against path counts computed here.

use std::collections::{BTreeMap, BTreeSet};

use lin::Db;

#[test]
fn match_counts_paths_and_hop_counts_nodes() {
    let mut db = Db::empty();
    let n = 7;
    let docs = (0..n).map(|i| format!(r#"{{ uri: "raw://m/{i}", title: "M{i}", layer: "wiki" }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert docs [{docs}]")).unwrap();
    let ids: Vec<String> = (0..n).map(|i| db.run(&format!(r#"docs | uri == "raw://m/{i}" | {{ id }}"#)).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    // Two diamonds in a row plus a repeated edge.
    let edges = [(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (3, 5), (4, 6), (5, 6), (0, 1)];
    for (a, b) in edges {
        db.run(&format!(r#"append edge wikilink "{}" -> "{}""#, ids[a], ids[b])).unwrap();
    }
    let distinct: BTreeSet<(usize, usize)> = edges.iter().copied().collect();
    // Paths from 0 of length 1..=3, counted per end node.
    let mut want_paths: BTreeMap<String, usize> = BTreeMap::new();
    let mut frontier: Vec<usize> = vec![0];
    for _ in 0..3 {
        let mut next = Vec::new();
        for &v in &frontier {
            for &(a, b) in &distinct {
                if a == v {
                    *want_paths.entry(format!("M{b}")).or_default() += 1;
                    next.push(b);
                }
            }
        }
        frontier = next;
    }
    let mut got_paths: BTreeMap<String, usize> = BTreeMap::new();
    for r in db.run(&format!(r#"docs | id == "{}" | match -wikilink*1..3-> b | {{ b.title }} | take all"#, ids[0])).unwrap().rows.iter() {
        *got_paths.entry(r.get("b.title").and_then(|c| c.text()).unwrap().to_string()).or_default() += 1;
    }
    assert_eq!(got_paths, want_paths, "paths per end node");
    let hop: Vec<String> = db.run(&format!(r#"docs | id == "{}" | hop wikilink depth=3 | {{ title }} | take all"#, ids[0])).unwrap().rows.iter().map(|r| r.get("title").and_then(|c| c.text()).unwrap().to_string()).collect();
    let hop_set: BTreeSet<&String> = hop.iter().collect();
    assert_eq!(hop.len(), hop_set.len(), "hop returned a node twice");
    assert_eq!(hop_set.into_iter().cloned().collect::<BTreeSet<_>>(), want_paths.keys().cloned().collect::<BTreeSet<_>>());
}
