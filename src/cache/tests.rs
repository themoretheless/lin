use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use super::*;

fn spawn_test_node(node_id: &str, max_entries: usize) -> (CacheNode, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let cfg = CacheConfig {
        node_id: node_id.to_string(),
        listen_addr: addr.clone(),
        max_entries,
        max_bytes: 64 * 1024 * 1024,
        vnodes: 32,
        replication_factor: 2,
        sharding_enabled: true,
        raft_replication: true,
        gossip_interval_ms: 100,
        node_timeout_ms: 5_000,
    };
    let node = CacheNode::new_in_memory(cfg).unwrap();
    let srv = node.clone();
    thread::spawn(move || {
        let _ = srv.serve(listener);
    });
    (node, addr)
}

#[test]
fn test_data_structures_and_lin_integration() {
    let (node, addr) = spawn_test_node("node-solo", 50);
    let mut client = CacheClient::connect(&addr).unwrap();

    // 1. Strings & INCR & TTL
    assert_eq!(
        client.execute_line("SET counter 10").unwrap(),
        CacheResponse::Ok
    );
    assert_eq!(
        client.execute_line("INCRBY counter 5").unwrap(),
        CacheResponse::Int(15)
    );
    assert_eq!(
        client.execute_line("GET counter").unwrap(),
        CacheResponse::Bulk(b"15".to_vec())
    );

    // 2. Hashes
    assert_eq!(
        client.execute_line("HSET user:1 name Alice").unwrap(),
        CacheResponse::Int(1)
    );
    assert_eq!(
        client.execute_line("HSET user:1 role admin").unwrap(),
        CacheResponse::Int(1)
    );
    assert_eq!(
        client.execute_line("HGET user:1 name").unwrap(),
        CacheResponse::Bulk(b"Alice".to_vec())
    );

    // 3. Lists
    assert_eq!(
        client.execute_line("RPUSH queue job1 job2 job3").unwrap(),
        CacheResponse::Int(3)
    );
    assert_eq!(
        client.execute_line("LPOP queue").unwrap(),
        CacheResponse::Bulk(b"job1".to_vec())
    );
    assert_eq!(
        client.execute_line("LRANGE queue 0 -1").unwrap(),
        CacheResponse::Array(vec![
            CacheResponse::Bulk(b"job2".to_vec()),
            CacheResponse::Bulk(b"job3".to_vec()),
        ])
    );

    // 4. Sets
    assert_eq!(
        client.execute_line("SADD tags rust db cache rust").unwrap(),
        CacheResponse::Int(3)
    );
    assert_eq!(
        client.execute_line("SISMEMBER tags cache").unwrap(),
        CacheResponse::Int(1)
    );

    // 5. Native Lin query over `cache_kv` and `docs`
    let CacheResponse::Text(lin_out) =
        client.execute_line("LIN cache_kv | { k, vtype }").unwrap()
    else {
        panic!("expected text from LIN query");
    };
    assert!(lin_out.contains("counter"));
    assert!(lin_out.contains("user:1"));
    assert!(lin_out.contains("queue"));
    assert!(lin_out.contains("tags"));

    node.stop();
}

#[test]
fn test_lru_eviction_and_ttl() {
    let (node, addr) = spawn_test_node("node-lru", 3);
    let mut client = CacheClient::connect(&addr).unwrap();

    client.execute_line("SET k1 v1").unwrap();
    client.execute_line("SET k2 v2").unwrap();
    client.execute_line("SET k3 v3").unwrap();
    // Touch k1 so k2 becomes least-recently-used
    assert_eq!(
        client.execute_line("GET k1").unwrap(),
        CacheResponse::Bulk(b"v1".to_vec())
    );
    // Inserting k4 exceeds max_entries=3 -> should evict k2
    client.execute_line("SET k4 v4").unwrap();
    assert_eq!(client.execute_line("GET k2").unwrap(), CacheResponse::Nil);
    assert_eq!(
        client.execute_line("GET k1").unwrap(),
        CacheResponse::Bulk(b"v1".to_vec())
    );

    // Short TTL expiration
    client.execute_line("SET temp secret PX 40").unwrap();
    thread::sleep(Duration::from_millis(60));
    assert_eq!(client.execute_line("GET temp").unwrap(), CacheResponse::Nil);

    node.stop();
}

