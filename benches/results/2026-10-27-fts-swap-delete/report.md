# Fused FTS deletion and tail move: correctness-qualified prototype

Instead of deleting all doomed-row postings then moving the last row independently, merge their sorted token sets. Common destination terms remain posted and only lose the tail position; old-only terms lose the destination, new-only terms move from tail to destination. Store calls this only for a real swap-remove; tail deletion retains its existing path. No borrowed-row/undo candidate or bitmap-emptiness prototype is included.

290 tests pass,2 existing ignored. Independent stateful oracle compares folded postings and live queries to FtsIndex::build after each delete, insert and update, including pending adds/dels, periodic folds, Unicode/snippets, tail deletions, and large token payloads. Deliberately deleting a common destination posting is detected (mutant test fails). Existing row-map/index/HNSW, rollback, WAL/reopen and quota tests pass. Production source restored after each test.

Retained old token storage is capped at64KiB of requested Vec/String capacity before computing moved-row tokens. Larger old rows use the sequential path, bounding additional requested token storage; allocator overhead/RSS are separate. Normal-path token arrays coexist, unlike the previous sequential tokenization. This resource tradeoff requires measurement.

Initial generation anchor mismatch touched no production source. Initial oracle compilation lacked its Cell import; failed log/status retained, corrected before functional conclusions. Candidate is not enabled and has no latency qualification. Next: allocation/CPU and full-engine baseline/candidate measurement, including disjoint/long/Unicode data and existing mutation controls, before any performance claim.
