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
        let mut st = node.state.lock().unwrap();
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
        let mut st = node2.state.lock().unwrap();
        st.db.close().unwrap();
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
