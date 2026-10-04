# Current durable insert phase diagnostic

Source: main c21d893 (includes retained row-major large-batch packing and current ASCII/default-bigram embedding optimizations). Two separately pinned diagnostic binaries: initial phase breakdown and independent finer per-column breakdown. Production src/exec.rs and src/persist.rs were restored byte-for-byte before either binary was run. Source snapshots and hashes are retained; no instrumentation is present in main.

Each binary ran three independent processes, each selecting existing compare/durable_insert_*/lin with 24 fresh samples, one operation per sample, no warmup. Existing checked fixture validation verifies insertion count and full expected row values outside timing. All six runs completed successfully. Logs contain 27 phase events per size per process, including validation/calibration operations in addition to measured samples; every reported phase has the same event count. Summaries use the median of events in each process, then median of the three process medians. These diagnostics are not peer benchmarks and cannot establish the named-peer objective.

## Initial phase breakdown

Milliseconds:

| Phase | 1k | 10k |
|---|---:|---:|
| build | 0.284333 | 1.896375 |
| checks | 0.143250 | 1.520667 |
| embed | 0.370625 | 4.017667 |
| index | 0.125625 | 1.192750 |
| fts | 0.183709 | 1.816583 |
| maps | 0.036000 | 0.452625 |
| pack | 0.228792 | 2.244958 |
| classify | 0.123916 | 1.246125 |
| encode | 0.672250 | 6.879666 |
| checksum | 0.015500 | 0.157291 |
| write | 0.045167 | 0.286334 |
| sync | 0.872417 | 1.428666 |

build is pure Row construction (including worker creation/join for the existing large-batch path); checks includes default IDs/hashes, uniqueness and FK checks; embed includes text preparation and vector assignment; index/fts/maps measure their existing slab operations. pack measures the durable rows_to_insert_cols call. encode measures column payload encoding, including sparse classification; checksum measures CRC/envelope preparation; write measures the existing four write_all calls; sync measures existing Full durable_sync.

Sparse classification is nested inside encode: do not add them. Phase medians must not be summed to estimate total duration or subtracted as an exact decomposition. Diagnostic Instant calls and stderr logging perturb the path; outer encode includes the classify diagnostic logging cost. Setup/checkpoint frames are excluded from per-insert frame statistics by pairing each WAL_ENCODE event with its next WAL_FRAME and verifying matching payload byte length. Other insert phases omit moving rows into the store and unrelated call overhead.

## Independent per-column breakdown

Milliseconds, measured by the second diagnostic binary:

| Column | 1k | 10k |
|---|---:|---:|
| body | 0.003417 | 0.053416 |
| embedding | 0.647750 | 6.609250 |
| hash | 0.004916 | 0.051459 |
| id | 0.003875 | 0.095458 |
| layer | 0.003417 | 0.055709 |
| title | 0.004250 | 0.044833 |
| ts | 0.000917 | 0.007417 |
| uri | 0.003375 | 0.037125 |
| wing | 0.002917 | 0.028917 |

Second-series aggregate encode medians: 0.777333 ms / 7.114083 ms; sparse classification: 0.124083 ms / 1.286917 ms. Column embedding includes sparse classification and its diagnostic logging cost. All nine per-column timers include field-name serialization. Extra per-column stderr logging lies within the outer encode timer, so comparing the two diagnostic series as a performance improvement would be invalid.

Payload bytes are constant within each size and across both series: 336968 at 1k and 3558340 at 10k. Normal float bits, negative zero/NaN handling and persisted codec behavior were not changed by these diagnostics.

The evidence points toward vector encoding as the next optimization target, rather than further text-column or syscall tuning. In the fine-grained diagnostic the embedding column records roughly 6.61 ms at 10k, versus at most 0.10 ms for any text column. A next experiment can divide sparse-vector encoding across a bounded number of workers for large batches, preserving exact row order, float bits, encoded bytes, decoded-budget validation, errors and durable-sync timing. Additional CPU/temporary buffers would need to be reported and benchmarked. This hypothesis is not implemented or proven here.

No production changes or new tests were retained, so a full workspace test rerun was not required. All changes in this commit are diagnostic evidence. Durable SQLite gaps and unverified MSSQL/Kusto benchmarks remain open.
