#!/usr/bin/env python3
"""Generate SBOM, dependency license inventory, and build provenance receipts.

Covers roadmap items:
- LIN-1795 (dependency provenance inventory)
- LIN-1921 / LIN-1923 / LIN-1924 (feature minimality, public leakage, license inventory)
- LIN-1945 / LIN-1946 / LIN-1947 (release manifest, CycloneDX SBOM, build provenance)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ALLOWED_LICENSE_TOKENS = {
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
    "0BSD",
    "CC0-1.0",
    "BSL-1.0",
    "MPL-2.0",
    "CDLA-Permissive-2.0",
    "OpenSSL",
}


def run_cmd(cmd: list[str], cwd: Path) -> str:
    proc = subprocess.run(cmd, cwd=cwd, check=True, capture_output=True, text=True)
    return proc.stdout.strip()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def license_is_allowed(expr: str | None, pkg_name: str, source: str | None) -> bool:
    if pkg_name in {"lin", "lin-derive", "lin-peer-worker"}:
        return expr == "MIT"
    if source and "github.com/themoretheless/airbug" in source:
        return True
    if not expr:
        return False
    normalized = expr.replace("(", " ").replace(")", " ")
    for alt in normalized.split(" OR "):
        parts = [p.strip() for p in alt.split(" AND ") if p.strip()]
        if parts and all(
            any(tok in p for tok in ALLOWED_LICENSE_TOKENS) for p in parts
        ):
            return True
    return any(tok in expr for tok in ALLOWED_LICENSE_TOKENS)


def collect_metadata(repo_root: Path) -> dict:
    raw = run_cmd(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(repo_root / "Cargo.toml")],
        cwd=repo_root,
    )
    return json.loads(raw)


def build_sbom_and_provenance(repo_root: Path, artifacts: list[Path]) -> tuple[dict, dict, list[str]]:
    meta = collect_metadata(repo_root)
    violations: list[str] = []

    lin_pkg = next((p for p in meta["packages"] if p["name"] == "lin"), None)
    if lin_pkg is None:
        raise RuntimeError("workspace package 'lin' not found in cargo metadata")

    # Verify runtime vs optional dependencies on `lin` (LIN-1921, LIN-1923)
    heavy_optional = {"tokio", "wgpu", "ureq", "fastembed", "rayon", "flate2", "lz4", "zstd"}
    for dep in lin_pkg["dependencies"]:
        if dep.get("kind") is None and dep["name"] in heavy_optional and not dep.get("optional", False):
            violations.append(f"heavy dependency '{dep['name']}' must remain optional")
        if dep.get("kind") is None and dep["name"] in {"duckdb", "rusqlite", "postgres", "mysql"}:
            violations.append(f"peer benchmark dependency '{dep['name']}' leaked into normal dependencies")

    components = []
    for pkg in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
        lic = pkg.get("license")
        src = pkg.get("source")
        if not license_is_allowed(lic, pkg["name"], src):
            violations.append(f"disallowed or missing license on {pkg['name']}@{pkg['version']}: {lic!r}")
        components.append(
            {
                "type": "library",
                "name": pkg["name"],
                "version": pkg["version"],
                "purl": f"pkg:cargo/{pkg['name']}@{pkg['version']}",
                "license": lic or "WORKSPACE-INTERNAL",
                "source": src or "workspace",
                "rust_version": pkg.get("rust_version"),
            }
        )

    sbom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": lin_pkg["name"],
                "version": lin_pkg["version"],
                "license": lin_pkg.get("license"),
            },
        },
        "components": components,
    }

    try:
        git_sha = run_cmd(["git", "rev-parse", "HEAD"], cwd=repo_root)
    except Exception:
        git_sha = "unknown"
    try:
        rustc_ver = run_cmd(["rustc", "--version"], cwd=repo_root)
    except Exception:
        rustc_ver = "unknown"

    cargo_toml_sha = sha256_file(repo_root / "Cargo.toml")
    cargo_lock_sha = sha256_file(repo_root / "Cargo.lock") if (repo_root / "Cargo.lock").exists() else None

    artifact_digests = []
    for art in artifacts:
        if art.exists() and art.is_file():
            artifact_digests.append(
                {
                    "path": str(art.relative_to(repo_root)) if art.is_relative_to(repo_root) else str(art),
                    "size_bytes": art.stat().st_size,
                    "sha256": sha256_file(art),
                }
            )

    provenance = {
        "schema_version": 1,
        "package": lin_pkg["name"],
        "version": lin_pkg["version"],
        "msrv": lin_pkg.get("rust_version"),
        "git_commit": git_sha,
        "rustc_version": rustc_ver,
        "host": {
            "system": platform.system(),
            "machine": platform.machine(),
        },
        "generated_at_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "manifests": {
            "cargo_toml_sha256": cargo_toml_sha,
            "cargo_lock_sha256": cargo_lock_sha,
        },
        "features_declared": sorted(lin_pkg.get("features", {}).keys()),
        "dependency_count": len(components),
        "artifacts": artifact_digests,
    }
    manifest_payload = json.dumps(provenance, sort_keys=True).encode("utf-8")
    provenance["manifest_sha256"] = hashlib.sha256(manifest_payload).hexdigest()

    return sbom, provenance, violations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--output-dir", type=Path, default=None)
    parser.add_argument("--artifact", type=Path, action="append", default=[])
    parser.add_argument("--verify", action="store_true", help="Fail with exit 1 on policy violations")
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    sbom, provenance, violations = build_sbom_and_provenance(repo_root, args.artifact)

    if args.output_dir is not None:
        out = args.output_dir.resolve()
        out.mkdir(parents=True, exist_ok=True)
        (out / "sbom.cdx.json").write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        (out / "provenance.json").write_text(
            json.dumps(provenance, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        checksums = [f"{ provenance['manifests']['cargo_toml_sha256'] }  Cargo.toml"]
        if provenance["manifests"]["cargo_lock_sha256"]:
            checksums.append(f"{ provenance['manifests']['cargo_lock_sha256'] }  Cargo.lock")
        for item in provenance["artifacts"]:
            checksums.append(f"{item['sha256']}  {item['path']}")
        (out / "SHA256SUMS").write_text("\n".join(checksums) + "\n", encoding="utf-8")

    if violations and args.verify:
        for v in violations:
            print(f"POLICY VIOLATION: {v}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "status": "ok" if not violations else "warnings",
                "package": provenance["package"],
                "version": provenance["version"],
                "dependency_count": provenance["dependency_count"],
                "manifest_sha256": provenance["manifest_sha256"],
                "violations": violations,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
