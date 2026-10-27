# Exact-cardinality full-engine qualification

Source gate: **FAIL**. 1332 processes, 10656 wall observations. Candidate SQLite wins: 48/204.

Source commit: `236eb8ba9536c8ca8e026354719b6323d7fb4b35`. Baseline SHA256: `47983101d0c75c6cb40d0817ae1040626ca8e72c1eaca1336d6430e36e2c6051`. Candidate SHA256: `83fc7b8bed48541774ca7349e68b3e2f6ae8fe75fca6224bb35f8e136b2b1a8b`.

Baseline/control share the production-source executable. The candidate changes only private BitSet cardinality and union bookkeeping. Both binaries use identical benchmark preflight, full Row output, indexes/FTS/embedding and durability contracts. Allocator/runtime settings are unchanged. All-eight-peer superiority is unproven.

Gain is positive when candidate source is faster than baseline source. A/A is the median absolute baseline/control difference. Each series contains six process pairs; every control remains in the table.

| Case | Series | Gain % | A/A % | Positive /6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | -1.603 | 2.167 | 2 | PASS |
| compare/update_1row_1k/lin | series2 | -2.405 | 0.723 | 1 | FAIL |
| compare/update_1row_1k/sqlite | series1 | -2.305 | 3.094 | 1 | PASS |
| compare/update_1row_1k/sqlite | series2 | -0.340 | 2.182 | 2 | PASS |
| compare/delete_1row_1k/lin | series1 | -0.136 | 1.247 | 3 | PASS |
| compare/delete_1row_1k/lin | series2 | -1.701 | 2.075 | 1 | PASS |
| compare/delete_1row_1k/sqlite | series1 | -0.750 | 3.576 | 2 | PASS |
| compare/delete_1row_1k/sqlite | series2 | -0.476 | 2.011 | 3 | PASS |
| compare/update_1row_10k/lin | series1 | -1.301 | 1.520 | 0 | PASS |
| compare/update_1row_10k/lin | series2 | -2.723 | 0.799 | 2 | FAIL |
| compare/update_1row_10k/sqlite | series1 | -0.033 | 1.503 | 3 | PASS |
| compare/update_1row_10k/sqlite | series2 | +1.705 | 1.861 | 4 | PASS |
| compare/delete_1row_10k/lin | series1 | +2.210 | 3.744 | 5 | PASS |
| compare/delete_1row_10k/lin | series2 | +0.348 | 2.373 | 4 | PASS |
| compare/delete_1row_10k/sqlite | series1 | +0.421 | 2.980 | 4 | PASS |
| compare/delete_1row_10k/sqlite | series2 | +4.904 | 6.497 | 4 | PASS |
| compare/update_1row_100k/lin | series1 | +5.918 | 2.101 | 5 | PASS |
| compare/update_1row_100k/lin | series2 | +3.888 | 2.716 | 5 | PASS |
| compare/update_1row_100k/sqlite | series1 | +3.106 | 3.390 | 4 | PASS |
| compare/update_1row_100k/sqlite | series2 | +16.349 | 9.724 | 5 | PASS |
| compare/delete_1row_100k/lin | series1 | -1.253 | 2.428 | 2 | PASS |
| compare/delete_1row_100k/lin | series2 | +0.166 | 0.973 | 3 | PASS |
| compare/delete_1row_100k/sqlite | series1 | +9.654 | 2.776 | 4 | PASS |
| compare/delete_1row_100k/sqlite | series2 | +4.208 | 9.848 | 4 | PASS |
| compare/tx_commit_1row_1k/lin | series1 | -1.999 | 1.554 | 2 | PASS |
| compare/tx_commit_1row_1k/lin | series2 | +0.721 | 1.589 | 4 | PASS |
| compare/tx_commit_1row_1k/sqlite | series1 | +0.034 | 2.466 | 3 | PASS |
| compare/tx_commit_1row_1k/sqlite | series2 | -2.367 | 2.939 | 2 | PASS |
| compare/tx_rollback_1row_1k/lin | series1 | +0.486 | 2.263 | 4 | PASS |
| compare/tx_rollback_1row_1k/lin | series2 | -0.642 | 2.105 | 2 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series1 | +0.365 | 2.526 | 3 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series2 | +0.730 | 3.189 | 4 | PASS |
| compare/tx_commit_1row_10k/lin | series1 | -0.352 | 2.346 | 2 | PASS |
| compare/tx_commit_1row_10k/lin | series2 | +1.075 | 3.096 | 3 | PASS |
| compare/tx_commit_1row_10k/sqlite | series1 | -3.505 | 1.693 | 2 | FAIL |
| compare/tx_commit_1row_10k/sqlite | series2 | -2.536 | 4.739 | 3 | PASS |
| compare/tx_rollback_1row_10k/lin | series1 | -0.427 | 2.238 | 2 | PASS |
| compare/tx_rollback_1row_10k/lin | series2 | -1.134 | 0.564 | 1 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series1 | -4.861 | 2.226 | 0 | FAIL |
| compare/tx_rollback_1row_10k/sqlite | series2 | -2.753 | 0.583 | 1 | FAIL |
| compare/insert_bulk_1k/lin | series1 | -0.361 | 0.829 | 3 | PASS |
| compare/insert_bulk_1k/lin | series2 | -1.307 | 0.776 | 1 | PASS |
| compare/insert_bulk_1k/sqlite | series1 | +0.116 | 1.274 | 3 | PASS |
| compare/insert_bulk_1k/sqlite | series2 | +1.062 | 0.476 | 4 | PASS |
| compare/insert_bulk_10k/lin | series1 | -0.306 | 1.273 | 2 | PASS |
| compare/insert_bulk_10k/lin | series2 | -1.154 | 0.980 | 2 | PASS |
| compare/insert_bulk_10k/sqlite | series1 | +0.145 | 0.656 | 4 | PASS |
| compare/insert_bulk_10k/sqlite | series2 | -0.178 | 0.705 | 3 | PASS |
| compare/durable_insert_1k/lin | series1 | -0.948 | 1.020 | 2 | PASS |
| compare/durable_insert_1k/lin | series2 | +0.669 | 1.537 | 5 | PASS |
| compare/durable_insert_1k/sqlite | series1 | +1.692 | 0.846 | 6 | PASS |
| compare/durable_insert_1k/sqlite | series2 | +1.178 | 0.972 | 4 | PASS |
| compare/durable_insert_10k/lin | series1 | -1.171 | 0.739 | 0 | PASS |
| compare/durable_insert_10k/lin | series2 | -0.828 | 0.975 | 1 | PASS |
| compare/durable_insert_10k/sqlite | series1 | -0.435 | 0.801 | 2 | PASS |
| compare/durable_insert_10k/sqlite | series2 | -0.198 | 1.058 | 3 | PASS |
| compare/filter_eq/lin | series1 | +17.906 | 1.594 | 6 | PASS |
| compare/filter_eq/lin | series2 | +18.347 | 1.720 | 6 | PASS |
| compare/filter_eq/sqlite | series1 | -0.346 | 0.611 | 1 | PASS |
| compare/filter_eq/sqlite | series2 | -1.127 | 0.701 | 1 | PASS |
| compare/filter_eq/duckdb | series1 | -5.036 | 3.496 | 2 | FAIL |
| compare/filter_eq/duckdb | series2 | +3.407 | 5.774 | 4 | PASS |
| compare/filter_range/lin | series1 | +15.171 | 2.654 | 6 | PASS |
| compare/filter_range/lin | series2 | +14.454 | 2.526 | 6 | PASS |
| compare/filter_range/sqlite | series1 | -0.270 | 0.972 | 2 | PASS |
| compare/filter_range/sqlite | series2 | -1.929 | 0.851 | 0 | PASS |
| compare/filter_range/duckdb | series1 | -2.926 | 4.576 | 2 | PASS |
| compare/filter_range/duckdb | series2 | +0.742 | 1.000 | 4 | PASS |
| compare/agg_count_by/lin | series1 | +0.252 | 1.254 | 4 | PASS |
| compare/agg_count_by/lin | series2 | -0.096 | 1.634 | 3 | PASS |
| compare/agg_count_by/sqlite | series1 | -0.322 | 1.057 | 3 | PASS |
| compare/agg_count_by/sqlite | series2 | +0.038 | 0.655 | 3 | PASS |
| compare/agg_count_by/duckdb | series1 | -1.965 | 6.522 | 3 | PASS |
| compare/agg_count_by/duckdb | series2 | +1.475 | 2.942 | 4 | PASS |

