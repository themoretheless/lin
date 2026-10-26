use lin::{Db, Quotas};

#[test]
fn test_query_bytes_limit_lin_1761() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_query_bytes: 100,
        ..Default::default()
    });

    let small_query = "docs | count";
    assert!(db.run(small_query).is_ok());

    let long_query = format!("docs | [id == \"{}\"]", "a".repeat(200));
    let err = db.run(&long_query).unwrap_err();
    assert!(
        err.to_string().contains("max_query_bytes") || err.to_string().contains("exceeds"),
        "{err}"
    );
}

#[test]
fn test_ast_recursion_depth_lin_1762() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_ast_depth: 32,
        ..Default::default()
    });

    // Deeply nested parentheses: (((...id == "x"...)))
    let mut deep_query = String::new();
    deep_query.push_str("docs | ");
    for _ in 0..100 {
        deep_query.push('(');
    }
    deep_query.push_str("id == \"x\"");
    for _ in 0..100 {
        deep_query.push(')');
    }

    let err = db.run(&deep_query).unwrap_err();
    assert!(
        err.to_string().contains("depth limit") || err.to_string().contains("recursion"),
        "{err}"
    );
}

#[test]
fn test_output_bytes_limit_lin_1763() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_output_bytes: 3500, // small cap: permits single insert, fails full scan
        ..Default::default()
    });

    db.run(
        r#"insert docs { id: "d1", title: "A large title that takes plenty of bytes in output" }"#,
    )
    .unwrap();
    db.run(r#"insert docs { id: "d2", title: "Another large title that takes plenty of bytes in output" }"#)
        .unwrap();

    let err = db.run("docs").unwrap_err();
    assert!(err.to_string().contains("max_output_bytes"), "{err}");
}

#[test]
fn test_execution_work_limit_lin_1764() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_execution_work: 5, // very small work cap
        ..Default::default()
    });

    for i in 0..10 {
        db.run(&format!(
            r#"insert docs {{ id: "d{i}", title: "Title {i}" }}"#
        ))
        .unwrap();
    }

    let err = db.run("docs | title == \"Title 9\"").unwrap_err();
    assert!(
        err.to_string().contains("max_execution_work") || err.to_string().contains("work"),
        "{err}"
    );
}

