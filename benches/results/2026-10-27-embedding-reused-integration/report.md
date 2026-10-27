# Integrated reusable-text embedding: full-engine gate failed

Candidate and correctness scope described in correctness-before-timing-report.md. Final source snapshots preserve exact Row alias, existing vectors, custom batch overrides/fallback and default embeddings/indexes/FTS/Full mode. Default tests410passed,2existingignored. Both source-restored release binaries built before timing.

Two series x6 rounds x3 roles =36 fresh processes;8 existing native/Full durable insertion cases,8 samples each =2304 observations. Cases share each role process; every callback has a fresh fixture and existing full readback/drop policy. Rotate A/A/B roles. All raw data, CPU load telemetry, hashes and errors retained. Local macOS host activity uncontrolled; these are same-run API comparisons, not isolated engine CPU evidence.

| Case | Series | Gain % | Absolute A/A % | Positive | Pass | SQLite wins |
|---|---|---:|---:|---:|---|---:|
| compare/durable_insert_10k/lin | series1 | 1.870 | 1.552 | 5/6 | False | 6 |
| compare/durable_insert_10k/lin | series2 | 0.785 | 1.274 | 5/6 | False | 6 |
| compare/durable_insert_10k/sqlite | series1 | -0.502 | 1.424 | 1/6 | True |  |
| compare/durable_insert_10k/sqlite | series2 | -0.827 | 0.895 | 1/6 | True |  |
| compare/durable_insert_1k/lin | series1 | 3.548 | 6.331 | 3/6 | False | 0 |
| compare/durable_insert_1k/lin | series2 | -0.968 | 8.378 | 3/6 | False | 0 |
| compare/durable_insert_1k/sqlite | series1 | 3.751 | 3.467 | 5/6 | True |  |
| compare/durable_insert_1k/sqlite | series2 | -7.290 | 7.220 | 1/6 | False |  |
| compare/insert_bulk_10k/lin | series1 | 2.634 | 2.648 | 4/6 | False | 6 |
| compare/insert_bulk_10k/lin | series2 | 1.675 | 1.325 | 6/6 | True | 6 |
| compare/insert_bulk_10k/sqlite | series1 | -1.032 | 0.748 | 2/6 | True |  |
| compare/insert_bulk_10k/sqlite | series2 | 1.228 | 1.111 | 4/6 | True |  |
| compare/insert_bulk_1k/lin | series1 | 0.742 | 0.798 | 3/6 | False | 6 |
| compare/insert_bulk_1k/lin | series2 | 0.236 | 0.579 | 3/6 | False | 6 |
| compare/insert_bulk_1k/sqlite | series1 | -0.475 | 0.918 | 3/6 | True |  |
| compare/insert_bulk_1k/sqlite | series2 | 0.378 | 0.960 | 4/6 | True |  |

Decision: do not promote. Strong component gains do not survive the declared full-engine acceptance gate. All Lin target groups except native10k series2 fail consistency/noise requirements. SQLite durable1k series2 control fails: −7.290% versus A/A7.220% (retain exact boundary, do not round to pass). No unchanged rerun to remove this failure. No cause of noise inferred.

Observed candidate SQLite wins36/48: native1k12/12, native10k12/12, durable10k12/12, durable1k0/12. These are local nearby same-process API medians with differing engine contracts; baseline may also win. They neither prove causal improvement nor supersede Linux results or qualify all peers. Full durable1k gap remains. No production source change, no API rollout, no full-eight-peer completion.

The frozen Linux mutation run40448137140 is a different candidate and host; do not attribute its outcome to this embedding experiment. Component result remains valid only in its measured scope. Additional trait API, cold/lifecycle/all-features/GPU/platform validation remains unshipped/unqualified.
