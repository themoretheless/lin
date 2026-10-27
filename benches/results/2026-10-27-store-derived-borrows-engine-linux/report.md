# Disjoint Store wrapper borrowing: full-engine source qualification

Source gate: **FAIL**. 1332 processes, 10656 wall observations. Candidate SQLite wins: 48/204.

Source commit: `1d29dcdf78853e80a2efa5a687998ebed7edecc5`. Baseline SHA256: `47983101d0c75c6cb40d0817ae1040626ca8e72c1eaca1336d6430e36e2c6051`. Candidate SHA256: `fa061ed3016b098d8e12f5327de6b34bdb2021a77c3b69a26c444874f292f91c`.

Baseline/control share the production-source executable. Candidate changes only borrowing in four derived-maintenance Store wrappers, retaining exact BTreeMap Row, original UPDATE/undo/Pack handling, full output, indexes/FTS/embedding and durability. Allocator/runtime settings are unchanged; all-eight-peer superiority remains unproven.

Gain is positive when candidate source is faster than baseline source. A/A is the median absolute baseline/control difference. Each series contains six process pairs; every control remains in the table.

| Case | Series | Gain % | A/A % | Positive /6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | +9.308 | 8.698 | 6 | PASS |
| compare/update_1row_1k/lin | series2 | +0.741 | 2.423 | 5 | FAIL |
| compare/update_1row_1k/sqlite | series1 | -2.358 | 2.482 | 2 | PASS |
| compare/update_1row_1k/sqlite | series2 | -1.366 | 3.497 | 2 | PASS |
| compare/delete_1row_1k/lin | series1 | +0.737 | 5.512 | 3 | PASS |
| compare/delete_1row_1k/lin | series2 | -2.836 | 5.575 | 1 | PASS |
| compare/delete_1row_1k/sqlite | series1 | +2.003 | 2.196 | 4 | PASS |
| compare/delete_1row_1k/sqlite | series2 | -4.330 | 1.503 | 2 | FAIL |
| compare/update_1row_10k/lin | series1 | +2.422 | 3.253 | 4 | FAIL |
| compare/update_1row_10k/lin | series2 | +0.953 | 1.964 | 5 | FAIL |
| compare/update_1row_10k/sqlite | series1 | -6.700 | 4.118 | 1 | FAIL |
| compare/update_1row_10k/sqlite | series2 | -3.885 | 5.968 | 2 | PASS |
| compare/delete_1row_10k/lin | series1 | +0.294 | 5.213 | 3 | PASS |
| compare/delete_1row_10k/lin | series2 | -0.296 | 2.072 | 3 | PASS |
| compare/delete_1row_10k/sqlite | series1 | -0.222 | 12.706 | 3 | PASS |
| compare/delete_1row_10k/sqlite | series2 | -3.328 | 3.301 | 0 | FAIL |
| compare/update_1row_100k/lin | series1 | +0.322 | 8.413 | 3 | FAIL |
| compare/update_1row_100k/lin | series2 | +4.933 | 6.813 | 4 | FAIL |
| compare/update_1row_100k/sqlite | series1 | -1.810 | 11.029 | 3 | PASS |
| compare/update_1row_100k/sqlite | series2 | +1.887 | 5.953 | 4 | PASS |
| compare/delete_1row_100k/lin | series1 | +5.525 | 5.222 | 4 | PASS |
| compare/delete_1row_100k/lin | series2 | -1.039 | 6.495 | 3 | PASS |
| compare/delete_1row_100k/sqlite | series1 | -0.681 | 7.816 | 2 | PASS |
| compare/delete_1row_100k/sqlite | series2 | -4.961 | 4.495 | 2 | FAIL |
| compare/tx_commit_1row_1k/lin | series1 | +7.792 | 5.039 | 6 | PASS |
| compare/tx_commit_1row_1k/lin | series2 | +12.625 | 6.616 | 6 | PASS |
| compare/tx_commit_1row_1k/sqlite | series1 | +3.317 | 3.972 | 4 | PASS |
| compare/tx_commit_1row_1k/sqlite | series2 | +0.059 | 1.488 | 3 | PASS |
| compare/tx_rollback_1row_1k/lin | series1 | +7.056 | 4.031 | 6 | PASS |
| compare/tx_rollback_1row_1k/lin | series2 | +8.605 | 6.010 | 4 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series1 | +4.854 | 2.555 | 5 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series2 | +7.982 | 1.040 | 5 | PASS |
| compare/tx_commit_1row_10k/lin | series1 | +3.290 | 2.914 | 4 | PASS |
| compare/tx_commit_1row_10k/lin | series2 | -2.510 | 3.059 | 1 | PASS |
| compare/tx_commit_1row_10k/sqlite | series1 | -3.553 | 8.444 | 2 | PASS |
| compare/tx_commit_1row_10k/sqlite | series2 | -1.457 | 1.875 | 2 | PASS |
| compare/tx_rollback_1row_10k/lin | series1 | +3.044 | 3.217 | 4 | PASS |
| compare/tx_rollback_1row_10k/lin | series2 | +4.031 | 5.441 | 4 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series1 | +0.838 | 14.161 | 4 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series2 | -2.384 | 7.954 | 2 | PASS |
| compare/insert_bulk_1k/lin | series1 | -0.320 | 1.217 | 3 | PASS |
| compare/insert_bulk_1k/lin | series2 | -0.577 | 0.387 | 2 | PASS |
| compare/insert_bulk_1k/sqlite | series1 | +0.087 | 1.430 | 3 | PASS |
| compare/insert_bulk_1k/sqlite | series2 | +0.132 | 0.856 | 4 | PASS |
| compare/insert_bulk_10k/lin | series1 | +3.112 | 1.695 | 4 | PASS |
| compare/insert_bulk_10k/lin | series2 | +0.868 | 1.516 | 4 | PASS |
| compare/insert_bulk_10k/sqlite | series1 | -1.523 | 0.670 | 1 | PASS |
| compare/insert_bulk_10k/sqlite | series2 | -0.457 | 0.601 | 2 | PASS |
| compare/durable_insert_1k/lin | series1 | +1.995 | 1.013 | 5 | PASS |
| compare/durable_insert_1k/lin | series2 | +0.418 | 1.532 | 3 | PASS |
| compare/durable_insert_1k/sqlite | series1 | -0.369 | 1.117 | 3 | PASS |
| compare/durable_insert_1k/sqlite | series2 | -0.094 | 2.689 | 3 | PASS |
| compare/durable_insert_10k/lin | series1 | -0.375 | 1.940 | 1 | PASS |
| compare/durable_insert_10k/lin | series2 | +1.258 | 5.501 | 4 | PASS |
| compare/durable_insert_10k/sqlite | series1 | +0.657 | 1.027 | 5 | PASS |
| compare/durable_insert_10k/sqlite | series2 | +1.596 | 2.515 | 4 | PASS |
| compare/filter_eq/lin | series1 | +0.555 | 1.421 | 4 | PASS |
| compare/filter_eq/lin | series2 | +2.442 | 3.274 | 5 | PASS |
| compare/filter_eq/sqlite | series1 | +2.036 | 1.323 | 5 | PASS |
| compare/filter_eq/sqlite | series2 | +0.898 | 2.673 | 5 | PASS |
| compare/filter_eq/duckdb | series1 | -1.878 | 4.919 | 2 | PASS |
| compare/filter_eq/duckdb | series2 | -3.444 | 15.146 | 1 | PASS |
| compare/filter_range/lin | series1 | -0.941 | 1.530 | 2 | PASS |
| compare/filter_range/lin | series2 | -2.561 | 4.713 | 2 | PASS |
| compare/filter_range/sqlite | series1 | -0.531 | 0.422 | 2 | PASS |
| compare/filter_range/sqlite | series2 | +0.711 | 1.459 | 4 | PASS |
| compare/filter_range/duckdb | series1 | -1.596 | 3.312 | 2 | PASS |
| compare/filter_range/duckdb | series2 | +0.972 | 2.838 | 4 | PASS |
| compare/agg_count_by/lin | series1 | +0.127 | 0.393 | 4 | PASS |
| compare/agg_count_by/lin | series2 | +0.089 | 0.388 | 3 | PASS |
| compare/agg_count_by/sqlite | series1 | +0.001 | 1.167 | 3 | PASS |
| compare/agg_count_by/sqlite | series2 | +1.033 | 2.262 | 4 | PASS |
| compare/agg_count_by/duckdb | series1 | -1.668 | 1.408 | 3 | PASS |
| compare/agg_count_by/duckdb | series2 | -2.438 | 3.744 | 2 | PASS |

