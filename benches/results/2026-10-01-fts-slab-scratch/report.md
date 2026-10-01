# Reuse FTS lowercase scratch across an appended slab

Retained: append_slab reuses one String for lowercase conversion across rows. Standalone append_row and pending-edit fallback keep previous semantics. No normalization, tokenization, posting or embedding semantics changed. Default lowercase fixture unchanged; opt-in LIN_BENCH_MIXED_CASE uses mixed-case titles for insertion-only comparisons and exact readback validation. Both pinned binaries include identical benchmark fixture changes.

Six alternating baseline/candidate process pairs per fixture, 24 fresh fixtures per case, one timed insert per fixture. Schema, preparation, validation and input destruction excluded; default embedding, FTS and scalar index enabled. No builds or tests overlapped timing. Fixed case order and uncontrolled host load; observations are not individual latency percentiles or evidence of universal peer victory. Values are median of process medians, milliseconds. Improvement is median paired relative decrease.

| Fixture | Rows | Baseline Lin | Candidate Lin | Paired decrease | Wins | SQLite baseline | SQLite candidate |
|---|---:|---:|---:|---:|---:|---:|---:|
| lower | 1k | 0.924958 | 0.931750 | -1.05% | 1/6 | 0.785771 | 0.800281 |
| lower | 10k | 9.942479 | 9.891365 | 1.07% | 4/6 | 10.276281 | 10.260479 |
| mixed | 1k | 0.962843 | 0.930052 | 2.96% | 6/6 | 0.790021 | 0.791990 |
| mixed | 10k | 10.229552 | 9.826375 | 3.90% | 6/6 | 10.346823 | 10.295250 |

Mixed-case Lin improves 6/6 pairs at both sizes; unchanged SQLite controls have paired median decreases -0.22% and +0.34%. Lowercase 1k Lin slower in 5/6 pairs (-1.05% median), but control SQLite also slower in 5/6 (-1.89%); lowercase 10k inconclusive (+1.07%, 4/6). Retain for repeatable mixed-case benefit without claiming lowercase improvement. SQLite still wins 1k in both fixtures. Durable writes and all eight peers were not measured in this experiment.

Validation: exact six-field insert readback outside timing in all 24 complete processes. Unit reference checks slab versus individual registration with changing buffer lengths, ASCII uppercase/lowercase, empty fields, Unicode expansion and snippet. Full offline workspace tests pass (tests-workspace.log); scoped rustfmt and git diff --check pass.
