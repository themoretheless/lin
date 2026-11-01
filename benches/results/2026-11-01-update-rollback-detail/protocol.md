# UPDATE and rollback detail diagnostic

Eleven caller-thread phases separate mutation position resolution, CAS, effective patch construction, guards/FK/undo capture, live mutation, forward indexes and remaining FTS/HNSW/maps/output. Restoration splits delta application, indexes, FTS and maps. Parts of transaction setup, pack accounting and dispatch remain residual. Cloning a Row before applying its delta is outside rollback_delta; do not attribute its cost to that phase. No source optimization or peer superiority claim.

Keep one ordinary and one instrumented binary; ordinary/control share the first, disabled/traced share the second. Observer overhead is mandatory. Preserve full-value functional oracles, six rounds, rotated process roles, eight observations and no warmup. Freeze exact cases and total process count before any Linux timings. Do not transfer earlier-source percentages or sum phases from different invocations.

Frozen diagnostic cases: durable UPDATE and ROLLBACK, 1k and 10k. Four cases, three smoke modes (12 checks), four timing roles, six rounds, eight observations: 96 fresh processes and 768 measured observations. No engine acceptance gate.