## SQLite gap in candidate-source processes

Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.

| Lin case | Series | Lin / SQLite | Wins /6 |
|---|---|---:|---:|
| compare/update_1row_1k/lin | series1 | 2.0061 | 0 |
| compare/update_1row_1k/lin | series2 | 2.0373 | 0 |
| compare/delete_1row_1k/lin | series1 | 2.7509 | 0 |
| compare/delete_1row_1k/lin | series2 | 2.6884 | 0 |
| compare/update_1row_10k/lin | series1 | 3.7062 | 0 |
| compare/update_1row_10k/lin | series2 | 3.9216 | 0 |
| compare/delete_1row_10k/lin | series1 | 4.4290 | 0 |
| compare/delete_1row_10k/lin | series2 | 4.6688 | 0 |
| compare/update_1row_100k/lin | series1 | 1.6735 | 0 |
| compare/update_1row_100k/lin | series2 | 2.0714 | 0 |
| compare/delete_1row_100k/lin | series1 | 2.2278 | 0 |
| compare/delete_1row_100k/lin | series2 | 2.7438 | 0 |
| compare/tx_commit_1row_1k/lin | series1 | 1.7584 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 1.7745 | 0 |
| compare/tx_rollback_1row_1k/lin | series1 | 2.0600 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 2.2016 | 0 |
| compare/tx_commit_1row_10k/lin | series1 | 2.4627 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 3.1799 | 0 |
| compare/tx_rollback_1row_10k/lin | series1 | 3.9008 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 3.8547 | 0 |
| compare/insert_bulk_1k/lin | series1 | 1.1077 | 0 |
| compare/insert_bulk_1k/lin | series2 | 1.1076 | 0 |
| compare/insert_bulk_10k/lin | series1 | 1.8956 | 0 |
| compare/insert_bulk_10k/lin | series2 | 1.9853 | 0 |
| compare/durable_insert_1k/lin | series1 | 0.8289 | 6 |
| compare/durable_insert_1k/lin | series2 | 0.8107 | 6 |
| compare/durable_insert_10k/lin | series1 | 2.2080 | 0 |
| compare/durable_insert_10k/lin | series2 | 2.1471 | 0 |
| compare/filter_eq/lin | series1 | 0.0117 | 6 |
| compare/filter_eq/lin | series2 | 0.0117 | 6 |
| compare/filter_range/lin | series1 | 0.0174 | 6 |
| compare/filter_range/lin | series2 | 0.0175 | 6 |
| compare/agg_count_by/lin | series1 | 0.5692 | 6 |
| compare/agg_count_by/lin | series2 | 0.5640 | 6 |