#[test]
fn test_multi_node_cluster_sharding_gossip_raft_and_pubsub() {
    let (n1, addr1) = spawn_test_node("node-1", 1000);
    let (n2, addr2) = spawn_test_node("node-2", 1000);
    let (n3, addr3) = spawn_test_node("node-3", 1000);

    let mut c1 = CacheClient::connect(&addr1).unwrap();
    let mut c2 = CacheClient::connect(&addr2).unwrap();
    let mut c3 = CacheClient::connect(&addr3).unwrap();

    // Join nodes via CLUSTER MEET
    assert_eq!(
        c1.execute_line(&format!("CLUSTER MEET node-2 {addr2}"))
            .unwrap(),
        CacheResponse::Ok
    );
    assert_eq!(
        c1.execute_line(&format!("CLUSTER MEET node-3 {addr3}"))
            .unwrap(),
        CacheResponse::Ok
    );
    // Allow gossip to converge across all 3 nodes
    let _ = n2.gossip_once_with(&addr1);
    let _ = n3.gossip_once_with(&addr1);
    let _ = n2.gossip_once_with(&addr3);

    // Verify cluster membership sees 3 nodes
    let CacheResponse::Text(nodes_txt) = c2.execute_line("CLUSTER NODES").unwrap() else {
        panic!("expected text");
    };
    assert!(nodes_txt.contains("node-1"));
    assert!(nodes_txt.contains("node-2"));
    assert!(nodes_txt.contains("node-3"));

    // Trigger Raft leader election on node-1
    let CacheResponse::Text(elect_txt) = c1.execute_line("RAFT ELECT").unwrap() else {
        panic!("expected election status");
    };
    assert!(elect_txt.contains("election_won=true"));

    // Write 20 keys through node-1; Consistent Hashing routes each key to its primary shard,
    // and Raft replicates mutations across peers!
    for i in 0..20 {
        let cmd = format!("SET item:{i} val-{i}");
        assert_eq!(c1.execute_line(&cmd).unwrap(), CacheResponse::Ok);
    }

    // Read all 20 keys from node-3 (transparent shard routing / replicated state)
    for i in 0..20 {
        let resp = c3.execute_line(&format!("GET item:{i}")).unwrap();
        assert_eq!(resp, CacheResponse::Bulk(format!("val-{i}").into_bytes()));
    }

    // Test Cross-Node Pub/Sub: subscribe on node-3, publish on node-1
    let addr3_sub = addr3.clone();
    let sub_handle = thread::spawn(move || {
        let mut sub_client = CacheClient::connect(&addr3_sub).unwrap();
        let ack = sub_client.execute_line("SUBSCRIBE events").unwrap();
        assert!(matches!(ack, CacheResponse::Text(_)));
        sub_client.read_next().unwrap()
    });

    thread::sleep(Duration::from_millis(50));
    c1.execute_line("PUBLISH events \"hello-cluster\"").unwrap();

    let received = sub_handle.join().unwrap();
    assert_eq!(
        received,
        CacheResponse::PubSubMessage {
            channel: "events".to_string(),
            payload: b"hello-cluster".to_vec(),
            origin_node: "node-1".to_string(),
        }
    );

    n1.stop();
    n2.stop();
    n3.stop();
}

