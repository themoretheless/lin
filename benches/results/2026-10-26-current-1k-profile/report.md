# Current 1k insertion profile — 2026-10-26

Frozen source `f8fb0db`; temporary timers in exec/persist only. Diagnostic release executable pinned, then production source restored byte-for-byte before measurement. Six independent processes:3 native and3 Full durable;24 timed one-operation samples per process plus preflights, one1k case each. All run.json complete, existing fixture validation passed, runner exits0. Source hashes verified after the series. No production optimization or peer win claimed.

## Median of three process medians

| Span | Native1k ms | Full durable1k ms |
|---|---:|---:|
| build_ns | 0.144042 | 0.145667 |
| checks_ns | 0.093917 | 0.108250 |
| text_ns | 0.055458 | 0.057625 |
| generate_ns | 0.182459 | 0.203209 |
| attach_ns | 0.023875 | 0.024000 |
| embed_ns | 0.294000 | 0.324625 |
| index_ns | 0.090875 | 0.097833 |
| fts_ns | 0.132333 | 0.138916 |
| hnsw_ns | 0.000041 | 0.000041 |
| maps_ns | 0.032667 | 0.033583 |
| rows_to_insert_cols_ns | — | 0.081084 |
| use_sparse_vectors_ns | — | 0.092541 |
| append_sparse_rows_serial_ns | — | 0.290792 |
| encode_insert_cols_v2_ns | — | 0.417792 |
| checksum_ns | — | 0.031792 |
| write_ns | — | 0.034583 |
| sync_ns | — | 0.440958 |

Every recorded phase has27 events/process (timed calls plus harness preflights). Partial spans are nested: embed contains text/generation/attachment plus logging; encode contains classifier and sparse serialization plus logging; write includes reserve/frame-copy/write. Independent medians cannot be summed or subtracted into exact total latency or fractions. Per-row nanosecond timers are avoided; embedding spans cover the whole1000-row batch.

Instrumentation perturbs timing and adds stderr output between nested spans. No instrumented/uninstrumented causal comparison is made. Setup/schema/parsing/validation/teardown are outside measured operation, and several outer execution/transaction/append spans are not instrumented. Recorded host load is not exclusive-host control. HNSW is inactive here (~timer overhead), not evidence of ANN insertion throughput.1k uses the sequential WAL path; accepted large-batch worker policy is unchanged.

## Next hypothesis

Sparse serialization (~0.291ms) remains a material CPU span inside payload encoding (~0.418ms), alongside sync (~0.441ms). Embedding generation (~0.203ms), row construction (~0.146ms) and FTS (~0.139ms) remain relevant; text preparation alone (~0.058ms) does not explain the entire SQLite gap. These are ranking clues, not predicted additive gains.

Investigate a technically distinct NEON8-lane presence-mask implementation of sparse serialization, retaining exact original frame order/float bits. The earlier scalar block-mask writer and NEON classifier-only experiments remain rejected/inconclusive and are not accepted by this profile. First require a rotated A/A/C CPU gate with raw-bit/unaligned/tail oracles, then a full-engine gate only if useful gains exceed controls. No new codec, approximation, omitted work or relaxed sync.

Full eight-peer goal remains unmet: durable1k SQLite gap and absent live Microsoft peers remain. Native100k has a retained outlier against DuckDB in the separate peer matrix; this1k profile does not explain that outlier.
