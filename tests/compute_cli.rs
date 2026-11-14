use std::process::Command;

#[cfg(not(feature = "gpu"))]
#[test]
fn gpu_flag_requires_compute_feature() {
    let output = Command::new(env!("CARGO_BIN_EXE_lin"))
        .args(["run", "--gpu", "docs | count"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--features gpu"));
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a hardware GPU"]
fn compute_cli_matches_cpu_for_fixture_and_durable_reopens() {
    let directory = std::env::temp_dir().join(format!("lin-compute-cli-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    std::fs::create_dir(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());
    {
        let mut db = lin::Db::open(&directory).unwrap();
        db.run(
            r#"insert docs { uri: "compute://one", title: "compute shader wal", layer: "wiki" }"#,
        )
        .unwrap();
        db.run(r#"insert docs { uri: "compute://two", title: "other shader", layer: "wiki" }"#)
            .unwrap();
        db.close().unwrap();
    }
    for durable in [false, true] {
        for query in [
            "docs | title ~ \"shader\" | count",
            "docs | title ~ /shader/ | { title } | sort title | take all",
            "docs | search vec \"wal\" | { title } | take all",
            "docs | search \"shader\" | { title } | take all",
        ] {
            let run = |gpu: bool| {
                let mut command = Command::new(env!("CARGO_BIN_EXE_lin"));
                if durable {
                    command.arg("--data").arg(&directory);
                }
                command.arg("run");
                if gpu {
                    command.arg("--gpu");
                }
                let output = command.arg(query).output().unwrap();
                assert!(
                    output.status.success(),
                    "{query}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                output.stdout
            };
            assert_eq!(run(true), run(false), "durable={durable}, {query}");
        }
    }
}

/// `--cold` opens the disk tier from the CLI; `cluster` / `compact` manage a cold
/// collection, and the order they produce survives a reopen.
#[test]
fn cold_cluster_and_compact_commands() {
    let directory = std::env::temp_dir().join(format!("lin-cold-cli-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).unwrap();
    let _cleanup = Cleanup(directory.clone());
    let data = directory.join("db");
    let lin = |args: &[&str]| -> (bool, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_lin"))
            .arg("--data")
            .arg(&data)
            .args(args)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    let mut program = String::from("col ev { id: text, k: i64 }\ninsert ev [");
    for i in 0..2000 {
        if i > 0 {
            program.push(',');
        }
        program.push_str(&format!(r#"{{ id: "e{i}", k: {} }}"#, (i * 7919) % 2000));
    }
    program.push(']');
    let file = directory.join("seed.lin");
    std::fs::write(&file, program).unwrap();
    let (ok, _, err) = lin(&["--cold", "run", "--file", file.to_str().unwrap()]);
    assert!(ok, "{err}");

    // The commands refuse to run against a database opened without the disk tier.
    let (ok, _, err) = lin(&["cluster", "ev"]);
    assert!(!ok);
    assert!(err.contains("--cold"), "{err}");

    assert_eq!(lin(&["--cold", "cluster", "ev"]).1.trim(), "ev: no cluster key");
    let (ok, out, err) = lin(&["--cold", "cluster", "ev", "k"]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "ev: cluster key k");
    let (ok, out, err) = lin(&["--cold", "compact", "ev"]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "ev: compacted");
    // Declared key and physical order persist across processes.
    assert_eq!(lin(&["--cold", "cluster", "ev"]).1.trim(), "ev: cluster key k");
    let (ok, out, err) = lin(&["--cold", "run", "ev | take 3 | pick { k }"]);
    assert!(ok, "{err}");
    let keys: Vec<&str> = out.lines().filter(|l| l.starts_with('{')).collect();
    assert_eq!(keys, ["{k: 0}", "{k: 1}", "{k: 2}"], "{out}");
    let (ok, out, _) = lin(&["--cold", "run", "ev | k >= 100 and k < 110 | count"]);
    assert!(ok);
    assert!(out.contains("{hits: 10}"), "{out}");

    assert_eq!(lin(&["--cold", "cluster", "ev", "--none"]).1.trim(), "ev: cluster key cleared");
    assert_eq!(lin(&["--cold", "cluster", "ev"]).1.trim(), "ev: no cluster key");
    let (ok, _, err) = lin(&["--cold", "compact", "missing"]);
    assert!(!ok);
    assert!(err.contains("not in the disk tier"), "{err}");
}
