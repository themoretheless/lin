# Temporary bulk row shape template

Baseline production source: `518db365dd6accd80e86377baf18fedbca15429f`; later main `e09b14a` has identical production sources. Candidate changes only `src/exec.rs`; exact before/candidate snapshots and binary SHA256 retained here.

Build one temporary BTreeMap shape per bulk chunk and clone it before filling values in sorted-key order. Avoid repeated insertion comparisons/rebalancing while retaining owned keys and public Row. Only batches >=128 and shapes of 1..32 input fields use the template. These thresholds bound overhead; universal crossover was not established. Last duplicate field wins via last-position mapping; differing field sequences fall back to scalar conversion. No Prepared cache, schema mutation, disabled embedding/FTS, or sync changes.

## Measurements

Two independent series, each six alternating baseline/candidate process pairs per mode, pinned release binaries, 24 fresh samples per case, max iterations 1, no warmup. Exact fixture validation outside timing. SQLite unchanged control. DuckDB excluded in this targeted experiment; MySQL skipped. No builds/tests initiated by this runner overlap timing; unrelated host activity was not controlled. Process medians and paired percent gains are not p99 or intrinsic CPU claims.

| Series | Case | Lin paired gain (candidate wins) | SQLite control gain | Candidate Lin ms | Candidate SQLite ms | Lin vs SQLite wins |
|---|---|---:|---:|---:|---:|---:|
| 2026-10-24-batch-row-shape-template | insert_bulk 1k | 6.045% (6/6) | -1.178% | 0.757396 | 0.787094 | 6/6 |
| 2026-10-24-batch-row-shape-template | insert_bulk 10k | 5.112% (6/6) | -0.898% | 9.036125 | 10.245812 | 6/6 |
| 2026-10-24-batch-row-shape-template | durable_insert 1k | 21.256% (4/6) | 2.360% | 2.289771 | 1.358864 | 0/6 |
| 2026-10-24-batch-row-shape-template | durable_insert 10k | 3.138% (4/6) | 0.920% | 14.410104 | 13.343979 | 1/6 |
| repeat | insert_bulk 1k | 7.196% (6/6) | -1.074% | 0.767240 | 0.788625 | 5/6 |
| repeat | insert_bulk 10k | 3.563% (6/6) | -0.840% | 9.401917 | 10.314667 | 6/6 |
| repeat | durable_insert 1k | 1.614% (3/6) | -1.677% | 2.194354 | 1.367969 | 0/6 |
| repeat | durable_insert 10k | 0.986% (6/6) | -0.769% | 14.211729 | 13.365396 | 0/6 |

## Decision and limitations

Retain for repeated native improvement: both native sizes improve in all six pairs in each series. Native 1k beats SQLite in 6/6 first-series processes and 5/6 repeat processes, so this is a local contract result, not universal dominance. Durable 1k is inconsistent: repeat paired gain is positive but aggregate median worsens and only 3/6 pairs improve. No durable 1k improvement claim. Durable 10k improves modestly, while direct SQLite gap remains. Other peers/read workloads require fresh qualification.

Nine focused record tests pass, including template threshold/parallel split boundaries, heterogeneous shapes, duplicate precedence, captured now, source preservation, widths 0/32/33/80 and existing worker fallback. Full workspace passed (exit 0); result is recorded in workspace-status.json and workspace-tests.log. No peak RSS measurement was collected; template lifetime is per chunk but memory performance is unqualified.
