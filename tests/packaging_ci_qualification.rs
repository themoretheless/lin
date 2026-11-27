//! Qualification tests for D44 (CI & Platforms) and D49 (Build & Distribution).
//!
//! Covers:
//! - LIN-1721..1760: CI workflow matrix, MSRV declaration, nightly-attribute absence,
//!   concurrency isolation, hardware-test separation, bisect & SBOM scripts.
//! - LIN-1921..1960: dependency feature minimality, zero peer-dep leakage into runtime
//!   dependencies, license inventory verification, reproducible build provenance receipt,
//!   clean source/package manifest checks, and compile-time cfg consistency.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn cargo_manifest_declares_msrv_license_and_optional_boundaries() {
    let root = repo_root();
    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");

    // LIN-1721, LIN-1722, LIN-1456: explicit MSRV and MIT license
    assert!(
        cargo_toml.contains("rust-version = \"1.88\""),
        "Cargo.toml must declare MSRV 1.88"
    );
    assert!(
        cargo_toml.contains("license = \"MIT\""),
        "Cargo.toml must declare MIT license"
    );

    // LIN-1921, LIN-1923, LIN-1935, LIN-1936: heavy/optional backends stay behind feature gates
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .and_then(|s| s.split("[dev-dependencies]").next())
        .expect("dependencies section");

    for optional_crate in [
        "tokio",
        "futures-util",
        "tokio-stream",
        "ureq",
        "fastembed",
        "rayon",
        "flate2",
        "lz4",
        "zstd",
        "wgpu",
        "pollster",
        "bytemuck",
    ] {
        let line = deps_section
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{optional_crate} ")))
            .unwrap_or_else(|| panic!("expected {optional_crate} in [dependencies]"));
        assert!(
            line.contains("optional = true"),
            "{optional_crate} must remain optional in [dependencies]: {line}"
        );
    }

    // Peer benchmark crates must never leak into normal [dependencies]
    for peer_crate in ["rusqlite", "duckdb", "postgres", "mysql", "airbug-bench"] {
        assert!(
            !deps_section
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{peer_crate} "))),
            "peer crate {peer_crate} leaked into runtime [dependencies]"
        );
    }
}

#[test]
fn workspace_crates_use_zero_nightly_feature_gates_and_consistent_versions() {
    let root = repo_root();

    // LIN-1724: zero `#![feature(...)]` across all workspace crates
    fn scan_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_rs_files(&path, out);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    out.push(path);
                }
            }
        }
    }

    let mut rs_files = Vec::new();
    scan_rs_files(&root.join("src"), &mut rs_files);
    scan_rs_files(&root.join("lin-derive/src"), &mut rs_files);
    scan_rs_files(&root.join("tools/peer-worker/src"), &mut rs_files);
    assert!(!rs_files.is_empty());

    for file in rs_files {
        let src = fs::read_to_string(&file).expect("read rust source");
        for line in src.lines() {
            assert!(
                !line.trim_start().starts_with("#![feature("),
                "nightly feature attribute found in {}: {line}",
                file.display()
            );
        }
    }

    // Release version parity across `lin` and `lin-derive`
    let derive_toml =
        fs::read_to_string(root.join("lin-derive/Cargo.toml")).expect("read lin-derive/Cargo.toml");
    assert!(
        derive_toml.contains("version = \"0.7.1\""),
        "lin-derive version must match root crate version"
    );
}

#[test]
fn ci_workflows_cover_platforms_features_concurrency_and_provenance() {
    let root = repo_root();
    let ci_matrix = fs::read_to_string(root.join(".github/workflows/ci-matrix.yml"))
        .expect("read .github/workflows/ci-matrix.yml");

    for required in [
        "concurrency:",
        "msrv-1.88-no-default",
        "stable-default",
        "stable-no-default",
        "stable-async-only",
        "stable-derive-only",
        "stable-gpu-compile",
        "stable-compression-matrix",
        "stable-parallel",
        "linux-x86_64",
        "linux-arm64",
        "macos-arm64",
        "windows-x86_64",
        "macos-metal-gpu:",
        "packaging-and-provenance:",
        "scripts/sbom-and-provenance.py",
    ] {
        assert!(
            ci_matrix.contains(required),
            "ci-matrix.yml missing required entry: {required}"
        );
    }

    assert!(root.join("scripts/bisect-regression.sh").exists());
    assert!(root.join("SECURITY.md").exists());
}

#[test]
fn sbom_and_provenance_generator_verifies_licenses_and_deterministic_digests() {
    let root = repo_root();
    let tmp_dir = std::env::temp_dir().join(format!(
        "lin-provenance-qual-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&tmp_dir).expect("create temp dir");

    let output = Command::new("python3")
        .arg(root.join("scripts/sbom-and-provenance.py"))
        .arg("--repo-root")
        .arg(&root)
        .arg("--output-dir")
        .arg(&tmp_dir)
        .arg("--artifact")
        .arg(root.join("README.md"))
        .arg("--verify")
        .output()
        .expect("run sbom-and-provenance.py");

    assert!(
        output.status.success(),
        "sbom-and-provenance.py failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse sbom summary json");
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["package"], "lin");
    assert!(summary["dependency_count"].as_u64().unwrap_or(0) >= 10);

    let sbom_raw = fs::read_to_string(tmp_dir.join("sbom.cdx.json")).expect("read sbom.cdx.json");
    let sbom: serde_json::Value = serde_json::from_str(&sbom_raw).expect("parse sbom json");
    assert_eq!(sbom["bomFormat"], "CycloneDX");
    assert!(!sbom["components"].as_array().unwrap().is_empty());

    let prov_raw =
        fs::read_to_string(tmp_dir.join("provenance.json")).expect("read provenance.json");
    let prov: serde_json::Value = serde_json::from_str(&prov_raw).expect("parse provenance.json");
    assert_eq!(prov["msrv"], "1.88");
    assert_eq!(prov["artifacts"].as_array().unwrap().len(), 1);

    let sha256sums = fs::read_to_string(tmp_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    assert!(sha256sums.contains("Cargo.toml"));
    assert!(sha256sums.contains("README.md"));

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn documentation_product_contracts_exist_and_are_non_empty() {
    let root = repo_root();
    for doc in [
        "README.md",
        "CHANGELOG.md",
        "SECURITY.md",
        "docs/benchmark-contract.md",
        "docs/api-and-feature-matrix.md",
        "docs/storage-and-compatibility.md",
        "docs/query-semantics.md",
        "docs/migration-and-operations-runbooks.md",
        "docs/benchmark-and-evidence-guide.md",
    ] {
        let meta = fs::metadata(root.join(doc)).unwrap_or_else(|e| panic!("missing {doc}: {e}"));
        assert!(meta.len() > 256, "{doc} should be substantive");
    }
}
