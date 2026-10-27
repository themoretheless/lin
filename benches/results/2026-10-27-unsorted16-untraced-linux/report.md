# Uninstrumented allocator runtime qualification

Runtime gate: **FAIL**. 1008 processes, 8064 wall observations. Candidate SQLite wins: 24/168.

Source commit: `083677b02896fa79d7389a1937e1010594a77a50`. Binary SHA256: `236c4a14e515b9061b815a876ba8f2ac1d1b2d5779ca75f6cd64041cde43de38`.

One identical Map/static-output prototype binary runs in all roles. The candidate changes the process glibc runtime setting for both engines. This measures runtime impact; it does not qualify the prototype against production main. All-eight-peer superiority is unproven.

Gain is positive when candidate runtime is faster than inherited runtime. A/A is the median absolute baseline/control difference. Each series contains six process pairs; every control remains in the table.

| Case | Series | Gain % | A/A % | Positive /6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | +2.964 | 4.601 | 4 | FAIL |
| compare/update_1row_1k/lin | series2 | +2.087 | 2.297 | 4 | FAIL |
| compare/update_1row_1k/sqlite | series1 | -1.566 | 4.743 | 2 | PASS |
| compare/update_1row_1k/sqlite | series2 | +0.590 | 1.776 | 3 | PASS |
| compare/delete_1row_1k/lin | series1 | +1.614 | 2.993 | 4 | PASS |
| compare/delete_1row_1k/lin | series2 | +4.386 | 2.489 | 6 | PASS |
| compare/delete_1row_1k/sqlite | series1 | -0.580 | 0.911 | 2 | PASS |
| compare/delete_1row_1k/sqlite | series2 | -1.110 | 1.882 | 1 | PASS |
| compare/update_1row_10k/lin | series1 | -1.419 | 0.868 | 1 | FAIL |
| compare/update_1row_10k/lin | series2 | +2.629 | 1.509 | 5 | FAIL |
| compare/update_1row_10k/sqlite | series1 | -7.205 | 4.424 | 1 | FAIL |
| compare/update_1row_10k/sqlite | series2 | +0.065 | 3.579 | 3 | PASS |
| compare/delete_1row_10k/lin | series1 | +0.147 | 2.317 | 3 | PASS |
| compare/delete_1row_10k/lin | series2 | -2.771 | 3.686 | 2 | PASS |
| compare/delete_1row_10k/sqlite | series1 | +2.010 | 8.238 | 4 | PASS |
| compare/delete_1row_10k/sqlite | series2 | -2.773 | 4.815 | 2 | PASS |
| compare/update_1row_100k/lin | series1 | -0.274 | 4.059 | 2 | FAIL |
| compare/update_1row_100k/lin | series2 | +0.810 | 2.290 | 5 | FAIL |
| compare/update_1row_100k/sqlite | series1 | +1.772 | 11.349 | 3 | PASS |
| compare/update_1row_100k/sqlite | series2 | -0.795 | 16.126 | 3 | PASS |
| compare/delete_1row_100k/lin | series1 | +0.820 | 1.479 | 4 | PASS |
| compare/delete_1row_100k/lin | series2 | +1.682 | 2.709 | 4 | PASS |
| compare/delete_1row_100k/sqlite | series1 | +1.486 | 7.583 | 3 | PASS |
| compare/delete_1row_100k/sqlite | series2 | -3.762 | 20.269 | 2 | PASS |
| compare/tx_commit_1row_1k/lin | series1 | +0.565 | 2.350 | 3 | PASS |
| compare/tx_commit_1row_1k/lin | series2 | +3.518 | 6.275 | 3 | PASS |
| compare/tx_commit_1row_1k/sqlite | series1 | +2.471 | 3.020 | 4 | PASS |
| compare/tx_commit_1row_1k/sqlite | series2 | +0.422 | 2.944 | 3 | PASS |
| compare/tx_rollback_1row_1k/lin | series1 | +1.826 | 2.857 | 4 | PASS |
| compare/tx_rollback_1row_1k/lin | series2 | -2.869 | 1.867 | 2 | FAIL |
| compare/tx_rollback_1row_1k/sqlite | series1 | +0.997 | 2.933 | 4 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series2 | -0.162 | 1.616 | 2 | PASS |
| compare/tx_commit_1row_10k/lin | series1 | -1.823 | 2.375 | 2 | PASS |
| compare/tx_commit_1row_10k/lin | series2 | +2.709 | 3.037 | 5 | PASS |
| compare/tx_commit_1row_10k/sqlite | series1 | +0.866 | 5.760 | 4 | PASS |
| compare/tx_commit_1row_10k/sqlite | series2 | +1.853 | 5.541 | 4 | PASS |
| compare/tx_rollback_1row_10k/lin | series1 | -1.763 | 2.003 | 1 | PASS |
| compare/tx_rollback_1row_10k/lin | series2 | +2.232 | 2.113 | 5 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series1 | +1.149 | 2.930 | 4 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series2 | +0.788 | 3.448 | 4 | PASS |
| compare/insert_bulk_1k/lin | series1 | +0.876 | 1.661 | 4 | PASS |
| compare/insert_bulk_1k/lin | series2 | -0.928 | 1.493 | 3 | PASS |
| compare/insert_bulk_1k/sqlite | series1 | -0.433 | 1.172 | 1 | PASS |
| compare/insert_bulk_1k/sqlite | series2 | -1.543 | 0.869 | 0 | PASS |
| compare/insert_bulk_10k/lin | series1 | +0.397 | 0.737 | 4 | PASS |
| compare/insert_bulk_10k/lin | series2 | +0.363 | 1.290 | 3 | PASS |
| compare/insert_bulk_10k/sqlite | series1 | +0.344 | 0.937 | 5 | PASS |
| compare/insert_bulk_10k/sqlite | series2 | -0.106 | 0.355 | 3 | PASS |
| compare/durable_insert_1k/lin | series1 | -0.341 | 1.593 | 3 | PASS |
| compare/durable_insert_1k/lin | series2 | +1.948 | 1.720 | 5 | PASS |
| compare/durable_insert_1k/sqlite | series1 | -3.272 | 1.079 | 1 | FAIL |
| compare/durable_insert_1k/sqlite | series2 | +1.004 | 1.308 | 4 | PASS |
| compare/durable_insert_10k/lin | series1 | +0.309 | 1.073 | 4 | PASS |
| compare/durable_insert_10k/lin | series2 | +0.939 | 1.080 | 4 | PASS |
| compare/durable_insert_10k/sqlite | series1 | -0.630 | 0.770 | 2 | PASS |
| compare/durable_insert_10k/sqlite | series2 | +0.113 | 0.513 | 4 | PASS |

