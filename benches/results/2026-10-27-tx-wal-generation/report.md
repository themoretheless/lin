# Interactive transaction WAL generation and atomic publication

Baseline: f02c6ce4a9cd614ff36f91dfc169790911f5e505; production sources unchanged during reproduction, hashes in baseline-status.json. Local macOS release tests; no performance claim.

## Reproduced failures

- Rollback: base generation 1, rollback leaves 2, following autocommit emits 3. Follower needs 2 and rejects 3.
- Two-operation commit: base 1, memory generation 3, WAL generations [4,4]. Follower needs 2 and rejects 4. Separate frames also fail the required single transaction publication boundary.

The first probe failed to compile because Rust 2024 reserves `gen`; that log is preserved as baseline-compile-failure.log. It is not defect evidence. The corrected probe has two actual runtime failures in baseline.log.

## Change

Tentative execution does not advance the committed generation. Commit folds accumulated packs using existing Pack::Batch (single-pack passthrough), publishes once, then advances once. Explicit rollback and Drop restore the original committed generation. The committed flag is set before fallible checkpoint maintenance; schema catalog hash changes follow successful WAL append.

GPU SearchCache uses vector Arc identity rather than the generation; this change does not require a new GPU invalidation mechanism. Public Row representation and synchronization mode are unchanged.

## Validation

- Full release library/integration suite: 412 passed, 2 ignored (candidate-tests.log).
- Final regression target: 6 passed (final-regressions.log), including the additional checkpoint failure test after that full run.
- Every strict prefix of the exported multi-operation transaction frame leaves follower generation and full rows unchanged.
- Rollback, uncommitted Drop, empty commit and in-memory multi-write commit have explicit generation assertions.
- Real filesystem failure at owned FENCE path forces checkpoint failure after the 32nd WAL write. Commit returns an error, memory retains the mutation, exported WAL applies to follower, and normal reopen matches.

This repairs correctness before durable mutation timing. It does not establish latency improvement or superiority over any peer. Linux full integration remains further qualification; no prototype performance result is promoted.

## Subprocess recovery follow-up

Eight regression tests pass locally (crash-regressions.log). An actual child process exits with process::exit without destructors before or after a two-operation commit. Reopen must recover respectively generation 1 with both original values, or generation 2 with both changed values; the recovered full WAL also reconstructs identical rows on a fresh follower.

The initial crash oracle incorrectly assumed codec 1 for the seed insertion, which uses supported codec 2. crash-oracle-codec-failure.log retains that test failure. Generation frame inspection now targets only the newly committed transaction; full initial and transaction bytes are still exported and applied to a fresh follower. This is process-exit recovery, not a power-loss/filesystem durability proof.

## Completed Linux qualification

[Run40529581051](../2026-10-27-tx-wal-generation-linux/report.md) completed successfully at frozen046226580:415 default release tests passed,2 ignored; all eight WAL/transaction regressions and both generation probes passed. Independent audit validates exact historical source hashes. This does not qualify later aborted-transaction exec changes or WAL-reserve latency.
