# Fixture-registration allocation-constant audit

Exact binaries are SHA256-bound to crossed-control run 41363558777. The script reuses the tested address-operand normalizer from `2026-11-02-duckdb-address-operand-audit/audit.py`, whose SHA256 is recorded, and rejects ambiguous symbol matches.

All 14 out-of-line `Fixture::new` functions and the complete `compare::main` function have matching instruction IDs/lengths and byte sequences after masking only branch immediates and verified RIP-relative disp32 operands. Every other byte, including immediate allocation-size constants, remains visible and matches. Full nonbranch immediate instruction listings and per-body hashes are in `summary.json`.

This does not support the hypothesis that these constructor/main allocation-size constants differ between BTreeMap and COW executables. It does not prove identical callees/data targets, heap addresses, startup allocator state, timing, or dynamic initialization. The persistent DuckDB GROUP BY latency difference remains unexplained and does not receive a waiver.

Reproduce with Capstone available: `python3 audit.py /path/to/exact-executable-artifact-directory`. No runtime, workload or acceptance gate was changed.
