//! End-to-end integration tests covering the three distributed/networked Lin pillars:
//! 1. Networked RPC server/client, bounded group commit, TLS, tokens, workload groups, readers
//! 2. Primary + replica session read-your-writes, stale replica fallback, and `--cold` WAL-gap bootstrap
//! 3. Horizontal `ShardedCluster` / `ShardRouter` vs single-node oracle (plans A-D, pruning, DML, batches)
//! 4. Remote cold tier (`MemoryObjectStore`, `FsObjectStore`, lazy 1-GET open, LRU eviction, CRC, HTTP S3 SigV4)

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use lin::ship::{TlsClient, TlsServer};
use lin::{
    Cell, ClientOpts, ColdObjectStore, Consistency, Db, FollowerSyncOpts, FsObjectStore,
    LinServer, MemoryObjectStore, NodeRole, OpenOpts, PullSyncPayload, RecordBatch,
    RemoteCacheConfig, ReplicaPoolClient, Row, S3Config, S3HttpObjectStore, ServerClient,
    ServerConfig, ShardStrategy, ShardedCluster, Value, WorkloadGroup, jump_consistent_hash,
    shard_hash_cell, shard_hash_value,
};

static NEXT_TMP: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lin_dist_test_{prefix}_{}_{}",
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cell_close(a: &Cell, b: &Cell) -> bool {
    match (a, b) {
        (Cell::Float(x), Cell::Float(y)) => {
            if x.is_nan() && y.is_nan() {
                true
            } else {
                let scale = x.abs().max(y.abs()).max(1.0);
                (x - y).abs() <= 1e-6 * scale
            }
        }
        (Cell::Int(x), Cell::Float(y)) | (Cell::Float(y), Cell::Int(x)) => {
            let x = *x as f64;
            let scale = x.abs().max(y.abs()).max(1.0);
            (x - y).abs() <= 1e-6 * scale
        }
        _ => a == b,
    }
}

fn assert_rows_match(label: &str, actual: &[Row], expected: &[Row], ordered: bool) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: row count mismatch (actual={}, expected={})",
        actual.len(),
        expected.len()
    );
    let canon = |r: &Row| -> String {
        let mut parts: Vec<String> = r
            .iter()
            .filter(|(_, v)| !matches!(v, Cell::Null))
            .map(|(k, v)| match v {
                Cell::Float(f) => format!("{k}=F:{:.6}", if *f == 0.0 { 0.0 } else { *f }),
                other => format!("{k}={other:?}"),
            })
            .collect();
        parts.sort();
        parts.join("|")
    };
    let compare_pair = |idx: usize, ar: &Row, er: &Row| {
        for (k, ev) in er.iter() {
            let av = ar.get(k).unwrap_or(&Cell::Null);
            assert!(
                cell_close(av, ev),
                "{label}: row {idx} field {k:?} actual={av:?} vs expected={ev:?}"
            );
        }
        for (k, av) in ar.iter() {
            if !matches!(av, Cell::Null) && er.get(k).is_none() {
                panic!("{label}: row {idx} unexpected extra non-null field {k:?}={av:?}");
            }
        }
    };
    if ordered {
        for (i, (ar, er)) in actual.iter().zip(expected.iter()).enumerate() {
            compare_pair(i, ar, er);
        }
    } else {
        let mut a_sorted: Vec<(String, &Row)> = actual.iter().map(|r| (canon(r), r)).collect();
        let mut e_sorted: Vec<(String, &Row)> = expected.iter().map(|r| (canon(r), r)).collect();
        a_sorted.sort_by(|x, y| x.0.cmp(&y.0));
        e_sorted.sort_by(|x, y| x.0.cmp(&y.0));
        for (i, ((_, ar), (_, er))) in a_sorted.iter().zip(e_sorted.iter()).enumerate() {
            compare_pair(i, ar, er);
        }
    }
}

fn oracle_and_cluster_match(
    oracle: &mut Db,
    cluster: &ShardedCluster,
    query: &str,
    ordered: bool,
) -> lin::ClusterStats {
    let oh = oracle
        .run(query)
        .unwrap_or_else(|e| panic!("oracle failed on `{query}`: {e}"));
    let (batch, stats) = cluster
        .router()
        .query_with_stats(query)
        .unwrap_or_else(|e| panic!("cluster failed on `{query}`: {e}"));
    let crows = batch.to_rows();
    assert_rows_match(query, &crows, &oh.rows, ordered);
    stats
}

// =========================================================================
// 1. Server group commit, error isolation, auth, TLS, workload groups & readers
// =========================================================================

