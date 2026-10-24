# Current WAL profile after Apple preallocation

2026-10-24, production core ca9eec4; repository baseline cf6136f. Diagnostic instrumentation only; before/instrumented snapshots, compressed patch (original context preserved), exact source/binary hashes and raw logs retained. Three independent pinned release compare processes, durable insert 1k/10k, 24 samples plus preflights (27 typed events/size/process), one operation, no warmup. Exact benchmark fixture validation outside timing. Production source restored byte-for-byte before running and verified afterward. No production changes or benchmark win claimed.

## Median of three process medians

| Phase | 1k ms | 10k ms |
|---|---:|---:|
| pack_ns | 0.078542 | 1.428334 |
| classify_ns | 0.092542 | 0.408041 |
| encode_ns | 0.435292 | 1.672709 |
| checksum_ns | 0.032500 | 0.343417 |
| write_ns | 0.042333 | 0.279625 |
| sync_ns | 0.458125 | 1.040250 |

Payloads remain 336,968 /3,558,340 bytes. Classify is inside encode; reserve is inside write, sparse worker spans overlap at 10k. Independent medians cannot be summed or subtracted into exact total latency. Logs/timers perturb execution, including route logging within the sync span and reserve logging within write. Setup, preparation, fixture validation and drop/checkpoint are outside operation timing. Unrelated host activity uncontrolled.

Physical reserve helper median-of-process-medians is 0.002458 ms at 1k and 0.068292 ms at 10k. These include metadata checks and any allocation; they are not purely allocation syscall latency. Initial empty-file reservation during setup is excluded. All 486 observed durable sync calls used F_BARRIERFSYNC successfully; zero fallback sync_data calls. This establishes the API route only, not physical power-cut behavior or equality with SQLite durability.

## What this changes in the next investigation

The 1k reserve check is ~2.46 us, too small to explain the remaining several-hundred-us SQLite gap by itself. Current encode remains ~0.435 ms, including ~0.093 ms classification; prior NEON count's large isolated CPU gain did not qualify at engine level and is not retried here. Checksum (~0.033 ms) and row pack (~0.079 ms) also cannot individually explain the whole gap. Sync remains material (~0.458 ms); do not weaken it or change peer settings.

Historical preallocation-baseline profile recorded sync ~0.827 ms, but these profiles are separated in time and instrumented: their difference is not a causal speedup claim. Use the previously calibrated preallocation A/B report for its accepted limited performance claim.

Next investigation should cover CPU work outside the WAL spans, especially FTS build/mutation or row construction, using exact independent token/index/result invariants and a current insertion-stage profile before changing data structures. Earlier ASCII tokenizer, inline text, sparse capture, packed entries, single-pass encoding and NEON hypotheses remain rejected/inconclusive; no unmodified retry justified by this report.

Full peer objective remains open: durable 1k SQLite loss and missing live MSSQL/Kusto qualification are not resolved by profiling. No new peer measurements, physical power-cut tests or workspace tests were run for restored diagnostic-only edits.
