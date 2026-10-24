# Current single-statement native insertion baseline

Production source `1164f3d`, pinned `/tmp/lin-prepared-count-candidate` worker (same production source as accepted `3a2e8d5`). Exact worker snapshot retained in worker.rs; binary/source hashes, native ARM64 image IDs, server settings and cleanup in environment.json/verification.json.

Six engines: Lin, SQLite, DuckDB, PostgreSQL, MySQL, MongoDB. Existing native insertion contracts from docs/benchmark-contract.md; fresh six-field fixture every sample, unique id/uri and wing/ts indexes; default Lin embedding/FTS. Three independent processes per size, 12 fresh samples, one operation, preparation/schema/validation/drop excluded. Driver/network/feature/durability costs differ; no physical-disk equivalence (servers tmpfs). All sentinels unchanged, no fixture objects remain; owned servers stopped after completion.

1k and 10k each pass --require-wins with all exact validation: 10/10 aggregate peer comparisons won across these sizes. At 100k the single prepared Lin AST estimates 29,285,868 bytes and correctly fails the default 8,388,608-byte AST quota in all three processes. No Lin timing is available there; the five comparisons are unqualified, not wins or timing losses. Peer timings and the failure are retained in rows-100000. Do not relax limits or report this run as complete.

The follow-up bounded-native-insert experiment uses the existing public memory transaction API with prepared chunks <=10k; it is a distinct large-input invocation path, with default quotas preserved. It does not erase this single-statement limit.

Run outcomes:

- 1000: complete, errors=[]
- 10000: complete, errors=[]
- 100000: incomplete, errors=[{'engine': 'lin', 'repetition': 0, 'reason': 'Lin worker exited'}, {'engine': 'lin', 'repetition': 1, 'reason': 'Lin worker exited'}, {'engine': 'lin', 'repetition': 2, 'reason': 'Lin worker exited'}]
