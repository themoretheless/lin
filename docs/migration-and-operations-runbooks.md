# Migration Cookbooks, Operational Runbooks, and Release Checklist

This guide covers query/schema/embedding/backup migrations, incident response runbooks (`ENOSPC`, corruption, follower lag, safe upgrades/rollbacks), offline builds, artifact verification, dependency provenance, and release gating (`LIN-1440`, `LIN-1794`, `LIN-1795`, `LIN-1913`–`LIN-1916`, `LIN-1948`, `LIN-1958`–`LIN-1960`, `LIN-1977`–`LIN-1980`).

---

## 1. Migration Cookbooks (`LIN-1977`–`LIN-1980`)

### 1.1 Query Migration Cookbook (`LIN-1977`)
- **SQL `SELECT a, b FROM t WHERE x > 10 ORDER BY y DESC LIMIT 20 OFFSET 40`** →
  `t | x > 10 | sort y desc | skip 40 | take 20 | { a, b }` (prefer keyset `.after("y", last_y)` over deep `skip`).
- **SQL `SELECT dept, COUNT(*), SUM(sal), PERCENTILE_CONT(0.95) ... GROUP BY dept`** →
  `employees | summarize { n: count, total_sal: sum sal, p95: percentile(sal, 95) } by dept`.
- **SQL `SELECT * EXCEPT (embedding, body)`** →
  `docs | without { embedding, body }`.
- **Implicit result limit note:** Interactive pipe queries without an explicit `take` apply a default safety cap (`take 50`); always append `| take all` or use `Queryable::take_all()` / `QueryCursor` when reading full result sets.

### 1.2 Schema Migration Cookbook (`LIN-1978`)
1. **Adding collections, relations, or secondary indexes:** Execute `col <name> { ... }`, `rel <name>: <from> -> <to>`, or `index <col> [f1, f2]` inside a `Db::transaction()` (or `lin run --data <dir>`). Both `Persist::log` and `checkpoint` persist `extra_collections` / `extra_indexes` and update `catalog_hash`.
2. **Adding optional fields:** Existing slotted `Row`s are sparse; new writes can include the added field immediately while older rows evaluate `new_field is null` (or decode as `None` in `Option<T>`).
3. **Migrating legacy `catalog_hash`:** If opening a pre-`2026-11-14` directory whose `head` hashed session `fk`/`guard` clauses, run:
   ```sh
   lin recover --data <dir> --fix-catalog-hash
   ```

### 1.3 Embedding Model Migration Cookbook (`LIN-1979`)
Swapping `Db::with_embedder(new_model)` on a populated database intentionally keeps the old `store.embed_id` until vectors are recomputed, refusing mixed-space `search vec` / `search hybrid` queries:
```rust
let mut db = Db::open("/var/lib/lin/prod")?.with_embedder(new_embedder);
// Recomputes vectors in parallel, updates HNSW if present, and durably logs Pack::Update + Head:
let updated_docs = db.reembed_collection("docs")?;
db.checkpoint()?;
```

### 1.4 Backup & Restore Migration Cookbook (`LIN-1980`)
- **Export self-contained backup (includes hot snapshot + cold `DiskSegment`s):**
  ```sh
  lin backup export --data /var/lib/lin/prod --out /backups/lin-2026-11-28
  ```
- **Safe restore (refuses non-empty target unless `--force` is given):**
  ```sh
  lin backup import --from /backups/lin-2026-11-28 --data /var/lib/lin/restored
  ```

---

## 2. Operational Incident Runbooks (`LIN-1913`–`LIN-1916`, `LIN-1440`)

### 2.1 Disk-Full (`ENOSPC`) Runbook (`LIN-1913`)
1. **Immediate behavior:** When WAL append or snapshot `.tmp` write hits `ENOSPC`, the write fails with an I/O error without advancing `store.gen` or mutating committed state; any partial trailing WAL bytes are ignored/truncated on reopen.
2. **Recovery steps:**
   - Free space on the volume (or move unreferenced backups/logs; **never** manually delete `log`, `snapshot`, `head`, or `segments/seg_*` while `LOCK` is held).
   - If remote S3 cold offloading is configured, evict local hydrated segment caches: `db.evict_cold_cache_to(max_bytes)`.
   - Reopen with `lin checkpoint --data <dir>` to compact `log` and drop `100%`-dead tombstoned cold segments (`vacuum`).

