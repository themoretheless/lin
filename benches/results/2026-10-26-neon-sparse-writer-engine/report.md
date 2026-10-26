# NEON sparse serialization — full-engine qualification

2026-10-26, frozen baseline `f8fb0db`. Managed isolated worktree/private build caches; production delta only src/persist.rs. Accepted persistent WAL worker policy, classifier, codecs, error/budget checks, checksum and Full sync remain unchanged. This qualifies a new NEON writer, not the previously rejected scalar mask or classifier-only NEON experiment.

## Mechanism and correctness

AArch64 runtime NEON check selects an eight-lane raw-bit presence mask, then emits index/value entries in the original ascending order. Loads occur only for complete eight-element chunks; scalar masks handle tails. Other architectures/unavailable NEON retain the exact prior scalar writer. No float arithmetic or approximate selection is introduced. Private functions only; public API/format unchanged.

Default workspace passed. New independent scalar-helper oracle checks complete bytes for lengths0..1024 with four pattern offsets, None/empty/negative-zero/NaN/Inf/deterministic raw bits, actual unaligned eight-lane slices, and4095/4096/4097 element tails. Existing pure-frame, decoded-budget/no-write, rollback/replay, shared-worker/error-precedence/queue/lifecycle tests also pass. This is finite AArch64 qualification; no non-AArch64 runtime or physical power-cut campaign was run. Original scalar code is retained and exercised directly by the oracle.

No-default and GPU/parallel/compression feature compilation passed. Final release compare/interleaved rebuilds match pinned measured binaries byte-for-byte. Exact source/binary hashes and both source snapshots retained. Before full qualification, [CPU gate](../2026-10-26-neon-sparse-writer-cpu/report.md) showed ~50% writer-only1k and ~35% serial-helper10k gains. It excludes the full engine; density crossover controls and dense sparse-helper losses remain saved.

## Conventional alternating process pairs

Two series ×6 alternating fresh baseline/candidate process pairs/native+Full durable mode:48 processes,4608 observations.24 samples/case,one operation,no warmup. Setup/preparation/readback/output destruction excluded; embedding/FTS/indexes and sync retained. Unchanged SQLite control; no own builds/tests overlap timing. Starting host load1:7.05–15.61, not CPU utilization. All processes exit0; times finite/positive and operations==1.

Gains are median per-pair process-median ratios; latency columns aggregate candidate process medians.

| Series | Case | Lin gain /positive pairs | SQLite control gain | Candidate Lin ms | Candidate SQLite ms |
|---:|---|---:|---:|---:|---:|
| 1 | insert_bulk_1k | +1.032% /3/6 | +0.222% | 0.804 | 0.816 |
| 1 | insert_bulk_10k | +1.395% /5/6 | +0.202% | 8.295 | 10.556 |
| 1 | durable_insert_1k | +5.794% /4/6 | -2.294% | 1.937 | 1.413 |
| 1 | durable_insert_10k | +2.330% /5/6 | +0.585% | 10.740 | 13.751 |
| 2 | insert_bulk_1k | -0.135% /3/6 | -0.575% | 0.776 | 0.802 |
| 2 | insert_bulk_10k | +1.800% /4/6 | -0.223% | 8.004 | 10.467 |
| 2 | durable_insert_1k | +7.097% /5/6 | +0.580% | 1.934 | 1.398 |
| 2 | durable_insert_10k | +3.032% /6/6 | -0.573% | 10.647 | 13.807 |

## Interleaved A/A and A/B

Two series,6 pairs/size/series,24 samples/worker plus preflight.96 workers,2304 measured operations plus96 preflights. Each request uses a fresh Full Db and checks all nine fixture fields including exact embedding bits after timing. A/A binaries are byte-identical baseline copies. All workers exit0; owned fixture cleanup assertions and source drift checks pass. This lifecycle differs from conventional compare; protocols are not pooled.

| Rows | Series | A/B gain | Positive pairs | A/A signed gain | A/A absolute difference |
|---:|---:|---:|---:|---:|---:|
| 1000 | 1 | +4.623% | 6/6 | -0.585% | 1.320% |
| 1000 | 2 | +4.332% | 6/6 | +1.509% | 2.889% |
| 10000 | 1 | +1.626% | 5/6 | -1.938% | 1.938% |
| 10000 | 2 | +1.722% | 6/6 | +0.337% | 0.921% |

1k reproduces above observed A/A dispersion in both series.10k first-series A/B1.626% is below A/A absolute1.938%; second-series1.722% exceeds0.921%. Do not assert a robust additional10k gain from these results. Native shifts are controls, since the sparse writer is not used by native execution. Every losing/noisy pair is retained.

## Replay and resources

For1k/10k and1/3/4 commits,12 baseline/candidate writer processes exit via process::exit(0), without database destructors.12 independent readonly verifiers recover every acknowledged row with exact embedding bits. WAL bytes match across variants in all6 scenarios; no new codec. This checks process-exit recovery rather than power removal.

Single peak-RSS observations per variant/scenario:1k one commit12.453/12.469MiB baseline/candidate;1k four commits32.391/32.422MiB;10k four commits276.688/278.484MiB. Raw time-l outputs retained; these are not statistical memory equivalence or universal bounds. The register-only writer introduces no persistent allocation/cache.

## Decision

**Accept as a limited Full durable1k improvement on the tested AArch64 fixtures.** Two conventional and two interleaved series support the target gain, independent byte/tail/unaligned tests and workspace pass, replay is exact, format/sync remain unchanged. Large-batch control does not show a reproducible slowdown; no stronger10k speed claim is made.

Durable1k still loses SQLite: candidate medians1.937/1.934ms versus1.413/1.398ms. Durable10k retains its SQLite advantage. This does not close LIN-0002 or the full eight-peer objective. The last [read/native peer matrix](../2026-10-26-persistent-peer-refresh/report.md) belongs to earlier f8fb0db code; no fresh all-peer matrix is claimed for this candidate. MSSQL/Kusto remain unmeasured, and this writer change does not explain or fix the retained native100k process outlier.
