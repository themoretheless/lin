//! Exact vector search against an oracle: with a deterministic embedder, `search vec`
//! must return exactly the documents whose cosine with the query exceeds 0.01, ranked
//! by that cosine, with Top-K and skip/take as slices of the full ranking, equal scores
//! in a stable order, deleted documents gone, updated documents re-embedded, and an
//! empty corpus answering nothing.

use std::sync::Arc;

use lin::{Cell, Db, Embedder};

/// Vectors from the text: a seeded generator over 16 dimensions with negative
/// coordinates. Text containing "dup" maps to one shared vector (exact ties); text
/// containing "zero" maps to the zero vector.
struct Seeded;

impl Embedder for Seeded {
    fn id(&self) -> &str {
        "seeded/16"
    }
    fn dim(&self) -> usize {
        16
    }
    fn embed(&self, text: &str) -> Arc<[f32]> {
        seeded_vector(text, self.dim())
    }
}

fn seeded_vector(text: &str, dim: usize) -> Arc<[f32]> {
    if text.contains("zero") {
        return Arc::from(vec![0.0f32; dim]);
    }
    let key = if text.contains("dup") { "dup" } else { text };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    let v: Vec<f32> = (0..dim)
        .map(|_| {
            h ^= h << 13;
            h ^= h >> 7;
            h ^= h << 17;
            ((h % 2001) as f32 - 1000.0) / 1000.0
        })
        .collect();
    Arc::from(v)
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na.sqrt() * nb.sqrt()) }
}

