# Rollback correctness prerequisite for benchmark qualification

Parent: `292b93fac`; runtime before these fixes matches experimental COW+CAS `a7d4b02c3`. Production main has not been changed. No performance improvement or all-peer superiority is claimed.

## Reproduced failures

1. DELETE followed by rollback restored the row but omitted its HNSW node. A one-row internal index check failed; a public vector-search regression with live neighboring rows also failed. An entirely empty HNSW could hide this by falling back to a linear scan.
2. INSERT rollback truncated rows but retained HNSW nodes for truncated slots. The direct index regression found extra IDs after abort.
3. A new mixed regression exposed a separate row-loss bug: DELETE followed by INSERT was rolled back in the wrong order. Undo restored deleted rows first, then truncated to an append mark captured after those deletes, losing original rows and keys. The HNSW-only patch still failed this test; that failure is retained.

The first two failed test attempts were fixture mistakes (assuming an automatically stored hash on a title-only row, then using an unsupported null literal). They are retained and explicitly excluded from runtime bug evidence in failure-ledger.json. The reachable DELETE and append regressions above established the actual failures.

## Fix

Append undo marks now appear in the row journal at their original chronological position, and reverse replay undoes later appends before earlier swap-deletes. Existing append-first transactions retain their initial base mark. Rollback removes HNSW nodes for truncated appended positions and re-inserts embeddings for restored deleted positions; displaced surviving nodes retain the existing inverse slot move.

Local macOS ARM64 nightly1.101 release suite: **454 passed, 2 ignored**, exit0. New tests cover direct HNSW results, three mixed append/delete modes with multiple swap-deletes, public vector search under explicit/Drop/error/mixed abort, and eight INSERT/DELETE sequences under explicit/Drop/error abort, including generated IDs. They check rows, lookup keys, generation, next_id, and indexed IDs/vector matches.

This validates the exercised logical row/search behavior. HNSW uses its regular insert/remove operations; no byte-identical graph topology, memory bound, or latency claim is made. Linux Rust1.96 qualification with three separately compiled runtime-negative variants is prepared in ci.py and the workflow, pending a completed run.

Historical latency results remain tied to their original source and workload. They do not qualify this corrected source. The full-source acceptance gate and eight-peer goal remain open, including SQLite FULL WAL and absent Kusto measurements.
