# UPDATE index dependencies: qualification pending

The positive-zero Full WAL candidate is not included: persist.rs is restored to corrected baseline 21dcd7d81 and its private positive_zero module removed. Previous frozen results remain archived and rejected.

The new candidate skips key construction and index insertion for unconditional secondary indexes whose exact field names are absent from the effective UPDATE patch. Derived body hash is included in that patch. Partial indexes always use the original insert_at path to reevaluate predicates, including time dependencies. INSERT, DELETE, rollback restoration and schema index creation retain full index maintenance.

No performance gain or peer win is established. A differential unit test compares full forward/reverse postings with full index maintenance. The initial test harness used nonexistent Store::clone and did not compile; this log is retained separately and is not runtime evidence. Full correctness tests and paired performance qualification remain required.

A second harness attempt compiled but rejected the duplicate docs[title] declaration before reaching the differential assertion. The corrected fixture uses unique uri and partial title indexes with distinct labels, asserts at least five indexes exist, and compares every forward and reverse map. This fixture failure is retained separately.

The first full-suite attempt stopped at the new fixture because extra is not a declared docs field (111 existing unit tests passed, one harness failure, two ignored). The corrected fixture uses declared optional snippet for missing/Null/numeric key cases. This failed log remains preserved; it is not a candidate runtime regression or a passing qualification.

## Dispatch decision

The corrected candidate passed all 455 release tests (two ignored). Performance qualification is intentionally not dispatched: the frozen setup_lin_write creates only docs[wing, ts] and changes wing, so there is no unaffected secondary index to skip in any of the three declared targets. This source inspection does not prove a timing loss or gain. The candidate is not accepted as progress on those measured UPDATE targets. Planned scripts and matrix remain preserved as unexecuted assets. Runtime mutation checks are separate correctness evidence, not timing evidence.

Both separately compiled runtime mutants were rejected by the differential test (exit 101): skipping partial-predicate reevaluation, and skipping changed keys. Original source was restored and matches the pre-test SHA256 manifest.