#[test]
fn test_durable_persistence_across_restart() {
    let tmp_dir = std::env::temp_dir().join(format!("lin_cacha_test_{}", unix_ms()));
    let _ = std::fs::remove_dir_all(&tmp_dir);

    let cfg = CacheConfig {
        node_id: "durable-1".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
        ..CacheConfig::default()
    };

    {
        let node = CacheNode::open_durable(&tmp_dir, cfg.clone()).unwrap();
        assert_eq!(
            node.execute(parse_cli_command("SET persist_str \"survives_restart\"").unwrap()),
            CacheResponse::Ok
        );
        assert_eq!(
            node.execute(parse_cli_command("HSET persist_hash f1 val1").unwrap()),
            CacheResponse::Int(1)
        );
        assert_eq!(
            node.execute(parse_cli_command("SADD persist_set m1 m2").unwrap()),
            CacheResponse::Int(2)
        );
        // Checkpoint & close underlying Db cleanly
        let mut st = node.state.write().unwrap();
        st.db.close().unwrap();
    }

    // Reopen durable CacheNode from the same `lin::Db` directory
    {
        let node2 = CacheNode::open_durable(&tmp_dir, cfg).unwrap();
        assert_eq!(
            node2.execute(parse_cli_command("GET persist_str").unwrap()),
            CacheResponse::Bulk(b"survives_restart".to_vec())
        );
        assert_eq!(
            node2.execute(parse_cli_command("HGET persist_hash f1").unwrap()),
            CacheResponse::Bulk(b"val1".to_vec())
        );
        assert_eq!(
            node2.execute(parse_cli_command("SISMEMBER persist_set m2").unwrap()),
            CacheResponse::Int(1)
        );
        let mut st = node2.state.write().unwrap();
        st.db.close().unwrap();
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn concurrent_read_modify_write_commands_do_not_lose_updates() {
    let cfg = CacheConfig {
        node_id: "node-rmw".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
        max_entries: 1_000,
        max_bytes: 64 * 1024 * 1024,
        vnodes: 32,
        replication_factor: 1,
        sharding_enabled: false,
        raft_replication: false,
        gossip_interval_ms: 100,
        node_timeout_ms: 5_000,
    };
    let node = CacheNode::new_in_memory(cfg).unwrap();
    const THREADS: usize = 8;
    const PER_THREAD: usize = 500;
    thread::scope(|s| {
        for t in 0..THREADS {
            let node = &node;
            s.spawn(move || {
                for i in 0..PER_THREAD {
                    node.execute(CacheRequest::IncrBy {
                        key: "hits".into(),
                        delta: 1,
                    });
                    node.execute(CacheRequest::RPush {
                        key: "queue".into(),
                        values: vec![format!("{t}-{i}").into_bytes()],
                    });
                }
            });
        }
    });
    let total = (THREADS * PER_THREAD) as i64;
    assert_eq!(
        node.execute(CacheRequest::IncrBy {
            key: "hits".into(),
            delta: 0,
        }),
        CacheResponse::Int(total)
    );
    assert_eq!(
        node.execute(CacheRequest::RPush {
            key: "queue".into(),
            values: Vec::new(),
        }),
        CacheResponse::Int(total)
    );
}

#[test]
fn slab_lru_slot_recycling_second_chance_and_exact_byte_accounting() {
    use std::sync::atomic::Ordering;
    use super::store::NUM_SHARDS;

    let cfg = CacheConfig {
        node_id: "node-slab".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
        max_entries: 128,
        max_bytes: 64 * 1024 * 1024,
        vnodes: 32,
        replication_factor: 1,
        sharding_enabled: false,
        raft_replication: false,
        gossip_interval_ms: 100,
        node_timeout_ms: 5_000,
    };
    let node = CacheNode::new_in_memory(cfg).unwrap();

    // Populate 128 keys, delete half, reinsert 64 new keys on the same shards and verify slab
    // free-list recycling + exact byte/entry accounting across all data structures.
    for i in 0..128 {
        node.execute(CacheRequest::Set {
            key: format!("k:{i}"),
            value: format!("val-{i:04}").into_bytes(),
            ttl_ms: None,
        });
    }
    assert_eq!(node.store.total_entries.load(Ordering::Acquire), 128);

    let peak_slots_before_del: usize = (0..NUM_SHARDS)
        .map(|s| node.store.stripe(s).lock.read().unwrap().slots.len())
        .sum();
    assert_eq!(peak_slots_before_del, 128);

    // Delete all 128 keys and reinsert 128 fresh keys -> total slab slots across shards stays
    // bounded by recycled free_head slots where shard distribution matches!
    for i in 0..128 {
        assert_eq!(
            node.execute(CacheRequest::Del {
                keys: vec![format!("k:{i}")],
            }),
            CacheResponse::Int(1)
        );
    }
    assert_eq!(node.store.total_entries.load(Ordering::Acquire), 0);
    assert_eq!(node.store.used_bytes.load(Ordering::Acquire), 0);

    // Reinsert the exact same 128 keys -> every shard reuses its `free_head` slots with 0 growth!
    for i in 0..128 {
        node.execute(CacheRequest::Set {
            key: format!("k:{i}"),
            value: format!("new-value-{i:06}").into_bytes(),
            ttl_ms: None,
        });
    }
    let slots_after_reinsert: usize = (0..NUM_SHARDS)
        .map(|s| node.store.stripe(s).lock.read().unwrap().slots.len())
        .sum();
    assert_eq!(slots_after_reinsert, peak_slots_before_del);

    // Exercise in-place mutations (INCRBY, HSET, HDEL, LPUSH, LPOP, SADD, SREM) and verify
    // global `used_bytes` matches the exact sum of live entries across all 64 shards.
    node.execute(CacheRequest::FlushAll);
    assert_eq!(node.store.used_bytes.load(Ordering::Acquire), 0);
    assert_eq!(node.store.total_entries.load(Ordering::Acquire), 0);

    node.execute(CacheRequest::Set {
        key: "num".into(),
        value: b"100".to_vec(),
        ttl_ms: None,
    });
    node.execute(CacheRequest::IncrBy {
        key: "num".into(),
        delta: 42,
    });
    node.execute(CacheRequest::HSet {
        key: "h".into(),
        field: "f1".into(),
        value: b"alpha".to_vec(),
    });
    node.execute(CacheRequest::HSet {
        key: "h".into(),
        field: "f1".into(),
        value: b"alphabet".to_vec(),
    });
    node.execute(CacheRequest::HSet {
        key: "h".into(),
        field: "f2".into(),
        value: b"beta".to_vec(),
    });
    node.execute(CacheRequest::HDel {
        key: "h".into(),
        fields: vec!["f2".into()],
    });
    node.execute(CacheRequest::RPush {
        key: "l".into(),
        values: vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()],
    });
    assert_eq!(
        node.execute(CacheRequest::LPop { key: "l".into() }),
        CacheResponse::Bulk(b"one".to_vec())
    );
    node.execute(CacheRequest::SAdd {
        key: "s".into(),
        members: vec!["x".into(), "y".into(), "z".into()],
    });
    node.execute(CacheRequest::SRem {
        key: "s".into(),
        members: vec!["y".into()],
    });

    let mut expected_bytes = 0usize;
    let mut expected_entries = 0usize;
    for s in 0..NUM_SHARDS {
        let sh = node.store.stripe(s).lock.read().unwrap();
        let mut shard_sum = 0usize;
        for (k, &slot_idx) in &sh.entries {
            let occ = sh.occupied(slot_idx);
            let exact = k.len() + occ.entry.value.estimated_bytes();
            assert_eq!(occ.entry_bytes, exact);
            shard_sum += exact;
            expected_entries += 1;
        }
        assert_eq!(sh.used_bytes, shard_sum);
        expected_bytes += shard_sum;
    }
    assert_eq!(node.store.total_entries.load(Ordering::Acquire), expected_entries);
    assert_eq!(node.store.used_bytes.load(Ordering::Acquire), expected_bytes);
}

#[test]
fn sharded_64_concurrent_mixed_reads_writes_and_evictions() {
    use std::sync::atomic::Ordering;
    use super::store::NUM_SHARDS;

    let cfg = CacheConfig {
        node_id: "node-64shard".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
        max_entries: 512,
        max_bytes: 64 * 1024 * 1024,
        vnodes: 32,
        replication_factor: 1,
        sharding_enabled: false,
        raft_replication: false,
        gossip_interval_ms: 100,
        node_timeout_ms: 5_000,
    };
    let node = CacheNode::new_in_memory(cfg).unwrap();

    const THREADS: usize = 8;
    const OPS: usize = 400;
    thread::scope(|s| {
        for t in 0..THREADS {
            let node = &node;
            s.spawn(move || {
                for i in 0..OPS {
                    let k = format!("stripe:{t}:{i}");
                    node.execute(CacheRequest::Set {
                        key: k.clone(),
                        value: format!("payload-{t}-{i}").into_bytes(),
                        ttl_ms: if i % 10 == 0 { Some(5_000) } else { None },
                    });
                    let _ = node.execute(CacheRequest::Get { key: k });
                    node.execute(CacheRequest::IncrBy {
                        key: format!("counter:{}", i % 16),
                        delta: 1,
                    });
                }
            });
        }
    });

    let live_entries = node.store.total_entries.load(Ordering::Acquire);
    assert!(
        live_entries <= 512,
        "expected <= 512 entries after concurrent eviction, got {live_entries}"
    );

    let mut actual_entries = 0usize;
    let mut actual_bytes = 0usize;
    for s in 0..NUM_SHARDS {
        let sh = node.store.stripe(s).lock.read().unwrap();
        actual_entries += sh.entries.len();
        actual_bytes += sh.used_bytes;
    }
    assert_eq!(live_entries, actual_entries);
    assert_eq!(node.store.used_bytes.load(Ordering::Acquire), actual_bytes);
}
