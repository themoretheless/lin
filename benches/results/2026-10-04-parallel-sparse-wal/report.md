# Parallel encoding of large sparse WAL vector columns

Retained final implementation on main baseline 73bd87a: when a sparse vector column has at least 4096 rows and at least 8MiB of decoded float data, validate the shared decoded budget before encoding, split rows in half, encode the left half into the existing buffer and the right half with one scoped worker into a temporary buffer, then append the right bytes. All other batches retain the original checked per-row encoder loop. A failure to create the additional worker falls back to serial encoding of the right half; a worker panic still propagates. The parallelism is bounded to two threads for this encoding stage, and the worker is joined before write/flush.

The same sparse selection, codec 3/tag 7, row order, exact float bits, presence semantics, maximum record length, CRC and Full durable_sync are preserved. Cumulative decoded memory validation applies across columns, including Option<Vec> metadata. Resource policy: the worker uses an additional buffer for roughly half of this column's encoded bytes, and the merge copies those bytes. Peak memory was not measured. Additional CPU concurrency is explicitly part of this implementation and comparison; this is not a single-thread engine comparison or a CPU-efficiency claim. The 4096/8MiB gates are conservative, not measured optimal crossovers.

## Paired local durable measurements

Each series: six alternating baseline/candidate process pairs, 24 fresh checked fixtures per case, one operation per sample, no warmup. No builds/tests overlapped these measurements. SQLite is an unchanged same-process control. Percentages are median paired improvements derived from process medians, not ratios of aggregate medians. CPU frequency and external background load are uncontrolled. Case validation checks full expected row values and insertion count outside timing.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial refactored serial path | -6.313%, 0/6 | +1.380%, 4/6 | +12.427%, 5/6 | +0.035%, 3/6 |
| Final original small path | -0.420%, 2/6 | +0.693%, 5/6 | +11.137%, 6/6 | -0.096%, 2/6 |
| Independent final repeat | +0.430%, 4/6 | +0.575%, 3/6 | +11.002%, 6/6 | +1.094%, 4/6 |

The initial candidate factored the serial loop into a helper and validated budgets in a separate pass, including small batches. It was rejected as implemented because 1k regressed in all six pairs. The final version restores the original checked loop for small/light columns and adds a separate large-column branch. Both final series show about 11% improvement on 10k with all six wins; the small case changes sign between the two series and shows no consistent regression. The unchanged SQLite control changes by roughly -0.10% / +1.09% at 10k, materially less than the repeated Lin change.

Final-repeat aggregate process medians, milliseconds:

| Size | Lin baseline | Lin final | SQLite final control |
|---|---:|---:|---:|
| 1k | 3.148698 | 3.101396 | 1.860291 |
| 10k | 23.824542 | 21.260292 | 19.053614 |

Lin still loses these durable insert comparisons to SQLite. This is concrete progress toward, not proof of, the complete named-peer objective. No MSSQL/Kusto or refreshed full-peer superiority claim is made.

## Verification and provenance

Initial eight WAL integrity tests passed. Full workspace tests passed for the final production implementation. After adding an additional budget regression, all nine focused WAL integrity tests passed; production code before #[cfg(test)] was verified byte-identical to the measured final source, with SHA-256 in verified-source-sha256.json. New tests use an independent scalar sparse encoder to compare all bytes of 4101 mixed rows across odd worker boundaries, None/Some(empty), negative zero, NaN payload, infinity and tail coordinates; compare actual WAL payload bytes; decode the final frame; and inject thread-creation failure to verify identical serial fallback. A two-column fixture exceeding the shared 64MiB decoded budget verifies the existing WAL bytes and byte counter remain unchanged after rejection. This injects resource errors and does not claim actual OS exhaustion testing.

Baseline executable is the previously pinned row-major final binary, copied without rebuilding; before-persist.rs is byte-identical to its retained final-persist.rs snapshot. Intervening commits were benchmark evidence only. Candidate-persist.rs identifies the initial variant, final-persist.rs the measured final variant, and verified-persist.rs the final source including the additional test. Raw outputs and initial/final/repeat source and binary hashes are separate. run-pairs.py, run-final.py and run-repeat-final.py record exact invocations using pinned executable paths. The goal remains active.
