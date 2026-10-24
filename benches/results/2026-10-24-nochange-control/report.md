# Identical-binary A/A control: durable 1k noise is material

Current source `567e4a53be66de4ba4fd63c8bed9a8956429875a`; both named executables are byte-identical copies, SHA256 verified. Frozen original executable reused after checking every current production source hash matches its original build provenance. No code/format/sync change and no optimization or peer win claim.

Two independent six-pair alternating-order series for native and Full durable 1k/10k, 24 fresh samples per case/process, one operation, no warmup. Same compare invocation as recent rejected candidates, SQLite measured as unchanged control. All 48 processes and run.json complete; existing exact readback gates passed. Raw observations/outliers retained. No builds/tests overlap measurements. Host load averages captured before/after each process; those readings do not identify causation or specific competing work.

| Case | A/A apparent gain S1 | A/A apparent gain S2 |
|---|---:|---:|
| insert_bulk_1k/lin | -0.150% | +3.280% |
| insert_bulk_1k/sqlite | -0.294% | +0.240% |
| insert_bulk_10k/lin | -0.944% | +0.224% |
| insert_bulk_10k/sqlite | +0.600% | -0.819% |
| durable_insert_1k/lin | -12.793% | -2.220% |
| durable_insert_1k/sqlite | +2.247% | +0.893% |
| durable_insert_10k/lin | -1.252% | +0.514% |
| durable_insert_10k/sqlite | +0.675% | -0.150% |

## Implications

Durable 1k has median absolute per-pair difference 12.54% across 12 pairs, maximum 28.32%; first six-pair median -12.79% and repeat -2.22% with literally identical code. SQLite controls +2.25% / +0.89% do not track that Lin variation. Therefore a stable SQLite control alone cannot prove that a small Lin durable change is causal. This empirical sample is not a confidence interval, universal noise floor or explanation of the kernel/filesystem mechanism.

Recent rejected small optimizations remain rejected for lack of reproducible evidence. Their negative values must not be read as proven causal code regressions. This control also does not retroactively prove or refute an earlier accepted optimization under different conditions. Source/fixture-specific read/native peer wins are separate claims; no new measurements of their six-peer matrix here.

Before accepting the next modest durable optimization, include same-source A/A calibration in comparable conditions, controls of the affected Lin path, retained all-pair/raw data and independent repetition. If no-change drift is comparable to the proposed gain, classify it inconclusive and improve isolation/protocol or seek a larger measured bottleneck effect. Do not weaken sync, suppress outliers, stop other users processes, or repeat-until-win. See updated benchmark-contract.md.

Full objective remains open: durable 1k SQLite gap and missing MSSQL/Kusto qualification persist. This campaign improves attribution evidence rather than engine throughput.