#[test]
fn server_group_commit_auth_tls_workload_and_concurrent_readers() {
    let dir = TempDir::new("srv_tls");
    let cert = rcgen::generate_simple_self_signed(["localhost".into()]).expect("rcgen cert");
    let cert_pem = cert.cert.pem().into_bytes();
    let key_pem = cert.key_pair.serialize_pem().into_bytes();

    let mut wgs = BTreeMap::new();
    wgs.insert(
        "tiny".to_string(),
        WorkloadGroup {
            name: "tiny".to_string(),
            max_result_bytes: 384,
            max_scan_rows: Some(100),
            query_timeout: None,
        },
    );

    let secret = "super-secret-bearer-token-999";
    let srv_cfg = ServerConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        role: NodeRole::Primary,
        follower: None,
        token: Some(secret.to_string()),
        tls: Some(TlsServer {
            cert_pem: cert_pem.clone(),
            key_pem,
        }),
        group_commit_window: Duration::from_millis(60),
        max_group_size: 8,
        max_group_bytes: 256 * 1024,
        consistency_wait_timeout: Duration::from_millis(500),
        workload_groups: wgs,
        default_workload_group: WorkloadGroup::default(),
        io_timeout: Duration::from_secs(10),
    };

    let authed_client_opts = ClientOpts {
        token: Some(secret.to_string()),
        tls: TlsClient::CaPem(cert_pem.clone()),
        server_name: Some("localhost".to_string()),
        consistency: Consistency::Strong,
        workload_group: None,
        timeout: Duration::from_secs(10),
    };

    // Debug output must redact bearer secrets.
    let srv_dbg = format!("{srv_cfg:?}");
    let cli_dbg = format!("{authed_client_opts:?}");
    assert!(srv_dbg.contains("[REDACTED]") && !srv_dbg.contains(secret));
    assert!(cli_dbg.contains("[REDACTED]") && !cli_dbg.contains(secret));

    let db = Db::open(dir.path()).expect("open primary db");
    let handle = LinServer::start(db, srv_cfg).expect("start tls server");
    let addr = handle.local_addr().to_string();

    // Missing or invalid bearer token must be rejected during handshake.
    let bad_opts = ClientOpts {
        token: Some("wrong-token".into()),
        tls: TlsClient::CaPem(cert_pem.clone()),
        server_name: Some("localhost".into()),
        ..Default::default()
    };
    let bad_err = ServerClient::connect(&addr, bad_opts).unwrap_err();
    assert!(
        bad_err.to_string().contains("unauthorized"),
        "expected unauthorized error, got {bad_err}"
    );

    let no_tok_opts = ClientOpts {
        token: None,
        tls: TlsClient::CaPem(cert_pem.clone()),
        server_name: Some("localhost".into()),
        ..Default::default()
    };
    let no_tok_err = ServerClient::connect(&addr, no_tok_opts).unwrap_err();
    assert!(
        no_tok_err.to_string().contains("unauthorized"),
        "expected unauthorized without token, got {no_tok_err}"
    );

    // Authorized TLS client defines typed schema.
    let admin = ServerClient::connect(&addr, authed_client_opts.clone()).expect("connect admin");
    let (role, g0) = admin.ping().expect("ping");
    assert_eq!(role, NodeRole::Primary);
    assert_eq!(g0, 0);
    admin
        .execute("col metrics { id: text, tag: text, val: i64 }")
        .expect("create typed collection");

    // 4 concurrent writers hit group commit window together -> at least one merged batch (group_size > 1),
    // while 1 concurrent failing write is isolated and doesn't abort sibling commits.
    let barrier = Arc::new(Barrier::new(5));
    let mut joins = Vec::new();
    for i in 0..4usize {
        let a = addr.clone();
        let opts = authed_client_opts.clone();
        let b = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            let cli = ServerClient::connect(&a, opts).unwrap();
            b.wait();
            cli.execute(&format!(
                r#"insert metrics {{ id: "m_{i}", tag: "tls_ok", val: {} }}"#,
                i * 10 + 1
            ))
        }));
    }
    {
        let a = addr.clone();
        let opts = authed_client_opts.clone();
        let b = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            let cli = ServerClient::connect(&a, opts).unwrap();
            b.wait();
            // Wrong field type (`val: "not_an_int"`) must fail cleanly only for this writer.
            cli.execute(r#"insert metrics { id: "bad", tag: "oops", val: "not_an_int" }"#)
        }));
    }

    let mut ok_group_max = 0usize;
    let mut err_seen = false;
    for (idx, j) in joins.into_iter().enumerate() {
        let res = j.join().unwrap();
        if idx < 4 {
            let receipt = res.unwrap_or_else(|e| panic!("writer {idx} unexpectedly failed: {e}"));
            ok_group_max = ok_group_max.max(receipt.group_size);
        } else {
            assert!(res.is_err(), "invalid write should fail per-item");
            err_seen = true;
        }
    }
    assert!(err_seen);
    assert!(
        ok_group_max >= 2,
        "expected concurrent writes to coalesce into group commit (max group_size={ok_group_max})"
    );

    // Seed enough payload for workload limit test.
    let mut batch = RecordBatch::with_capacity(["id", "tag", "val"].map(str::to_string), 60);
    for i in 0..60i64 {
        batch.push_row(&[
            Cell::from(format!("bulk_{i:03}_payload_extension_text")),
            Cell::from("wg_test"),
            Cell::Int(i),
        ]);
    }
    admin
        .insert_batch("metrics", &batch)
        .expect("insert_batch metrics");

    // Workload group `tiny` (max_result_bytes = 128) rejects large result set, succeeds for tiny aggregate.
    let tiny_opts = ClientOpts {
        workload_group: Some("tiny".into()),
        ..authed_client_opts.clone()
    };
    let tiny_cli = ServerClient::connect(&addr, tiny_opts).expect("connect tiny wg");
    let wg_err = tiny_cli.query("metrics | take 50").unwrap_err();
    assert!(
        wg_err.to_string().contains("workload_limit"),
        "expected workload_limit error, got {wg_err}"
    );
    let agg_batch = tiny_cli.query("metrics | count").expect("small agg allowed");
    assert_eq!(
        agg_batch.to_rows()[0].get("hits"),
        Some(&Cell::Int(64)),
        "4 group-committed writes + 60 bulk rows"
    );

    // Concurrent lock-free readers reading alongside an active writer.
    let stop_readers = Arc::new(AtomicBool::new(false));
    let mut reader_threads = Vec::new();
    for _ in 0..4 {
        let a = addr.clone();
        let opts = ClientOpts {
            consistency: Consistency::Weak,
            ..authed_client_opts.clone()
        };
        let stop = Arc::clone(&stop_readers);
        reader_threads.push(thread::spawn(move || {
            let cli = ServerClient::connect(&a, opts).unwrap();
            let mut reads = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let r = cli.query("metrics | count").unwrap();
                let n = match r.to_rows()[0].get("hits") {
                    Some(Cell::Int(v)) => *v,
                    other => panic!("unexpected count cell {other:?}"),
                };
                assert!(n >= 64);
                reads += 1;
            }
            assert!(reads > 0);
        }));
    }
    for i in 0..8 {
        admin
            .execute(&format!(
                r#"insert metrics {{ id: "rw_{i}", tag: "stream", val: {i} }}"#
            ))
            .unwrap();
    }
    stop_readers.store(true, Ordering::Release);
    for rt in reader_threads {
        rt.join().unwrap();
    }

    let final_rows = admin.query("metrics | count").unwrap().to_rows();
    assert_eq!(final_rows[0].get("hits"), Some(&Cell::Int(72)));

    let mut db = handle.shutdown().expect("shutdown tls server");
    db.close().unwrap();
}

#[test]
fn server_group_commit_max_bytes_splits_batches() {
    let dir = TempDir::new("srv_bytes");
    let db = Db::open(dir.path()).unwrap();
    let cfg = ServerConfig {
        bind_addr: "127.0.0.1:0".into(),
        group_commit_window: Duration::from_millis(80),
        max_group_size: 16,
        max_group_bytes: 120,
        ..Default::default()
    };
    let handle = LinServer::start(db, cfg).unwrap();
    let addr = handle.local_addr().to_string();

    let setup_cli = ServerClient::connect(&addr, ClientOpts::default()).unwrap();
    setup_cli
        .execute("col items { id: text, payload: text }")
        .unwrap();

    let b = Arc::new(Barrier::new(2));
    let mut joins = Vec::new();
    let big_pad = "x".repeat(160);
    for i in 0..2 {
        let a = addr.clone();
        let br = Arc::clone(&b);
        let pad = big_pad.clone();
        joins.push(thread::spawn(move || {
            let cli = ServerClient::connect(&a, ClientOpts::default()).unwrap();
            br.wait();
            cli.execute(&format!(
                r#"insert items {{ id: "b_{i}", payload: "{pad}" }}"#
            ))
            .unwrap()
        }));
    }
    for j in joins {
        let rec = j.join().unwrap();
        assert_eq!(
            rec.group_size, 1,
            "oversize writes exceeding max_group_bytes must split instead of combining"
        );
    }

    let cli = ServerClient::connect(&addr, ClientOpts::default()).unwrap();
    assert_eq!(
        cli.query("items | count").unwrap().to_rows()[0].get("hits"),
        Some(&Cell::Int(2))
    );
    handle.shutdown().unwrap().close().unwrap();
}

