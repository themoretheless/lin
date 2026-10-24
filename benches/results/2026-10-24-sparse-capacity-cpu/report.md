# Sparse encoder spare-capacity writes: reject for target workload

Current source `18d4afe0d0ea2ee134a948cc82a58894437ef694`; production unchanged. Trusted CPU-only experiment.

Candidate preserves eight-value zero blocks and exact legacy sparse bytes. For each nonempty block it reserves at most values.len()*8 bytes before obtaining the output pointer, writes each nonzero (index,bits) pair as one unaligned little-endian u64, and advances Vec length only across initialized bytes. No writes exceed reserved capacity, no allocation occurs while the pointer is used, row dimension checks precede emission. This replaces per-entry extend_from_slice capacity checks with one reservation per nonempty block; it is distinct from the rejected October 5 whole-column size-reservation experiment. The prototype is retained only as evidence, not shipped unsafe code.

Six independent pinned release processes, twelve alternating rounds, current/candidate in the same process, identical preallocated output buffers cleared between loops. Public Db-generated default 1k/10k embeddings, forced dense encoder and special bit/presence/tail fixtures. Exact candidate bytes checked against a source snapshot of the current encoder, including nonempty-prefix output. This is not an independent decoder, allocation-failure, Miri or durable-recovery gate. No full workspace gate needed for an unshipped diagnostic.

| Case | Baseline µs | Candidate µs | Paired gain | Faster processes |
|---|---:|---:|---:|---:|
| boundaries | 0.014 | 0.012 | +10.797% | 6/6 |
| default_10k | 2819.042 | 2780.070 | +1.568% | 6/6 |
| default_1k | 226.991 | 226.115 | +0.036% | 3/6 |
| dense | 539.685 | 376.336 | +29.692% | 6/6 |

Reject for the targeted small-batch optimization: default 1k +0.036%, 3/6 processes is effectively flat. Default serial 10k +1.57% cannot establish a gain in the actual parallel large-batch path. Dense forced-encoder gain ~29.7% does not represent production, which selects dense encoding for these values. Tiny boundary fixtures are correctness/control evidence, not a product speed claim. No end-to-end durable or RSS run warranted after the flat target CPU gate.

Raw observations, exact diagnostic/reference/candidate and binary/core hashes retained. Temporary example removed, all six exit codes zero, production hashes unchanged. Durable 1k SQLite and missing MSSQL/Kusto remain open; full objective not complete.
