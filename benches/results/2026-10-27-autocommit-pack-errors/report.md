# Autocommit Pack recovery: failure and output-boundary qualification

Compared production main047 snapshots against the combined static-output Map Row prototype. Two independent oracle tests pass on both variants (UPDATE and DELETE in each scenario):

* Full durable commit failure: replace the live log handle with a read-only handle to the same file, attempt a mutation, restore the original handle. Operation errors; full row contents, generation and complete WAL bytes remain unchanged. Indexed wing count, FTS query and durable reopen preserve the original row.
* Output quota: set max_output_bytes=1 after seeding. UPDATE/DELETE returns the output-quota error while preserving the already-committed mutation and incremented generation. This is existing main behavior, not a new rollback guarantee. Full payload accounting remains active.

Sensitivity: intentionally removing the new commit-error rollback causes full-row equality failure. Candidate mutant exits101, with one test passing and the failure oracle failing. The real baseline/candidate each exit0. Logs and exact test sources retained. Read-only-handle failure exercises a write rejection before durable append; it does not establish arbitrary partial-write, fsync or power-loss behavior. Existing broader persistence qualification remains separately scoped.

All four production source files restored and verified byte-for-byte after every run. Source/oracle hashes in provenance.json. No timing, peer win, production change or completion of the eight-peer objective is claimed.