## SQLite gap in candidate-source processes

Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.

| Lin case | Series | Lin / SQLite | Wins /6 |
|---|---|---:|---:|
| compare/update_1row_1k/lin | series1 | 1.8085 | 0 |
| compare/update_1row_1k/lin | series2 | 1.8254 | 0 |
| compare/delete_1row_1k/lin | series1 | 2.0906 | 0 |
| compare/delete_1row_1k/lin | series2 | 2.1086 | 0 |
| compare/update_1row_10k/lin | series1 | 2.4560 | 0 |
| compare/update_1row_10k/lin | series2 | 2.1934 | 0 |
| compare/delete_1row_10k/lin | series1 | 2.9629 | 0 |
| compare/delete_1row_10k/lin | series2 | 2.7493 | 0 |
| compare/update_1row_100k/lin | series1 | 1.9416 | 0 |
| compare/update_1row_100k/lin | series2 | 2.1198 | 0 |
| compare/delete_1row_100k/lin | series1 | 2.5666 | 0 |
| compare/delete_1row_100k/lin | series2 | 2.1637 | 0 |
| compare/tx_commit_1row_1k/lin | series1 | 1.4331 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 1.4310 | 0 |
| compare/tx_rollback_1row_1k/lin | series1 | 1.8833 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 1.8975 | 0 |
| compare/tx_commit_1row_10k/lin | series1 | 1.9644 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 1.8938 | 0 |
| compare/tx_rollback_1row_10k/lin | series1 | 2.4302 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 2.2884 | 0 |
| compare/insert_bulk_1k/lin | series1 | 1.1642 | 0 |
| compare/insert_bulk_1k/lin | series2 | 1.1435 | 0 |
| compare/insert_bulk_10k/lin | series1 | 1.6892 | 0 |
| compare/insert_bulk_10k/lin | series2 | 1.7020 | 0 |
| compare/durable_insert_1k/lin | series1 | 0.8443 | 6 |
| compare/durable_insert_1k/lin | series2 | 0.8366 | 6 |
| compare/durable_insert_10k/lin | series1 | 1.5070 | 0 |
| compare/durable_insert_10k/lin | series2 | 1.5135 | 0 |
| compare/filter_eq/lin | series1 | 0.0085 | 6 |
| compare/filter_eq/lin | series2 | 0.0084 | 6 |
| compare/filter_range/lin | series1 | 0.0131 | 6 |
| compare/filter_range/lin | series2 | 0.0129 | 6 |
| compare/agg_count_by/lin | series1 | 0.3745 | 6 |
| compare/agg_count_by/lin | series2 | 0.3722 | 6 |

