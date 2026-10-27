# Incremental rollback across FTS pending edits and folds

Focused functional stress of the exact candidate engine frozen at CI95cba5c51. A600-row fixture retains default docs embedding/FTS and a wing index plus unique URI index.1200explicit transaction rollbacks visit all600positions twice and alternate old/new title terms sharing common tokens, exercising pending lists beyond the existing256-entry fold threshold. No claim of an instrumented exact fold count is made.

400transactions include an actual unique-index error after an accepted update;110include append+update, exercising Mixed undo. Every rollback checks the complete target Row,600-row cardinality, indexed wing count and unchanged committed generation. After every step a fresh FtsIndex is rebuilt independently from current complete rows;10token/prefix queries compare candidate index positions to the fresh reference:12000comparisons. Full canonical collection bytes match the initial fixture at the end. This validates posting membership, not a separate ranking/latency or physical power-loss contract.

One release test passes, with the structured probe retained in tests.log/result.json. Immutable Git core/candidate asset hashes, oracle hash and source restoration were independently checked; audit.json records those checks. No engine code changed, and CI was not restarted. Existing missing-inverse-FTS and wrong-rebuild-order runtime mutants remain separate sensitivity evidence.

Reproduce from repository root: `python3 benches/results/2026-10-27-incremental-rollback-fts-fold/run.py`. Performance/peer/RSS acceptance remains pending the separate complete Linux matrix.
