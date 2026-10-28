# BTreeMap single-statement CAS functional qualification

436release tests passed,2ignored. This includes four behavior regressions for multi-row CAS,held results,multi-statement conflicts,unique-error rollback and transaction commit/rollback,plus an actual UPDATE-result assignment proving exact public BTreeMap/Row type identity. Incorrectly disabling multi-statement snapshot/written tracking was rejected by the runtime regression. Candidate source restored and hashes verified.

No COW ported;Row remains the exact BTreeMap alias. No performance observations started. Native Linux paired component comparison is the next gate; the independent combined COW/CAS full-engine Linux run continues separately. Neither this functional result nor COW component gains establish this source's performance or the eight-peer objective.
