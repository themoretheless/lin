# Incremental rollback row-map fallback oracle

Additional local focused checks against the exact candidate source of frozen CI `95cba5c519d935e76ae027d4bd22f1ce7d61bd6c`; no candidate change or timing restart. Original core hashes and candidate asset hashes are pinned in provenance.json and independently checked against immutable Git objects.

Three focused tests pass: six short-cache fixtures cover docs title/layer/wing, orders user_id/total and users email; a deliberately wrong id-to-slot mapping rebuilds correctly; a missing rollback position leaves full rows and maps intact. The oracle compares canonical restored rows, id slots, URI lookup and collection-specific cached columns. These are controlled Store-state fixtures, not a claim to repair every arbitrary externally corrupted cache.

A runtime mutant moves row-map rebuild before installing the incoming Row. Candidate passes3/3; mutant fails short-cache and stale-id tests while absent-position still passes (1passed/2failed). This proves the tests detect this specific ordering defect instead of only checking row bytes. Existing inverse FTS/index-failure/identity/transaction regression evidence remains separate. Source restoration succeeds after both builds.

No speed, RSS or broader corruption-recovery claim. The running Linux qualification remains unchanged. Reproduce from repository root with `python3 benches/results/2026-10-27-incremental-rollback-fallback/run.py`.
