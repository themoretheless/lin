# Single-comparison index coverage hypothesis

A single Pred::Cmp with exactly one IndexUse now calls the existing index_covers_conjunct directly, avoiding the original DNF clone of a single atomic branch. The original precision/residual/partial-index checks remain the authority. Empty/multiple uses and compound/IN/other predicates retain the original bounded DNF path.

Two focused release tests pass. The atomic reference test checks432combinations of values/operators/partial-index predicates/matching fields/use counts against the unchanged DNF reference, including2^53and infinity. A separate test covers AND/OR branch alignment. The full release library/integration suite passed451tests,2ignored. Paired timing remains pending. No speedup,allocation count,bulk/filter repair or peer superiority is claimed.

The candidate starts from preceding COW+CAS a7d4b02c3. Pending SoA and bulk-template candidates are excluded to keep source attribution explicit. Only src/index.rs changes. No WAL,sync policy,Row API or query fixture changes.

A runtime mutation test replaces the atomic fast-path decision with unconditional true. The reference test fails with an actual assertion (0passed/1failed), proving it catches unsafe coverage acceptance rather than merely compiling the candidate. All production source hashes were restored; the frozen Linux input is unchanged. See mutant-runtime.log and mutant-status.json.