// =========================================================================
// 2. ReplicaPoolClient Session RYOW, stale replica fallback & `--cold` Bootstrap
// =========================================================================

#[test]
fn replica_pool_session_ryow_and_cold_bootstrap_replication() {
    let pdir = TempDir::new("repl_primary");
    let fdir_caught = TempDir::new("repl_f_caught");
    let fdir_lag = TempDir::new("repl_f_lag");

    // 1. Seed primary durable `--cold` DB with 80 rows (>= COLD_MIN_ROWS=32) + checkpoint,
    //    then write 5 more rows so WAL only retains post-checkpoint frames (WAL gap from gen 0).
    let mut pdb = Db::open_with(
        pdir.path(),
        OpenOpts {
            cold: true,
            ..Default::default()
        },
    )
    .unwrap();
    pdb.run("col logs { id: text, env: text, seq: i64 }").unwrap();
    for i in 0..80 {
        pdb.run(&format!(
            r#"insert logs {{ id: "cold_{i:03}", env: "prod", seq: {i} }}"#
        ))
        .unwrap();
    }
    pdb.checkpoint().unwrap();
    for i in 80..85 {
        pdb.run(&format!(
            r#"insert logs {{ id: "hot_{i:03}", env: "prod", seq: {i} }}"#
        ))
        .unwrap();
    }

    let primary = LinServer::start(
        pdb,
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Primary,
            ..Default::default()
        },
    )
    .unwrap();
    let p_addr = primary.local_addr().to_string();

    // Direct protocol check: pulling from gen 0 with WAL gap yields PullSyncPayload::Bootstrap with `.lseg` bundles.
    let probe = ServerClient::connect(&p_addr, ClientOpts::default()).unwrap();
    match probe.pull_sync(0, false).unwrap() {
        PullSyncPayload::Bootstrap {
            r#gen,
            cold_segments,
            ..
        } => {
            assert_eq!(r#gen, primary.r#gen());
            assert!(
                !cold_segments.is_empty(),
                "expected cold .lseg segments in bootstrap payload"
            );
        }
        other => panic!("expected PullSyncPayload::Bootstrap across checkpoint gap, got {other:?}"),
    }

    // 2. Start follower 1 (`--cold`) with fast poll -> bootstraps snapshot + `.lseg` cold segments automatically.
    let f1_db = Db::open_follower_with(
        fdir_caught.path(),
        OpenOpts {
            cold: true,
            ..Default::default()
        },
    )
    .unwrap();
    let f1 = LinServer::start(
        f1_db,
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Follower,
            follower: Some(FollowerSyncOpts {
                primary_addr: p_addr.clone(),
                client: ClientOpts::default(),
                poll_interval: Duration::from_millis(15),
            }),
            ..Default::default()
        },
    )
    .unwrap();
    f1.wait_gen(primary.r#gen(), Duration::from_secs(5))
        .expect("follower 1 bootstrap catchup");

    // Verify follower 1 has all 85 cold + post-checkpoint rows.
    let f1_cli = ServerClient::connect(f1.local_addr().to_string(), ClientOpts::default()).unwrap();
    assert_eq!(
        f1_cli.query("logs | count").unwrap().to_rows()[0].get("hits"),
        Some(&Cell::Int(85))
    );

    // 3. Start follower 2 with huge poll_interval so we control catchup explicitly -> tests stale replica fallback!
    let f2_db = Db::open_follower_with(
        fdir_lag.path(),
        OpenOpts {
            cold: true,
            ..Default::default()
        },
    )
    .unwrap();
    let f2 = LinServer::start(
        f2_db,
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Follower,
            follower: Some(FollowerSyncOpts {
                primary_addr: p_addr.clone(),
                client: ClientOpts::default(),
                poll_interval: Duration::from_secs(300),
            }),
            consistency_wait_timeout: Duration::from_millis(40),
            ..Default::default()
        },
    )
    .unwrap();
    f2.sync_once().expect("initial f2 sync");
    assert_eq!(f2.r#gen(), primary.r#gen());

    // Build pool targeting ONLY lagging follower f2 first (so we observe fallback to primary on stale gen),
    // then also test caught-up pool.
    let pool = ReplicaPoolClient::connect(
        &p_addr,
        &[f2.local_addr().to_string()],
        ClientOpts {
            consistency: Consistency::Session,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(pool.replicas().len(), 1);

    // Write via pool: advances pool.session_gen() above f2's generation.
    let rec = pool
        .execute(r#"insert logs { id: "ryow_86", env: "edge", seq: 86 }"#)
        .unwrap();
    assert!(pool.session_gen() >= rec.r#gen);

    // Direct query on lagging f2 with AtLeastGen(rec.r#gen) must fail with stale_replica.
    let direct_f2_err = f2_cli_err(&f2.local_addr().to_string(), rec.r#gen);
    assert!(
        direct_f2_err.to_string().contains("stale_replica"),
        "expected stale_replica from lagging f2, got {direct_f2_err}"
    );

    // Pool query with Session consistency falls back transparently to primary and returns newly written row!
    let batch = pool
        .query(r#"logs | id == "ryow_86" | pick { id, seq }"#)
        .unwrap();
    let rows = batch.to_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("seq"), Some(&Cell::Int(86)));

    // Sync lagging follower incrementally via WAL and verify read from follower succeeds!
    f2.sync_once().expect("f2 incremental wal sync");
    assert_eq!(f2.r#gen(), rec.r#gen);
    let f2_cli = ServerClient::connect(f2.local_addr().to_string(), ClientOpts::default()).unwrap();
    let (rep_batch, _) = f2_cli
        .query_with_consistency(
            r#"logs | id == "ryow_86" | pick { id, seq }"#,
            Consistency::AtLeastGen(rec.r#gen),
        )
        .unwrap();
    assert_eq!(rep_batch.to_rows().len(), 1);

    f1.shutdown().unwrap().close().unwrap();
    f2.shutdown().unwrap().close().unwrap();
    primary.shutdown().unwrap().close().unwrap();
}

fn f2_cli_err(addr: &str, min_gen: u64) -> lin::Error {
    let cli = ServerClient::connect(addr, ClientOpts::default()).unwrap();
    cli.query_with_consistency("logs | count", Consistency::AtLeastGen(min_gen))
        .unwrap_err()
}

// =========================================================================
// 3. ShardedCluster vs Single-Node Oracle: Pruning, Jump Hash, Plans A-D, DML
// =========================================================================

#[test]
fn sharded_cluster_kusto_olap_matches_single_node_oracle() {
    // 3A. Canonical hashing parity & Jump Consistent Hash minimal movement (~1/5 buckets from 4 -> 5).
    assert_eq!(
        shard_hash_cell(&Cell::Int(42)),
        shard_hash_value(&Value::Int(42)).unwrap()
    );
    assert_eq!(shard_hash_cell(&Cell::Float(42.0)), shard_hash_cell(&Cell::Int(42)));
    assert_eq!(
        shard_hash_cell(&Cell::Float(-0.0)),
        shard_hash_cell(&Cell::Float(0.0))
    );
    let mut moved = 0usize;
    let total_keys = 10_000u64;
    for k in 0..total_keys {
        let h = shard_hash_cell(&Cell::Int(k as i64));
        if jump_consistent_hash(h, 4) != jump_consistent_hash(h, 5) {
            moved += 1;
        }
    }
    assert!(
        (1600..=2400).contains(&moved),
        "expected ~20% movement adding 5th bucket, got {moved}/{total_keys}"
    );

    // 3B. 3-shard ShardedCluster vs single-node Db oracle with 5,200 skewed rows.
    let keys = BTreeMap::from([("events".to_string(), "tenant".to_string())]);
    let cluster =
        ShardedCluster::start_in_memory(3, keys, ShardStrategy::JumpConsistentHash).unwrap();
    let mut oracle = Db::empty();

    let schema = "col events { id: text, tenant: text, user_id: text, kind: text, val: i64, big: i64, score: f64, ts: time }";
    oracle.run(schema).unwrap();
    cluster.router().execute(schema).unwrap();

    // Generate 5,200 rows: skewed tenants + 620 unique users + extreme int sums + nulls.
    let mut batch = RecordBatch::with_capacity(
        ["id", "tenant", "user_id", "kind", "val", "big", "score", "ts"].map(str::to_string),
        4500,
    );
    let tenants = [
        "hot_tenant", "t_alpha", "t_beta", "t_gamma", "t_delta", "t_eps", "t_zeta", "t_eta",
    ];
    let mut dsl_inserts = Vec::with_capacity(700);
    let big_unit = i64::MAX / 4;

    for i in 0..5200usize {
        let tenant = if i == 5199 {
            "solo"
        } else if i % 10 < 6 {
            "hot_tenant"
        } else {
            tenants[1 + (i % (tenants.len() - 1))]
        };
        let user_id = if tenant == "t_alpha" {
            // Keep `t_alpha` small (< 20 unique users) to test exact `dcount` & `percentile` alongside HLL/TDigest.
            format!("u_{:04}", i % 17)
        } else {
            format!("u_{:04}", i % 620)
        };
        let kind = match i % 11 {
            0 => "ignore",
            1..=5 => "click",
            _ => "view",
        };
        let val_cell = if i % 29 == 0 {
            Cell::Null
        } else {
            Cell::Int((i % 97) as i64 - 10)
        };
        let big_cell = if i < 20 {
            // 20 * (i64::MAX / 4) overflows i64 sum -> tests safe f64 accumulator promotion!
            Cell::Int(big_unit)
        } else {
            Cell::Int(1)
        };
        let score_cell = if i % 37 == 0 {
            Cell::Null
        } else {
            Cell::Float(((i * 17) % 1000) as f64 / 10.0)
        };
        let ts = 1_700_000_000_000i64 + ((i % 12) as i64) * 3_600_000 + ((i / 12) as i64) * 100;
        let id = format!("ev_{i:05}");

        let push_fields = |with_big: i64| -> String {
            let mut flds = format!(
                r#"id: "{id}", tenant: "{tenant}", user_id: "{user_id}", kind: "{kind}", big: {with_big}, ts: timestamp({ts})"#
            );
            if let Cell::Int(v) = val_cell {
                flds.push_str(&format!(", val: {v}"));
            }
            if let Cell::Float(f) = score_cell {
                flds.push_str(&format!(", score: {f:.1}"));
            }
            format!("insert events {{ {flds} }}")
        };
        if i < 4500 {
            batch.push_row(&[
                Cell::from(id.clone()),
                Cell::from(tenant),
                Cell::from(user_id.clone()),
                Cell::from(kind),
                val_cell.clone(),
                big_cell.clone(),
                score_cell.clone(),
                Cell::Time(ts),
            ]);
            let big_v = match big_cell {
                Cell::Int(v) => v,
                _ => 1,
            };
            dsl_inserts.push(push_fields(big_v));
        } else {
            let stmt = push_fields(1);
            oracle.run(&stmt).unwrap();
            cluster.router().execute(&stmt).unwrap();
        }
    }

    // Load the 4,500-row batch into oracle (grouped script) and cluster (`insert_batch` columnar scatter).
    for chunk in dsl_inserts.chunks(250) {
        oracle.run(&chunk.join("\n")).unwrap();
    }
    let (batch_receipt, batch_stats) = cluster.router().insert_batch("events", &batch).unwrap();
    assert_eq!(batch_receipt.affected, 4500);
    assert_eq!(batch_stats.shards_queried, 3);

    // Confirm data actually distributed across all 3 shards (not concentrated on 1 shard).
    for (idx, client) in cluster.router().shard_clients().iter().enumerate() {
        let sc = client.query("events | count").unwrap().to_rows();
        let n = match sc[0].get("hits") {
            Some(Cell::Int(v)) => *v,
            other => panic!("unexpected count {other:?}"),
        };
        assert!(n > 0, "shard {idx} unexpectedly empty");
    }

    // 3C. Predicate shard pruning checks.
    let s1 = oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant == "hot_tenant" | count"#,
        false,
    );
    assert_eq!(s1.shards_queried, 1, "equality on shard key prunes to 1 shard");

    let s2 = oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant in ["hot_tenant", "t_alpha"] | count"#,
        false,
    );
    assert!(s2.shards_queried <= 2, "`in` prunes to <= 2 shards");

    let s0 = oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant == "hot_tenant" and tenant == "t_alpha" | count"#,
        false,
    );
    assert_eq!(s0.shards_queried, 0, "contradictory shard key predicate prunes all shards");

    let s3 = oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | kind == "click" | count"#,
        false,
    );
    assert_eq!(s3.shards_queried, 3, "non-shard-key filter fans out to all shards");

    // 3D. Plan A: Ungrouped summarize + overflowing sum(big) + HLL dcount + TDigest percentile + scalar arg_max/min.
    // Note: HLL (~1% RSE) and TDigest can differ slightly between merged partials vs single-node stream on >5k rows,
    // so assert exact fields strictly vs oracle and verify dcount/percentile within tight relative bounds!
    let q_ungrouped = r#"
        events
        | summarize {
            n: count,
            sv: sum(val),
            sb: sum(big),
            av: avg(score),
            lo: min(score),
            hi: max(score),
            du: dcount(user_id),
            p50: percentile(score, 50),
            p95: percentile(score, 95),
            amx: arg_max(ts, id),
            amn: arg_min(ts, id)
        }
    "#;
    let o_un_h = oracle.run(q_ungrouped).unwrap();
    let o_un = &o_un_h.rows;
    let c_un = cluster.router().query(q_ungrouped).unwrap().to_rows();
    assert_eq!(o_un.len(), 1);
    assert_eq!(c_un.len(), 1);
    for exact_col in ["n", "sv", "sb", "av", "lo", "hi", "amx", "amn"] {
        assert!(
            cell_close(c_un[0].get(exact_col).unwrap(), o_un[0].get(exact_col).unwrap()),
            "ungrouped {exact_col}: cluster={:?} oracle={:?}",
            c_un[0].get(exact_col),
            o_un[0].get(exact_col)
        );
    }
    let du_c = c_un[0].get("du").unwrap().as_f64().unwrap();
    assert!(
        (du_c - 620.0).abs() <= 25.0,
        "HLL dcount expected ~620, got {du_c}"
    );
    let p50_c = c_un[0].get("p50").unwrap().as_f64().unwrap();
    let p50_o = o_un[0].get("p50").unwrap().as_f64().unwrap();
    assert!((p50_c - p50_o).abs() <= 2.5, "p50 cluster={p50_c} vs oracle={p50_o}");
    let p95_c = c_un[0].get("p95").unwrap().as_f64().unwrap();
    let p95_o = o_un[0].get("p95").unwrap().as_f64().unwrap();
    assert!((p95_c - p95_o).abs() <= 2.5, "p95 cluster={p95_c} vs oracle={p95_o}");

    // Plan A exact small-cardinality group (`t_alpha` has 17 unique users -> exact dcount & exact percentile).
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant == "t_alpha" | summarize { n: count, sv: sum(val), du: dcount(user_id), p50: percentile(score, 50), p90: percentile(score, 90) }"#,
        false,
    );

    // Plan A multi-key `by tenant, b: bin(ts, 1h)` + post-summarize sort & take.
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | kind != "ignore" | summarize { n: count, sv: sum(val), av: avg(score), lo: min(score), hi: max(score), top_id: arg_max(ts, id) } by tenant, b: bin(ts, 1h) | sort tenant asc | sort b asc | take 20"#,
        true,
    );

    // 3E. Plan B: Legacy count / sum / avg / min / max.
    oracle_and_cluster_match(&mut oracle, &cluster, "events | count by tenant | sort tenant asc", true);
    oracle_and_cluster_match(&mut oracle, &cluster, "events | sum val by tenant | sort tenant asc", true);
    oracle_and_cluster_match(&mut oracle, &cluster, "events | avg score by tenant | sort tenant asc", true);
    oracle_and_cluster_match(&mut oracle, &cluster, "events | min score", false);
    oracle_and_cluster_match(&mut oracle, &cluster, "events | max ts", false);

    // 3F. Plan C: Whole-row ArgExtremum (`arg_max(ts, *)` ungrouped and grouped + sort).
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        "events | arg_max(ts, *)",
        false,
    );
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        "events | arg_min(ts, *) by tenant | sort tenant asc",
        true,
    );

    // 3G. Plan D: Distributed Sort + Skip + Take (top-k pushdown) and Window coordinator step.
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        "events | kind == \"click\" | sort ts desc | skip 10 | take 25 | pick { id, tenant, ts }",
        true,
    );
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant == "t_alpha" | sort ts asc | window row_number by tenant sort [ts asc, id asc] as rn | pick { id, tenant, rn } | take 15"#,
        true,
    );

    // 3H. Distributed Update & Delete + shard-key mutation rejection.
    let sk_err = cluster
        .router()
        .execute(r#"update events [tenant == "t_alpha"] cas each { tenant: "t_beta" }"#)
        .unwrap_err();
    assert!(
        sk_err.to_string().contains("cannot update shard key"),
        "expected shard key mutation rejection, got {sk_err}"
    );

    let upd = r#"update events [tenant == "t_beta" and kind == "ignore"] cas each { val: 999 }"#;
    oracle.run(upd).unwrap();
    let upd_rec = cluster.router().execute(upd).unwrap();
    assert!(upd_rec.affected > 0);
    oracle_and_cluster_match(
        &mut oracle,
        &cluster,
        r#"events | tenant == "t_beta" | summarize { n: count, s: sum(val) }"#,
        false,
    );

    let del = r#"delete events [kind == "ignore"] cas each"#;
    oracle.run(del).unwrap();
    let del_rec = cluster.router().execute(del).unwrap();
    assert!(del_rec.affected > 400);
    oracle_and_cluster_match(&mut oracle, &cluster, "events | count by tenant | sort tenant asc", true);

    for mut db in cluster.shutdown().unwrap() {
        db.close().unwrap();
    }
    oracle.close().unwrap();
}