### 2.2 Corruption Investigation Runbook (`LIN-1914`)
1. **Copy before touching:** Copy the entire `<dir>` (excluding `LOCK`) to an investigation directory before running writable opens.
2. **Dry-run & frame inspection:** Use `lin::persist::dry_run_recovery(&copy_dir)` and `lin::persist::inspect_wal_file(&copy_dir.join("log"))` to obtain the exact failing `byte_offset`, `codec`, `crc_valid` flag, and 64-byte `failed_frame_hexdump`.
3. **Distinguish tail tear vs mid-file corruption:**
   - *Torn final WAL frame (power loss before `fsync`):* Normal `Db::open` truncates cleanly to the last acknowledged prefix.
   - *Mid-log generation gap or silent bit-rot in `segments/seg_*/col_*` / `snapshot` (`LCRC` mismatch):* `Db::open` refuses to start silently; restore from the latest verified backup or replica.

### 2.3 Follower Lag & Promotion Playbook (`LIN-1915`, `LIN-1440`)
1. **Diagnosing lag:** Inspect `ServerHandle::replica_statuses()` / `ShipWalHub` metrics (`lag_gens`, `lag_bytes`). If a follower falls behind `max_replica_lag_gens` (after primary log compaction), `ReplicaPoolClient` / `bootstrap_follower` automatically re-bootstraps from primary snapshot + cold `.lseg` bundles.
2. **Planned failover / promotion:**
   - Fence the old primary first (stop `lin serve` / release writer `LOCK` so split-brain writes are impossible).
   - Verify the candidate follower reached the target `gen` (`db.stats().r#gen`).
   - Close the follower handle and reopen the directory as primary via `Db::open_with(&dir, opts)` (acquiring exclusive `LOCK`). Point remaining followers at the new primary endpoint.

### 2.4 Safe Upgrade & Toolchain Rollback Runbook (`LIN-1916`, `LIN-1959`)
1. **Pre-upgrade:** Run `lin checkpoint --data <dir>` (compacts `log` to empty) and `lin backup export --data <dir> --out <backup_dir>`.
2. **Canary check:** Open a copy of `<dir>` in read-only mode (`Db::open_read`) with the new binary and verify `stats()`.
3. **Binary/Toolchain rollback:** Because `DISK_FORMAT_VERSION = 3` and `snapshot` `LCRC` trailers are backward-compatible with `v0.7.1` (`./scripts/format-compat.sh`), rolling back to the previous release binary requires only running `checkpoint` (if optional WAL codecs `4`/`5` were enabled) and restarting with the pinned binary/toolchain.

---

## 3. Build Provenance, Checksums, Security & Release Checklist (`LIN-1794`, `LIN-1795`, `LIN-1948`, `LIN-1958`, `LIN-1960`)

- **Checksum verification (`LIN-1948`):**
  ```sh
  shasum -a 256 -c lin-release.sha256
  ```
- **Offline dependency & provenance inventory (`LIN-1795`, `LIN-1958`):**
  - Exact dependency hashes are pinned in [`Cargo.lock`](../Cargo.lock). Generate an offline bundle and machine-readable dependency inventory via:
    ```sh
    cargo metadata --format-version 1 --locked > target/dependency-provenance.json
    cargo vendor --locked target/vendor
    ```
- **Responsible Security Disclosure (`LIN-1794`):**
  - Report memory-safety, authentication/TLS bypass, path-traversal, or silent data-loss vulnerabilities privately via GitHub Security Advisories on `themoretheless/lin` (do not post exploit payloads in public issues before a fix is tagged). Include the commit SHA, feature flags, and minimal reproducer.
- **Release Regression Checklist (`LIN-1960`):**
  1. `RUSTFLAGS="-D warnings" cargo check --all-targets` (`0` warnings)
  2. `RUSTFLAGS="-D warnings" cargo check --no-default-features --all-targets` (`0` warnings)
  3. `RUSTFLAGS="-D warnings" cargo check --all-targets --all-features` (`0` warnings)
  4. `cargo test --release --all-features` (`100%` pass) + `--test gpu --test gpu_parity_qualification -- --ignored` on physical GPU host
  5. `./scripts/format-compat.sh` (`100%` bidirectional compatibility with previous release tag)
  6. `AIRBUG_HUB="" AIRBUG_OFFLINE=1 ./scripts/ci-bench.sh` (`exit 0`, zero budget or peer regressions)
  7. `CHANGELOG.md` updated and `release` fast-forwarded to tagged `main` commit.
