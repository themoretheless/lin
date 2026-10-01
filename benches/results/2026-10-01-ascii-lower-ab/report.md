# Retained ASCII lowercase fast path

Lowercase ASCII borrows the source after byte classification; uppercase ASCII reuses scratch and make_ascii_lowercase. Unicode continues through the original per-character mapping. No content-hash algorithm or embedding formula changed.

Both binaries were rebuilt before measurement after a concurrent src/exec.rs change invalidated the first build pair. The matched current-build logs and current-common-source-sha256.json identify the actual compared state; initial build logs are historical and were not measured.

Three independent process pairs, reversing the second pair; 16 fresh fixtures per phase.

| Pair | Baseline full ms | ASCII full ms | Reduction | Baseline no embed/index/FTS ms | ASCII control ms |
|---|---:|---:|---:|---:|---:|
| 1 | 13.221 | 12.322 | 6.8% | 6.342 | 5.686 |
| 2 | 12.135 | 11.042 | 9.0% | 5.792 | 5.679 |
| 3 | 12.406 | 11.461 | 7.6% | 6.167 | 5.846 |

Median paired full-insertion reduction: 7.6%; all three pairs won. Controls also improved and background load is uncontrolled. The entire observed reduction cannot be confidently attributed to this patch, and no statistical significance or universal peer win is claimed.

## Fresh native peer validation

One process, 12 fresh fixtures, affected-count and row readback outside timing; Appender flush inside timing. Native work/schema differences are documented in the benchmark contract.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.072 | 0.785 | 1.199 |
| 10k | 11.666 | 10.856 | 10.409 |

Lin still loses both sizes to SQLite and 10k to Appender. A single-process 1k Appender win does not close the overall goal. Native delete and MSSQL/Kusto gaps remain.

Validation: complete default workspace tests passed after the external exec changes. The new test includes all 128 ASCII characters, controls, mixed case, Unicode expansions/Greek/Cyrillic, scratch reuse and borrowed pointer identity for unchanged ASCII. Existing independent embedding-bit/reference, FTS mutation/codec, rollback and durable-readback tests passed. Scoped rustfmt and git diff --check passed.
