# Standalone baseline-reader compatibility: positive-zero WAL candidate

The producer and reader are separately compiled release executables. The reader uses the unchanged persistence source from production commit `04749994b9ca7a090476b95401f0f7166d8b870a`; the rest of the core includes subsequent correctness fixes, frozen at `c464574c5ef43afeb8afd858235078ca3329390a`. This is not a separately released historical binary.

The producer uses the exact bounded positive-zero candidate tested by the full-engine workflow. Both executables exited successfully. The baseline reader compares complete canonical rows against producer memory captured before WAL encoding, and verifies generations after every phase.

| Phase | Generation | WAL bytes | Canonical frame bytes |
|---|---:|---:|---:|
| Seed | 1 | 6368 | 7957 |
| Two UPDATE operations in one Batch | 2 | 2275 | 8099 |
| DELETE | 3 | 1149 | 4057 |
| Rollback | 3 | 0 | 0 |
| Mixed INSERT/UPDATE Batch, canonical fallback | 4 | 8103 | 8103 |

An additional record preserves exact float bits for positive/negative zero, denormal, minimum normal, infinities and NaN payloads, plus an empty vector and other cell types. The reader also rejects an intentionally damaged frame and altered full-row expectations at runtime (both exit 101).

Reproduce from the repository root: run `build.py`, then `run.py` in a fresh output directory without existing fixtures, then `check-controls.py`. Build scripts temporarily install candidate source and restore it in `finally`. Run `python3 benches/results/2026-10-27-wal-zero-legacy-reader/verify.py` to audit retained evidence against immutable Git source objects.

This establishes compatibility for the retained fixtures. It does not establish a performance improvement, arbitrary-format compatibility, or hardware power-loss safety. The candidate remains experimental pending the independent full-engine benchmark gate.
