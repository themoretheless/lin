# Bounded Linux WAL reserve: rejected full-engine candidate

[Run40530861048](https://github.com/themoretheless/lin/actions/runs/40530861048),frozen6d518a8a74618b9af0d0c4d88095daca807d7dee,completed every measurement and independent audit. The final source gate failed;this is not a timeout or incomplete experiment. Native Ubuntu24.04 x86_64,Rust1.96.0. Candidate correctness passed421 release tests /2 ignored,the missing-KEEP_SIZE runtime mutant,and122 correctness smoke cases.

**All2196 processes and17568 observations are complete. Candidate rejected:10/12 target windows and8/110 control windows fail the predeclared gate.** Source/functionality success is not performance acceptance. No optimization is enabled in production;no retry of the unchanged candidate is justified by these data.

Compared only bounded Linux FALLOC_FL_KEEP_SIZE reservation against the transaction-generation-corrected baseline. All payload/checksum/sync modes,Row alias and benchmark bodies remain unchanged. Baseline/control use one binary,candidate a separate binary. Two series of6 rotated process pairs. Later transaction-abort and stale-HEAD recovery source fixes are outside this frozen experiment.

## All target outcomes

Positive gain means candidate faster. Targets require6/6 positive pairs and median gain above A/A in each series. All losses are retained.

| Target | Series | Gain % | A/A % | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | 1.873 | 1.240 | 4/6 | False |
| compare/durable_update_1row_1k/lin | series2 | 5.045 | 3.568 | 6/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | 4.041 | 3.192 | 5/6 | False |
| compare/durable_delete_1row_1k/lin | series2 | 3.914 | 5.616 | 5/6 | False |
| compare/durable_commit_1row_1k/lin | series1 | 0.338 | 4.869 | 3/6 | False |
| compare/durable_commit_1row_1k/lin | series2 | 5.378 | 5.110 | 6/6 | True |
| compare/durable_update_1row_10k/lin | series1 | 3.895 | 5.417 | 4/6 | False |
| compare/durable_update_1row_10k/lin | series2 | -3.267 | 7.020 | 2/6 | False |
| compare/durable_delete_1row_10k/lin | series1 | 1.667 | 2.099 | 3/6 | False |
| compare/durable_delete_1row_10k/lin | series2 | -2.666 | 4.319 | 2/6 | False |
| compare/durable_commit_1row_10k/lin | series1 | 4.711 | 1.685 | 5/6 | False |
| compare/durable_commit_1row_10k/lin | series2 | -2.177 | 6.179 | 3/6 | False |

10k second-series medians are negative,but their magnitudes are within observed A/A. These data reject acceptance;they do not establish a unique causal regression mechanism.1k UPDATE/commit pass only the second series. No target passes both required series.

## Failed controls

Controls require gain >= -max(2%,A/A). External SQLite controls are retained too;they are not removed as inconvenient host variation.

| Control | Series | Gain % | A/A % |
|---|---|---:|---:|
| compare/durable_delete_1row_1k/sqlite_delete | series2 | -4.517 | 2.984 |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | -2.655 | 1.599 |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | -6.237 | 3.004 |
| compare/update_1row_10k/sqlite | series1 | -4.200 | 4.194 |
| compare/tx_rollback_1row_1k/lin | series2 | -2.039 | 1.701 |
| compare/filter_eq/lin | series1 | -2.146 | 2.123 |
| compare/agg_count_by/sqlite | series1 | -4.842 | 1.649 |
| compare/agg_count_by/sqlite | series2 | -2.644 | 0.945 |

## Candidate peer results

| Peer/contract | Lin wins | Comparisons |
|---|---:|---:|
| sqlite_delete | 96 | 96 |
| sqlite_wal | 0 | 96 |
| sqlite | 48 | 204 |
| duckdb | 36 | 36 |

SQLite FULL DELETE:96/96;SQLite FULL WAL:0/96. The native/read SQLite and DuckDB controls are different contracts;do not combine these with the separate prepared/API peer refresh into universal superiority. The full eight-peer objective remains open.

## Evidence and reproduction

[Complete table](measured-report.md),[independent audit](independent-audit.json),[qualification](measured-qualification.json),source/build/runtime identities and all failed windows are retained. Whole-process RSS includes setup,oracle,reopen and teardown;it is not isolated mutation memory.

raw-artifact.tar.gz preserves the entire downloaded linux-wal-reserve-engine directory,including raw samples,logs,resource records,source snapshots and original verifiers. raw-manifest.json hashes every unpacked file. The full directory is also kept unpacked locally. After cloning,extract the archive into this directory,then run from the repository root:

```sh
python3 scripts/audit-wal-reserve-engine.py benches/results/2026-10-27-linux-wal-reserve-engine-linux
```

The audit reconstructs exact frozen core/Cargo/benchmark files from immutable Git objects in a temporary directory,verifies all input hashes,and runs the original functional and raw-data verifiers. It does not reset current sources or weaken thresholds. No full-peer or final-source performance claim follows from the6d518a8a experiment.
