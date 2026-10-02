# Rejected embedding zero-marker experiment

Removed Scratch.seen, using a zero accumulator slot as the untouched marker.
All feature weights are positive and finish resets touched slots. Existing bit
reference, batch reuse and normalization tests passed (5 tests). Six alternating
independent process pairs per mode, 24 fresh one-operation samples per case,
no warmup. Fixture setup/drop excluded; no builds or tests overlapped timing.

| Lin case | Baseline ms | Candidate ms | Paired median gain | Wins |
|---|---:|---:|---:|---:|
| Native 1k | 1.0222 | 0.9849 | 3.83% | 6/6 |
| Native 10k | 11.0968 | 10.6345 | 4.70% | 5/6 |
| Durable 1k | 2.6532 | 2.7568 | -5.29% | 1/6 |
| Durable 10k | 21.0068 | 20.5908 | 2.00% | 6/6 |

SQLite durable 1k control had paired median gain 0.06% and wins 3/6.
Rejected because the small durable workload regressed while its SQLite control
was stable. The mechanism of that regression is not established by these
wall-clock measurements. Raw samples, binary/context/source hashes and candidate
source are retained. Production embed.rs is restored exactly to baseline.

The full eight-engine objective is unproven. Small native and durable insertion
still loses to SQLite in these runs. SQL Server and Kusto test endpoints are
absent. Next investigation should profile small durable insertion/WAL costs,
rather than infer durability speed from native embedding improvements.

Full candidate cargo test --offline --workspace passed. Baseline source was
restored byte-for-byte; baseline workspace validation is recorded in the
previous FTS optimization evidence. Trailing blank log lines were normalized;
raw JSON observations are unchanged.
