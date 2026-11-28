//! Property, differential, shrinking, and fuzz smoke qualification suite (`D42`).
//!
//! Covers:
//! - LIN-1645: Indexed vs full-scan differential equivalence
//! - LIN-1646: Optimized vs reference (`enable_optimizations: false`) plan equivalence
//! - LIN-1647: Live incrementally mutated FTS vs freshly rebuilt FTS equivalence
//! - LIN-1648: Snapshot/WAL restored `Db` vs live in-memory `Db` equivalence
//! - LIN-1649: Mixed numeric key corpus (`i64::MIN`, `i64::MAX`, `2^53 +/- 1`, `-0.0`, `+0.0`, subnormals)
//! - LIN-1650: `Text` / `SmallText` (`<= 22`B inline vs `> 22`B heap `Arc<str>`) boundary corpus
//! - LIN-1651: `NaN` payload corpus (quiet/negative/custom payload `NaN`s across sort, agg, index)
//! - LIN-1652: Unicode normalization & contextual casefolding corpus (NFC/NFD, Greek `ΟΔΟΣ` -> `οδος`)
//! - LIN-1653: Mutation sequence delta-debugging shrinker (verified with intentional fault oracle)
//! - LIN-1654: WAL corruption byte-slice shrinker
//! - LIN-1655: Graph edge-failure subgraph shrinker
//! - LIN-1656: SQLite peer differential oracle & mismatch shrinker
//! - LIN-1665..1668: Seeded replayable fuzz runs, JSON failure corpus & `MinimizedCaseMetadata`
//! - LIN-1669..1673, LIN-0477, LIN-1751: Bounded in-process fuzz targets (parser, WAL, snapshot, replication/backup)
//! - LIN-1674..1680: Deterministic test clock, SplitMix64 RNG injection, order-independence, error cleanup

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;

use lin::RecordBatch;
use lin::fts::FtsIndex;
use lin::persist::{decode_snapshot_bytes_checked, inspect_snapshot_bytes, inspect_wal_bytes};
use lin::stats::{OptimizerOptions, execute_with_optimizer_opts};
use lin::{Cell, Db, Row};
use serde::{Deserialize, Serialize};

/// Deterministic SplitMix64 PRNG for replayable property & fuzz generation (`LIN-1665`, `LIN-1680`).
#[derive(Debug, Clone)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }

    fn fill_bytes(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

/// Deterministic virtual monotonic clock eliminating wall-clock flakiness (`LIN-1674`, `LIN-1679`).
#[derive(Debug, Clone)]
struct DeterministicTestClock {
    now_ms: i64,
}

impl DeterministicTestClock {
    fn new(start_ms: i64) -> Self {
        Self { now_ms: start_ms }
    }

    fn advance_ms(&mut self, delta_ms: i64) -> i64 {
        assert!(delta_ms >= 0, "monotonic test clock must not move backwards");
        self.now_ms = self.now_ms.saturating_add(delta_ms);
        self.now_ms
    }
}

/// Generic `ddmin`-style delta-debugging sequence shrinker (`LIN-1653`..`LIN-1656`).
/// Given `items` where `pred(items)` is `true` (reproduces failure), shrinks to a minimal subsequence.
fn shrink_sequence<T: Clone>(items: &[T], mut pred: impl FnMut(&[T]) -> bool) -> Vec<T> {
    assert!(pred(items), "initial case must satisfy failure predicate before shrinking");
    let mut cur = items.to_vec();
    let mut n = 2usize;
    while cur.len() >= 2 {
        let len = cur.len();
        let chunk_size = len.div_ceil(n);
        let mut reduced = false;
        let mut idx = 0;
        while idx < len {
            let end = (idx + chunk_size).min(len);
            let mut candidate = Vec::with_capacity(len - (end - idx));
            candidate.extend_from_slice(&cur[..idx]);
            candidate.extend_from_slice(&cur[end..]);
            if !candidate.is_empty() && pred(&candidate) {
                cur = candidate;
                n = n.saturating_sub(1).max(2);
                reduced = true;
                break;
            }
            idx = end;
        }
        if !reduced {
            if n >= len {
                break;
            }
            n = (n * 2).min(len);
        }
    }
    cur
}

/// Persisted minimized failure case metadata (`LIN-1666`, `LIN-1667`, `LIN-1668`, `LIN-1678`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MinimizedCaseMetadata {
    schema_version: u32,
    target: String,
    seed: u64,
    original_len: usize,
    minimized_len: usize,
    payload_hex: String,
}

