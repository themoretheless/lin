# Rejected bulk row construction

One independent process per variant, 12 fresh fixtures per case.

| 10k insertion | Original | Collect into BTreeMap |
|---|---:|---:|
| Full | 12.480 ms | 13.371 ms |
| No embedding | 8.909 ms | 9.394 ms |
| No embedding/scalar index | 8.007 ms | 8.355 ms |
| No embedding/scalar index/FTS | 5.691 ms | 6.441 ms |

All measured cases regressed. Background load was uncontrolled; this is not
universal performance proof. The experiment was removed. Full workspace tests
passed the candidate; duplicate-field and execution-time regression coverage
remains. No peer win is claimed.
