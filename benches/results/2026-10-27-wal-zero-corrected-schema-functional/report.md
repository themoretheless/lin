# Positive-zero candidate combined with schema-WAL rejection correction

Functional qualification only. Core baseline is `065427b25` (see exact full SHA and source hashes in `test-status.json`). The bounded positive-zero helper and bit-exact oracle are copied unchanged from the frozen full-engine experiment. The generator overlays only persistence source, runs the complete default-feature release library/integration suite, and restores all original core bytes in `finally`.

The combined source passes 436 tests with 2 ignored, including the new rejected-schema close/reopen/next-generation regression. The independent functional audit verifies the actual full-test log, restored source hashes, exact legacy decoder equality, canonical 16 MiB rejection budget, and two 4125-to-1169-byte frame probes. The deliberate negative-zero normalization mutant fails at runtime and is caught by the oracle.

Only this copy of the functional verifier updates its expected test count from 435 to 436 because of the newly added regression. The running remote benchmark remains frozen at `c464574c5ef43afeb8afd858235078ca3329390a`; its scripts, thresholds and raw outputs are unchanged. No timing, peer victory or production adoption is established by these functional tests.

Reproduce: `LIN_ZERO_FULL_SUITE=1 python3 benches/results/2026-10-27-wal-zero-corrected-schema-functional/test.py`, then `check-mutant.py`, then `verify-functional.py`, from the repository root with no concurrent source mutation.