// =========================================================================
// 4. Remote Cold Tier: Memory/FS Object Store, Lazy Open, LRU Eviction, CRC & S3 SigV4 HTTP
// =========================================================================

fn seed_two_cold_tables(dir: &Path) -> Db {
    let mut db = Db::open_with(
        dir,
        OpenOpts {
            cold: true,
            ..Default::default()
        },
    )
    .unwrap();
    db.run("col logs_a { id: text, region: text, code: i64 }\ncol logs_b { id: text, service: text, ms: i64 }")
        .unwrap();
    for i in 0..80 {
        db.run(&format!(
            r#"insert logs_a {{ id: "a_{i:03}", region: "{}", code: {i} }}"#,
            if i % 2 == 0 { "us" } else { "eu" }
        ))
        .unwrap();
        db.run(&format!(
            r#"insert logs_b {{ id: "b_{i:03}", service: "{}", ms: {} }}"#,
            if i % 3 == 0 { "auth" } else { "api" },
            i * 5
        ))
        .unwrap();
    }
    db.checkpoint().unwrap();
    db
}

#[test]
fn remote_cold_tier_lazy_lru_corruption_fs_and_http_s3_sigv4() {
    let src_dir = TempDir::new("rem_src");
    let cache_dir = TempDir::new("rem_cache");
    let corrupt_dir = TempDir::new("rem_corrupt");
    let fs_obj_dir = TempDir::new("rem_fs_obj");
    let fs_cache_dir = TempDir::new("rem_fs_cache");
    let s3_src_dir = TempDir::new("rem_s3_src");
    let s3_cache_dir = TempDir::new("rem_s3_cache");

    // 4A. Push to MemoryObjectStore + verify incremental push reuses segments.
    let mem_store = Arc::new(MemoryObjectStore::new());
    let mut src_db = seed_two_cold_tables(src_dir.path());
    let stats1 = src_db
        .push_cold_to_object_store(mem_store.as_ref(), "org1/env", true)
        .expect("first push");
    assert!(stats1.segments_uploaded >= 2);
    assert!(stats1.bytes_uploaded > 0);

    let stats2 = src_db
        .push_cold_to_object_store(mem_store.as_ref(), "org1/env", true)
        .expect("idempotent push");
    assert_eq!(stats2.segments_uploaded, 0);
    assert_eq!(stats2.segments_reused, stats1.segments_uploaded);
    src_db.close().unwrap();

    // 4B. Lazy open does minimal metadata GETs (<= 2: manifest + snapshot), 0 local cache bytes.
    let gets_before = mem_store.counters().gets;
    let mut reader = Db::open_cold_from_object_store(
        cache_dir.path(),
        mem_store.clone(),
        "org1/env",
        RemoteCacheConfig {
            lazy: true,
            max_local_bytes: u64::MAX,
        },
    )
    .expect("lazy open");
    let gets_after_open = mem_store.counters().gets - gets_before;
    assert!(
        gets_after_open <= 2,
        "lazy open should only read manifest (+ optional snapshot), did {gets_after_open} GETs"
    );
    assert_eq!(reader.cold_local_cache_bytes(), 0);

    // Query only `logs_a`: hydrates `logs_a` only (`logs_b` stays unhydrated in remote tier).
    let gets_before_a = mem_store.counters().gets;
    let h_a = reader
        .run_remote(r#"logs_a | region == "us" | count"#)
        .expect("run_remote logs_a");
    assert_eq!(h_a.rows[0].get("hits"), Some(&Cell::Int(40)));
    assert!(mem_store.counters().gets > gets_before_a);
    let bytes_a = reader.cold_local_cache_bytes();
    assert!(bytes_a > 0);

    // Query `logs_b`: hydrates `logs_b`, so cache grows beyond `bytes_a`.
    let h_b = reader
        .run_remote(r#"logs_b | service == "auth" | summarize { n: count, tot: sum(ms) }"#)
        .expect("run_remote logs_b");
    assert_eq!(h_b.rows[0].get("n"), Some(&Cell::Int(27)));
    let bytes_ab = reader.cold_local_cache_bytes();
    assert!(bytes_ab > bytes_a);

    // Whole-table LRU eviction: shrink budget down to `bytes_ab - 1`, leaving <= 1 newest table (`logs_b`).
    reader
        .evict_cold_cache_to(bytes_ab - 1)
        .expect("evict LRU table");
    let bytes_after_evict = reader.cold_local_cache_bytes();
    assert!(
        bytes_after_evict < bytes_ab,
        "LRU eviction should drop oldest table logs_a ({bytes_after_evict} < {bytes_ab})"
    );

    // Querying evicted `logs_a` transparently re-hydrates it!
    let gets_re = mem_store.counters().gets;
    let h_a2 = reader
        .run_remote(r#"logs_a | sort code desc | take 3 | pick { id, code }"#)
        .expect("rehydrate logs_a");
    assert_eq!(h_a2.rows.len(), 3);
    assert_eq!(h_a2.rows[0].get("code"), Some(&Cell::Int(79)));
    assert!(mem_store.counters().gets > gets_re);
    reader.close().unwrap();

    // 4C. Corrupt `.lseg` byte in object store -> hydrate must reject CRC mismatch.
    let all_keys = mem_store.list("org1/env/segments").unwrap();
    let seg_key = all_keys
        .into_iter()
        .find(|k| k.ends_with(".lseg"))
        .expect("at least one .lseg");
    mem_store
        .corrupt_byte(&seg_key, 12, 0xff)
        .expect("corrupt segment byte");
    let mut bad_reader = Db::open_cold_from_object_store(
        corrupt_dir.path(),
        mem_store.clone(),
        "org1/env",
        RemoteCacheConfig {
            lazy: true,
            max_local_bytes: u64::MAX,
        },
    )
    .expect("open metadata before hydration");
    let crc_err = bad_reader.hydrate_cold_collection("logs_a").unwrap_err();
    let crc_err_b = bad_reader.hydrate_cold_collection("logs_b");
    let any_crc = crc_err.to_string().contains("CRC mismatch")
        || crc_err_b
            .err()
            .is_some_and(|e| e.to_string().contains("CRC mismatch"));
    assert!(any_crc, "expected CRC mismatch on corrupted .lseg bundle");
    bad_reader.close().unwrap();

    // 4D. FsObjectStore round-trip.
    let fs_store = Arc::new(FsObjectStore::open(fs_obj_dir.path()).unwrap());
    let mut fs_src = seed_two_cold_tables(s3_src_dir.path());
    fs_src
        .push_cold_to_object_store(fs_store.as_ref(), "fs_prod", true)
        .unwrap();
    let mut fs_reader = Db::open_cold_from_object_store(
        fs_cache_dir.path(),
        fs_store,
        "fs_prod",
        RemoteCacheConfig {
            lazy: true,
            max_local_bytes: u64::MAX,
        },
    )
    .unwrap();
    let fs_h = fs_reader
        .run_remote("logs_a | count")
        .unwrap();
    assert_eq!(fs_h.rows[0].get("hits"), Some(&Cell::Int(80)));
    fs_reader.close().unwrap();

    // 4E. Loopback HTTP/1.1 Mock S3 Server testing SigV4 header, PUT/GET/DELETE & 2-page ListObjectsV2 pagination.
    let mock = MockS3::start();
    let s3_store = Arc::new(
        S3HttpObjectStore::new(S3Config {
            endpoint: format!("http://{}", mock.addr),
            region: "us-east-1".into(),
            bucket: "lin-cold".into(),
            prefix: "cluster-z".into(),
            access_key: "AKIA_TEST_KEY".into(),
            secret_key: "TEST_SECRET_KEY_12345".into(),
            session_token: Some("SESSION_TOK".into()),
            path_style: true,
            ca_pem: None,
            timeout: Duration::from_secs(5),
        })
        .expect("init S3HttpObjectStore"),
    );

    let push_s3 = fs_src
        .push_cold_to_object_store(s3_store.as_ref(), "s3sub", true)
        .expect("push over HTTP SigV4");
    assert!(push_s3.segments_uploaded >= 2);
    assert!(mock.puts.load(Ordering::Acquire) >= 3);
    assert!(mock.saw_sigv4.load(Ordering::Acquire));

    // List with max_keys=2 in mock forces >= 2 HTTP ListObjectsV2 pages via continuation-token.
    let listed = s3_store.list("").expect("list v2 paginated");
    assert!(listed.len() >= 3);
    assert!(
        mock.lists.load(Ordering::Acquire) >= 2,
        "expected >= 2 paginated ListObjectsV2 calls, saw {}",
        mock.lists.load(Ordering::Acquire)
    );

    // Lazy open + query through HTTP S3 store.
    let mut s3_reader = Db::open_cold_from_object_store(
        s3_cache_dir.path(),
        s3_store.clone(),
        "s3sub",
        RemoteCacheConfig {
            lazy: true,
            max_local_bytes: u64::MAX,
        },
    )
    .expect("lazy open via S3HttpObjectStore");
    let h_s3_a = s3_reader.run_remote("logs_a | count").unwrap();
    let h_s3_b = s3_reader.run_remote("logs_b | count").unwrap();
    assert_eq!(h_s3_a.rows[0].get("hits"), Some(&Cell::Int(80)));
    assert_eq!(h_s3_b.rows[0].get("hits"), Some(&Cell::Int(80)));

    // Exercise S3 DELETE object path as well.
    let first_key = listed[0].clone();
    s3_store.delete(&first_key).expect("s3 delete");
    assert!(mock.deletes.load(Ordering::Acquire) >= 1);
    assert!(s3_store.get(&first_key).unwrap().is_none());

    s3_reader.close().unwrap();
    fs_src.close().unwrap();
    mock.shutdown();
}

// -------------------------------------------------------------------------
// Minimal HTTP/1.1 Mock S3 Server verifying AWS SigV4 + paginated ListObjectsV2
// -------------------------------------------------------------------------

struct MockS3 {
    addr: String,
    puts: Arc<AtomicUsize>,
    lists: Arc<AtomicUsize>,
    deletes: Arc<AtomicUsize>,
    saw_sigv4: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockS3 {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock s3");
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::new(Mutex::new(BTreeMap::new()));
        let puts = Arc::new(AtomicUsize::new(0));
        let lists = Arc::new(AtomicUsize::new(0));
        let deletes = Arc::new(AtomicUsize::new(0));
        let saw_sigv4 = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let o2 = Arc::clone(&objects);
        let p2 = Arc::clone(&puts);
        let l2 = Arc::clone(&lists);
        let d2 = Arc::clone(&deletes);
        let sig2 = Arc::clone(&saw_sigv4);
        let st2 = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !st2.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = handle_mock_s3_conn(&mut stream, &o2, &p2, &l2, &d2, &sig2);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            puts,
            lists,
            deletes,
            saw_sigv4,
            stop,
            thread: Some(handle),
        }
    }

    fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn handle_mock_s3_conn(
    stream: &mut std::net::TcpStream,
    objects: &Mutex<BTreeMap<String, Vec<u8>>>,
    puts: &AtomicUsize,
    lists: &AtomicUsize,
    deletes: &AtomicUsize,
    saw_sigv4: &AtomicBool,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line)? == 0 {
        return Ok(());
    }
    let parts: Vec<&str> = req_line.trim_end().split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let method = parts[0];
    let target = parts[1];

    let mut content_len = 0usize;
    let mut auth_ok = false;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("authorization") {
                let val = v.trim();
                auth_ok = val.starts_with("AWS4-HMAC-SHA256 ")
                    && val.contains("Credential=AKIA_TEST_KEY/")
                    && val.contains("SignedHeaders=")
                    && val.contains("Signature=");
            }
        }
    }
    let mut body = vec![0u8; content_len];
    if content_len > 0 {
        reader.read_exact(&mut body)?;
    }

    if !auth_ok {
        return write_http_resp(stream, 403, b"missing or invalid SigV4 header");
    }
    saw_sigv4.store(true, Ordering::Release);

    let (raw_path, raw_query) = target.split_once('?').unwrap_or((target, ""));
    let path = pct_decode(raw_path);
    let rel_after_bucket = path
        .strip_prefix("/lin-cold/")
        .or_else(|| path.strip_prefix("/lin-cold"))
        .unwrap_or("");

    let mut qparams = BTreeMap::new();
    for pair in raw_query.split('&').filter(|s| !s.is_empty()) {
        if let Some((k, v)) = pair.split_once('=') {
            qparams.insert(pct_decode(k), pct_decode(v));
        }
    }

    match method {
        "PUT" => {
            objects
                .lock()
                .unwrap()
                .insert(rel_after_bucket.to_string(), body);
            puts.fetch_add(1, Ordering::AcqRel);
            write_http_resp(stream, 200, b"")
        }
        "GET" if qparams.get("list-type").map(String::as_str) == Some("2") => {
            lists.fetch_add(1, Ordering::AcqRel);
            let pfx = qparams.get("prefix").map(String::as_str).unwrap_or("");
            let start_idx: usize = qparams
                .get("continuation-token")
                .and_then(|t| t.strip_prefix("off:"))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);

            let guard = objects.lock().unwrap();
            let matched: Vec<&String> =
                guard.keys().filter(|k| k.starts_with(pfx)).collect();
            let page_size = 2usize;
            let end = (start_idx + page_size).min(matched.len());
            let is_truncated = end < matched.len();

            let mut xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult>",
            );
            xml.push_str(&format!("<IsTruncated>{is_truncated}</IsTruncated>"));
            if is_truncated {
                xml.push_str(&format!(
                    "<NextContinuationToken>off:{end}</NextContinuationToken>"
                ));
            }
            for k in &matched[start_idx..end] {
                xml.push_str(&format!("<Contents><Key>{k}</Key></Contents>"));
            }
            xml.push_str("</ListBucketResult>");
            write_http_resp(stream, 200, xml.as_bytes())
        }
        "GET" => {
            let guard = objects.lock().unwrap();
            if let Some(bytes) = guard.get(rel_after_bucket) {
                write_http_resp(stream, 200, bytes)
            } else {
                write_http_resp(stream, 404, b"NoSuchKey")
            }
        }
        "DELETE" => {
            objects.lock().unwrap().remove(rel_after_bucket);
            deletes.fetch_add(1, Ordering::AcqRel);
            write_http_resp(stream, 204, b"")
        }
        _ => write_http_resp(stream, 405, b"MethodNotAllowed"),
    }
}

