# Borrow lowercase ASCII in lexical scoring

Lexical scoring borrows an already-lowercase ASCII blob instead of allocating
a lowercase copy. Uppercase ASCII and all Unicode still use str::to_lowercase,
including contextual Greek sigma handling. Field concatenation, token/phrase
scoring, top-k ordering and returned rows are unchanged.

Six alternating independent process pairs, 32 warm samples per case, 50ms
warmup, 5ms target, max 100 operations/sample. Same 10k-row fixture, preparation
outside timing. No tests/builds overlap performance. Process medians aggregated
before comparison; host load/cache state uncontrolled.

| Lex query | Baseline µs | Candidate µs | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Common | 159.956 | 145.700 | 8.20% | 6/6 |
| Selective | 0.712 | 0.687 | 3.46% | 5/6 |
| Missing | 0.298 | 0.300 | -1.01% | 2/6 |

The common-term improvement is sustained and exceeds the earlier ~2.2%
FTS-common regression observed with shared AST literals, though measurements
from separate runs are not directly interchangeable. Missing queries never
invoke this scorer; no improvement is claimed for that submicrosecond case.
No peer-speed, native/durable insertion or full eight-engine win is inferred.
SQLite small/durable insertion and MSSQL/Kusto validation remain unresolved.

Focused exec integration tests (61) and baseline/candidate benchmark builds
passed. Added score regression covers lowercase/uppercase ASCII, contextual
Greek sigma and a phrase spanning fields. Full workspace result recorded in
workspace-tests.log. Source/binary hashes and raw samples retained.

Full cargo test --offline --workspace passed. Scoped rustfmt and diff checks
passed. final-exec.rs also records the added regression test; the performance
binary source is candidate-exec.rs. Trailing empty log lines normalized; raw
JSON observations unchanged.
