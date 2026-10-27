# Combined writer-lock and explicit take-all qualification

All src/ file hashes exactly match main `3ac69908090cbec49a51e747db9ec50390d176f8`, with only src/exec.rs from take-all PR5 head `c914eea9fe428dd83df3f92b836fc264e4903fa6`. No COW or selective-index production source is included. Shipped integration-test file hashes match that same composite (tests/exec.rs from PR5). There is one additional experimental-branch regression target, tests/mutation_contract.rs; its hash and source commit are explicitly retained. This is the combined production source with a superset of regression tests, not an exact main test tree.

Default library/integration validation: 407 passed, 0 failed, 2 existing ignored, 24 test suites on macOS ARM64. Both real-fork writer lock ownership regression and exact take-all ID/limit regression run together. Production checkout restored afterward. This is combined correctness evidence, not a speedup or a replacement for PR platform CI. PR5 CI is still pending at collection time.

An initial post-run provenance assertion rejected the additional test file rather than falsely certifying an exact test tree. The raw successful test run was retained; the corrected provenance explicitly declares and verifies the extra regression target. No tests were dropped or rerun to hide that mismatch.