fn write_http_resp(stream: &mut std::net::TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(body)?;
    }
    stream.flush()
}

fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
                && let Ok(v) = u8::from_str_radix(hex, 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn replica_push_wakeup_status_and_lag_cutoff_bootstrap() {
    let pdir = TempDir::new("rep_push_p");
    let fdir1 = TempDir::new("rep_push_f1");
    let fdir2 = TempDir::new("rep_push_f2");

    let mut p_db = Db::open_with(
        pdir.path(),
        OpenOpts {
            cold: false,
            sync: lin::SyncMode::Normal,
        },
    )
    .unwrap();
    p_db.run("col events { id: text, v: i64 }").unwrap();
    p_db.close().unwrap();

    let primary = LinServer::start(
        Db::open_with(
            pdir.path(),
            OpenOpts {
                cold: false,
                sync: lin::SyncMode::Normal,
            },
        )
        .unwrap(),
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Primary,
            ..Default::default()
        },
    )
    .unwrap();
    let p_addr = primary.local_addr().to_string();

    // Follower 1 uses `poll_interval = 200ms` (within long-poll push window), so writes
    // on primary wake `gen_cv` immediately rather than waiting 200ms.
    let f1 = LinServer::start(
        Db::open_follower_with(
            fdir1.path(),
            OpenOpts {
                cold: false,
                sync: lin::SyncMode::Normal,
            },
        )
        .unwrap(),
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Follower,
            follower: Some(FollowerSyncOpts {
                primary_addr: p_addr.clone(),
                client: ClientOpts::default(),
                poll_interval: Duration::from_millis(200),
            }),
            ..Default::default()
        },
    )
    .unwrap();

    let p_cli = ServerClient::connect(&p_addr, ClientOpts::default()).unwrap();
    let rec = p_cli
        .execute(r#"insert events { id: "e1", v: 10 }"#)
        .unwrap();
    f1.wait_gen(rec.r#gen, Duration::from_secs(2)).unwrap();

    // Wait briefly for f1's next long-poll request to report its updated generation.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        let statuses = primary.replica_statuses();
        if statuses
            .iter()
            .any(|s| s.reported_gen >= rec.r#gen && s.lag_gens == 0)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let statuses = primary.replica_statuses();
    assert!(
        statuses.iter().any(|s| s.reported_gen >= rec.r#gen),
        "expected primary.replica_statuses() to report follower gen >= {}, got {statuses:?}",
        rec.r#gen
    );

    // Enforce strict lag cutoff (`max_lag = 2`) and advance primary by 5 generations:
    // direct `pull_sync(0, false)` must return `Bootstrap` instead of WAL tail!
    primary.set_max_replica_lag_gens(Some(2));
    assert_eq!(primary.max_replica_lag_gens(), Some(2));
    for i in 2..=6 {
        p_cli
            .execute(&format!(r#"insert events {{ id: "e{i}", v: {} }}"#, i * 10))
            .unwrap();
    }
    let payload = p_cli.pull_sync(0, false).unwrap();
    assert!(
        matches!(payload, lin::PullSyncPayload::Bootstrap { .. }),
        "expected Bootstrap when lag exceeds max_replica_lag_gens, got {payload:?}"
    );

    // A fresh follower f2 still bootstraps and catches up cleanly under the lag cutoff.
    let f2 = LinServer::start(
        Db::open_follower_with(
            fdir2.path(),
            OpenOpts {
                cold: false,
                sync: lin::SyncMode::Normal,
            },
        )
        .unwrap(),
        ServerConfig {
            bind_addr: "127.0.0.1:0".into(),
            role: NodeRole::Follower,
            follower: Some(FollowerSyncOpts::new(&p_addr, ClientOpts::default())),
            ..Default::default()
        },
    )
    .unwrap();
    f2.wait_gen(primary.r#gen(), Duration::from_secs(2))
        .unwrap();
    let f2_cli = ServerClient::connect(f2.local_addr().to_string(), ClientOpts::default()).unwrap();
    assert_eq!(
        f2_cli.query("events | count").unwrap().to_rows()[0].get("hits"),
        Some(&Cell::Int(6))
    );

    f2.shutdown().unwrap().close().unwrap();
    f1.shutdown().unwrap().close().unwrap();
    primary.shutdown().unwrap().close().unwrap();
}

#[test]
fn remote_integer_filter_downloads_only_matching_segments() {
    let source = TempDir::new("prune_src");
    let cache = TempDir::new("prune_cache");
    let mut disk = lin::DiskEngine::open(source.path(), true).unwrap();
    for n in [10_i64, 20, 30] {
        let mut row = Row::new();
        row.insert("id", Cell::Text(Arc::from(n.to_string())));
        row.insert("code", Cell::Int(n));
        disk.write_bulk_segment("prunable", &[row]).unwrap();
    }
    let objects = Arc::new(MemoryObjectStore::new());
    disk.push_to_object_store(objects.as_ref(), "prune", false)
        .unwrap();
    let mut db = Db::open_cold_from_object_store(
        cache.path(),
        objects.clone(),
        "prune",
        RemoteCacheConfig {
            lazy: true,
            max_local_bytes: u64::MAX,
        },
    )
    .unwrap();
    let gets = objects.counters().gets;
    let filtered = db
        .run_remote("prunable | code >= 20 and code < 30 | count")
        .unwrap();
    assert_eq!(filtered.rows[0].get("hits"), Some(&Cell::Int(1)));
    assert_eq!(objects.counters().gets - gets, 1);
    let gets = objects.counters().gets;
    let empty = db.run_remote("prunable | code > 100 | count").unwrap();
    assert_eq!(empty.rows[0].get("hits"), Some(&Cell::Int(0)));
    assert_eq!(objects.counters().gets, gets);
    let full = db.run_remote("prunable | count").unwrap();
    assert_eq!(full.rows[0].get("hits"), Some(&Cell::Int(3)));
}
