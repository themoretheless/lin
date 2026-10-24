# TLS content-hash cache CPU gate

Source `3b1b0edc132bb386d1335b05731976481c4f0ad3`; production unchanged. Proposed token-slot cache was found to match a previously rejected experiment (../2026-10-04-batch-token-slots/report.md) and was not rebuilt or rerun. This is a distinct hypothesis: cached_content_hash uses a Mutex inside thread_local storage even though the cache is not shared between threads. Compare the exact source-derived function with only lock().unwrap() replaced by RefCell.borrow_mut() and the local container type changed. Hash computation, keys, Arc values, <=1023-entry cap and arbitrary-key eviction remain identical.

Six independent release diagnostic processes, 24 alternating rounds per case/process, 16 loops per round. Four cases: warm 1000 unique short bodies, 2048-body churn beyond the cache cap, repeated 256-byte Unicode body, and unique long/UTF-8 bodies that fit the cache. Exact hash values compared to the current direct FNV/format reference before timing; final cache bounds checked. Both caches cleared/repopulated outside each case, raw rounds retained. Timed calls include returned Arc destruction. No full Db, WAL/sync, rows, FTS, allocations/RSS gate, or independent-thread production integration is measured. No runner build/test overlap; host activity uncontrolled.

| Case | Mutex µs/batch | RefCell µs/batch | Paired CPU gain | Faster processes |
|---|---:|---:|---:|---:|
| churn_2k | 147.876 | 142.070 | +3.922% | 6/6 |
| repeated_long | 20.322 | 16.312 | +19.207% | 6/6 |
| utf8_boundaries | 16.081 | 12.831 | +19.707% | 6/6 |
| warm_1k | 11.109 | 8.018 | +27.138% | 6/6 |

## Decision and next gate

Component feasibility is positive, but the warm 1k absolute difference is only about 3 microseconds per 1000 calls. This cannot close the roughly millisecond-scale durable gap by itself, and it is below observed whole-operation variability. No production change is accepted on a 27% component number alone. A future replacement requires actual thread-isolation/cache churn/value checks, default workspace, and affected-engine interleaved A/A/A-B qualification plus native/large guards. RefCell borrow/reentrancy and Mutex poisoning semantics differ and must be assessed on the real non-reentrant helper rather than assumed equivalent from this fixture. Do not infer database speed from these component timings.

Exact diagnostic/reference/candidate and core/binary hashes retained, all six process exits zero. The temporary Cargo example was removed. Existing production/read/native matrices remain unchanged. Full named-peer objective remains open, especially durable 1k SQLite and missing MSSQL/Kusto.
