# Four-way sparse WAL encoding

Retained on main baseline bab6848: change only append_sparse_rows_parallel to partition rows into at most four consecutive chunks rather than two halves. Create at most three scoped workers, encode the first chunk into the existing buffer and append worker buffers in original row order. Each worker-creation failure uses the existing serial fallback for that chunk. All workers are joined before WAL writing and durability flush. Small batches retain the existing serial encoder; existing 4096-row and 8MiB decoded-float gates, shared decoded-budget validation, sparse classification, wire codec/tag, exact float bits, CRC and Full sync remain unchanged.

Resource tradeoff: this encoding stage uses up to four CPU execution contexts instead of two, with up to three temporary output buffers. Roughly three quarters of encoded column bytes now pass through worker buffers/merge rather than half. There is also a small worker-handle vector and more thread creation/join. CPU energy/peak RSS were not measured. This is a latency optimization with additional parallel CPU resources, not a per-core throughput claim.

## Measured durable inserts

Two independent series on exactly the same pinned binaries, each six alternating baseline/candidate process pairs, 24 fresh checked fixtures per case, one operation per sample, no warmup. No builds/tests overlapped timing. Existing exact row validation runs outside timing. Percentages are median paired improvements from six process medians, not aggregate-median ratios. SQLite is an unchanged control. Background load and CPU frequency are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | +2.673%, 6/6 | -0.651%, 2/6 | +8.001%, 6/6 | +0.158%, 3/6 |
| Independent repeat | +6.832%, 5/6 | +4.202%, 3/6 | +8.282%, 5/6 | +4.189%, 5/6 |

Both series are positive at 10k with six/five wins. The first unchanged SQLite control is nearly flat at 10k, whereas repeat control also improves 4.189%; the entire 8.282% repeat difference cannot be attributed to this change. The small serial path also appears faster, although no workers are created there; do not attribute that difference to parallel encoding. Keep the change for the repeated positive large-case observations, with these control/resource limits explicit.

Aggregate medians of process medians, milliseconds; these are separate statistics from the paired gains:

| Series/size | Lin baseline | Lin candidate | SQLite candidate control | Lin-vs-SQLite wins |
|---|---:|---:|---:|---:|
| First series 1k | 3.133583 | 3.023062 | 1.826490 | 0/6 |
| First series 10k | 21.131218 | 19.468104 | 18.937448 | 1/6 |
| Independent repeat 1k | 3.431490 | 3.311719 | 1.978500 | 0/6 |
| Independent repeat 10k | 20.964792 | 20.429709 | 18.950969 | 2/6 |

SQLite remains faster in both aggregate comparisons. First-series 10k is close, but the repeat has a larger gap. This does not complete the named-peer goal, prove universal speedup, or establish MSSQL/Kusto results.

## Verification/provenance

All nine focused WAL integrity tests and full workspace tests passed. Existing independent scalar-reference comparison now exercises four chunks over 4101 rows, including the uneven last chunk and None/Some(empty), negative zero, NaN payload, infinity and last coordinates; actual WAL payload bytes and decoded values are checked. Worker fallback injection preserves exact bytes. The multi-column shared-64MiB budget test preserves existing WAL bytes/counter on error. No actual OS resource exhaustion was induced.

Baseline binary SHA256 matches the retained two-thread final executable metadata from ../2026-10-04-parallel-sparse-wal/final-binary-sha256.json. before-persist.rs production code is byte-identical to that measured source; later changes only added a budget test and evidence. Candidate source stayed byte-identical through both measurements and full tests. Raw results, source/binary SHA256 hashes, build/test logs and invocation scripts are retained here. The goal remains active.
