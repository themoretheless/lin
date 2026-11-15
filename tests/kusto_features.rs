//! Integration tests for Kusto / ADX V3 features in Lin:
//! - multi-metric & multi-key `summarize`, `bin(...)`, `dcount` (exact -> HLL), `percentile` (exact -> t-digest),
//!   and scalar / whole-row `arg_max` & `arg_min` across hot and cold tiers;
//! - excluding projection `without { ... }` preserving row sparsity and skipping cold-tier columns;
//! - fluent `Queryable` / `BoundQueryable` APIs and `explain` plans;
//! - cost-based predicate flattening & reordering equivalence (including deep chains) across hot and cold tiers.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lin::{
    AggFunc, AggSpec, BinStep, Cell, Db, Field, GroupKey, OpenOpts, Queryable, SyncMode,
};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cold_opts() -> OpenOpts {
    OpenOpts {
        sync: SyncMode::Normal,
        cold: true,
    }
}

fn canonical_row_key(row: &lin::Row) -> String {
    row.iter()
        .map(|(k, c)| match (c.text(), c.as_f64()) {
            (Some(t), _) => format!("{k}={t}"),
            (None, Some(n)) => format!("{k}={:.6}", n),
            _ => format!("{k}={c:?}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn summarize_multi_metric_multi_key_hot_and_cold_parity() {
    let dir = TempDir::new("lin-kusto-sum");

    let ddl = "col ev { id: text, region: text, status: text, user_id: text, qty: i64, total: f64, ts: time }";
    let seed = |db: &mut Db| {
        db.run(ddl).unwrap();
        // Insert >= 40 rows so the cold engine persists the table to disk (>= COLD_MIN_ROWS = 32),
        // then query `region != "filler"` (and unfiltered multi-group) across both hot and cold.
        let mut ins = String::from(
            r#"insert ev [
                { id: "e1", region: "us", status: "ok", user_id: "u1", qty: 2, total: 10.0, ts: timestamp(1000) },
                { id: "e2", region: "us", status: "ok", user_id: "u2", qty: 5, total: 30.0, ts: timestamp(2000) },
                { id: "e3", region: "us", status: "ok", user_id: "u1", qty: 3, total: 20.0, ts: timestamp(3000) },
                { id: "e4", region: "us", status: "err", user_id: "u3", qty: 4, total: 40.0, ts: timestamp(3600000) },
                { id: "e5", region: "eu", status: "ok", user_id: "u4", qty: 7, total: 50.0, ts: timestamp(100) },
                { id: "e6", region: "eu", status: "ok", qty: 1, total: 10.0, ts: timestamp(50) }"#,
        );
        for i in 0..34usize {
            ins.push_str(&format!(
                r#", {{ id: "pad-{i:02}", region: "filler", status: "ok", user_id: "upad", qty: 0, total: 0.0, ts: timestamp(0) }}"#
            ));
        }
        ins.push_str("\n]");
        db.run(&ins).unwrap();
    };

    let mut hot = Db::empty();
    seed(&mut hot);

    {
        let mut init = Db::open_with(dir.path(), cold_opts()).unwrap();
        seed(&mut init);
        init.checkpoint().unwrap();
        init.close().unwrap();
    }
    let mut cold = Db::open_with(dir.path(), cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("ev"));

    let q_braced = r#"
        ev
        | region != "filler"
        | summarize {
            n: count,
            q: sum qty,
            rev: sum total,
            avg_rev: avg total,
            lo: min total,
            hi: max total,
            uniq_u: dcount user_id,
            p50: percentile(total, 50),
            p95: percentile(total, 95),
            latest_id: arg_max(ts, id),
            earliest_id: arg_min(ts, id)
        } by region, status
        | sort rev desc
    "#;

    let hot_h = hot.run(q_braced).unwrap();
    let cold_h = cold.run(q_braced).unwrap();
    let hot_rows = &hot_h.rows;
    let cold_rows = &cold_h.rows;
    assert_eq!(hot_rows.len(), 3);
    assert_eq!(cold_rows.len(), 3);

    // Check exact values on ("us", "ok") -> n=3, q=Int(10), rev=Float(60.0), avg=20.0, lo=10.0, hi=30.0, uniq_u=2, latest=e3, earliest=e1.
    let us_ok = hot_rows
        .iter()
        .find(|r| {
            r.get("region").and_then(Cell::text) == Some("us")
                && r.get("status").and_then(Cell::text) == Some("ok")
        })
        .unwrap();
    assert_eq!(us_ok.get("n"), Some(&Cell::Int(3)));
    assert_eq!(us_ok.get("q"), Some(&Cell::Int(10)));
    assert_eq!(us_ok.get("rev"), Some(&Cell::Float(60.0)));
    assert_eq!(us_ok.get("avg_rev"), Some(&Cell::Float(20.0)));
    assert_eq!(us_ok.get("lo"), Some(&Cell::Float(10.0)));
    assert_eq!(us_ok.get("hi"), Some(&Cell::Float(30.0)));
    assert_eq!(us_ok.get("uniq_u"), Some(&Cell::Int(2)));
    assert_eq!(us_ok.get("p50"), Some(&Cell::Float(20.0)));
    assert!((us_ok.get("p95").unwrap().as_f64().unwrap() - 29.0).abs() < 1e-9);
    assert_eq!(us_ok.get("latest_id").and_then(Cell::text), Some("e3"));
    assert_eq!(us_ok.get("earliest_id").and_then(Cell::text), Some("e1"));

    // e6 has null user_id: verify dcount ignores nulls on ("eu", "ok") -> 1 distinct user_id ("u4").
    let eu_ok = hot_rows
        .iter()
        .find(|r| r.get("region").and_then(Cell::text) == Some("eu"))
        .unwrap();
    assert_eq!(eu_ok.get("uniq_u"), Some(&Cell::Int(1)));
    assert_eq!(eu_ok.get("earliest_id").and_then(Cell::text), Some("e6"));
    assert_eq!(eu_ok.get("latest_id").and_then(Cell::text), Some("e5"));

    // Verify cold matches hot identically.
    let mut h_keys: Vec<String> = hot_rows.iter().map(canonical_row_key).collect();
    let mut c_keys: Vec<String> = cold_rows.iter().map(canonical_row_key).collect();
    h_keys.sort();
    c_keys.sort();
    assert_eq!(h_keys, c_keys);
    assert!(cold.store.is_disk_backed("ev"));

    // Brace-less syntax + [by ...] + skip + take (including unfiltered multi-group on cold DiskEngine).
    let q_braceless = r#"ev | region != "filler" | summarize count, sum qty by [region, status] | sort total desc | skip 1 | take 1"#;
    let h_page_h = hot.run(q_braceless).unwrap();
    let c_page_h = cold.run(q_braceless).unwrap();
    assert_eq!(h_page_h.rows.len(), 1);
    assert_eq!(c_page_h.rows.len(), 1);
    assert_eq!(h_page_h.rows[0].get("total"), Some(&Cell::Int(8)));
    assert_eq!(c_page_h.rows[0].get("total"), Some(&Cell::Int(8)));
    assert_eq!(h_page_h.rows[0].get("hits"), Some(&Cell::Int(2)));

    // Unfiltered cold `agg_summarize_disk` path matches hot `agg_summarize`.
    let q_unfiltered = "ev | summarize count, sum qty by [region, status] | sort region asc";
    let uh = hot.run(q_unfiltered).unwrap();
    let uc = cold.run(q_unfiltered).unwrap();
    let mut uh_keys: Vec<String> = uh.rows.iter().map(canonical_row_key).collect();
    let mut uc_keys: Vec<String> = uc.rows.iter().map(canonical_row_key).collect();
    uh_keys.sort();
    uc_keys.sort();
    assert_eq!(uh_keys, uc_keys);
    assert_eq!(uh.rows.len(), 4);
}

#[test]
fn summarize_empty_input_semantics() {
    let mut db = Db::empty();
    db.run("col ev { region: text, user_id: text, qty: i64, total: f64 }")
        .unwrap();

    // Ungrouped summarize on 0 matching rows returns 1 identity row.
    let ungrouped_h = db
        .run(
            r#"ev | region == "missing" | summarize { count, dcount user_id, q: sum qty, t: sum total, lo: min total, p50: percentile(total, 50) }"#,
        )
        .unwrap();
    let ungrouped = &ungrouped_h.rows;
    assert_eq!(ungrouped.len(), 1);
    assert_eq!(ungrouped[0].get("hits"), Some(&Cell::Int(0)));
    assert_eq!(ungrouped[0].get("dcount"), Some(&Cell::Int(0)));
    assert_eq!(ungrouped[0].get("q"), Some(&Cell::Int(0)));
    assert_eq!(ungrouped[0].get("t"), Some(&Cell::Float(0.0)));
    assert_eq!(ungrouped[0].get("lo"), Some(&Cell::Null));
    assert_eq!(ungrouped[0].get("p50"), Some(&Cell::Null));

    // Grouped summarize on 0 matching rows returns 0 rows.
    let grouped_h = db
        .run(r#"ev | region == "missing" | summarize { count } by region"#)
        .unwrap();
    assert!(grouped_h.rows.is_empty());
}

#[test]
fn dcount_and_percentile_exact_to_sketch_transitions() {
    let mut db = Db::empty();
    db.run("col metrics { id: text, tag: text, v: f64 }")
        .unwrap();

    // Batch 1 (<= 512 distinct): verify exact dcount (mixing SmallText and >22-byte Text) and exact percentile.
    let mut recs = String::from("insert metrics [\n");
    for i in 0..400usize {
        if i > 0 {
            recs.push(',');
        }
        let val = i % 100;
        let tag = if val % 2 == 0 {
            format!("short_{val}")
        } else {
            format!("long_category_label_exceeding_22_bytes_{val}")
        };
        if i % 17 == 0 {
            recs.push_str(&format!(r#"{{ id: "m{i}", v: {i}.0 }}"#));
        } else {
            recs.push_str(&format!(r#"{{ id: "m{i}", tag: "{tag}", v: {i}.0 }}"#));
        }
    }
    recs.push_str("\n]");
    db.run(&recs).unwrap();

    let small_h = db
        .run("metrics | summarize { dc: dcount tag, p50: percentile(v, 50), p90: percentile(v, 90) }")
        .unwrap();
    let small = &small_h.rows;
    // 100 distinct labels total (none omitted: 0..100 all hit at non-multiples of 20 across 400 rows).
    assert_eq!(small[0].get("dc"), Some(&Cell::Int(100)));
    let p50 = small[0].get("p50").unwrap().as_f64().unwrap();
    let p90 = small[0].get("p90").unwrap().as_f64().unwrap();
    assert!((p50 - 199.5).abs() < 1e-9, "exact p50 got {p50}");
    assert!((p90 - 359.1).abs() < 1e-6, "exact p90 got {p90}");

    // Batch 2 (5,000 total rows, ~2,500 distinct long tags): triggers exact -> HLL and exact -> TDigest.
    let mut more = String::from("insert metrics [\n");
    for i in 400..5_000usize {
        if i > 400 {
            more.push(',');
        }
        let uniq = i % 2_500;
        let tag = format!("hll_distinct_bucket_identifier_0000_{uniq:04}");
        more.push_str(&format!(r#"{{ id: "m{i}", tag: "{tag}", v: {i}.0 }}"#));
    }
    more.push_str("\n]");
    db.run(&more).unwrap();

    let large_h = db
        .run("metrics | summarize { dc: dcount tag, p0: percentile(v, 0), p50: percentile(v, 50), p95: percentile(v, 95), p100: percentile(v, 100) }")
        .unwrap();
    let large = &large_h.rows;
    let dc = large[0].get("dc").unwrap().as_i64().unwrap() as f64;
    // Total distinct = 100 (from batch 1) + 2,500 (hll_* buckets) = 2,600.
    assert!(
        (dc - 2_600.0).abs() / 2_600.0 <= 0.05,
        "HLL p=12 estimate out of tolerance: {dc}"
    );
    assert_eq!(large[0].get("p0"), Some(&Cell::Float(0.0)));
    assert_eq!(large[0].get("p100"), Some(&Cell::Float(4999.0)));
    let lp50 = large[0].get("p50").unwrap().as_f64().unwrap();
    let lp95 = large[0].get("p95").unwrap().as_f64().unwrap();
    assert!((lp50 - 2499.5).abs() / 5000.0 <= 0.02, "TDigest p50 got {lp50}");
    assert!((lp95 - 4749.5).abs() / 5000.0 <= 0.02, "TDigest p95 got {lp95}");
}

#[test]
fn bin_bucketing_time_int_float_and_validation() {
    let mut db = Db::empty();
    db.run("col pts { id: text, ts: time, score: i64, val: f64 }")
        .unwrap();
    db.run(
        r#"insert pts [
            { id: "p1", ts: timestamp(-1), score: -1, val: -0.0 },
            { id: "p2", ts: timestamp(-3600000), score: -10, val: 0.25 },
            { id: "p3", ts: timestamp(0), score: 0, val: 0.5 },
            { id: "p4", ts: timestamp(3599999), score: 9, val: 0.75 },
            { id: "p5", ts: timestamp(3600000), score: 10, val: 1.25 }
        ]"#,
    )
    .unwrap();

    // Time binning uses Euclidean division: timestamp(-1) -> -3_600_000, sharing bucket with p2.
    let t_bins_h = db
        .run("pts | count by bin(ts, 1h) | sort ts asc")
        .unwrap();
    let t_bins = &t_bins_h.rows;
    assert_eq!(t_bins.len(), 3);
    assert_eq!(t_bins[0].get("ts"), Some(&Cell::Time(-3_600_000)));
    assert_eq!(t_bins[0].get("hits"), Some(&Cell::Int(2)));
    assert_eq!(t_bins[1].get("ts"), Some(&Cell::Time(0)));
    assert_eq!(t_bins[1].get("hits"), Some(&Cell::Int(2)));
    assert_eq!(t_bins[2].get("ts"), Some(&Cell::Time(3_600_000)));
    assert_eq!(t_bins[2].get("hits"), Some(&Cell::Int(1)));

    // Int & float binning (including `-0.0` normalized to `0.0`, no duplicate `0.0` group).
    let n_bins_h = db
        .run("pts | summarize { n: count } by sb: bin(score, 10), vb: bin(val, 0.5) | sort sb asc")
        .unwrap();
    let n_bins = &n_bins_h.rows;
    assert_eq!(n_bins[0].get("sb"), Some(&Cell::Int(-10)));
    assert_eq!(n_bins[0].get("vb"), Some(&Cell::Float(0.0)));
    assert_eq!(n_bins[0].get("n"), Some(&Cell::Int(2)));
    // Confirm there is NO `-0.0` bit pattern anywhere in float bin keys.
    for row in n_bins {
        if let Some(&Cell::Float(f)) = row.get("vb") {
            assert!(!f.is_sign_negative() || f != 0.0);
        }
    }

    // Legacy aggregate shorthands with multi-key / bin lower to Summarize cleanly.
    let multi_h = db
        .run("pts | sum val by score, bin(ts, 1h)")
        .unwrap();
    assert_eq!(multi_h.rows.len(), 5);

    // Negative validation cases.
    for bad in [
        "pts | count by bin(ts, 0s)",
        "pts | count by bin(score, -5)",
        "pts | count by bin(id, 10)",
    ] {
        assert!(db.run(bad).is_err(), "expected error for `{bad}`");
    }
}

#[test]
fn arg_extremum_whole_row_tie_break_and_nulls() {
    let mut db = Db::empty();
    db.run("col jobs { id: text, svc: text, ts: time, msg: text }")
        .unwrap();
    db.run(
        r#"insert jobs [
            { id: "j1", svc: "auth", msg: "null ts first" },
            { id: "j2", svc: "auth", ts: timestamp(100), msg: "t100 first" },
            { id: "j3", svc: "auth", ts: timestamp(100), msg: "t100 tie" },
            { id: "j4", svc: "auth", ts: timestamp(50), msg: "t50 min" },
            { id: "j5", svc: "bill", msg: "only null 1" },
            { id: "j6", svc: "bill", msg: "only null 2" }
        ]"#,
    )
    .unwrap();

    // Grouped arg_max * (both shorthand and parenthesized `arg_max(ts, *)`).
    for q in [
        "jobs | arg_max ts by svc | sort id asc",
        "jobs | arg_max(ts, *) by svc | sort id asc",
    ] {
        let h = db.run(q).unwrap();
        let rows = &h.rows;
        assert_eq!(rows.len(), 2);
        // For "auth": j2 wins over j3 on tie (first seen wins) and over j1 (null loses).
        let auth = rows
            .iter()
            .find(|r| r.get("svc").and_then(Cell::text) == Some("auth"))
            .unwrap();
        assert_eq!(auth.get("id").and_then(Cell::text), Some("j2"));
        assert_eq!(auth.get("msg").and_then(Cell::text), Some("t100 first"));
        // For "bill": all ts are null -> first row j5 wins.
        let bill = rows
            .iter()
            .find(|r| r.get("svc").and_then(Cell::text) == Some("bill"))
            .unwrap();
        assert_eq!(bill.get("id").and_then(Cell::text), Some("j5"));
    }

    // Grouped arg_min * -> "auth" picks j4 (t50 beats t100 and null beats nothing).
    let mins_h = db.run("jobs | arg_min ts by svc").unwrap();
    let auth_min = mins_h
        .rows
        .iter()
        .find(|r| r.get("svc").and_then(Cell::text) == Some("auth"))
        .unwrap();
    assert_eq!(auth_min.get("id").and_then(Cell::text), Some("j4"));

    // Ungrouped arg_max *.
    let top_h = db.run("jobs | arg_max ts").unwrap();
    assert_eq!(top_h.rows.len(), 1);
    assert_eq!(top_h.rows[0].get("id").and_then(Cell::text), Some("j2"));
}

#[test]
fn without_projection_sparsity_join_cold_and_validation() {
    let dir = TempDir::new("lin-kusto-without");

    // 1. In-memory built-in `docs`: braced & brace-less `without`.
    let mut fix = Db::fixture();
    for q in [
        "docs | without { body, embedding } | sort title asc",
        "docs | without body, embedding | sort title asc",
    ] {
        let h = fix.run(q).unwrap();
        assert_eq!(h.rows.len(), 3);
        for r in &h.rows {
            assert!(r.get("id").is_some());
            assert!(r.get("title").is_some());
            assert!(r.get("body").is_none());
            assert!(r.get("embedding").is_none());
        }
    }

    // 2. Sparsity preservation + `join` + `without`.
    let mut sp = Db::empty();
    sp.run("col notes { id: text, title: text, opt_note: text, body: text }")
        .unwrap();
    sp.run(
        r#"insert notes [
            { id: "n1", title: "Alpha", opt_note: "present", body: "heavy" },
            { id: "n2", title: "Beta", body: "heavy2" }
        ]"#,
    )
    .unwrap();
    let sparse_h = sp.run("notes | without body | sort id asc").unwrap();
    assert_eq!(
        sparse_h.rows[0].get("opt_note").and_then(Cell::text),
        Some("present")
    );
    assert!(
        sparse_h.rows[1].get("opt_note").is_none(),
        "without must preserve sparsity (no synthesized Null)"
    );

    let joined_h = fix
        .run("orders | join users on user_id | without user_id")
        .unwrap();
    assert!(!joined_h.rows.is_empty());
    assert!(joined_h.rows[0].get("user_id").is_none());
    assert!(joined_h.rows[0].get("users.email").is_some());

    // 3. Negative checks.
    for bad in [
        "docs | without {}",
        "docs | without nope",
        "orders | join users on user_id | without users.email",
        "notes | without { id, title, opt_note, body }",
    ] {
        assert!(
            sp.run(bad).is_err() && fix.run(bad).is_err(),
            "expected error for `{bad}`"
        );
    }

    // 4. Cold-tier `DiskEngine` column exclusion + memtable overlay + trailing sort/take (using `docs` with auto-generated `embedding`).
    {
        let mut init = Db::open_with(dir.path(), cold_opts()).unwrap();
        let mut ins = String::from(
            r#"insert docs [
                { uri: "wiki://k0", title: "Zeta", wing: "rag", layer: "wiki", room: "k1", body: "huge body 1" },
                { uri: "wiki://k1", title: "Alpha", wing: "rag", layer: "wiki", body: "huge body 2" },
                { uri: "wiki://k2", title: "Gamma", wing: "rag", layer: "wiki", body: "huge body 3" }"#,
        );
        for i in 0..37usize {
            ins.push_str(&format!(
                r#", {{ uri: "wiki://pad/{i:02}", title: "Z-pad-{i:02}", wing: "rag", layer: "wiki", body: "filler {i}" }}"#
            ));
        }
        ins.push_str("\n]");
        init.run(&ins).unwrap();
        init.checkpoint().unwrap();
        init.close().unwrap();
    }
    let mut cold = Db::open_with(dir.path(), cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("docs"));
    // Uncheckpointed overlay insert: memtable rows must also be stripped by `without`.
    cold.run(
        r#"insert docs { uri: "wiki://overlay", title: "Beta", wing: "rag", layer: "wiki", body: "overlay body" }"#,
    )
    .unwrap();
    assert!(cold.store.is_disk_backed("docs"));

    let cold_h = cold
        .run("docs | without { body, embedding } | sort title asc | take 3")
        .unwrap();
    let cold_rows = &cold_h.rows;
    assert_eq!(cold_rows.len(), 3);
    let titles: Vec<_> = cold_rows
        .iter()
        .map(|r| r.get("title").and_then(Cell::text).unwrap())
        .collect();
    assert_eq!(titles, vec!["Alpha", "Beta", "Gamma"]);
    for r in cold_rows {
        assert!(r.get("body").is_none());
        assert!(r.get("embedding").is_none());
    }
    assert!(cold_rows[0].get("room").is_none());
    assert!(cold.store.is_disk_backed("docs"));
}

#[test]
fn fluent_queryable_and_explain_nodes() {
    let mut db = Db::empty();
    db.run("col ev { id: text, region: text, user_id: text, total: f64, ts: time, body: text }")
        .unwrap();
    db.run(
        r#"insert ev [
            { id: "a", region: "us", user_id: "u1", total: 10.0, ts: timestamp(100), body: "x" },
            { id: "b", region: "us", user_id: "u1", total: 30.0, ts: timestamp(200), body: "y" },
            { id: "c", region: "eu", user_id: "u2", total: 20.0, ts: timestamp(300), body: "z" }
        ]"#,
    )
    .unwrap();

    // Queryable + BoundQueryable parity.
    let wo_q = Queryable::from("ev")
        .without(["body"])
        .sort("id", true)
        .to_vec(&mut db)
        .unwrap();
    let wo_b = db
        .from("ev")
        .without(["body"])
        .sort("id", true)
        .to_vec()
        .unwrap();
    assert_eq!(wo_q.len(), 3);
    assert_eq!(wo_b.len(), 3);
    assert!(wo_q[0].get("body").is_none());

    assert_eq!(db.from("ev").dcount("user_id").scalar().unwrap(), Cell::Int(2));
    assert_eq!(
        Queryable::from("ev")
            .dcount_by("user_id", "region")
            .to_vec(&mut db)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.from("ev").percentile("total", 50.0).scalar().unwrap(),
        Cell::Float(20.0)
    );
    assert_eq!(
        db.from("ev")
            .percentile_by("total", 50.0, "region")
            .to_vec()
            .unwrap()
            .len(),
        2
    );

    let max_row = db.from("ev").arg_max("ts").first().unwrap();
    assert_eq!(max_row.get("id").and_then(Cell::text), Some("c"));
    let min_by = db.from("ev").arg_min_by("ts", "region").to_vec().unwrap();
    assert_eq!(min_by.len(), 2);
    let max_by = Queryable::from("ev")
        .arg_max_by("ts", "region")
        .to_vec(&mut db)
        .unwrap();
    assert_eq!(max_by.len(), 2);

    let sum_rows = db
        .from("ev")
        .summarize(
            vec![
                AggSpec {
                    alias: "n".into(),
                    func: AggFunc::Count,
                },
                AggSpec {
                    alias: "uniq".into(),
                    func: AggFunc::DCount(Field::name("user_id")),
                },
            ],
            vec![GroupKey::bin(
                "hr",
                Field::name("ts"),
                BinStep::Duration(3_600_000),
            )],
        )
        .to_vec()
        .unwrap();
    assert_eq!(sum_rows.len(), 1);
    assert_eq!(sum_rows[0].get("n"), Some(&Cell::Int(3)));
    assert_eq!(sum_rows[0].get("uniq"), Some(&Cell::Int(2)));

    // Explain formatting checks.
    let ex_wo = db
        .explain_as("ev | without { body }", None)
        .unwrap();
    assert!(ex_wo.contains("Without [body]"), "got `{ex_wo}`");

    let ex_sum = db
        .explain_as(
            "ev | summarize { n: count, d: dcount user_id } by bin(ts, 1h)",
            None,
        )
        .unwrap();
    assert!(ex_sum.contains("Agg summarize("), "got `{ex_sum}`");
    assert!(ex_sum.contains("bin(ts, 3600000ms)"), "got `{ex_sum}`");

    let ex_arg = db
        .explain_as("ev | arg_max ts by region", None)
        .unwrap();
    assert!(ex_arg.contains("Agg arg_max(ts) by region"), "got `{ex_arg}`");

    // Batch & cursor paths.
    let batch_wo = db.run_batch("ev | without { body } | sort id asc").unwrap();
    assert_eq!(batch_wo.n(), 3);
    assert!(!batch_wo.names.iter().any(|n| n == "body"));

    let batch_sum = db
        .run_batch("ev | summarize { n: count, d: dcount user_id } by region")
        .unwrap();
    assert_eq!(batch_sum.n(), 2);

    let cur_wo_rows: Vec<_> = db
        .from("ev")
        .without(["body"])
        .sort("id", true)
        .cursor()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(cur_wo_rows.len(), 3);
    assert!(cur_wo_rows[0].get("body").is_none());

    let cur_arg_rows: Vec<_> = db
        .from("ev")
        .arg_max_by("ts", "region")
        .cursor()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(cur_arg_rows.len(), 2);
}

#[test]
fn cost_reordering_equivalence_and_deep_chain_hot_and_cold() {
    let dir = TempDir::new("lin-kusto-pred");
    let ddl = "col logs { id: text, kind: text, tag: text, n: i64, body: text }";

    let seed = |db: &mut Db| {
        db.run(ddl).unwrap();
        let mut ins = String::from("insert logs [\n");
        for i in 0..400usize {
            if i > 0 {
                ins.push(',');
            }
            let kind = ["open", "closed", "pending"][i % 3];
            let word = if i % 17 == 0 { "needle" } else { "noise" };
            if i % 4 == 0 {
                ins.push_str(&format!(
                    r#"{{ id: "r{i:03}", kind: "{kind}", n: {}, body: "LOG line {i} {word} walmart" }}"#,
                    i % 50
                ));
            } else {
                ins.push_str(&format!(
                    r#"{{ id: "r{i:03}", kind: "{kind}", tag: "t{}", n: {}, body: "LOG line {i} {word} walmart" }}"#,
                    i % 5,
                    i % 50
                ));
            }
        }
        ins.push_str("\n]");
        db.run(&ins).unwrap();
    };

    let mut hot = Db::empty();
    seed(&mut hot);

    {
        let mut init = Db::open_with(dir.path(), cold_opts()).unwrap();
        seed(&mut init);
        init.checkpoint().unwrap();
        init.close().unwrap();
    }
    let mut cold = Db::open_with(dir.path(), cold_opts()).unwrap();
    assert!(cold.store.is_disk_backed("logs"));

    let pairs = [
        (
            r#"logs | body ~ /wal.*/i and tag is not null and kind == "open" and n >= 10 and body has_ci "log" | { id } | sort id asc"#,
            r#"logs | tag is not null and kind == "open" and n >= 10 and body has_ci "log" and body ~ /wal.*/i | { id } | sort id asc"#,
        ),
        (
            r#"logs | (body ~ "needle" or kind != "open") and not (tag == "t0") | { id } | sort id asc"#,
            r#"logs | not (tag == "t0") and (kind != "open" or body ~ "needle") | { id } | sort id asc"#,
        ),
    ];

    for (expensive_first, cheap_first) in pairs {
        let he_h = hot.run(expensive_first).unwrap();
        let hc_h = hot.run(cheap_first).unwrap();
        let ce_h = cold.run(expensive_first).unwrap();
        let cc_h = cold.run(cheap_first).unwrap();
        assert_eq!(he_h.rows, hc_h.rows);
        assert_eq!(ce_h.rows, cc_h.rows);
        assert_eq!(he_h.rows, ce_h.rows);
        assert!(!he_h.rows.is_empty());
    }

    // Deep 600-term `and` chain (verifies iterative flattening in PredSlots & SegPred without stack overflow).
    let mut chain = String::from("logs | ");
    for i in 0..600usize {
        if i > 0 {
            chain.push_str(" and ");
        }
        if i % 2 == 0 {
            chain.push_str(r#"kind != "impossible""#);
        } else {
            chain.push_str("n >= 0");
        }
    }
    chain.push_str(r#" and body ~ "needle" | { id } | sort id asc"#);

    let hot_deep = hot.run(&chain).unwrap();
    let cold_deep = cold.run(&chain).unwrap();
    assert_eq!(hot_deep.rows, cold_deep.rows);
    assert_eq!(hot_deep.rows.len(), 24); // (0..400).filter(|i| i % 17 == 0).count() == 24
}
