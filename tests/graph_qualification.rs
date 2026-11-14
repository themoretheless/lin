//! Graph traversal against a breadth-first oracle over the edges the test created —
//! cycles, self-loops, repeated edges and edges to nodes that do not exist — and the
//! same answers after log replay, after a checkpoint, and after a rolled-back
//! transaction that added and removed edges.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use lin::Db;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("lin-graph-qual-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn crash_image(from: &Path, tag: &str) -> PathBuf {
    let to = tmp(tag);
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() && entry.file_name() != "LOCK" {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
    to
}

const N: usize = 30;

/// Insert the nodes and the edges; returns the node ids by index and the edge list.
fn build(db: &mut Db) -> (Vec<String>, Vec<(String, String)>) {
    db.run("rel link stub").unwrap();
    let docs = (0..N)
        .map(|i| format!(r#"{{ uri: "raw://g/{i:02}", title: "Node {i}", layer: "wiki" }}"#))
        .collect::<Vec<_>>()
        .join(",");
    db.run(&format!("insert docs [{docs}]")).unwrap();
    let ids: Vec<String> = (0..N)
        .map(|i| {
            db.run(&format!(r#"docs | uri == "raw://g/{i:02}" | {{ id }}"#)).unwrap().rows[0]
                .get("id")
                .and_then(|c| c.text())
                .unwrap()
                .to_string()
        })
        .collect();
    let mut x: u64 = 0xDEAD_BEEF_1234_5678;
    let mut next = move |n: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % n
    };
    let mut edges: Vec<(String, String)> = Vec::new();
    for i in 0..N {
        for _ in 0..2 {
            let j = next(N as u64) as usize;
            edges.push((ids[i].clone(), ids[j].clone()));
        }
    }
    edges.push((ids[3].clone(), ids[3].clone())); // self-loop
    edges.push((ids[4].clone(), ids[5].clone())); // repeated below
    edges.push((ids[4].clone(), ids[5].clone()));
    edges.push((ids[6].clone(), "ghost-node".to_string())); // dangling target
    let src = edges
        .iter()
        .map(|(a, b)| format!(r#"link "{a}" -> "{b}""#))
        .collect::<Vec<_>>()
        .join(",\n");
    db.run(&format!("append edges [\n{src}\n]")).unwrap();
    edges.sort();
    edges.dedup();
    (ids, edges)
}

/// Nodes reached in 1..=depth steps from `start`, breadth first.
fn reachable(edges: &[(String, String)], start: &str, depth: usize) -> BTreeSet<String> {
    let mut out_edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in edges {
        out_edges.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<&str> = vec![start];
    let mut visited: BTreeSet<&str> = BTreeSet::from([start]);
    for _ in 0..depth {
        let mut next = Vec::new();
        for node in frontier {
            for &to in out_edges.get(node).into_iter().flatten() {
                seen.insert(to.to_string());
                if visited.insert(to) {
                    next.push(to);
                }
            }
        }
        frontier = next;
    }
    seen
}

fn graph_targets(db: &mut Db, start: &str, depth: usize) -> BTreeSet<String> {
    db.run(&format!(r#"docs | id == "{start}" | graph link depth={depth} | take all"#))
        .unwrap()
        .rows
        .iter()
        .map(|r| r.get("to").and_then(|c| c.text()).unwrap().to_string())
        .collect()
}

fn hop_ids(db: &mut Db, start: &str, depth: usize) -> BTreeSet<String> {
    db.run(&format!(r#"docs | id == "{start}" | hop link depth={depth} | {{ id }} | take all"#))
        .unwrap()
        .rows
        .iter()
        .map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string())
        .collect()
}

fn answers(db: &mut Db, ids: &[String]) -> Vec<(BTreeSet<String>, BTreeSet<String>)> {
    let mut out = Vec::new();
    for s in [0usize, 3, 4, 6, 17] {
        for depth in [1usize, 2, 3] {
            out.push((graph_targets(db, &ids[s], depth), hop_ids(db, &ids[s], depth)));
        }
    }
    out
}

#[test]
fn traversal_matches_a_breadth_first_oracle() {
    let mut db = Db::empty();
    let (ids, edges) = build(&mut db);
    let existing: BTreeSet<String> = ids.iter().cloned().collect();
    for s in [0usize, 3, 4, 6, 17, 29] {
        for depth in [1usize, 2, 3] {
            let want = reachable(&edges, &ids[s], depth);
            let got = graph_targets(&mut db, &ids[s], depth);
            assert_eq!(got, want, "graph from {s} depth {depth}");
            let want_nodes: BTreeSet<String> = want.intersection(&existing).cloned().collect();
            let hops = hop_ids(&mut db, &ids[s], depth);
            assert!(hops.is_subset(&want_nodes), "hop from {s} depth {depth} reached nodes outside the oracle");
            let mut want_without_start = want_nodes.clone();
            want_without_start.remove(&ids[s]);
            let mut hops_without_start = hops.clone();
            hops_without_start.remove(&ids[s]);
            assert_eq!(hops_without_start, want_without_start, "hop from {s} depth {depth}");
        }
    }
}

/// Depth is capped at 3: a deeper request is refused at compile time, not truncated.
#[test]
fn deeper_traversal_is_refused() {
    let mut db = Db::empty();
    let (ids, _) = build(&mut db);
    for q in ["graph link depth=4", "hop link depth=4", "graph link depth=1000000"] {
        assert!(db.run(&format!(r#"docs | id == "{}" | {q}"#, ids[0])).is_err(), "{q}");
    }
}

#[test]
fn traversal_is_the_same_after_replay_checkpoint_and_rollback() {
    let mut mem = Db::empty();
    let (mem_ids, _) = build(&mut mem);
    let dir = tmp("durable");
    let mut db = Db::open(&dir).unwrap();
    let (ids, _) = build(&mut db);
    // Generated ids follow the same sequence on both databases.
    assert_eq!(ids, mem_ids);
    let want = answers(&mut mem, &ids);
    assert_eq!(answers(&mut db, &ids), want, "live");

    // A rolled-back transaction that adds and removes edges changes nothing.
    {
        let mut tx = db.transaction().unwrap();
        tx.run(&format!(r#"append edge link "{}" -> "{}""#, ids[0], ids[29])).unwrap();
        tx.run(&format!(r#"delete edge link "{}" -> "{}""#, ids[4], ids[5])).unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(answers(&mut db, &ids), want, "after rollback");

    let mut replayed = Db::open(crash_image(&dir, "replay")).unwrap();
    assert_eq!(answers(&mut replayed, &ids), want, "log replay");
    drop(replayed);
    db.checkpoint().unwrap();
    db.close().unwrap();
    drop(db);
    let mut reopened = Db::open(&dir).unwrap();
    assert_eq!(answers(&mut reopened, &ids), want, "after checkpoint");
}