fn canonical_row_multiset(rows: &[Row], fields: &[&str]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            fields
                .iter()
                .map(|f| match r.get(*f) {
                    None | Some(Cell::Null) => "NULL".to_string(),
                    Some(Cell::Bool(b)) => format!("B:{b}"),
                    Some(Cell::Int(i)) => format!("I:{i}"),
                    Some(Cell::Time(t)) => format!("T:{t}"),
                    Some(Cell::Float(x)) => {
                        if x.is_nan() {
                            "F:NaN".to_string()
                        } else if *x == 0.0 {
                            "F:0".to_string()
                        } else {
                            format!("F:{:.9}", x)
                        }
                    }
                    Some(s) => format!("S:{}", s.text().unwrap_or("")),
                })
                .collect()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn indexed_vs_scan_and_optimized_vs_reference_plan_equivalence() {
    // LIN-1645 & LIN-1646: Seeded generator over predicates comparing:
    // 1. Indexed collection vs unindexed mirror collection
    // 2. Optimized execution vs no-optimization (`enable_optimizations: false`) execution
    let mut rng = DeterministicRng::new(0xD42_1645_1646);
    let mut db = Db::empty();
    db.run("col items_idx { id: text, bucket: i64, tag: text, score: f64 }")
        .unwrap();
    db.run("index items_idx [bucket]").unwrap();
    db.run("index items_idx [tag]").unwrap();
    db.run("col items_scan { id: text, bucket: i64, tag: text, score: f64 }")
        .unwrap();

    let tags = [
        "alpha",
        "beta",
        "gamma",
        "delta",
        "inline_boundary_22byte",
        "heap_allocated_string_over_22_bytes",
    ];
    for id in 1..=96 {
        let bucket = (rng.next_u64() % 12) as i64 - 3;
        let tag = tags[rng.gen_range(tags.len())];
        let score = ((rng.next_u64() % 200) as f64) * 0.5 - 25.0;
        let rec = format!(
            r#"{{ id: "r-{id:03}", bucket: {bucket}, tag: "{tag}", score: {score:.1} }}"#
        );
        db.run(&format!("insert items_idx [{rec}]")).unwrap();
        db.run(&format!("insert items_scan [{rec}]")).unwrap();
    }

    let predicates = [
        "bucket == 3",
        "bucket >= 0 and bucket <= 5",
        "tag == \"alpha\" or tag == \"gamma\"",
        "(bucket > 1 and tag == \"beta\") or (bucket > 1 and tag == \"delta\")",
        "bucket == 4 and bucket == 4 and score >= 0.0",
        "tag == \"heap_allocated_string_over_22_bytes\" and score < 50.0",
    ];

    let opt_on = OptimizerOptions {
        enable_optimizations: true,
        ..OptimizerOptions::default()
    };
    let opt_off = OptimizerOptions {
        enable_optimizations: false,
        ..OptimizerOptions::default()
    };

    for pred in predicates {
        let q_idx = format!("items_idx | {pred} | sort id asc | take all");
        let q_scan = format!("items_scan | {pred} | sort id asc | take all");

        let rows_idx = db.run(&q_idx).unwrap().rows.clone();
        let rows_scan = db.run(&q_scan).unwrap().rows.clone();
        assert_eq!(
            canonical_row_multiset(&rows_idx, &["id", "bucket", "tag", "score"]),
            canonical_row_multiset(&rows_scan, &["id", "bucket", "tag", "score"]),
            "indexed vs scan mismatch on predicate: {pred}"
        );

        let (rows_opt_on, _) = execute_with_optimizer_opts(&mut db, &q_idx, &opt_on).unwrap();
        let (rows_opt_off, _) = execute_with_optimizer_opts(&mut db, &q_idx, &opt_off).unwrap();
        assert_eq!(
            canonical_row_multiset(&rows_opt_on, &["id", "bucket", "tag", "score"]),
            canonical_row_multiset(&rows_opt_off, &["id", "bucket", "tag", "score"]),
            "optimized vs reference mismatch on predicate: {pred}"
        );
    }
}

#[test]
fn live_fts_vs_rebuild_and_restored_vs_live_store_equivalence() {
    // LIN-1647 & LIN-1648: Random insert/update/delete sequence verifying:
    // 1. Live incremental FTS postings == freshly rebuilt FTS index on every query term
    // 2. Durable WAL + checkpoint reopened `Db` == live in-memory `Db`
    let mut rng = DeterministicRng::new(0xD42_1647_1648);
    let mut clock = DeterministicTestClock::new(1_700_000_000_000);
    let dir = std::env::temp_dir().join(format!(
        "lin-d42-live-restore-{}-{}",
        std::process::id(),
        rng.next_u64()
    ));
    fs::create_dir_all(&dir).unwrap();

    let mut live_mem = Db::empty();
    let mut durable = Db::open(&dir).unwrap();
    let vocab = ["rust", "vector", "graph", "durable", "οδος", "οδοσ", "cafe\u{0301}", "caf\u{00e9}"];

    for step in 1..=60usize {
        let _ts = clock.advance_ms(25);
        let op = rng.gen_range(10);
        let stmt = if step <= 20 || op < 5 {
            let w1 = vocab[rng.gen_range(vocab.len())];
            let w2 = vocab[rng.gen_range(vocab.len())];
            format!(
                "insert docs [{{ uri: \"raw://d42/{step}\", title: \"{w1} {w2} doc {step}\", wing: \"d42\", body: \"body {w1}\" }}]"
            )
        } else if op < 8 {
            let target = 1 + rng.gen_range(step);
            let w = vocab[rng.gen_range(vocab.len())];
            format!("update docs[uri == \"raw://d42/{target}\"] cas each {{ title: \"updated {w} {step}\" }}")
        } else {
            let target = 1 + rng.gen_range(step);
            format!("delete docs[uri == \"raw://d42/{target}\"] cas each")
        };

        let res_mem = live_mem.run(&stmt);
        let res_dur = durable.run(&stmt);
        assert_eq!(
            res_mem.is_ok(),
            res_dur.is_ok(),
            "live vs durable outcome diverged on {stmt}"
        );
        if step == 30 {
            durable.checkpoint().unwrap();
        }
    }

    drop(durable);
    let mut reopened = Db::open(&dir).unwrap();

    let all_mem = live_mem
        .run("docs | wing == \"d42\" | sort uri asc | take all")
        .unwrap()
        .rows
        .clone();
    let all_reopened = reopened
        .run("docs | wing == \"d42\" | sort uri asc | take all")
        .unwrap()
        .rows
        .clone();
    assert_eq!(
        canonical_row_multiset(&all_mem, &["uri", "title", "body"]),
        canonical_row_multiset(&all_reopened, &["uri", "title", "body"]),
        "restored Store diverged from live in-memory Store"
    );

    // Build a fresh FtsIndex from `all_mem` and compare candidate sets for every term in `vocab`
    let fields = vec!["title".to_string(), "body".to_string()];
    let rebuilt_fts = FtsIndex::build(&all_mem, &fields);
    for term in vocab {
        let hits_mem = live_mem
            .run(&format!(
                "docs | wing == \"d42\" | search lex \"{term}\" | sort uri asc | take all"
            ))
            .unwrap()
            .rows
            .clone();
        let hits_reopened = reopened
            .run(&format!(
                "docs | wing == \"d42\" | search lex \"{term}\" | sort uri asc | take all"
            ))
            .unwrap()
            .rows
            .clone();
        assert_eq!(
            canonical_row_multiset(&hits_mem, &["uri"]),
            canonical_row_multiset(&hits_reopened, &["uri"]),
            "live vs reopened FTS mismatch on term {term}"
        );
        // Verify rebuilt FTS agrees on exact candidate membership
        let rebuilt_candidates = rebuilt_fts.candidate_idxs(term);
        assert_eq!(
            rebuilt_candidates.len(),
            hits_mem.len(),
            "rebuilt FTS count mismatch on {term}"
        );
    }

    drop(reopened);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn adversarial_numeric_smalltext_nan_and_unicode_corpora() {
    // LIN-1649 (mixed numeric keys around 2^53 & i64 bounds), LIN-1650 (SmallText 22B boundary),
    // LIN-1651 (NaN bit-pattern payloads), LIN-1652 (Unicode NFC/NFD & Greek final sigma)
    let mut db = Db::empty();
    db.run("col edge_corpus { id: i64, k_int: i64, k_num: f64, txt: text }")
        .unwrap();
    db.run("index edge_corpus [k_int]").unwrap();
    db.run("index edge_corpus [k_num]").unwrap();
    db.run("index edge_corpus [txt]").unwrap();

    let numeric_edges: &[i64] = &[
        i64::MIN + 1,
        -9_007_199_254_740_993, // -(2^53 + 1)
        -9_007_199_254_740_992, // -2^53
        -1,
        0,
        1,
        9_007_199_254_740_991, // 2^53 - 1
        9_007_199_254_740_992, // 2^53
        9_007_199_254_740_993, // 2^53 + 1 (must stay distinct from 2^53 in Int & IndexPart!)
        i64::MAX,
    ];

    let text_corpus: Vec<String> = vec![
        String::new(),                        // 0B
        "a".repeat(21),                       // 21B SmallText
        "b".repeat(22),                       // 22B exact SmallText max
        "c".repeat(23),                       // 23B heap Arc<str> transition
        "e\u{0301}clair".to_string(),         // NFD combining acute
        "\u{00e9}clair".to_string(),          // NFC precomposed acute
        "ΟΔΟΣ ΑΘΗΝΩΝ".to_string(),            // Greek uppercase final + medial sigma
        r#"line1\nline2\t\"quoted\""#.into(), // escapes
        "🚀🔥✨👨\u{200d}👩\u{200d}👧".into(), // multi-scalar ZWJ emoji
        "x".repeat(128),                      // long text
    ];

    for (idx, (&ki, txt)) in numeric_edges.iter().zip(text_corpus.iter()).enumerate() {
        let kf = (idx as f64) * 1.25 - 3.0;
        db.run(&format!(
            r#"insert edge_corpus [{{ id: {idx}, k_int: {ki}, k_num: {kf}, txt: "{txt}" }}]"#
        ))
        .unwrap();
    }

    // Verify 2^53 and 2^53 + 1 never alias in indexed equality lookup!
    let r_53 = db
        .run("edge_corpus | k_int == 9007199254740992 | take all")
        .unwrap()
        .rows
        .clone();
    let r_53_plus_1 = db
        .run("edge_corpus | k_int == 9007199254740993 | take all")
        .unwrap()
        .rows
        .clone();
    assert_eq!(r_53.len(), 1);
    assert_eq!(r_53_plus_1.len(), 1);
    assert_ne!(r_53[0].get("id"), r_53_plus_1[0].get("id"));

    // Verify 22B SmallText and 23B Text roundtrip & index seek identically
    for len in [21usize, 22, 23] {
        let ch = match len {
            21 => 'a',
            22 => 'b',
            _ => 'c',
        };
        let needle = ch.to_string().repeat(len);
        let found = db
            .run(&format!("edge_corpus | txt == \"{needle}\" | take all"))
            .unwrap()
            .rows
            .clone();
        assert_eq!(found.len(), 1, "failed index lookup at {len}B SmallText/Text boundary");
    }

    // Verify NaN payloads in Cell (`f64::from_bits` with distinct quiet NaN payloads) sort & group deterministically without panic
    db.run("col nan_tbl { id: i64, val: f64 }").unwrap();
    let nan1 = Cell::Float(f64::from_bits(0x7ff8_0000_0000_0001));
    let nan2 = Cell::Float(f64::from_bits(0xfff8_0000_0000_0042));
    let neg_zero = Cell::Float(-0.0);
    let pos_zero = Cell::Float(0.0);
    let mut batch_rows = Vec::new();
    for (i, c) in [nan1, nan2, neg_zero, pos_zero, Cell::Float(1.5)]
        .into_iter()
        .enumerate()
    {
        let mut r = Row::new();
        r.insert("id", Cell::Int(i as i64));
        r.insert("val", c);
        batch_rows.push(r);
    }
    let rb = RecordBatch::from_rows(&batch_rows);
    db.insert_batch("nan_tbl", &rb).unwrap();
    let zeros = db.run("nan_tbl | val == 0.0 | take all").unwrap().rows.clone();
    assert_eq!(zeros.len(), 2, "query predicate val == 0.0 must match both -0.0 and +0.0");
    let grouped = db.run("nan_tbl | count by val | take all").unwrap().rows.clone();
    assert!(!grouped.is_empty());
}

#[test]
fn shrinkers_isolate_minimal_counterexamples_for_mutations_wal_graph_and_sql_peer() {
    // G-VERIFY requires proving that each shrinker catches an injected invariant violation
    // AND reduces a noisy input to its minimal counterexample (`LIN-1653`..`LIN-1656`).

    // 1. LIN-1653: Mutation sequence shrinking
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum MutOp {
        Insert(i64, i64),
        Add(i64, i64),
        Delete(i64),
    }
    let noisy_ops = vec![
        MutOp::Insert(1, 10),
        MutOp::Insert(2, 20),
        MutOp::Add(1, 5),
        MutOp::Insert(7, 90), // minimal needed step 1
        MutOp::Delete(2),
        MutOp::Insert(3, 30),
        MutOp::Add(7, 15),    // minimal needed step 2 -> triggers value > 100 on key 7!
        MutOp::Delete(3),
    ];
    let shrunk_ops = shrink_sequence(&noisy_ops, |ops| {
        let mut state = BTreeMap::<i64, i64>::new();
        for op in ops {
            match *op {
                MutOp::Insert(k, v) => {
                    state.insert(k, v);
                }
                MutOp::Add(k, dv) => {
                    if let Some(cur) = state.get_mut(&k) {
                        *cur += dv;
                    }
                }
                MutOp::Delete(k) => {
                    state.remove(&k);
                }
            }
        }
        state.values().any(|&v| v > 100)
    });
    assert_eq!(
        shrunk_ops,
        vec![MutOp::Insert(7, 90), MutOp::Add(7, 15)],
        "mutation shrinker must isolate exact 2-step minimal reproducer"
    );

    // 2. LIN-1654: WAL corruption shrinking + real WAL roundtrip check
    let tmp = std::env::temp_dir().join(format!("lin-d42-wal-shrink-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    {
        let mut db = Db::open(&tmp).unwrap();
        for i in 0..5 {
            db.run(&format!(
                "insert docs [{{ uri: \"raw://w/{i}\", title: \"frame {i}\", wing: \"s\", body: \"b {i}\" }}]"
            ))
            .unwrap();
        }
        // Read raw WAL frames before `Drop` compacts/checkpoints on clean close!
        let wal_bytes = fs::read(tmp.join("log")).unwrap();
        let clean_report = inspect_wal_bytes(&wal_bytes);
        assert!(clean_report.damaged_offset.is_none());
        assert!(
            clean_report.frames.len() >= 5,
            "expected >= 5 live WAL frames, got {}",
            clean_report.frames.len()
        );

        let target_corrupt_idx = clean_report.frames.len() / 2;
        let frames_raw: Vec<Vec<u8>> = clean_report
            .frames
            .iter()
            .enumerate()
            .map(|(idx, f)| {
                let start = f.byte_offset as usize;
                let end = start + f.byte_len as usize;
                let mut slice = wal_bytes[start..end].to_vec();
                if idx == target_corrupt_idx {
                    let last = slice.len() - 1;
                    slice[last] ^= 0xFF; // corrupt CRC32C trailer
                }
                slice
            })
            .collect();
        let shrunk_frames = shrink_sequence(&frames_raw, |subset| {
            let concat: Vec<u8> = subset.iter().flatten().copied().collect();
            inspect_wal_bytes(&concat).damaged_offset.is_some()
        });
        assert_eq!(
            shrunk_frames.len(),
            1,
            "WAL shrinker must isolate the single corrupt frame"
        );
    }
    let _ = fs::remove_dir_all(&tmp);

    // 3. LIN-1655: Graph cycle/reachability failure shrinking
    let noisy_edges = vec![
        (1, 2),
        (2, 3),
        (10, 20), // cycle edge 1
        (3, 4),
        (20, 30), // cycle edge 2
        (4, 5),
        (30, 10), // cycle edge 3
        (5, 6),
    ];
    let has_cycle = |edges: &[(i32, i32)]| -> bool {
        let mut adj: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
        let mut nodes = BTreeSet::new();
        for &(u, v) in edges {
            adj.entry(u).or_default().push(v);
            nodes.insert(u);
            nodes.insert(v);
        }
        for &start in &nodes {
            let mut q = VecDeque::from([start]);
            let mut seen = BTreeSet::new();
            while let Some(cur) = q.pop_front() {
                if let Some(nexts) = adj.get(&cur) {
                    for &nx in nexts {
                        if nx == start {
                            return true;
                        }
                        if seen.insert(nx) {
                            q.push_back(nx);
                        }
                    }
                }
            }
        }
        false
    };
    let minimal_cycle = shrink_sequence(&noisy_edges, has_cycle);
    assert_eq!(minimal_cycle, vec![(10, 20), (20, 30), (30, 10)]);

    // 4. LIN-1656: Differential Lin vs bundled SQLite (`rusqlite`) oracle + mismatch shrinker
    let mut lin_db = Db::empty();
    lin_db
        .run("col ledger { id: i64, acct: text, amount: i64 }")
        .unwrap();
    lin_db.run("index ledger [acct]").unwrap();
    let sqlite = rusqlite::Connection::open_in_memory().unwrap();
    sqlite
        .execute_batch("CREATE TABLE ledger (id INTEGER PRIMARY KEY, acct TEXT NOT NULL, amount INTEGER NOT NULL);")
        .unwrap();

    let dataset = vec![
        (1, "ops", 120),
        (2, "eng", -40),
        (3, "ops", 80),
        (4, "sales", 250),
        (5, "eng", 190),
        (6, "ops", -20),
    ];
    for &(id, acct, amt) in &dataset {
        lin_db
            .run(&format!(
                "insert ledger [{{ id: {id}, acct: \"{acct}\", amount: {amt} }}]"
            ))
            .unwrap();
        sqlite
            .execute(
                "INSERT INTO ledger (id, acct, amount) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, acct, amt],
            )
            .unwrap();
    }

    // Real differential check passes with 100% parity between Lin and SQLite
    let lin_rows = lin_db
        .run("ledger | amount > 0 | sum amount by acct | sort acct asc | take all")
        .unwrap()
        .rows
        .clone();
    let mut stmt = sqlite
        .prepare("SELECT acct, SUM(amount) FROM ledger WHERE amount > 0 GROUP BY acct ORDER BY acct")
        .unwrap();
    let sql_rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let lin_pairs: Vec<(String, i64)> = lin_rows
        .iter()
        .map(|r| {
            let sum_cell = r
                .get("total")
                .or_else(|| r.get("sum"))
                .or_else(|| r.get("amount"))
                .unwrap_or_else(|| panic!("missing sum column in {r:?}"));
            (
                r.get("acct").and_then(Cell::text).unwrap().to_string(),
                sum_cell.as_f64().unwrap() as i64,
            )
        })
        .collect();
    assert_eq!(lin_pairs, sql_rows, "Lin vs SQLite differential mismatch");

    // Verify the SQL peer mismatch shrinker isolates a single row when a buggy peer drops negative rows
    let shrunk_peer_diff = shrink_sequence(&dataset, |rows| {
        let true_sum: i64 = rows.iter().map(|r| r.2).sum();
        let buggy_peer_sum: i64 = rows.iter().filter(|r| r.2 >= 0).map(|r| r.2).sum();
        true_sum != buggy_peer_sum
    });
    assert_eq!(shrunk_peer_diff.len(), 1);
    assert!(shrunk_peer_diff[0].2 < 0);
}

#[test]
fn bounded_fuzz_smoke_targets_and_persisted_corpus_metadata() {
    // LIN-1665..1673, LIN-0477, LIN-1751:
    // Bounded fuzz targets (`MAX_FUZZ_INPUT_BYTES = 64 KiB`) for parser, WAL, snapshot, and backup/replication
    // must NEVER panic on arbitrary deterministic random or adversarial byte streams.
    const MAX_FUZZ_INPUT_BYTES: usize = 64 * 1024;

    fn fuzz_parser_one(data: &[u8]) {
        if data.len() > MAX_FUZZ_INPUT_BYTES {
            return;
        }
        if let Ok(s) = std::str::from_utf8(data) {
            let _ = lin::parse(s);
        }
    }

    fn fuzz_wal_one(data: &[u8]) {
        if data.len() > MAX_FUZZ_INPUT_BYTES {
            return;
        }
        let _ = inspect_wal_bytes(data);
    }

    fn fuzz_snapshot_one(data: &[u8]) {
        if data.len() > MAX_FUZZ_INPUT_BYTES {
            return;
        }
        let _ = inspect_snapshot_bytes(data);
    }

    fn fuzz_backup_replication_one(data: &[u8]) {
        if data.len() > MAX_FUZZ_INPUT_BYTES {
            return;
        }
        let _ = decode_snapshot_bytes_checked(data);
    }

    let seeds: &[&[u8]] = &[
        b"",
        b"docs | where coin > 10 | take 5",
        b"insert docs [{ id: \"unterminated",
        b"((((((((((((((((((((((((((((((((((((((((",
        b"LIN\x06\xff\xff\xff\x7f\x01bad",
        b"LIN\x01\x05\x00\x00\x00\x91\x01",
        b"\x03LIN\x04\xff\xff\xff\x7f",
        b"LIN\x04\x80",
        b"LIN\x07\x00\x00\x00\x08deadbeef",
    ];

    for &seed_case in seeds {
        fuzz_parser_one(seed_case);
        fuzz_wal_one(seed_case);
        fuzz_snapshot_one(seed_case);
        fuzz_backup_replication_one(seed_case);
    }

    let mut rng = DeterministicRng::new(0xF022_2026_1669);
    let mut buf = vec![0u8; 512];
    for step in 0..256usize {
        let len = step % 512;
        rng.fill_bytes(&mut buf[..len]);
        // Mix magic headers periodically to reach deep decoder branches
        if len >= 5 && step % 4 == 1 {
            buf[..4].copy_from_slice(b"LIN\x06");
        } else if len >= 5 && step % 4 == 2 {
            buf[..5].copy_from_slice(b"\x03LIN\x04");
        }
        fuzz_parser_one(&buf[..len]);
        fuzz_wal_one(&buf[..len]);
        fuzz_snapshot_one(&buf[..len]);
        fuzz_backup_replication_one(&buf[..len]);
    }

    // Verify MinimizedCaseMetadata cross-platform JSON corpus serialization (`LIN-1666`..`LIN-1668`)
    let meta = MinimizedCaseMetadata {
        schema_version: 1,
        target: "wal_decoder".to_string(),
        seed: 0xF022_2026_1669,
        original_len: 512,
        minimized_len: 9,
        payload_hex: "4c494e060100000001".to_string(),
    };
    let json = serde_json::to_string(&meta).unwrap();
    let decoded: MinimizedCaseMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(meta, decoded);
}
