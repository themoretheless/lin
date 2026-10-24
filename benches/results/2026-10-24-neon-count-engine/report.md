# NEON sparse count — full engine gate

2026-10-24 baseline da7e318. Candidate adds a private AArch64 NEON exact raw-bit nonzero counter, used in both serial sparse size classification paths (including scoped worker computation). Other architectures retain the original iterator count. No writer format, count encoding, dictionary policy, embedding, indexes, FTS, or sync changes. Candidate source snapshot is diagnostic until an explicit final acceptance decision.

Prior CPU gate ../2026-10-24-sparse-neon-count-cpu/report.md: two independent six-process series, exact reference comparison across every length 0..1024 and four starting offsets, deterministic raw bits plus special values. NEON loads fit within sixteen-float chunks and the scalar tail handles remaining elements. Count has exactly the same meaning for positive zero, negative zero, NaN and infinities; no float arithmetic or approximate sampling. This finite check is not universal formal proof or non-AArch64 runtime qualification.

Fresh baseline/candidate compare builds with exact source/binary hashes and compiler versions in provenance.json. Full default cargo test --workspace on the candidate passed. Both source snapshots retained; build.py restores baseline before measurements. Two series × six alternating process pairs at native/durable 1k/10k, 24 samples/case/process, no warmup, one operation; setup/preparation/exact fixture validation/drop outside timing, unchanged SQLite controls. No benchmark runner builds/tests overlap timing; unrelated host activity uncontrolled. Native timing shifts are controls because this helper is used by durable WAL. Keep all raw outliers, do not pool process-block measurements with unrelated protocols. Paired gains use median of per-pair percentages.

## Conventional results

| Case | Engine | Series 1 gain /positive pairs | Series 2 gain /positive pairs |
|---|---|---:|---:|
| insert_bulk_1k | lin | +0.408% /3/6 | +1.110% /4/6 |
| insert_bulk_1k | sqlite | +0.053% /3/6 | +0.524% /4/6 |
| insert_bulk_10k | lin | -0.950% /2/6 | -0.378% /1/6 |
| insert_bulk_10k | sqlite | -0.189% /2/6 | +0.142% /3/6 |
| durable_insert_1k | lin | +6.627% /5/6 | +5.574% /4/6 |
| durable_insert_1k | sqlite | -11.226% /2/6 | +1.143% /4/6 |
| durable_insert_10k | lin | +0.589% /4/6 | -2.055% /1/6 |
| durable_insert_10k | sqlite | -0.643% /3/6 | -0.660% /1/6 |

All 48 process runs completed; first-series durable 10k candidate median 19.083 ms includes substantial variability, while SQLite was 14.089 ms. Second-series 13.800 /13.522 ms also loses that peer comparison. Durable 1k remains slower than SQLite in both series. Do not suppress these losses or claim the old peer matrix validates this candidate.

## Interleaved calibration

Fresh workers built from identical compiler and worker source, exact bits checked after operations. Six process pairs/size, 24 samples/worker; A/A uses byte-identical baseline copies. Across A/A+A/B: 1152 measured samples and 48 preflights. Full worker lifecycle differs from conventional compare; protocols are not pooled.

| Mode | Rows | Median paired gain | Median absolute difference | Positive pairs |
|---|---:|---:|---:|---:|
| aa | 1000 | -2.267% | 3.794% | 3/6 |
| aa | 10000 | -0.857% | 1.316% | 2/6 |
| ab | 1000 | +1.169% | 1.981% | 5/6 |
| ab | 10000 | -0.733% | 0.858% | 2/6 |

## Decision: not accepted

Strong CPU count improvement did not establish a calibrated full-engine win: interleaved 1k effect is below A/A dispersion and 10k observations are mixed, with conventional peer losses. This is inconclusive evidence of causal performance, not proof NEON itself is slower. No further unchanged-candidate repetitions are justified here. Baseline production bytes are restored and verified. No new format, unsafe intrinsic helper, or default policy is committed to src. No physical power-cut/non-AArch64 runtime/peak RSS qualification was conducted for this rejected candidate.

Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas objective remains open: candidate does not close durable SQLite; live MSSQL/Kusto comparisons still missing. Raw diagnostics and negative observations retained for future profiling.
