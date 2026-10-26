# Single reverse-map write: allocation control

Three independent processes per variant, nine fresh-fixture operations per process, 54 exact readback/index validations pass. Prepared::run and returned Handle drop included, setup/readback excluded. Source and binary hashes retained in provenance.json, production source restored exactly.

No allocation or requested-byte saving in any of 27 pairs. Wing UPDATE 71 -> 71; title UPDATE 85 -> 85; DELETE 71 -> 71 at 1k,10k,100k. IndexKey text values already share ownership, so eliminating this key clone does not eliminate a heap allocation in these workloads. CPU tree traversal may differ; this allocation result neither proves a speedup nor a slowdown.

Candidate remains unqualified and production-disabled. Do not present reduced source operations as measured improvement or prioritize this as a solution to the multi-fold SQLite mutation gap without CPU evidence. Rejected FTS and row-undo candidates remain rejected; broad peer goal is unchanged.