## SQLite gap under the same candidate runtime

Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.

| Lin case | Series | Lin / SQLite | Wins /6 |
|---|---|---:|---:|
| compare/update_1row_1k/lin | series1 | 1.4854 | 0 |
| compare/update_1row_1k/lin | series2 | 1.5623 | 0 |
| compare/delete_1row_1k/lin | series1 | 2.1942 | 0 |
| compare/delete_1row_1k/lin | series2 | 2.0720 | 0 |
| compare/update_1row_10k/lin | series1 | 1.5730 | 0 |
| compare/update_1row_10k/lin | series2 | 1.6864 | 0 |
| compare/delete_1row_10k/lin | series1 | 2.0318 | 0 |
| compare/delete_1row_10k/lin | series2 | 2.3969 | 0 |
| compare/update_1row_100k/lin | series1 | 1.4267 | 0 |
| compare/update_1row_100k/lin | series2 | 1.5399 | 0 |
| compare/delete_1row_100k/lin | series1 | 2.1154 | 0 |
| compare/delete_1row_100k/lin | series2 | 2.1113 | 0 |
| compare/tx_commit_1row_1k/lin | series1 | 1.3692 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 1.3095 | 0 |
| compare/tx_rollback_1row_1k/lin | series1 | 1.7443 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 1.6692 | 0 |
| compare/tx_commit_1row_10k/lin | series1 | 1.4945 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 1.4011 | 0 |
| compare/tx_rollback_1row_10k/lin | series1 | 1.7485 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 1.7159 | 0 |
| compare/insert_bulk_1k/lin | series1 | 1.1341 | 0 |
| compare/insert_bulk_1k/lin | series2 | 1.1393 | 0 |
| compare/insert_bulk_10k/lin | series1 | 0.9609 | 6 |
| compare/insert_bulk_10k/lin | series2 | 0.9602 | 6 |
| compare/durable_insert_1k/lin | series1 | 0.8431 | 6 |
| compare/durable_insert_1k/lin | series2 | 0.8668 | 6 |
| compare/durable_insert_10k/lin | series1 | 1.5102 | 0 |
| compare/durable_insert_10k/lin | series2 | 1.5086 | 0 |

