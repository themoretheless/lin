# Reverse-before-unique prototype, not accepted

Current main047 plus COW-inplace, explicit take-all fix and Eq/Hash traits. Baseline-cow source files contain this integrated COW without the reverse/unique reorder. Candidate adds only moving the existing reverse-key equality return before the unique forward-tree lookup. No patch dependency metadata or partial-predicate skipping.

Default library/integration validation passed 410 tests,2 existing ignored. New direct index oracle covers repeated same-key no-ops, changed-key unique conflict, old-key release and exact postings. Existing partial membership/rollback/replay and COW body/hash/FTS isolation oracles pass. This assumes maintained live-index invariants and does not qualify arbitrary externally corrupted public index fields.

Incremental warm CPU results versus the integrated COW baseline: UPDATE1k/10k/100k -0.679/-0.887/-1.088%, A/A0.570/0.954/1.007%, positive pairs0/6,2/6,2/6. Rollback0.000/-0.376/+1.068%, A/A2.975/0.775/1.101%. All samples, including first-process outliers, are retained in ../2026-10-27-cow-unique-reverse-first-warm/. This is not fresh-fixture evidence. No sustained gain supports advancing this reorder; it is not shipped or included in the next profile.

Fixture-applicability correction from the integrated fresh profile: setup creates only non-unique docs[wing,ts], and UPDATE patches wing. There were no untouched scalar indexes to skip and no scalar unique lookups to bypass in this performance fixture. The semantic unique/partial oracles remain valid, but warm results are not evidence about schemas that actually exercise the proposed skip. Neither mechanism is advanced for the measured target.
