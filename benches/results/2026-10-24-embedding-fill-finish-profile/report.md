# Sampled embedding fill/finish diagnostic

2026-10-24 baseline 5fff490. Source-derived default HashingEmbedder prefix and original lowercase helper compiled into a standalone diagnostic. Production source unchanged; original prefix, instrumented gate, source/binary hashes, compiler, build log and raw data retained. The sampled batch implementation times every sixteenth row: fill includes lowercase preparation and feature accumulation; finish includes normalization, output Arc allocation/copy and scratch reset. Other rows follow the existing fill/finish sequence. Results match the actual public lin::HashingEmbedder bit-for-bit in preflight for every fixture/dimension.

Three independent release processes, twelve warm batches per fixture, stride 16, 63/625 selected rows per 1k/10k batch. Input text construction and validation outside timing. Profiling Vec push is outside individual phase timers but inside whole batch; output drop outside batch timer. Per-sample timers, branch and diagnostic collection perturb execution. No runner builds/tests overlap measurements; unrelated host activity uncontrolled. Fixed row sampling is not random sampling and may correlate with fixture structure. Fixtures are synthetic standalone embedding inputs modeled after default title/body, not complete Db insertion.

| Fixture | Fill median ns/selected row | Finish median ns/selected row | Instrumented batch ms | Selected rows/process |
|---|---:|---:|---:|---:|
| default_10k | 83.0 | 125.0 | 1.938562 | 7500 |
| default_1k | 83.0 | 125.0 | 0.171501 | 756 |
| small_dim | 125.0 | 41.0 | 0.147146 | 756 |
| unicode_1k | 1458.0 | 125.0 | 1.539000 | 756 |

## Interpretation and limitation

Observed short-row phase medians are quantized near 41/83/125 ns. Timer resolution/overhead is material relative to these spans; do not multiply medians by all rows, sum phase medians, derive fill/finish percentages or subtract them from an engine profile. This diagnostic does not establish finish as a dominant actionable bottleneck. Unicode fill observations are longer, but workload differences do not establish cross-fixture causal costs. Batch times include instrumentation and are not production throughput measurements or peer results.

The next allocation investigation needs coarser timing or independent allocation tracing, with the original shared-scratch lifecycle preserved. Do not choose a new output policy from these short sampled medians. Existing rejected sparse-output, pool, zero-marker, worker and text-assembly prototypes remain rejected; this report is not permission to repeat them unchanged.

All measured preflights match public baseline vector lengths and every f32 bit, including Unicode fixtures at dimension 768 and short dimension 8. This is finite runtime equivalence evidence; it is not universal embedding correctness or a candidate acceptance test. No production change, workspace rerun, RSS claim, engine acceleration or live peer benchmark. Durable SQLite gap and missing live MSSQL/Kusto comparisons remain open; full eight-peer objective unchanged.
