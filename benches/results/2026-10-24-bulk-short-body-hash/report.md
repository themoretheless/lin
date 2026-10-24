# Retain: short-body hash cache bypass in large bulk insertion

Baseline source `20739a4`, production-identical to `311e11f`. Exact before/measured candidate/final exec.rs snapshots and API/guard/Rust-compare binary hashes retained. Measured source hashes verified unchanged after all runs. Final source differs by one explanatory comment only, recorded in provenance.json; no semantic/performance code changed after testing.

## Change and correctness

The 100k chunked profile measured ~28.6ms checks/defaults and ~31.4ms embedding. Short unique bodies churn the ~1023-entry TLS hash cache. The first, unconditional <=64-byte bypass regressed warm 1k cases and is rejected with all raw evidence in ../2026-10-24-short-body-hash-cache.

Only bulk default hashing now directly calls existing content_hash_arc at >=4096 statement records and <=64 UTF-8 body bytes. Smaller batches, longer bodies and updates retain the old cache. Explicit caller hashes remain unchanged. FNV/format/text values, default embedding/FTS/indexes, sync, quotas and wire semantics preserved. Thresholds are engineering heuristics, not universal crossovers.

Actual bulk unit coverage: 127/128/4095/4096/8192 records, unique/repeated bodies, 64/66-byte Unicode and caller hashes; stored hash values match the existing direct reference. Targeted test, release builds and full cargo test --workspace passed (finish-status.json). Profiling/candidate diagnostic binaries do not alter production configuration.

## Local native API measurements

Two independent series of six alternating process pairs at 1k/10k/100k, Lin/SQLite/DuckDB, 12 fresh samples each. <=10k single prepared run; 100k ten prepared chunks in one memory transaction. Setup/schema/preparation/exact six-field validation/cleanup/IPC excluded. Pinned binaries, no builds/tests initiated by this runner overlap timing; host activity otherwise uncontrolled. All runs complete/exact-validated, all outliers retained (including candidate 100k ~117ms and baseline DuckDB ~225ms in one pair). Rust versus Python driver/API feature costs differ; no isolated CPU, operation-p99 or equal-durability claim. See benchmark-contract.md.

| Series | Rows | Lin paired gain (wins vs baseline) | Candidate Lin ms | Candidate SQLite ms | Candidate DuckDB ms | Lin vs DuckDB processes | Control gains SQLite / DuckDB |
|---|---:|---:|---:|---:|---:|---:|---|
| 1 | 1000 | -1.887% (2/6) | 0.777854 | 1.038552 | 1.194427 | 6/6 | -2.478% / 0.161% |
| 1 | 10000 | 11.173% (6/6) | 8.014291 | 12.719250 | 9.736198 | 6/6 | -0.668% / 0.661% |
| 1 | 100000 | 11.588% (5/6) | 92.968219 | 145.539604 | 101.301084 | 5/6 | -0.374% / 0.298% |
| 2 | 1000 | -0.665% (1/6) | 0.788438 | 1.038854 | 1.243688 | 6/6 | 0.425% / -2.632% |
| 2 | 10000 | 12.437% (5/6) | 8.064198 | 13.089989 | 9.926354 | 6/6 | -1.123% / -0.512% |
| 2 | 100000 | 14.779% (6/6) | 92.369823 | 145.931594 | 101.087791 | 5/6 | -0.400% / 0.773% |

100k aggregate medians beat DuckDB in both series; candidate wins 10/12 individual processes. Small API 1k effects remain slightly negative; no 1k acceleration claimed.

## Native Rust / durable checks

Fresh source-matched compare binaries; two independent series, six alternating process pairs per mode, 24 fresh samples/case, one operation, warmup 0, sample-ms 1. SQLite unchanged control; DuckDB excluded/MySQL skipped in this targeted check. Exact fixtures validated outside timers, same full sync and feature work. Process/batch medians are not operation p99.

| Series | Case | Lin paired gain (wins vs baseline) | SQLite control gain | Candidate Lin ms | Candidate SQLite ms | Lin vs SQLite processes |
|---|---|---:|---:|---:|---:|---:|
| 1 | insert_bulk_1k | -2.584% (0/6) | -0.452% | 0.756261 | 0.777125 | 5/6 |
| 1 | insert_bulk_10k | 12.017% (6/6) | -1.438% | 7.976198 | 10.223261 | 6/6 |
| 1 | durable_insert_1k | -1.544% (3/6) | 3.915% | 2.420635 | 1.390802 | 1/6 |
| 1 | durable_insert_10k | 10.945% (6/6) | 0.676% | 12.982760 | 13.475334 | 6/6 |
| 2 | insert_bulk_1k | -0.311% (3/6) | -0.026% | 0.772115 | 0.793552 | 6/6 |
| 2 | insert_bulk_10k | 14.399% (6/6) | 0.540% | 8.069677 | 10.358781 | 6/6 |
| 2 | durable_insert_1k | -8.454% (1/6) | -1.982% | 2.948459 | 1.442729 | 0/6 |
| 2 | durable_insert_10k | 9.291% (6/6) | 0.065% | 12.521875 | 12.874094 | 6/6 |

Native 1k remains ahead of SQLite by aggregate median in both series and 11/12 processes, despite -2.584% first-series and -0.311% repeat changes versus baseline. Native/durable 10k each improve in all twelve baseline/candidate pairs; candidate durable 10k beats SQLite in 12/12 processes.

Durable 1k remains a loss. Its negative paired shifts, including -8.454% second-series, are retained. A separate concern-driven six-pair check with 48 samples/case gave Lin -2.398% (1/6 wins), SQLite -2.257% (1/6). Large -8.454% penalty did not reproduce, but this does not establish no regression or replace either original series. Direct bypass is inactive at 1k; CPU/layout and I/O variance remain practical limitations. No durable 1k gain claimed.

## Repeated-body and memory controls

Diagnostic worker changes only untimed body generation; repeated short text and repeated 256-byte text at 1k/10k. Six alternating pairs, 12 samples each, every row independently checked by Python. Long-body cache path unchanged. These Lin-only guards are not peer victories or universal no-regression proof.

| Body / rows | Paired gain | Candidate wins vs baseline |
|---|---:|---:|
| short_repeat/1000 | 0.721% | 3/6 |
| short_repeat/10000 | -1.502% | 1/6 |
| long_repeat/1000 | -0.150% | 3/6 |
| long_repeat/10000 | -1.700% | 1/6 |

At 10k both short-repeat and unchanged long-repeat controls decrease similarly (~1.5%/~1.7%); no extra short-specific penalty isolated. Negative observations remain visible.

After timing, three additional alternating /usr/bin/time -l pairs on macOS, each whole-worker request 100k rows/12 fresh samples. Peak includes parsing/store/canonicalization/output and allocator effects, not insertion live memory. Median RSS: baseline 897.609 MiB, candidate 934.656 MiB (+4.13%). Candidate higher in 3/3 pairs; observed memory tradeoff, no causal allocator attribution or memory-saving claim.

## Decision / remaining scope

Retain for repeated native large-batch and durable 10k gains, acknowledging small-case negative shifts and higher whole-worker RSS. Correctness/workspace gates passed. Local aggregate native 100k DuckDB and Rust durable 10k SQLite gaps close within these finite repeated contracts. Durable 1k gap remains. Full read/server-native matrices need refreshing on accepted source before broadening claims. PostgreSQL/MySQL/MongoDB not rerun here; MSSQL/Kusto endpoints/native ingestion remain unqualified. Pandas construction is a separate nontransactional contract. Full named-peer objective stays open.
