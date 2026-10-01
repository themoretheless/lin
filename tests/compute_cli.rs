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