## Whole-process peak RSS

KiB from GNU time for the complete fixture lifecycle. The delta compares medians; the maximum preserves isolated high-water observations. These are process peaks, not retained-cache measurements or per-operation allocation counts.

| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |
|---|---|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 18880 | 18858 | 18812 | -0.360 | 18888 |
| compare/update_1row_1k/lin | series2 | 18886 | 18932 | 18834 | -0.275 | 18944 |
| compare/update_1row_1k/sqlite | series1 | 12716 | 12706 | 12678 | -0.299 | 12764 |
| compare/update_1row_1k/sqlite | series2 | 12712 | 12630 | 12826 | +0.897 | 12860 |
| compare/delete_1row_1k/lin | series1 | 18860 | 18864 | 18884 | +0.127 | 19008 |
| compare/delete_1row_1k/lin | series2 | 18870 | 18852 | 18872 | +0.011 | 19012 |
| compare/delete_1row_1k/sqlite | series1 | 12682 | 12598 | 12686 | +0.032 | 12788 |
| compare/delete_1row_1k/sqlite | series2 | 12608 | 12640 | 12758 | +1.190 | 12860 |
| compare/update_1row_10k/lin | series1 | 71976 | 72070 | 71908 | -0.094 | 72136 |
| compare/update_1row_10k/lin | series2 | 72142 | 72068 | 72056 | -0.119 | 72328 |
| compare/update_1row_10k/sqlite | series1 | 16490 | 16452 | 16488 | -0.012 | 16636 |
| compare/update_1row_10k/sqlite | series2 | 16496 | 16516 | 16458 | -0.230 | 16580 |
| compare/delete_1row_10k/lin | series1 | 72064 | 72022 | 72088 | +0.033 | 72216 |
| compare/delete_1row_10k/lin | series2 | 72068 | 72008 | 72060 | -0.011 | 72236 |
| compare/delete_1row_10k/sqlite | series1 | 16448 | 16510 | 16552 | +0.632 | 16672 |
| compare/delete_1row_10k/sqlite | series2 | 16450 | 16422 | 16446 | -0.024 | 16664 |
| compare/update_1row_100k/lin | series1 | 589964 | 589934 | 590004 | +0.007 | 590076 |
| compare/update_1row_100k/lin | series2 | 589982 | 590042 | 589894 | -0.015 | 590036 |
| compare/update_1row_100k/sqlite | series1 | 55688 | 55572 | 55590 | -0.176 | 55760 |
| compare/update_1row_100k/sqlite | series2 | 55658 | 55624 | 55700 | +0.075 | 55756 |
| compare/delete_1row_100k/lin | series1 | 589986 | 590070 | 589978 | -0.001 | 590252 |
| compare/delete_1row_100k/lin | series2 | 589984 | 590032 | 590010 | +0.004 | 590136 |
| compare/delete_1row_100k/sqlite | series1 | 55618 | 55624 | 55650 | +0.058 | 55828 |
| compare/delete_1row_100k/sqlite | series2 | 55598 | 55606 | 55602 | +0.007 | 55744 |
| compare/tx_commit_1row_1k/lin | series1 | 18830 | 18870 | 18884 | +0.287 | 18896 |
| compare/tx_commit_1row_1k/lin | series2 | 18836 | 18874 | 18882 | +0.244 | 18988 |
| compare/tx_commit_1row_1k/sqlite | series1 | 12636 | 12660 | 12712 | +0.601 | 12772 |
| compare/tx_commit_1row_1k/sqlite | series2 | 12628 | 12534 | 12710 | +0.649 | 12852 |
| compare/tx_rollback_1row_1k/lin | series1 | 18938 | 18852 | 18856 | -0.433 | 18888 |
| compare/tx_rollback_1row_1k/lin | series2 | 18874 | 18858 | 18866 | -0.042 | 18976 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 12624 | 12620 | 12636 | +0.095 | 12748 |
| compare/tx_rollback_1row_1k/sqlite | series2 | 12670 | 12710 | 12628 | -0.331 | 12704 |
| compare/tx_commit_1row_10k/lin | series1 | 72086 | 72014 | 72058 | -0.039 | 72080 |
| compare/tx_commit_1row_10k/lin | series2 | 71972 | 72096 | 72068 | +0.133 | 72296 |
| compare/tx_commit_1row_10k/sqlite | series1 | 16430 | 16454 | 16494 | +0.390 | 16664 |
| compare/tx_commit_1row_10k/sqlite | series2 | 16452 | 16552 | 16456 | +0.024 | 16664 |
| compare/tx_rollback_1row_10k/lin | series1 | 72040 | 72104 | 72132 | +0.128 | 72200 |
| compare/tx_rollback_1row_10k/lin | series2 | 72052 | 72004 | 72132 | +0.111 | 72180 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 16552 | 16432 | 16454 | -0.592 | 16580 |
| compare/tx_rollback_1row_10k/sqlite | series2 | 16450 | 16456 | 16422 | -0.170 | 16580 |
| compare/insert_bulk_1k/lin | series1 | 19314 | 19344 | 19362 | +0.249 | 19424 |
| compare/insert_bulk_1k/lin | series2 | 19296 | 19508 | 19476 | +0.933 | 19548 |
| compare/insert_bulk_1k/sqlite | series1 | 13246 | 13214 | 13158 | -0.664 | 13300 |
| compare/insert_bulk_1k/sqlite | series2 | 13256 | 13270 | 13272 | +0.121 | 13280 |
| compare/insert_bulk_10k/lin | series1 | 78768 | 78598 | 78742 | -0.033 | 78888 |
| compare/insert_bulk_10k/lin | series2 | 78742 | 78724 | 78764 | +0.028 | 78800 |
| compare/insert_bulk_10k/sqlite | series1 | 21658 | 21614 | 21640 | -0.083 | 21680 |
| compare/insert_bulk_10k/sqlite | series2 | 21674 | 21678 | 21664 | -0.046 | 21680 |
| compare/durable_insert_1k/lin | series1 | 21666 | 20612 | 20626 | -4.800 | 22320 |
| compare/durable_insert_1k/lin | series2 | 20678 | 20538 | 20728 | +0.242 | 20980 |
| compare/durable_insert_1k/sqlite | series1 | 12992 | 13204 | 12972 | -0.154 | 13260 |
| compare/durable_insert_1k/sqlite | series2 | 13008 | 13236 | 12970 | -0.292 | 13080 |
| compare/durable_insert_10k/lin | series1 | 88110 | 86930 | 87118 | -1.126 | 88948 |
| compare/durable_insert_10k/lin | series2 | 87692 | 87286 | 88472 | +0.889 | 89432 |
| compare/durable_insert_10k/sqlite | series1 | 22322 | 22388 | 22324 | +0.009 | 22532 |
| compare/durable_insert_10k/sqlite | series2 | 22386 | 22480 | 22376 | -0.045 | 22528 |

Read workloads and DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas are outside this run. RSS records cover the full process lifecycle and do not establish a bound on retained allocator caches. No production setting is accepted by this report alone.