fn load(db: &mut Db, n: usize) {
    for i in 0..n {
        let title = match i % 23 {
            0 => format!("dup {i}"),
            11 => format!("zero {i}"),
            _ => format!("doc number {i}"),
        };
        let layer = if i % 3 == 0 { "wiki" } else { "raw" };
        db.run(&format!(r#"insert docs {{ uri: "raw://v/{i:03}", title: "{title}", layer: "{layer}" }}"#)).unwrap();
    }
}

/// (uri, score) of every document scoring above the threshold, best first.
fn oracle(db: &mut Db, query: &str, filter: &str) -> Vec<(String, f64)> {
    let qv = Seeded.embed(query);
    let rows = db.run(&format!("docs {filter}| {{ uri, embedding }} | take all")).unwrap().rows.clone();
    let mut scored: Vec<(String, f64)> = rows
        .iter()
        .filter_map(|r| {
            let uri = r.get("uri").and_then(|c| c.text())?.to_string();
            let Some(Cell::Vec(v)) = r.get("embedding") else { return None };
            let s = cosine(&qv, v);
            (s > 0.01).then_some((uri, s))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored
}

fn search(db: &mut Db, q: &str) -> Vec<String> {
    db.run(q)
        .unwrap_or_else(|e| panic!("{q}: {e}"))
        .rows
        .iter()
        .map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string())
        .collect()
}

/// `got` ranks like `want`: the same documents, and scores never increasing in `got`'s
/// order (ties may come in either order between documents with equal scores).
fn assert_ranks_like(got: &[String], want: &[(String, f64)], what: &str) {
    let score = |u: &str| want.iter().find(|(w, _)| w == u).map(|(_, s)| *s);
    for u in got {
        assert!(score(u).is_some(), "{what}: {u} returned but scores at or below the threshold");
    }
    for w in got.windows(2) {
        let (a, b) = (score(&w[0]).unwrap(), score(&w[1]).unwrap());
        assert!(a >= b - 1e-6, "{what}: {} ({a}) ranked above {} ({b})", w[0], w[1]);
    }
    // The cut-off is a score boundary: nothing left out scores clearly above the last.
    if let Some(last) = got.last().and_then(|u| score(u)) {
        for (u, s) in want {
            if !got.contains(u) {
                assert!(*s <= last + 1e-6, "{what}: {u} ({s}) left out above the cut-off ({last})");
            }
        }
    }
}

#[test]
fn exact_vector_search_matches_the_oracle() {
    let mut db = Db::empty().with_embedder(Arc::new(Seeded));
    // An empty corpus answers nothing.
    assert!(search(&mut db, r#"docs | search vec "anything" | { uri } | take all"#).is_empty());
    load(&mut db, 230);
    for query in ["doc number 7", "dup", "something else entirely", "zero"] {
        let want = oracle(&mut db, query, "");
        let full = search(&mut db, &format!(r#"docs | search vec "{query}" | {{ uri }} | take all"#));
        assert_eq!(full.len(), want.len(), "{query}: result size");
        assert_ranks_like(&full, &want, query);
        for k in [1usize, 5, 20, 1000] {
            let top = search(&mut db, &format!(r#"docs | search vec "{query}" | {{ uri }} | take {k}"#));
            assert_eq!(top, full[..k.min(full.len())], "{query} | take {k}");
        }
        let page = search(&mut db, &format!(r#"docs | search vec "{query}" | {{ uri }} | skip 3 | take 4"#));
        assert_eq!(page, full[3.min(full.len())..7.min(full.len())], "{query} | skip 3 | take 4");
        // The same query twice ranks equal scores the same way.
        assert_eq!(search(&mut db, &format!(r#"docs | search vec "{query}" | {{ uri }} | take all"#)), full);
    }
    // The zero vector scores nothing and never appears.
    assert!(search(&mut db, r#"docs | search vec "zero" | { uri } | take all"#).is_empty());

    // Deletes and updates are reflected: deleted documents vanish, an updated title is
    // re-embedded.
    let embedding_of = |db: &mut Db, uri: &str| -> Vec<u32> {
        let rows = db.run(&format!(r#"docs | uri == "{uri}" | {{ embedding }}"#)).unwrap().rows.clone();
        match rows.first().and_then(|r| r.get("embedding")) {
            Some(Cell::Vec(v)) => v.iter().map(|x| x.to_bits()).collect(),
            other => panic!("{uri}: {other:?}"),
        }
    };
    let before = embedding_of(&mut db, "raw://v/050");
    db.run(r#"delete docs[uri == "raw://v/007" or uri == "raw://v/023"] cas each"#).unwrap();
    db.run(r#"update docs[uri == "raw://v/050"] cas each { title: "doc number 7" }"#).unwrap();
    assert_ne!(embedding_of(&mut db, "raw://v/050"), before, "the updated title was not re-embedded");
    let want = oracle(&mut db, "doc number 7", "");
    let full = search(&mut db, r#"docs | search vec "doc number 7" | { uri } | take all"#);
    assert_eq!(full.len(), want.len());
    assert_ranks_like(&full, &want, "after writes");
    assert!(!full.contains(&"raw://v/007".to_string()) && !full.contains(&"raw://v/023".to_string()));
}

/// Changing the text of a document re-embeds it in memory, on the cold tier and after
/// log replay, and leaves the embedding alone when the text does not change.
#[test]
fn text_updates_re_embed_on_every_tier() {
    let embedding_of = |db: &mut Db, uri: &str| -> Vec<u32> {
        let rows = db.run(&format!(r#"docs | uri == "{uri}" | {{ embedding }}"#)).unwrap().rows.clone();
        match rows.first().and_then(|r| r.get("embedding")) {
            Some(Cell::Vec(v)) => v.iter().map(|x| x.to_bits()).collect(),
            other => panic!("{uri}: {other:?}"),
        }
    };
    for cold in [false, true] {
        let dir = std::env::temp_dir().join(format!("lin-vec-reembed-{}-{cold}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let opts = || lin::OpenOpts { sync: lin::SyncMode::Normal, cold };
        {
            let mut db = Db::open_with(&dir, opts()).unwrap().with_embedder(Arc::new(Seeded));
            load(&mut db, 60);
            db.checkpoint().unwrap();
            db.close().unwrap();
        }
        let mut db = Db::open_with(&dir, opts()).unwrap().with_embedder(Arc::new(Seeded));
        if cold {
            assert!(db.store.is_disk_backed("docs"), "docs is not cold");
        }
        let before = embedding_of(&mut db, "raw://v/010");
        // A field outside the embedded text keeps the vector.
        db.run(r#"update docs[uri == "raw://v/010"] cas each { layer: "wiki" }"#).unwrap();
        assert_eq!(embedding_of(&mut db, "raw://v/010"), before, "cold={cold}: re-embedded without a text change");
        db.run(r#"update docs[uri == "raw://v/010"] cas each { title: "a completely new title" }"#).unwrap();
        let after = embedding_of(&mut db, "raw://v/010");
        assert_ne!(after, before, "cold={cold}: the new title was not embedded");
        let expected: Vec<u32> = {
            let rows = db.run(r#"docs | uri == "raw://v/010" | { title, body, snippet }"#).unwrap().rows.clone();
            let text: Vec<&str> = ["title", "body", "snippet"].iter().filter_map(|k| rows[0].get(*k).and_then(|c| c.text())).collect();
            Seeded.embed(&text.join(" ")).iter().map(|x| x.to_bits()).collect()
        };
        assert_eq!(after, expected, "cold={cold}: embedding does not match the new text");
        // A point update by id with a content hash takes the fast path; same rule.
        let row = db.run(r#"docs | uri == "raw://v/020" | { id, hash, embedding }"#).unwrap().rows[0].clone();
        let id = row.get("id").and_then(|c| c.text()).unwrap().to_string();
        let hash = row.get("hash").and_then(|c| c.text()).unwrap_or("").to_string();
        let point_before = embedding_of(&mut db, "raw://v/020");
        db.run(&format!(r#"update docs[id == "{id}"] cas "{hash}" {{ title: "point update title" }}"#)).unwrap();
        assert_ne!(embedding_of(&mut db, "raw://v/020"), point_before, "cold={cold}: point update kept the old vector");
        // The re-embedded vector is what a crash recovers.
        let image = std::env::temp_dir().join(format!("lin-vec-reembed-img-{}-{cold}", std::process::id()));
        let _ = std::fs::remove_dir_all(&image);
        std::fs::create_dir_all(&image).unwrap();
        fn copy(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap() {
                let e = e.unwrap();
                let dst = to.join(e.file_name());
                if e.file_type().unwrap().is_dir() {
                    copy(&e.path(), &dst);
                } else if e.file_name() != "LOCK" {
                    std::fs::copy(e.path(), dst).unwrap();
                }
            }
        }
        copy(&dir, &image);
        let mut recovered = Db::open_with(&image, opts()).unwrap().with_embedder(Arc::new(Seeded));
        assert_eq!(embedding_of(&mut recovered, "raw://v/010"), after, "cold={cold}: crash image");
    }
}

#[test]
fn filtered_hnsw_top_k_respects_filter_and_exact_fallback() {
    let mut db = Db::empty().with_embedder(Arc::new(Seeded));
    load(&mut db, 6200);
    let exact_query = r#"docs | layer == "wiki" | search vec "doc number 6" | take 10"#;
    let expected = search(&mut db, exact_query);
    db.build_hnsw("docs").unwrap();
    db.set_filtered_hnsw(true);
    let result = search(
        &mut db,
        r#"docs | layer == "raw" | search vec "doc number 7" | take 10"#,
    );
    assert_eq!(result.len(), 10);
    assert!(
        result
            .iter()
            .all(|uri| uri.rsplit('/').next().unwrap().parse::<usize>().unwrap() % 3 != 0)
    );
    assert_eq!(search(&mut db, exact_query), expected);
}

/// Run with `cargo test --release --test vector_qualification filtered_hnsw_benchmark -- --ignored --nocapture`.
#[test]
#[ignore = "manual latency and recall benchmark"]
fn filtered_hnsw_benchmark() {
    use std::collections::HashSet;
    use std::time::Instant;

    struct WideSeeded;
    impl Embedder for WideSeeded {
        fn id(&self) -> &str {
            "seeded/384"
        }
        fn dim(&self) -> usize {
            384
        }
        fn embed(&self, text: &str) -> Arc<[f32]> {
            seeded_vector(text, self.dim())
        }
    }
    let mut exact = Db::empty().with_embedder(Arc::new(WideSeeded));
    let mut adaptive = Db::empty().with_embedder(Arc::new(WideSeeded));
    for i in 0..12000 {
        let tag = match i % 100 {
            0 => "one ten fifty",
            1..=9 => "ten fifty",
            10..=49 => "fifty",
            _ => "rest",
        };
        let insert = format!(
            r#"insert docs {{ uri: "raw://bench/{i}", title: "item {i}", layer: "raw", body: "{tag}" }}"#
        );
        exact.run(&insert).unwrap();
        adaptive.run(&insert).unwrap();
    }
    let queries: Vec<_> = ["one", "ten", "fifty"]
        .into_iter()
        .map(|tag| {
            (0..200)
                .map(|i| {
                    format!(
                        r#"docs | body has "{tag}" | search vec "item {}" | take 10"#,
                        i * 23,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    adaptive.build_hnsw("docs").unwrap();
    adaptive.set_filtered_hnsw(true);
    for (selectivity, qs) in [1, 10, 50].into_iter().zip(&queries) {
        for q in qs {
            drop(exact.run(q).unwrap());
            drop(adaptive.run(q).unwrap());
        }
        let sample = |db: &mut Db, q: &str, times: &mut Vec<u128>| {
            let start = Instant::now();
            let rows = search(db, q);
            times.push(start.elapsed().as_micros());
            rows
        };
        let mut exact_times = Vec::new();
        let mut adaptive_times = Vec::new();
        let mut found = 0;
        let mut wanted = 0;
        for (i, q) in qs.iter().enumerate() {
            let (expected, actual) = if i % 2 == 0 {
                (
                    sample(&mut exact, q, &mut exact_times),
                    sample(&mut adaptive, q, &mut adaptive_times),
                )
            } else {
                let actual = sample(&mut adaptive, q, &mut adaptive_times);
                (sample(&mut exact, q, &mut exact_times), actual)
            };
            let expected: HashSet<_> = expected.iter().collect();
            found += actual.iter().filter(|uri| expected.contains(uri)).count();
            wanted += expected.len();
        }
        exact_times.sort_unstable();
        adaptive_times.sort_unstable();
        let p99_idx = qs.len() * 99 / 100 - 1;
        assert!(wanted > 0 && found > 0);
        println!(
            "{selectivity}%: exact p99 {} µs, adaptive p99 {} µs, recall@10 {:.3}",
            exact_times[p99_idx],
            adaptive_times[p99_idx],
            found as f64 / wanted as f64
        );
    }
}
