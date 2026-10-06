# FTS mutation optimization — 2026-10-06

Base commit: `c93807a9c95cafb5b2368aebde84fd59c2d972b4`.

Retained: walk the two sorted unique token lists in `sync_row` with two cursors instead of repeated linear membership searches. The difference stage changes from O(m*n) to O(m+n); tokenization/sorting and posting maintenance still have their own costs.

Correctness fix: use the same declared-field-plus-snippet text selection for incremental insert/remove/move and index construction. Include snippet changes in the update early-exit guard. The new snippet regression fails on the baseline and passes on the final source.

Rejected experiment: reuse a single normalization scratch buffer in `build` through `append_slab`. The first paired run improved the Russian fixture but regressed both ASCII fixtures. The build change was reverted; raw observations remain in `scratch-candidate.json`. These observations are insufficient to conclude a universal regression or win.

## Final local measurement

Seven pairs of alternating fresh processes, same compiler and locked dependency versions. Each update sample alternates between two texts with 50% token overlap, 200 updates per process; tokens are ASCII words. Fixture construction is outside timing. The manual measurement test checks a restored posting; separate regression tests compare full postings with a fresh rebuild over 1024 old/new token-set pairs.

| Unique tokens per text | Baseline µs/update | Final µs/update | Ratio |
|---|---:|---:|---:|
| 10 | 2.971 | 2.395 | 1.24× |
| 100 | 92.101 | 34.268 | 2.69× |
| 1000 | 4835.527 | 364.236 | 13.28× |

These are FTS-only microbenchmarks, not end-to-end DB throughput or peer-engine comparisons. Other machine activity was not isolated. The unchanged build controls varied by roughly 6–15%, so small timing changes should not be treated as reliable evidence. The large-token update improvement is consistent with eliminating quadratic comparisons.

## Verification

- Release library tests: 47 passed, 1 manual performance test ignored.
- Integration suites: delete_readback 4, exec 61, lang 62, persist 39 — all 166 passed.
- Formatting and `git diff --check` passed.
- Tests compiled the actual repository source through `verification-manifest.toml`, using the repository Cargo.lock dependency versions. The temporary manifest omits unrelated peer benchmark dependencies (including bundled DuckDB); this is not a full workspace/all-feature or CI run.

`run.json` contains raw process outputs, medians, and test-binary SHA-256 values. `environment.json` records compiler, base commit, lockfile and source hashes. `baseline-snippet.txt` contains the reproduced failure. `lib-tests.txt` and `integration-tests.txt` contain validation output.

To rerun the timing comparison after compiling baseline and final library-test binaries with the same tests and release settings:

```sh
python3 measure.py /path/to/baseline-test /path/to/final-test
```