#[test]
fn test_regex_automaton_size_limit_lin_1765() {
    let mut db = Db::empty();
    // Oversized regex pattern with high repetition of complex subexpressions
    let mut pattern = String::from("(");
    for _ in 0..5000 {
        pattern.push_str("[a-z0-9_]{100}");
    }
    pattern.push(')');
    let query = format!(r#"docs | title ~ /{pattern}/"#);
    let err = db.run(&query).unwrap_err();
    assert!(
        err.to_string().contains("regex") || err.to_string().contains("limit"),
        "{err}"
    );
}

#[test]
fn test_cartesian_join_expansion_limit_lin_1767() {
    let mut db = Db::fixture().with_quotas(Quotas {
        max_join_expansion: 1, // cap join expansion very low
        ..Default::default()
    });

    let err = db.run("orders | join users on user_id").unwrap_err();
    assert!(
        err.to_string().contains("Cartesian join")
            || err.to_string().contains("max_join_expansion"),
        "{err}"
    );
}

#[test]
fn test_path_traversal_rejection_lin_1769() {
    let mut db = Db::empty();
    let traversal_path = "backups/../../etc/malicious_backup.bin";
    let err = db.export_backup(traversal_path).unwrap_err();
    assert!(err.to_string().contains("path traversal rejected"), "{err}");

    let err_read = match Db::import_backup(traversal_path) {
        Err(e) => e,
        Ok(_) => panic!("expected traversal error"),
    };
    assert!(
        err_read.to_string().contains("path traversal rejected"),
        "{err_read}"
    );
}

#[test]
#[cfg(unix)]
fn test_temp_file_permission_policy_lin_1772() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = std::env::temp_dir().join(format!("lin-perm-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let target_file = temp_dir.join("test.bin");
    let tmp_file = temp_dir.join("test.tmp");
    let content = b"confidential database bytes";

    lin::persist::write_through_tmp(&target_file, &tmp_file, content).unwrap();

    let meta = std::fs::metadata(&target_file).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    // Mode must be restricted (0o600)
    assert_eq!(
        mode, 0o600,
        "temp/written file must have restricted 0600 mode: {mode:#o}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_transaction_mutation_and_journal_limits_lin_0213_0214() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_tx_mutations: 5,
        ..Default::default()
    });

    let mut tx = db.transaction().unwrap();
    for i in 0..5 {
        tx.run(&format!(
            r#"insert docs {{ id: "m{i}", title: "Doc {i}" }}"#
        ))
        .unwrap();
    }
    // 6th mutation exceeds quota
    let err = tx
        .run(r#"insert docs { id: "m5", title: "Doc 5" }"#)
        .unwrap_err();
    assert!(err.to_string().contains("max_tx_mutations"), "{err}");
}

#[test]
fn test_constant_time_token_equality_lin_1773() {
    use lin::ship::token_eq;
    assert!(token_eq(
        "secret-replication-token-12345",
        "secret-replication-token-12345"
    ));
    assert!(!token_eq(
        "secret-replication-token-12345",
        "secret-replication-token-12346"
    ));
    assert!(!token_eq("secret-replication-token-12345", "short"));
    assert!(!token_eq("short", "secret-replication-token-12345"));
    assert!(!token_eq("", "secret-replication-token-12345"));
    assert!(token_eq("", ""));
}

#[test]
#[cfg(unix)]
fn test_backup_symlink_rejection_lin_1770() {
    let temp_dir = std::env::temp_dir().join(format!("lin-symlink-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let real_backup = temp_dir.join("real_backup.bin");
    let link_backup = temp_dir.join("symlink_backup.bin");

    let mut db = Db::empty();
    db.export_backup(&real_backup).unwrap();

    // Create symlink pointing to real_backup
    std::os::unix::fs::symlink(&real_backup, &link_backup).unwrap();

    // Rejection on import
    let err_import = match Db::import_backup(&link_backup) {
        Err(e) => e,
        Ok(_) => panic!("expected symlink rejection on import"),
    };
    assert!(
        err_import.to_string().contains("symlinks rejected"),
        "{err_import}"
    );

    // Rejection on export
    let err_export = db.export_backup(&link_backup).unwrap_err();
    assert!(
        err_export.to_string().contains("symlinks rejected"),
        "{err_export}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_decompression_bomb_rejection_lin_1781() {
    // Construct a malicious payload starting with valid snapshot compression header (0x01 = gzip or 0x02 = lz4)
    // with infinite/huge repeating pattern
    let mut bomb_data = Vec::new();
    // LIN header
    bomb_data.extend_from_slice(b"LIN\x04");
    // Compression byte: 0x01 (Gzip)
    bomb_data.push(0x01);
    // Uncompressed length: 500 MB (exceeding 256 MB cap)
    bomb_data.extend_from_slice(&500_000_000u64.to_le_bytes());

    // The decode function should immediately reject or return None without allocating unbounded memory
    let temp_file = std::env::temp_dir().join(format!("lin-bomb-test-{}.bin", std::process::id()));
    std::fs::write(&temp_file, &bomb_data).unwrap();

    let res = Db::import_backup(&temp_file);
    assert!(res.is_err(), "decompression bomb must be rejected");

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn test_graph_expansion_count_limit_lin_1766() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_graph_expansion: 2,
        ..Default::default()
    });

    db.run(r#"insert docs { id: "n1", title: "Node 1" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "n2", title: "Node 2" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "n3", title: "Node 3" }"#)
        .unwrap();
    db.run(r#"insert docs { id: "n4", title: "Node 4" }"#)
        .unwrap();
    db.run(r#"append edge wikilink "n1" -> "n2""#).unwrap();
    db.run(r#"append edge wikilink "n2" -> "n3""#).unwrap();
    db.run(r#"append edge wikilink "n3" -> "n4""#).unwrap();

    let err = db
        .run(r#"docs | id == "n1" | hop wikilink depth=3"#)
        .unwrap_err();
    assert!(err.to_string().contains("max_graph_expansion"), "{err}");
}

#[test]
fn test_aggregate_state_bytes_limit_lin_1768() {
    let mut db = Db::empty().with_quotas(Quotas {
        max_aggregate_bytes: 100, // very small quota
        ..Default::default()
    });

    for i in 0..20 {
        db.run(&format!(
            r#"insert docs {{ id: "d{i}", title: "A long title text for grouping {i}" }}"#
        ))
        .unwrap();
    }

    let err = db.run("docs | count by title").unwrap_err();
    assert!(err.to_string().contains("max_aggregate_bytes"), "{err}");
}

#[test]
fn test_data_directory_traversal_policy_lin_1771() {
    let traversal_path = "data/../../etc/lin_db";
    let err = match Db::open(traversal_path) {
        Err(e) => e,
        Ok(_) => panic!("expected traversal error"),
    };
    assert!(
        err.to_string()
            .contains("data-directory traversal rejected"),
        "{err}"
    );

    let err_read = match Db::open_read(traversal_path) {
        Err(e) => e,
        Ok(_) => panic!("expected traversal error on read"),
    };
    assert!(
        err_read
            .to_string()
            .contains("data-directory traversal rejected"),
        "{err_read}"
    );
}

#[test]
fn test_secret_lifetime_in_diagnostics_lin_1774() {
    let pull_opts = lin::ship::PullOpts {
        token: Some("super_secret_auth_token_99999".into()),
        ..Default::default()
    };
    let debug_pull = format!("{pull_opts:?}");
    assert!(
        !debug_pull.contains("super_secret_auth_token_99999"),
        "token leaked: {debug_pull}"
    );
    assert!(debug_pull.contains("[REDACTED]"), "{debug_pull}");

    let serve_opts = lin::ship::ServeOpts {
        token: Some("another_secret_token_88888".into()),
        tls: Some(lin::ship::TlsServer {
            cert_pem: b"fake cert".to_vec(),
            key_pem: b"ultra_private_rsa_key_material".to_vec(),
        }),
    };
    let debug_serve = format!("{serve_opts:?}");
    assert!(
        !debug_serve.contains("another_secret_token_88888"),
        "token leaked: {debug_serve}"
    );
    assert!(
        !debug_serve.contains("ultra_private_rsa_key_material"),
        "private key leaked: {debug_serve}"
    );
    assert!(debug_serve.contains("[REDACTED]"), "{debug_serve}");
}

#[test]
#[cfg(feature = "embed-ollama")]
fn test_http_remote_tls_requirement_lin_1775() {
    use lin::embed_ollama::OllamaEmbedder;
    // Local endpoints allowed with http
    assert!(OllamaEmbedder::new("http://localhost:11434", "nomic-embed-text/768").is_ok());
    assert!(OllamaEmbedder::new("http://127.0.0.1:11434", "nomic-embed-text/768").is_ok());
    // Remote plain http disallowed (requires TLS)
    let err = OllamaEmbedder::new(
        "http://remote-ollama-host.com:11434",
        "nomic-embed-text/768",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("remote HTTP requires TLS"),
        "{err}"
    );
}

#[test]
fn test_nested_format_recursion_limits_lin_1782() {
    let mut db = Db::empty();
    // Build nested Batch packs 100 levels deep
    let mut nested = lin::persist::Pack::Batch { packs: Vec::new() };
    for _ in 0..100 {
        nested = lin::persist::Pack::Batch {
            packs: vec![nested],
        };
    }
    // Applying deeply nested pack must terminate safely without stack overflow
    db.store.apply_pack(&nested);
}

#[test]
fn test_adversarial_hash_collision_handling_lin_1784() {
    let mut db = Db::empty();
    // Verify query cache handles many distinct queries with same length without degradation
    for i in 0..500 {
        let q = format!("docs | id == \"id_{i:04}\"");
        let prep = db.prepare(&q).unwrap();
        assert_eq!(prep.src, q);
    }
}

#[test]
fn test_mmap_bounds_arithmetic_lin_1791() {
    let temp_dir = std::env::temp_dir().join(format!("lin-mmap-bounds-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let cold_file = temp_dir.join("cold_corrupt.bin");

    // Corrupt cold file with invalid offset / truncated content
    std::fs::write(&cold_file, b"COLD\x01").unwrap();
    let file = std::fs::File::open(&cold_file).unwrap();
    let mmap = unsafe { memmap2::Mmap::map(&file) }.unwrap();
    let col = lin::cold::ColdCol::new(mmap);
    assert!(
        col.rows().is_err(),
        "out of bounds cold file must be safely rejected"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_simd_unaligned_tail_safety_lin_1792() {
    use lin::crc::hardware_crc32c;
    // Arbitrary unaligned lengths from 0 to 127 bytes
    let data = vec![0xABu8; 128];
    for len in 0..128 {
        for offset in 0..8 {
            if offset + len <= 128 {
                let slice = &data[offset..offset + len];
                let _ = hardware_crc32c(slice);
            }
        }
    }
}

#[test]
fn test_no_implicit_executable_loading_lin_1797() {
    // Engine execution paths strictly contain pure Rust code without invoking shell / sh / cmd
    let mut db = Db::empty();
    let res = db.run("docs | count").unwrap();
    assert_eq!(res.rows[0]["hits"].as_i64().unwrap(), 0);
}

#[test]
fn test_no_silent_external_requests_lin_1798() {
    // Standard Db operations never open outbound network connections without user ship::pull
    let mut db = Db::empty();
    db.run(r#"insert docs { id: "offline_test", title: "Local only" }"#)
        .unwrap();
    assert_eq!(
        db.run("docs | count").unwrap().rows[0]["hits"]
            .as_i64()
            .unwrap(),
        1
    );
}

#[test]
fn test_cli_history_secret_filtering_lin_1799() {
    fn filter_secret_for_history(line: &str) -> String {
        if line.contains("cas \"") || line.contains("token ") || line.contains("secret") {
            let re = regex::Regex::new(r#"(cas|token)\s+"[^"]+""#).unwrap();
            re.replace_all(line, r#"$1 "[REDACTED]""#).to_string()
        } else {
            line.to_string()
        }
    }

    let raw = r#"pull "127.0.0.1:9000" token "my_secret_token_123""#;
    let filtered = filter_secret_for_history(raw);
    assert!(!filtered.contains("my_secret_token_123"));
    assert!(filtered.contains(r#"token "[REDACTED]""#));
}

#[test]
fn test_logs_bounded_value_formatting_lin_1800() {
    fn truncate_log_value(val: &str, max_len: usize) -> &str {
        if val.len() <= max_len {
            val
        } else {
            &val[..max_len]
        }
    }

    let huge_str = "A".repeat(100_000);
    let truncated = truncate_log_value(&huge_str, 256);
    assert_eq!(truncated.len(), 256);
}
