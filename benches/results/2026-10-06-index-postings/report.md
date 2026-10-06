# Unchanged secondary-index keys — 2026-10-06

Base: `585a351bb86fb56a69d6e63359553dd7769512af`.

The retained change returns early in `LiveIndex::insert_key` when the stored reverse key equals the newly computed key. The existing unique conflict check runs first. Updates of unrelated fields no longer search the position list, remove its entry, and append it again. Key construction and the reverse-map lookup remain; the improvement concerns index maintenance, not the entire DB update.

## Final measurement

Seven alternating pairs of fresh release-test processes. A nonunique index is populated before timing; 1000 repeated updates target the last row in a shared-key posting list. `black_box` hides the row and position from the optimizer. Cardinalities are checked outside timing. The successful unique-insert benchmark is an unchanged control and includes construction/drop of 10k-entry indexes.

| Positions sharing a key | Baseline µs/update | Final µs/update | Local ratio |
|---|---:|---:|---:|
| 1000 | 0.374 | 0.015 | 24.9× |
| 10000 | 3.301 | 0.015 | 220.1× |
| 100000 | 32.581 | 0.015 | 2172.1× |

The unchanged unique-insert control differed by about 2%, which is treated as noise. These are component microbenchmarks on a shared machine, not end-to-end DB throughput or peer-engine comparisons. Large ratios arise from replacing a linear scan with a key equality check; they do not apply to updates that change the index key. Changed-key performance was not separately measured.

## Rejected experiments

- Borrow clean FTS postings through `Cow` and only own merged results. This eliminated intermediate copies but the first paired measurements were slower for multiword requests, including pending-edit cases. Reverted because a consistent performance win was not established.
- Combine the unique duplicate check with BTreeMap entry insertion. The paired measurements differed by roughly 3%, with no demonstrated improvement. Reverted.

Raw measurements and source patch for these experiments are preserved in `experiments.json` and `experiments.diff`. Machine conditions varied substantially between the experimental and final runs; do not compare their absolute times as if they were one controlled run.

## Final verification

- Release library tests: 48 passed; two manual performance tests ignored.
- Integration: delete_readback 4, exec 61, lang 62, persist 39 — all 166 passed after the rejected changes were reverted.
- New regression checks unchanged keys, changed keys, deletions, unique conflicts, and preservation of forward/reverse state on rejected inserts. Existing suites cover composite keys, DB updates, rollback, and checkpoint/reopen.
- Formatting and `git diff --check` passed.

The temporary verification manifest compiles the repository's actual sources with the repository Cargo.lock dependency versions while excluding unrelated peer benchmark dependencies. This is not a full workspace/all-feature or CI run. `verification-manifest.toml`, compiler/source hashes, raw timing output, and test logs are included.

Reproduce the final component comparison with two release library-test binaries containing the same measurement test:

```sh
python3 measure.py /path/to/baseline-test /path/to/final-test --index-only
```

An unrelated concurrent edit appeared in `src/exec.rs` after final verification. It was preserved and is outside this change and this report's validation scope.
