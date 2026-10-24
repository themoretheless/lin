# FTS ASCII append tokenization: rejected

Baseline source `9182469f0fbd953c7de43d15e2f8129ba5c63e2f`. Baseline executable reused only after every core source hash and binary hash matched the pinned reader-only baseline. Exact candidate source retained. Production fts.rs restored byte-for-byte.

## Candidate and checks

Only append_row_with_scratch changed: after lowering, ASCII without vertical tab uses split_ascii_whitespace, otherwise the original Unicode split_whitespace. Vertical tab requires fallback because ASCII splitting excludes it. Token values, case rules, duplicate postings, implicit snippet, pending-edit fallback, query code and durable format remain unchanged. No dependency or sync/feature tradeoff.

Ten focused FTS tests passed (one ignored), including a new exact posting-map comparison against tokenize (unchanged Unicode splitter), ASCII whitespace/vertical tab, Unicode separators/case, NUL, duplicates and implicit snippets. Query candidates also compared to the general insertion path. Full default workspace and release build passed. The first new-test compilation passed &&str instead of &str; corrected before all executed checks, with original log retained.

## Paired timing

Two independent six-pair alternating baseline/candidate series, native and Full durable 1k/10k, 24 fresh samples per process/case, one operation and no warmup. Setup, preparation, exact readback and teardown outside timing; SQLite unchanged control, all raw observations/outliers retained. No runner build/test overlap; other host work uncontrolled. Positive paired gain means faster.

| Case | Series 1 gain | Series 2 gain | Faster pairs S1/S2 |
|---|---:|---:|---:|
| insert_bulk_1k/lin | -34.737% | +0.386% | 3/6, 4/6 |
| insert_bulk_1k/sqlite | +1.016% | -0.989% | 3/6, 1/6 |
| insert_bulk_10k/lin | -1.022% | +2.245% | 2/6, 4/6 |
| insert_bulk_10k/sqlite | +28.199% | -0.984% | 4/6, 1/6 |
| durable_insert_1k/lin | -0.708% | +9.215% | 3/6, 4/6 |
| durable_insert_1k/sqlite | -0.354% | +3.554% | 3/6, 4/6 |
| durable_insert_10k/lin | -1.259% | -2.844% | 0/6, 2/6 |
| durable_insert_10k/sqlite | +0.090% | +1.690% | 3/6, 4/6 |

Reject: durable 10k regresses in both series (-1.26%, -2.84%) while SQLite control is flat/faster. Durable 1k first -0.71%, then +9.22% with a +3.55% control shift; this does not reproduce a stable targeted gain. First-series native timings have major control/outlier shifts and are retained, not used to assert a causal -34.7% production regression. Do not select only the positive repeat. No dedicated RSS/server refresh warranted for rejection.

Existing production read/native wins remain source-identical; durable 1k SQLite gap and absent MSSQL/Kusto remain unresolved. This diagnostic establishes no new peer win or full-goal completion.
