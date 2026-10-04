# Parallel exact sparse-vector size classification

Retained on baseline main f1b5219: for at least 4096 rows and estimated dense bytes at least 8MiB, compute the exact sparse serialized size in at most four consecutive chunks with up to three scoped workers. Sum per-chunk nonnegative sizes with saturating arithmetic, then retain the original sparse < dense decision. Empty-vector preservation still returns true before this branch; small columns retain the original serial classification. Worker-creation failures recompute the affected chunk serially. Worker panics propagate. No per-row count cache, new format, byte encoding, default embedding or durability changes.

The new classification workers finish before the existing four-way encoder workers start. Peak parallelism remains at most four execution contexts in each stage, but a large vector column now creates up to three additional scoped workers and has another parallel stage. Classification does not clone vector payloads or keep a count table; it reads immutable slices and returns integer sizes. Small worker-handle storage and OS thread resources are additional; peak RSS/CPU energy were not measured. Gates are conservative, not optimal crossover measurements.

## Durable measurements

Two independent six-pair series using exactly the same pinned binaries; alternating baseline/candidate order, 24 fresh checked fixtures per case, one operation/sample, no warmup. No builds/tests overlapped timing. Existing full row/count validation runs outside timing. SQLite is an unchanged control. Reported percentages are medians of paired percentage improvements from per-process medians, not ratios of aggregate medians. CPU frequency/background load are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | -0.929%, 3/6 | +0.318%, 4/6 | +4.730%, 6/6 | -0.716%, 2/6 |
| Independent repeat | -0.164%, 3/6 | -0.245%, 2/6 | +3.889%, 5/6 | +0.093%, 3/6 |

Large-case gains repeat with six/five wins, while unchanged SQLite controls shift by -0.716% / +0.093%, smaller than Lin's gains. Small-case results are slightly negative and mixed (3/6 each): no small-case acceleration is claimed, and these measurements do not demonstrate an isolated causal penalty from the new guard. Both aggregate small-case differences are roughly 0.01–0.04 ms and remain an open workload to improve.

Aggregate medians of process medians, milliseconds (separate from paired gain statistics):

| Series/size | Lin baseline | Lin candidate | SQLite candidate control | Lin-vs-SQLite wins |
|---|---:|---:|---:|---:|
| First series 1k | 3.068510 | 3.107333 | 1.844011 | 0/6 |
| First series 10k | 19.729896 | 18.726000 | 18.964865 | 5/6 |
| Independent repeat 1k | 3.187906 | 3.200708 | 1.891417 | 0/6 |
| Independent repeat 10k | 19.289750 | 18.587021 | 18.872573 | 5/6 |

For the existing local 10k durable fixture, the retained candidate has a small aggregate lead over SQLite in both series and wins five of six same-process comparisons each. The margin is about 1–2%, so do not generalize to universal engine superiority, unmeasured workloads/hardware, or equal per-core cost. Lin still loses durable 1k, prior native 1k remains unresolved, and full current named-peer proof including MSSQL/Kusto remains incomplete. This is progress toward the full goal, not completion.

## Validation and provenance

All ten focused WAL integrity tests and full workspace tests passed. New coverage verifies exact size counts against fixtures with known nonzero bits, None and sparse/dense vectors; dimension-independent negative zero/NaN/infinity counting; zero/one/three-row helper boundaries; 4095/4096/4101-row selection; Some(empty) override; and injected WouldBlock worker-creation fallback. Existing independent scalar-reference WAL byte comparison, vector replay, corruption/truncation/CRC and shared decoded-budget/no-write-on-error tests passed. No actual OS resource exhaustion was induced.

before-persist.rs is byte-identical to the retained four-way candidate source; baseline binary hash matches ../2026-10-04-four-way-sparse-wal/binary-sha256.json candidate. Candidate source remained byte-identical through both timed series and full workspace tests. Raw outputs, separate source/binary hashes, environment and invocation scripts are retained here. The goal remains active.
