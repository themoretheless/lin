# Compile-time output ownership specialization

Changed hypothesis: const-generic executor selects output handling at compile time, eliminating the new flag branch from explicit transactions. Baseline remains in-place Map Row without output recovery. Current main is not the baseline of this warm diagnostic.

Full cargo test --lib --tests passes including full-output/reopen oracle. Intentional omission of recovered output is caught (101). Source restored. 18 rotated A/A/B processes, 1728 observations; same warm harness as prior Pack recovery. CPU process/load snapshots retained in status.json; other local work was active, so results remain observational.

| Rows | UPDATE gain % | A/A % | Positive pairs | Rollback gain % | A/A % |
|---|---:|---:|---:|---:|---:|
| 1000 | 18.849 | 3.546 | 5/6 | -4.598 | 6.216 |
| 10000 | 19.736 | 2.195 | 6/6 | -1.564 | 1.619 |
| 100000 | 18.726 | 0.570 | 6/6 | -1.005 | 2.299 |

No acceptance: 1k UPDATE only 5/6 positive; rollback remains slower in medians, within A/A this time. These results do not establish a causal fix to the previous rollback regressions. Binary bytes baseline5610576/candidate5625728 (+15152). No peer qualification or production rollout. A separate fresh case-isolated Linux comparison against production main is prepared, with all original controls retained; its outcome must be reported independently.
