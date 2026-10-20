# Current native/durable SQLite comparison, 2026-10-21

Production source 64fd272, including intervening October 16–20 changes to SSO, WAL/checksums, row pooling, grouping, FTS and vector search. No production code changed for this measurement. Pinned release compare executable SHA-256, source hashes and final source verification retained. Release binary built before timing; 61 library tests pass, 2 ignored. This is not an all-feature/workspace qualification. build-status.json and tests-status.json record exit zero.

Six independent fresh-fixture processes per mode. Each has Lin and SQLite at 1k/10k, 24 samples per case, one operation/sample, zero warmup. Native/durable process order alternates between repetitions; case order within each process remains fixed. Existing benchmark exact fixture validation runs outside timing; every report status complete. No compilation/test overlap. MYSQL skipped and mixed-case environment override removed, DuckDB explicitly excluded from this targeted SQLite check. These exclusions define this run's scope and are not wins over excluded engines.

Median of process medians in milliseconds; wins compare paired same-process Lin/SQLite medians:

| Mode/rows | Lin ms | SQLite ms | Lin process wins |
|---|---:|---:|---:|
| native-1000 | 0.998062 | 0.974239 | 0/6 |
| native-10000 | 12.832292 | 13.019146 | 5/6 |
| durable-1000 | 2.730563 | 2.162052 | 1/6 |
| durable-10000 | 20.716385 | 18.092271 | 1/6 |

## Variation and interpretation

| Mode/rows | Lin process-median range ms | SQLite range ms |
|---|---:|---:|
| native-1000 | 0.842–1.927 | 0.796–1.488 |
| native-10000 | 9.935–19.055 | 10.282–21.061 |
| durable-1000 | 2.437–4.132 | 1.381–3.145 |
| durable-10000 | 16.531–41.296 | 14.148–48.111 |

Host load/clocks/scheduling are uncontrolled; strong between-process variation is visible. No stable SLA, tail percentile or isolated kernel CPU claim. Nonetheless native 1k loses all six same-process comparisons and both durable sizes lose five of six. Native 10k has five wins, but aggregate margin is small relative to observed variation. Repetition medians and all raw observations are saved, not just the aggregate.

The all-named-peer objective is not achieved. In particular, the old October 6 durable 10k lead cannot be carried forward onto this materially changed source. These are not paired old/new binaries, so do not attribute a regression percentage to any intervening change. The October 4 six-peer read matrix likewise remains historical evidence until rerun against current source; this targeted write run does not prove or disprove those read wins. Current outstanding local work includes native 1k and durable 1k/10k; next diagnosis should profile current WAL encode/pack/sync rather than reuse old phase timings. No changes to default embedding, FTS, indexes, schema or sync were made to obtain results.

MSSQL/Kusto endpoint variables remain unset (presence booleans only in endpoint-presence.json). Their absence is not counted as wins. Full named-peer read/write coverage, matching contracts and current source verification remain required. The gate must not be narrowed to the lone winning native 10k case.
