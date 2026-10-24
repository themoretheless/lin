# Bounded native bulk insertion: current five-peer matrix

2026-10-24, production source `1164f3d` unchanged; benchmark worker and adapter changed, exact snapshots and hashes retained. **14/15 aggregate comparisons won**; **100k DuckDB remains a measured loss**. All sizes are complete and pass exact six-field/multiplicity validation. --require-wins correctly exits 3 at 100k; the runner's own success means execution/cleanup completed, not universal wins.

## Why the invocation changed

The previous worker's 100k single AST estimated 29,285,868 bytes and correctly hit the default 8 MiB quota. That incomplete baseline is retained in ../2026-10-24-current-native-insert-matrix, including errors and peer results. Do not relabel it as a win.

At <=10k keep the existing single prepared run. Larger fixtures prepare statements of <=10k rows outside timing, then time one public Db::empty memory transaction with every prepared call and commit. At 100k this is ten statements, not one large AST. Quotas remain unchanged. Affected count aggregation is timed; exact readback/default embeddings checked outside. This is a separate large-input API invocation contract, not a production engine acceleration. Native contract metadata records statement bound/count/execution path. Physical-disk performance and durable transaction atomicity are not established by a memory transaction test.

## Validation and provenance

Release worker regression inserts 100k with default quotas, checks affected count, default embeddings and every six-field value against generated expectations; boundary metadata and final timestamp/id are asserted. It passed, as did all existing Python adapter tests and release worker build (checks.json and logs). No core production source changed; a full workspace rerun was not required for this benchmark-only change. Exact candidate worker/adapter snapshots and hashes retained; all core and benchmark source hashes verified unchanged after measurement.

## Method and limitations

Three independent processes per size, rotated engine order, 12 fresh fixtures per engine/process, one operation per sample. Schema/index creation, preparation, input serialization, exact validation, cleanup and Lin IPC excluded; transaction execution/acknowledgement included. Medians of process sample medians, not p99. No builds/tests initiated by this runner overlap timing; unrelated host activity not controlled. Single campaign, not a repeated before/after acceleration claim.

SQLite executemany+commit; DuckDB prepared INSERT SELECT from registered pandas input+commit; PostgreSQL COPY CSV+commit; MySQL multi-row executemany+commit; MongoDB ordered insert_many w=1/j=false. Unique id/uri and wing/ts index in fixtures; Lin default embedding/FTS retained. APIs, network/driver/materialization and feature work differ. Dedicated native ARM64 PostgreSQL/MySQL/MongoDB data on tmpfs; settings/versions/image digests/ownership recorded in environment.json. PostgreSQL sync/fsync/full-page-writes on, MySQL flush/binlog 1/1. No equal durability or physical disk claim.

Sentinels unchanged after every size, no owned fixture objects remained; all three reused task-owned servers stopped. Metadata stores actual native contracts and source/binary hashes. See docs/benchmark-contract.md for scope.

## Results

| Rows | Peer | Lin ms | Peer ms | Peer/Lin | Result |
|---:|---|---:|---:|---:|---|
| 1000 | sqlite | 0.749958 | 1.030417 | 1.374 | win |
| 1000 | duckdb | 0.749958 | 1.156583 | 1.542 | win |
| 1000 | postgres | 0.749958 | 4.717771 | 6.291 | win |
| 1000 | mysql | 0.749958 | 8.381937 | 11.177 | win |
| 1000 | mongo | 0.749958 | 4.914063 | 6.552 | win |
| 10000 | sqlite | 9.411938 | 12.924542 | 1.373 | win |
| 10000 | duckdb | 9.411938 | 9.744396 | 1.035 | win |
| 10000 | postgres | 9.411938 | 27.028458 | 2.872 | win |
| 10000 | mysql | 9.411938 | 79.980542 | 8.498 | win |
| 10000 | mongo | 9.411938 | 46.631479 | 4.955 | win |
| 100000 | sqlite | 112.805979 | 145.003396 | 1.285 | win |
| 100000 | duckdb | 112.805979 | 101.463020 | 0.899 | loss |
| 100000 | postgres | 112.805979 | 280.560687 | 2.487 | win |
| 100000 | mysql | 112.805979 | 868.357042 | 7.698 | win |
| 100000 | mongo | 112.805979 | 600.929208 | 5.327 | win |

## Next steps

100k Lin 112.806 ms versus DuckDB 101.463 ms: Lin is about 11.2% slower in this campaign, requiring about a 10.1% time reduction to match. Profile the transaction/chunk/pack lifetime and insertion phases; do not disable embedding/FTS or weaken quotas to close the gap. The 10k DuckDB margin is small, so require independent controls/repeats before new performance claims. Durable Rust SQLite gaps are separate; Python SQLite wins here do not close those gaps.

Pandas has no native transactional insertion adapter (DataFrame construction is a different contract). MSSQL/Kusto ingestion adapters and dedicated endpoints remain unqualified. Prior 108/108 read wins remain source-current because only benchmark insertion paths changed, but do not establish these missing write contracts or overall goal completion.
