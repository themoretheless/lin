//! Reverse traversal against a breadth-first oracle over the reversed edge list:
//! `hop backlink` (the declared reverse of `wikilink`) at depths 1 to 3 and
//! `match <-wikilink- b` reach exactly the nodes with edges leading to the start, through
//! cycles, without duplicates. The start node itself is never part of the result (a
//! self-loop does not return it), in either direction.

use std::collections::{BTreeSet, HashMap};

use lin::Db;

const N: usize = 24;

fn ids_of(db: &mut Db, q: &str) -> BTreeSet<String> {
    db.run(q).unwrap_or_else(|e| panic!("{q}: {e}")).rows.iter().map(|r| r.get("id").and_then(|c| c.text()).unwrap().to_string()).collect()
}

#[test]
fn reverse_traversal_matches_reversed_bfs() {
    let mut db = Db::empty();
    let docs = (0..N).map(|i| format!(r#"{{ uri: "raw://r/{i:02}", title: "R{i}", layer: "wiki" }}"#)).collect::<Vec<_>>().join(",");
    db.run(&format!("insert docs [{docs}]")).unwrap();
    let ids: Vec<String> = (0..N).map(|i| db.run(&format!(r#"docs | uri == "raw://r/{i:02}" | {{ id }}"#)).unwrap().rows[0].get("id").and_then(|c| c.text()).unwrap().to_string()).collect();
    let mut x: u64 = 0x1234_5678_9ABC_DEF1;
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for i in 0..N {
        for _ in 0..2 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            edges.push((i, (x % N as u64) as usize));
        }
    }
    edges.push((5, 5));
    for (a, b) in &edges {
        db.run(&format!(r#"append edge wikilink "{}" -> "{}""#, ids[*a], ids[*b])).unwrap();
    }
    let mut incoming: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    for (a, b) in &edges {
        incoming.entry(*b).or_default().insert(*a);
    }
    let reach = |start: usize, depth: usize| -> BTreeSet<String> {
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut frontier = vec![start];
        for _ in 0..depth {
            let mut next = Vec::new();
            for n in frontier {
                for &p in incoming.get(&n).into_iter().flatten() {
                    if seen.insert(p) {
                        next.push(p);
                    }
                }
            }
            frontier = next;
        }
        seen.into_iter().map(|i| ids[i].clone()).collect()
    };
    for start in [0usize, 5, 11, 23] {
        for depth in 1..=3usize {
            let got = ids_of(&mut db, &format!(r#"docs | id == "{}" | hop backlink depth={depth} | {{ id }} | take all"#, ids[start]));
            let mut want = reach(start, depth);
            want.remove(&ids[start]);
            assert!(!got.contains(&ids[start]), "hop backlink returned the start node");
            assert_eq!(got, want, "hop backlink from {start} depth {depth}");
        }
        let one: BTreeSet<String> = db.run(&format!(r#"docs | id == "{}" | match <-wikilink- b | {{ b.id }} | take all"#, ids[start])).unwrap().rows.iter()
            .map(|r| r.get("b.id").and_then(|c| c.text()).unwrap().to_string()).collect();
        let mut want = reach(start, 1);
        want.remove(&ids[start]);
        assert_eq!(one, want, "match <-wikilink- from {start}");
    }
}
