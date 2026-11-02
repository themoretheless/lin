# Exact current root native worker: CPU sampling diagnostic

Unmodified public ELF4a4c3918e, runtime60e64559e, default+extent, original Rust1.96.0 build. No new compiler flags or source instrumentation. Three100k/48-sample processes;300000final six-field rows independently checked,14.4million rows internally checked by the unchanged worker. All raw profiles, stack traces, requests, reports and SHA manifests retained. Zero lost CPU samples.

Scope below selects stacks containing Transaction::run_prepared or Transaction::commit. It excludes much preparation/validation/teardown. This is a conservative call-chain subset, not exact operation-timer boundaries: some worker-thread construction and Handle drops have no transaction ancestor. Known offthread row-builder samples are reported separately in JSON. Unknown libc leaf symbols remain in caller attribution; incomplete symbol names do not become omitted samples. No elapsed-time gain, CPU/wall conversion or external peer win follows.

| Process | CPU samples | TX ancestor samples | TX share of whole profile % | Embedding within TX % | FTS within TX % | Index within TX % | Maps within TX % |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 3120 | 1044 | 33.46 | 29.60 | 16.48 | 14.08 | 10.82 |
| 2 | 3132 | 1046 | 33.40 | 31.07 | 18.55 | 13.29 | 10.42 |
| 3 | 3275 | 1085 | 33.13 | 29.95 | 19.72 | 12.44 | 9.86 |

Preparation is a major part of the whole profile; optimizing parser self time would not directly target the measured insertion clock. Within the selected transaction stacks, embedding, FTS, secondary indexes and row maps provide consistent next investigation areas. Instruction-level or allocation hypotheses still require their own source-bound correctness and unchanged operation benchmarks. Prior rejected normalization, modulus and rollback variants remain rejected.
