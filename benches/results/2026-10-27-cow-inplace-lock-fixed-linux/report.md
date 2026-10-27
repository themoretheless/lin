# COW in-place Linux paired results

Run 40428404395, frozen source 4f27182a39014536665d09a0686a2acf6794a20a. Both baseline and candidate include the owner-PID writer-lock fix. Correctness and sensitivity checks passed before release-binary measurement. All 36 processes completed, 5760 observations and 240 baseline/control/candidate comparisons independently recomputed from raw wall samples. No pair or loser is excluded.

UPDATE gains versus baseline: 1k +54.834/+58.321%, 10k +55.459/+57.838%, 100k +55.548/+54.012% (series1/series2). All six target series have 6/6 positive pairs and exceed their A/A medians. This is strong scoped evidence for the changed UPDATE pipeline, but the unchanged-control acceptance gate failed. The candidate is NOT accepted for production by this run.

SQLite wins: UPDATE 1k 8/12, 10k 10/12, 100k 10/12. Transaction commit 1k/10k each 12/12, rollback1k 1/12, rollback10k 0/12, DELETE all sizes 0/12. Overall 53/120 candidate comparisons beat SQLite; this is not an all-peer or durable qualification. All actual ratios remain in summary.json. Heap lifecycle tradeoffs are separately retained in ../2026-10-27-cow-lifecycle-memory/.

Failed unchanged controls:
- compare/update_1row_1k/sqlite series1: -12.415% versus A/A 6.727%.
- compare/tx_commit_1row_1k/sqlite series1: -3.357% versus A/A 3.112%.
- compare/tx_rollback_1row_1k/sqlite series2: -3.497% versus A/A 2.638%.
- compare/tx_rollback_1row_10k/sqlite series1: -8.021% versus A/A 6.707%.

These failures do not establish whether runner conditions, binary layout, preceding-case allocator/cache state, or another cause produced the SQLite differences. No cause is inferred from gate color alone. Keep this frozen run; do not rerun unchanged until it turns green. The next source hypothesis is to avoid rebuilding/checking unaffected non-partial scalar indexes during UPDATE, while always maintaining partial indexes and preserving rollback/uniqueness errors.