## DuckDB read controls and peer gap

| Lin case | Series | Lin / DuckDB | Wins /6 |
|---|---|---:|---:|
| compare/filter_eq/lin | series1 | 0.0077 | 6 |
| compare/filter_eq/lin | series2 | 0.0066 | 6 |
| compare/filter_range/lin | series1 | 0.0049 | 6 |
| compare/filter_range/lin | series2 | 0.0050 | 6 |
| compare/agg_count_by/lin | series1 | 0.3283 | 6 |
| compare/agg_count_by/lin | series2 | 0.3488 | 6 |

Candidate read DuckDB wins: 36/36.


## Whole-process peak RSS

KiB from GNU time for the complete fixture lifecycle. The delta compares medians; the maximum preserves isolated high-water observations. These are process peaks, not retained-cache measurements or per-operation allocation counts.

| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |
|---|---|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 18788 | 18946 | 18846 | +0.309 | 19160 |
| compare/update_1row_1k/lin | series2 | 18820 | 18888 | 18896 | +0.404 | 19024 |
| compare/update_1row_1k/sqlite | series1 | 12694 | 12784 | 12796 | +0.804 | 12984 |
| compare/update_1row_1k/sqlite | series2 | 12776 | 12806 | 12774 | -0.016 | 12856 |
| compare/delete_1row_1k/lin | series1 | 18856 | 18958 | 18850 | -0.032 | 18944 |
| compare/delete_1row_1k/lin | series2 | 19042 | 19014 | 18856 | -0.977 | 19152 |
| compare/delete_1row_1k/sqlite | series1 | 12724 | 12780 | 12804 | +0.629 | 12832 |
| compare/delete_1row_1k/sqlite | series2 | 12784 | 12820 | 12788 | +0.031 | 12832 |
| compare/update_1row_10k/lin | series1 | 72022 | 72090 | 72064 | +0.058 | 72360 |
| compare/update_1row_10k/lin | series2 | 72056 | 72002 | 72042 | -0.019 | 72200 |
| compare/update_1row_10k/sqlite | series1 | 16538 | 16572 | 16526 | -0.073 | 16776 |
| compare/update_1row_10k/sqlite | series2 | 16492 | 16590 | 16644 | +0.922 | 16756 |
| compare/delete_1row_10k/lin | series1 | 72066 | 72148 | 72150 | +0.117 | 72344 |
| compare/delete_1row_10k/lin | series2 | 72064 | 72072 | 72202 | +0.191 | 72216 |
| compare/delete_1row_10k/sqlite | series1 | 16600 | 16612 | 16596 | -0.024 | 16736 |
| compare/delete_1row_10k/sqlite | series2 | 16590 | 16550 | 16594 | +0.024 | 16656 |
| compare/update_1row_100k/lin | series1 | 590094 | 590082 | 589990 | -0.018 | 590212 |
| compare/update_1row_100k/lin | series2 | 589996 | 590054 | 590040 | +0.007 | 590140 |
| compare/update_1row_100k/sqlite | series1 | 55818 | 55600 | 55770 | -0.086 | 55828 |
| compare/update_1row_100k/sqlite | series2 | 55794 | 55784 | 55800 | +0.011 | 55836 |
| compare/delete_1row_100k/lin | series1 | 590070 | 589944 | 590042 | -0.005 | 590224 |
| compare/delete_1row_100k/lin | series2 | 589982 | 590002 | 589930 | -0.009 | 590208 |
| compare/delete_1row_100k/sqlite | series1 | 55764 | 55742 | 55796 | +0.057 | 55952 |
| compare/delete_1row_100k/sqlite | series2 | 55706 | 55634 | 55650 | -0.101 | 55856 |
| compare/tx_commit_1row_1k/lin | series1 | 18898 | 19022 | 19026 | +0.677 | 19152 |
| compare/tx_commit_1row_1k/lin | series2 | 18976 | 18968 | 18968 | -0.042 | 19056 |
| compare/tx_commit_1row_1k/sqlite | series1 | 12776 | 12718 | 12752 | -0.188 | 12832 |
| compare/tx_commit_1row_1k/sqlite | series2 | 12804 | 12784 | 12778 | -0.203 | 12892 |
| compare/tx_rollback_1row_1k/lin | series1 | 18906 | 18980 | 18974 | +0.360 | 19164 |
| compare/tx_rollback_1row_1k/lin | series2 | 19016 | 18976 | 18926 | -0.473 | 19024 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 12682 | 12792 | 12792 | +0.867 | 12944 |
| compare/tx_rollback_1row_1k/sqlite | series2 | 12736 | 12768 | 12776 | +0.314 | 12856 |
| compare/tx_commit_1row_10k/lin | series1 | 72194 | 72192 | 72146 | -0.066 | 72284 |
| compare/tx_commit_1row_10k/lin | series2 | 72224 | 72224 | 72122 | -0.141 | 72304 |
| compare/tx_commit_1row_10k/sqlite | series1 | 16534 | 16610 | 16586 | +0.315 | 16632 |
| compare/tx_commit_1row_10k/sqlite | series2 | 16588 | 16524 | 16550 | -0.229 | 16672 |
| compare/tx_rollback_1row_10k/lin | series1 | 72126 | 72156 | 72150 | +0.033 | 72168 |
| compare/tx_rollback_1row_10k/lin | series2 | 72182 | 72072 | 72112 | -0.097 | 72192 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 16582 | 16548 | 16556 | -0.157 | 16736 |
| compare/tx_rollback_1row_10k/sqlite | series2 | 16578 | 16618 | 16588 | +0.060 | 16808 |
| compare/insert_bulk_1k/lin | series1 | 19394 | 19358 | 19394 | +0.000 | 19592 |
| compare/insert_bulk_1k/lin | series2 | 19362 | 19380 | 19304 | -0.300 | 19604 |
| compare/insert_bulk_1k/sqlite | series1 | 13322 | 13346 | 13306 | -0.120 | 13368 |
| compare/insert_bulk_1k/sqlite | series2 | 13334 | 13274 | 13356 | +0.165 | 13420 |
| compare/insert_bulk_10k/lin | series1 | 78224 | 78304 | 78364 | +0.179 | 78780 |
| compare/insert_bulk_10k/lin | series2 | 78320 | 78212 | 78320 | +0.000 | 78544 |
| compare/insert_bulk_10k/sqlite | series1 | 21662 | 21770 | 21736 | +0.342 | 22572 |
| compare/insert_bulk_10k/sqlite | series2 | 21728 | 21744 | 21768 | +0.184 | 21860 |
| compare/durable_insert_1k/lin | series1 | 21528 | 20544 | 20546 | -4.562 | 22340 |
| compare/durable_insert_1k/lin | series2 | 20674 | 20458 | 20648 | -0.126 | 22600 |
| compare/durable_insert_1k/sqlite | series1 | 13094 | 13056 | 13200 | +0.810 | 13324 |
| compare/durable_insert_1k/sqlite | series2 | 13144 | 13024 | 13032 | -0.852 | 13240 |
| compare/durable_insert_10k/lin | series1 | 87600 | 87748 | 87888 | +0.329 | 89084 |
| compare/durable_insert_10k/lin | series2 | 87606 | 88594 | 87998 | +0.447 | 89900 |
| compare/durable_insert_10k/sqlite | series1 | 22388 | 22478 | 22562 | +0.777 | 23008 |
| compare/durable_insert_10k/sqlite | series2 | 22506 | 22516 | 22782 | +1.226 | 23092 |
| compare/filter_eq/lin | series1 | 73580 | 73720 | 73676 | +0.130 | 73908 |
| compare/filter_eq/lin | series2 | 73632 | 73548 | 73630 | -0.003 | 73776 |
| compare/filter_eq/sqlite | series1 | 15868 | 15924 | 15840 | -0.176 | 15964 |
| compare/filter_eq/sqlite | series2 | 15832 | 15868 | 15964 | +0.834 | 16032 |
| compare/filter_eq/duckdb | series1 | 46712 | 46418 | 46466 | -0.527 | 46652 |
| compare/filter_eq/duckdb | series2 | 46540 | 46634 | 46778 | +0.511 | 47016 |
| compare/filter_range/lin | series1 | 73718 | 73624 | 73584 | -0.182 | 73720 |
| compare/filter_range/lin | series2 | 73638 | 73720 | 73624 | -0.019 | 73724 |
| compare/filter_range/sqlite | series1 | 15932 | 15900 | 15968 | +0.226 | 15968 |
| compare/filter_range/sqlite | series2 | 15932 | 15786 | 15836 | -0.603 | 15968 |
| compare/filter_range/duckdb | series1 | 46308 | 46330 | 46872 | +1.218 | 46992 |
| compare/filter_range/duckdb | series2 | 46430 | 46478 | 46410 | -0.043 | 46868 |
| compare/agg_count_by/lin | series1 | 73608 | 73632 | 73610 | +0.003 | 73800 |
| compare/agg_count_by/lin | series2 | 73604 | 73624 | 73604 | +0.000 | 73904 |
| compare/agg_count_by/sqlite | series1 | 15932 | 15832 | 15872 | -0.377 | 15968 |
| compare/agg_count_by/sqlite | series2 | 15900 | 15932 | 15776 | -0.780 | 15968 |
| compare/agg_count_by/duckdb | series1 | 46858 | 47046 | 47070 | +0.452 | 47276 |
| compare/agg_count_by/duckdb | series2 | 47042 | 46796 | 47266 | +0.476 | 47668 |

PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas are outside this run. RSS records cover the full process lifecycle and do not establish a bound on retained index memory. No Row, BitSet, posting or Vec representation change. Four Store wrappers borrow stored rows through disjoint fields; allocation reductions are recorded in a separate counter experiment, not inferred from RSS. No production source is accepted by this report alone.
