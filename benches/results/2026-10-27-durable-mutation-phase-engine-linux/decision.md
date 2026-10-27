# Decision: investigate durable I/O, not serialize-only speedups

Run40510531528 on90304aebef12f6d4120f046f066c00ffdcde47e0 completed successfully. Independent local audit confirms576 isolated processes,4608 measured wall observations and528 separately scoped invocation frames. Original/control share the uninstrumented production-source binary;disabled/traced share the instrumented binary. No source optimization or all-eight-peer victory is accepted.

## Observed bottleneck

The following are medians of six process-level medians of per-invocation fractions, including fresh validation/calibration work. They are not a one-to-one decomposition of uninstrumented measured wall samples.

| Operation | Rows | Sync share % | Execute share % | Encode share % | Write share % | Payload bytes |
|---|---:|---:|---:|---:|---:|---:|
| UPDATE | 1k | 85.76 | 5.71 | 1.70 | 5.06 | 4112 |
| DELETE | 1k | 83.80 | 7.32 | 1.69 | 4.99 | 4112 |
| Commit | 1k | 86.34 | 5.52 | 1.70 | 4.11 | 4112 |
| UPDATE | 10k | 83.27 | 5.44 | 1.29 | 8.13 | 4112 |
| DELETE | 10k | 81.98 | 7.03 | 1.25 | 7.98 | 4112 |
| Commit | 10k | 82.95 | 5.49 | 1.30 | 8.16 | 4112 |

Each committed invocation has exactly one encode/CRC/write/sync;checkpoint count is zero. Rollback has no encode/write/sync and spends its time in execution/undo,not durable I/O. Its execute/rollback median shares are51.96/34.18%(1k),58.99/28.72%(10k). No background encoder queue is on this actual mutation path.

Observer controls matter:among committed Lin cases,disabled/original median ratios span0.9387–1.0697;traced/disabled0.9718–1.0589. Original/control median absolute A/A spans1.73–11.49%. Ratios below1 are not negative overhead;layout/scheduling/host variation remain visible. Thus sync dominance is strong diagnostic evidence,not an exact assertion that untraced execution has the same percentages or a causal explanation of the Linux gap. Inference:CPU/serialization-only work is unlikely to close most of this durable gap;test a real I/O hypothesis.

## Retained peer results

Uninstrumented labels win96/96 against SQLite FULL DELETE and1/96 against SQLite FULL WAL. The single WAL win is retained in1k UPDATE/control (1/6 pairs;median Lin/SQLite1.2611,still slower overall). Instrumented peer numbers remain separate in report.md. No outlier or journal mode is excluded.

## Next controlled hypothesis

Production reserve_wal_space has an Apple preallocation implementation and is a no-op on Linux. Test a bounded Linux KEEP_SIZE reservation path:it must preserve logical EOF,WAL bytes/codec/replay,append position,quotas,writer locks,reader/follower behavior and ordinary-write fallback when unsupported. Reserve only a bounded range near the current end,not an arbitrarily large sparse backlog. No logical file padding,disabled sync,omitted row/embedding/index/FTS payload or global filesystem/allocator tuning is allowed. Establish platform functional/failure behavior first,then compare uninstrumented final binaries with same-run A/A,all durable SQLite WAL/DELETE modes,native/read/insert controls and retained cold/setup/resource costs. This is not implemented or measured by this archive;sync dominance alone does not prove reservation will help.

COW/lazy-COW changes are separate future API experiments. Their native gains cannot be applied as a durable speedup or an answer to this main-path I/O observation. Exact Row API is unchanged in this source. Kusto and complete eight-peer qualification remain open;full objective stays active and incomplete.
