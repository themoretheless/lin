# Bounded SmallText packing cache: rejected as inapplicable to target

Committed baseline c4d223b7dcd514faec7e4b9080915d4840cdc405 in the existing isolated worktree. Concurrent uncommitted primary source and test changes remain excluded and untouched. Prototype uses per-column FxHashMap borrowing original inline text with at most eight cached values, then disables caching on the ninth distinct inline value. Heap Arc cells retain original sharing, nontext coercions unchanged, no new serialized format. Uniform slabs only, gate >=128 rows.

Important falsification: in this committed source, exec.rs value_cell(Value::String) builds Cell::Text(Arc::clone(...)), and Cell::text_arc likewise constructs heap Text. Only Cell::from_text opts into SmallText. Thus the actual prepared SQL insertion fixture does not exercise the allocation targeted by this cache. Do not change the fixture to make the candidate look beneficial or infer gains from a synthetic inline-only workload.

The first tests accidentally used text_arc and failed sharing/cache-disable assertions, while serialized values still matched. These initial source/logs are preserved. Corrected tests explicitly use from_text; all three pass: byte-exact named MessagePack Pack versus an independent expected value, shared inline values including Unicode/empty string, bounded unique-value disable, and heap/nontext original coercions. The fixture error is not a production cache failure. No release candidate build, peer performance run, independent repeat or full workspace run was performed after falsifying target applicability. Runner/build helper files are prepared but not evidence of execution. No performance claim.

Prototype removed; isolated persist.rs restored byte-for-byte. Future SmallText ingestion paths would require a new explicitly scoped experiment. Current primary dirty source may differ and is not qualified by this result.

## Still-open committed-source gaps

committed-baseline-summary.json derives only the six baseline process medians from the already published ../2026-10-24-uniform-field-list/medians.json (not a fresh run): native 1k Lin 0.880614 ms vs SQLite 0.812594 ms, 0/6 wins; native 10k 10.457385 vs 10.554458 ms, 6/6; durable 1k 2.952428 vs 2.174104 ms, 2/6; durable 10k 21.524521 vs 14.761844 ms, 0/6. These bulk cases remain distinct from the recent official single-row transaction cases. Host variation and different contracts prevent extrapolation to all workloads.

Next hypothesis: rows_to_insert_cols ColData::Vec invokes cell.as_vec().map(<[f32]>::to_vec), allocating/copying a dense vector per row before sparse WAL encoding. Profile those copies/allocations before selecting a representation change; preserve Pack/API compatibility, serialized values/bits, rollback and Full-sync. Full named-peer objective remains active; MSSQL/Kusto access and remaining coverage remain unresolved.