## DuckDB read controls and peer gap

| Lin case | Series | Lin / DuckDB | Wins /6 |
|---|---|---:|---:|
| compare/filter_eq/lin | series1 | 0.0046 | 6 |
| compare/filter_eq/lin | series2 | 0.0045 | 6 |
| compare/filter_range/lin | series1 | 0.0042 | 6 |
| compare/filter_range/lin | series2 | 0.0042 | 6 |
| compare/agg_count_by/lin | series1 | 0.2533 | 6 |
| compare/agg_count_by/lin | series2 | 0.2578 | 6 |

Candidate read DuckDB wins: 36/36.


## Whole-process peak RSS

KiB from GNU time for the complete fixture lifecycle. The delta compares medians; the maximum preserves isolated high-water observations. These are process peaks, not retained-cache measurements or per-operation allocation counts.

| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |
|---|---|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 18660 | 18704 | 18856 | +1.050 | 18868 |
| compare/update_1row_1k/lin | series2 | 18792 | 18772 | 18796 | +0.021 | 18844 |
| compare/update_1row_1k/sqlite | series1 | 12658 | 12656 | 12676 | +0.142 | 12732 |
| compare/update_1row_1k/sqlite | series2 | 12576 | 12576 | 12590 | +0.111 | 12652 |
| compare/delete_1row_1k/lin | series1 | 18720 | 18784 | 18842 | +0.652 | 18868 |
| compare/delete_1row_1k/lin | series2 | 18732 | 18696 | 18916 | +0.982 | 18972 |
| compare/delete_1row_1k/sqlite | series1 | 12646 | 12596 | 12628 | -0.142 | 12844 |
| compare/delete_1row_1k/sqlite | series2 | 12686 | 12596 | 12672 | -0.110 | 12772 |
| compare/update_1row_10k/lin | series1 | 71970 | 71838 | 72038 | +0.094 | 72156 |
| compare/update_1row_10k/lin | series2 | 71996 | 71874 | 72048 | +0.072 | 72144 |
| compare/update_1row_10k/sqlite | series1 | 16422 | 16490 | 16412 | -0.061 | 16464 |
| compare/update_1row_10k/sqlite | series2 | 16542 | 16450 | 16418 | -0.750 | 16532 |
| compare/delete_1row_10k/lin | series1 | 71894 | 71930 | 72048 | +0.214 | 72124 |
| compare/delete_1row_10k/lin | series2 | 71972 | 72014 | 72056 | +0.117 | 72168 |
| compare/delete_1row_10k/sqlite | series1 | 16428 | 16484 | 16428 | +0.000 | 16552 |
| compare/delete_1row_10k/sqlite | series2 | 16398 | 16416 | 16416 | +0.110 | 16544 |
| compare/update_1row_100k/lin | series1 | 589936 | 589928 | 589986 | +0.008 | 590124 |
| compare/update_1row_100k/lin | series2 | 589920 | 589844 | 590002 | +0.014 | 590140 |
| compare/update_1row_100k/sqlite | series1 | 55700 | 55628 | 55632 | -0.122 | 55720 |
| compare/update_1row_100k/sqlite | series2 | 55590 | 55638 | 55584 | -0.011 | 55664 |
| compare/delete_1row_100k/lin | series1 | 589966 | 589898 | 589980 | +0.002 | 590040 |
| compare/delete_1row_100k/lin | series2 | 589856 | 590036 | 589962 | +0.018 | 590124 |
| compare/delete_1row_100k/sqlite | series1 | 55596 | 55602 | 55580 | -0.029 | 55760 |
| compare/delete_1row_100k/sqlite | series2 | 55512 | 55662 | 55676 | +0.295 | 55752 |
| compare/tx_commit_1row_1k/lin | series1 | 18780 | 18856 | 18754 | -0.138 | 18960 |
| compare/tx_commit_1row_1k/lin | series2 | 18878 | 18812 | 18826 | -0.275 | 19036 |
| compare/tx_commit_1row_1k/sqlite | series1 | 12598 | 12596 | 12678 | +0.635 | 12852 |
| compare/tx_commit_1row_1k/sqlite | series2 | 12664 | 12578 | 12682 | +0.142 | 12756 |
| compare/tx_rollback_1row_1k/lin | series1 | 18832 | 18776 | 18966 | +0.712 | 19032 |
| compare/tx_rollback_1row_1k/lin | series2 | 18672 | 18904 | 18962 | +1.553 | 18972 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 12574 | 12580 | 12618 | +0.350 | 12716 |
| compare/tx_rollback_1row_1k/sqlite | series2 | 12630 | 12666 | 12728 | +0.776 | 12748 |
| compare/tx_commit_1row_10k/lin | series1 | 72052 | 72034 | 72154 | +0.142 | 72276 |
| compare/tx_commit_1row_10k/lin | series2 | 72092 | 72008 | 72100 | +0.011 | 72224 |
| compare/tx_commit_1row_10k/sqlite | series1 | 16410 | 16450 | 16460 | +0.305 | 16672 |
| compare/tx_commit_1row_10k/sqlite | series2 | 16418 | 16462 | 16506 | +0.536 | 16544 |
| compare/tx_rollback_1row_10k/lin | series1 | 72044 | 72054 | 72172 | +0.178 | 72232 |
| compare/tx_rollback_1row_10k/lin | series2 | 72136 | 72004 | 72144 | +0.011 | 72284 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 16458 | 16472 | 16496 | +0.231 | 16584 |
| compare/tx_rollback_1row_10k/sqlite | series2 | 16454 | 16482 | 16402 | -0.316 | 16668 |
| compare/insert_bulk_1k/lin | series1 | 18894 | 18944 | 19326 | +2.286 | 19448 |
| compare/insert_bulk_1k/lin | series2 | 19030 | 18952 | 19354 | +1.703 | 19460 |
| compare/insert_bulk_1k/sqlite | series1 | 13166 | 13174 | 13104 | -0.471 | 13420 |
| compare/insert_bulk_1k/sqlite | series2 | 13242 | 13210 | 13172 | -0.529 | 13300 |
| compare/insert_bulk_10k/lin | series1 | 78172 | 78146 | 78312 | +0.179 | 78328 |
| compare/insert_bulk_10k/lin | series2 | 78106 | 78096 | 78286 | +0.230 | 78316 |
| compare/insert_bulk_10k/sqlite | series1 | 21532 | 21592 | 21570 | +0.176 | 21676 |
| compare/insert_bulk_10k/sqlite | series2 | 21624 | 21434 | 21486 | -0.638 | 21676 |
| compare/durable_insert_1k/lin | series1 | 20508 | 20376 | 20618 | +0.536 | 22312 |
| compare/durable_insert_1k/lin | series2 | 20498 | 20446 | 20502 | +0.020 | 22436 |
| compare/durable_insert_1k/sqlite | series1 | 13060 | 13128 | 13098 | +0.291 | 13264 |
| compare/durable_insert_1k/sqlite | series2 | 13094 | 13072 | 13112 | +0.137 | 13212 |
| compare/durable_insert_10k/lin | series1 | 86770 | 86782 | 87086 | +0.364 | 87824 |
| compare/durable_insert_10k/lin | series2 | 87132 | 86428 | 87162 | +0.034 | 89828 |
| compare/durable_insert_10k/sqlite | series1 | 22508 | 22282 | 22342 | -0.738 | 22768 |
| compare/durable_insert_10k/sqlite | series2 | 22278 | 22354 | 22334 | +0.251 | 22364 |
| compare/filter_eq/lin | series1 | 73560 | 73534 | 73646 | +0.117 | 73680 |
| compare/filter_eq/lin | series2 | 73614 | 73458 | 73572 | -0.057 | 73728 |
| compare/filter_eq/sqlite | series1 | 15772 | 15676 | 15716 | -0.355 | 15780 |
| compare/filter_eq/sqlite | series2 | 15772 | 15890 | 15716 | -0.355 | 15908 |
| compare/filter_eq/duckdb | series1 | 46966 | 46992 | 46682 | -0.605 | 47408 |
| compare/filter_eq/duckdb | series2 | 46480 | 47216 | 46816 | +0.723 | 47296 |
| compare/filter_range/lin | series1 | 73490 | 73312 | 73568 | +0.106 | 73684 |
| compare/filter_range/lin | series2 | 73430 | 73444 | 73680 | +0.340 | 73744 |
| compare/filter_range/sqlite | series1 | 15644 | 15708 | 15718 | +0.473 | 15872 |
| compare/filter_range/sqlite | series2 | 15772 | 15772 | 15748 | -0.152 | 15908 |
| compare/filter_range/duckdb | series1 | 47532 | 46810 | 47196 | -0.707 | 48992 |
| compare/filter_range/duckdb | series2 | 46896 | 46788 | 46890 | -0.013 | 47668 |
| compare/agg_count_by/lin | series1 | 73452 | 73532 | 73596 | +0.196 | 73728 |
| compare/agg_count_by/lin | series2 | 73530 | 73642 | 73578 | +0.065 | 73728 |
| compare/agg_count_by/sqlite | series1 | 15766 | 15644 | 15716 | -0.317 | 15780 |
| compare/agg_count_by/sqlite | series2 | 15644 | 15772 | 15778 | +0.857 | 15780 |
| compare/agg_count_by/duckdb | series1 | 47482 | 47342 | 47658 | +0.371 | 47936 |
| compare/agg_count_by/duckdb | series2 | 47126 | 47332 | 47500 | +0.794 | 47952 |

PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas are outside this run. RSS records cover the full process lifecycle and do not establish a bound on retained index memory. Tail cardinality preserves posting type layout but adds one heap word per bitmap; Vec growth and allocator rounding can increase reserved capacity further. These are component layout observations, not engine RSS. No production source is accepted by this report alone.
