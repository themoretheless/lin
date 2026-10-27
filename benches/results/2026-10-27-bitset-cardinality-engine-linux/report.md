# Exact-cardinality full-engine qualification

Source gate: **FAIL**. 1332 processes, 10656 wall observations. Candidate SQLite wins: 45/204.

Source commit: `613dd7c823e49d2e9ef0537cc674665a79aeb09e`. Baseline SHA256: `47983101d0c75c6cb40d0817ae1040626ca8e72c1eaca1336d6430e36e2c6051`. Candidate SHA256: `b273cd1eacb2166ddb7b27d18a183df214e3394d13b050003d9295bbf0a8cc8e`.

Baseline/control share the production-source executable. The candidate changes only private BitSet cardinality and union bookkeeping. Both binaries use identical benchmark preflight, full Row output, indexes/FTS/embedding and durability contracts. Allocator/runtime settings are unchanged. All-eight-peer superiority is unproven.

Gain is positive when candidate source is faster than baseline source. A/A is the median absolute baseline/control difference. Each series contains six process pairs; every control remains in the table.

| Case | Series | Gain % | A/A % | Positive /6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | +0.876 | 3.303 | 4 | PASS |
| compare/update_1row_1k/lin | series2 | +3.172 | 3.449 | 6 | PASS |
| compare/update_1row_1k/sqlite | series1 | -1.941 | 5.095 | 2 | PASS |
| compare/update_1row_1k/sqlite | series2 | -3.271 | 10.831 | 2 | PASS |
| compare/delete_1row_1k/lin | series1 | +1.123 | 2.716 | 4 | PASS |
| compare/delete_1row_1k/lin | series2 | +3.897 | 1.238 | 5 | PASS |
| compare/delete_1row_1k/sqlite | series1 | -2.948 | 1.670 | 1 | FAIL |
| compare/delete_1row_1k/sqlite | series2 | +1.252 | 4.007 | 3 | PASS |
| compare/update_1row_10k/lin | series1 | +3.687 | 2.962 | 5 | PASS |
| compare/update_1row_10k/lin | series2 | +3.251 | 1.466 | 5 | PASS |
| compare/update_1row_10k/sqlite | series1 | +0.478 | 5.836 | 3 | PASS |
| compare/update_1row_10k/sqlite | series2 | -0.134 | 2.589 | 3 | PASS |
| compare/delete_1row_10k/lin | series1 | +0.561 | 1.196 | 3 | PASS |
| compare/delete_1row_10k/lin | series2 | -1.349 | 4.185 | 1 | PASS |
| compare/delete_1row_10k/sqlite | series1 | +4.177 | 6.936 | 5 | PASS |
| compare/delete_1row_10k/sqlite | series2 | -2.746 | 2.143 | 2 | FAIL |
| compare/update_1row_100k/lin | series1 | +7.286 | 3.686 | 6 | PASS |
| compare/update_1row_100k/lin | series2 | +3.926 | 6.351 | 6 | PASS |
| compare/update_1row_100k/sqlite | series1 | +3.559 | 6.405 | 4 | PASS |
| compare/update_1row_100k/sqlite | series2 | +2.612 | 3.256 | 4 | PASS |
| compare/delete_1row_100k/lin | series1 | +6.461 | 2.056 | 6 | PASS |
| compare/delete_1row_100k/lin | series2 | +8.647 | 7.298 | 5 | PASS |
| compare/delete_1row_100k/sqlite | series1 | +2.083 | 2.833 | 3 | PASS |
| compare/delete_1row_100k/sqlite | series2 | +2.039 | 5.310 | 4 | PASS |
| compare/tx_commit_1row_1k/lin | series1 | -0.650 | 1.932 | 2 | PASS |
| compare/tx_commit_1row_1k/lin | series2 | +4.798 | 9.253 | 5 | PASS |
| compare/tx_commit_1row_1k/sqlite | series1 | -0.055 | 4.901 | 3 | PASS |
| compare/tx_commit_1row_1k/sqlite | series2 | -3.172 | 5.803 | 2 | PASS |
| compare/tx_rollback_1row_1k/lin | series1 | +0.177 | 4.665 | 4 | PASS |
| compare/tx_rollback_1row_1k/lin | series2 | +4.859 | 1.700 | 4 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series1 | -4.617 | 2.544 | 1 | FAIL |
| compare/tx_rollback_1row_1k/sqlite | series2 | -9.650 | 10.301 | 1 | PASS |
| compare/tx_commit_1row_10k/lin | series1 | +4.658 | 2.336 | 5 | PASS |
| compare/tx_commit_1row_10k/lin | series2 | -2.491 | 4.222 | 2 | PASS |
| compare/tx_commit_1row_10k/sqlite | series1 | +1.458 | 3.982 | 4 | PASS |
| compare/tx_commit_1row_10k/sqlite | series2 | +6.476 | 1.789 | 6 | PASS |
| compare/tx_rollback_1row_10k/lin | series1 | -1.458 | 1.989 | 2 | PASS |
| compare/tx_rollback_1row_10k/lin | series2 | +1.815 | 3.849 | 4 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series1 | -0.338 | 2.682 | 3 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series2 | -1.374 | 5.376 | 2 | PASS |
| compare/insert_bulk_1k/lin | series1 | -8.269 | 10.171 | 0 | PASS |
| compare/insert_bulk_1k/lin | series2 | +5.035 | 7.517 | 5 | PASS |
| compare/insert_bulk_1k/sqlite | series1 | +1.121 | 1.015 | 4 | PASS |
| compare/insert_bulk_1k/sqlite | series2 | -0.533 | 1.274 | 2 | PASS |
| compare/insert_bulk_10k/lin | series1 | +44.487 | 1.732 | 6 | PASS |
| compare/insert_bulk_10k/lin | series2 | +33.901 | 5.233 | 6 | PASS |
| compare/insert_bulk_10k/sqlite | series1 | -1.542 | 0.096 | 1 | PASS |
| compare/insert_bulk_10k/sqlite | series2 | +1.233 | 2.100 | 4 | PASS |
| compare/durable_insert_1k/lin | series1 | -0.530 | 1.697 | 3 | PASS |
| compare/durable_insert_1k/lin | series2 | -9.253 | 3.931 | 1 | FAIL |
| compare/durable_insert_1k/sqlite | series1 | -0.237 | 1.848 | 3 | PASS |
| compare/durable_insert_1k/sqlite | series2 | -2.620 | 4.860 | 2 | PASS |
| compare/durable_insert_10k/lin | series1 | +1.938 | 3.003 | 4 | PASS |
| compare/durable_insert_10k/lin | series2 | -1.958 | 9.154 | 1 | PASS |
| compare/durable_insert_10k/sqlite | series1 | -0.396 | 1.557 | 2 | PASS |
| compare/durable_insert_10k/sqlite | series2 | +0.648 | 2.906 | 4 | PASS |
| compare/filter_eq/lin | series1 | +11.452 | 0.393 | 6 | PASS |
| compare/filter_eq/lin | series2 | +8.746 | 5.572 | 6 | PASS |
| compare/filter_eq/sqlite | series1 | +0.311 | 2.926 | 4 | PASS |
| compare/filter_eq/sqlite | series2 | +2.536 | 12.166 | 4 | PASS |
| compare/filter_eq/duckdb | series1 | -1.752 | 7.694 | 2 | PASS |
| compare/filter_eq/duckdb | series2 | +9.815 | 7.145 | 4 | PASS |
| compare/filter_range/lin | series1 | +8.100 | 1.684 | 6 | PASS |
| compare/filter_range/lin | series2 | +11.479 | 6.861 | 5 | FAIL |
| compare/filter_range/sqlite | series1 | +0.287 | 1.143 | 3 | PASS |
| compare/filter_range/sqlite | series2 | +1.038 | 8.950 | 3 | PASS |
| compare/filter_range/duckdb | series1 | +0.259 | 2.044 | 3 | PASS |
| compare/filter_range/duckdb | series2 | +4.602 | 5.720 | 4 | PASS |
| compare/agg_count_by/lin | series1 | -1.491 | 6.622 | 2 | PASS |
| compare/agg_count_by/lin | series2 | +5.682 | 9.410 | 4 | PASS |
| compare/agg_count_by/sqlite | series1 | +5.052 | 5.264 | 5 | PASS |
| compare/agg_count_by/sqlite | series2 | +14.266 | 5.947 | 5 | PASS |
| compare/agg_count_by/duckdb | series1 | +2.602 | 6.858 | 4 | PASS |
| compare/agg_count_by/duckdb | series2 | +2.500 | 14.702 | 4 | PASS |

## SQLite gap in candidate-source processes

Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.

| Lin case | Series | Lin / SQLite | Wins /6 |
|---|---|---:|---:|
| compare/update_1row_1k/lin | series1 | 2.1932 | 0 |
| compare/update_1row_1k/lin | series2 | 2.7039 | 0 |
| compare/delete_1row_1k/lin | series1 | 2.6959 | 0 |
| compare/delete_1row_1k/lin | series2 | 3.6621 | 0 |
| compare/update_1row_10k/lin | series1 | 2.2510 | 0 |
| compare/update_1row_10k/lin | series2 | 1.8098 | 0 |
| compare/delete_1row_10k/lin | series1 | 2.6368 | 0 |
| compare/delete_1row_10k/lin | series2 | 2.2010 | 0 |
| compare/update_1row_100k/lin | series1 | 1.7866 | 0 |
| compare/update_1row_100k/lin | series2 | 1.3652 | 0 |
| compare/delete_1row_100k/lin | series1 | 2.1357 | 0 |
| compare/delete_1row_100k/lin | series2 | 1.6547 | 0 |
| compare/tx_commit_1row_1k/lin | series1 | 1.7035 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 2.0931 | 0 |
| compare/tx_rollback_1row_1k/lin | series1 | 2.0988 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 2.6614 | 0 |
| compare/tx_commit_1row_10k/lin | series1 | 1.7689 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 1.5613 | 0 |
| compare/tx_rollback_1row_10k/lin | series1 | 2.0903 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 1.7958 | 0 |
| compare/insert_bulk_1k/lin | series1 | 1.2112 | 0 |
| compare/insert_bulk_1k/lin | series2 | 1.4908 | 0 |
| compare/insert_bulk_10k/lin | series1 | 1.1079 | 1 |
| compare/insert_bulk_10k/lin | series2 | 1.6596 | 0 |
| compare/durable_insert_1k/lin | series1 | 0.8825 | 6 |
| compare/durable_insert_1k/lin | series2 | 1.1063 | 2 |
| compare/durable_insert_10k/lin | series1 | 1.9353 | 0 |
| compare/durable_insert_10k/lin | series2 | 2.1434 | 0 |
| compare/filter_eq/lin | series1 | 0.0085 | 6 |
| compare/filter_eq/lin | series2 | 0.0084 | 6 |
| compare/filter_range/lin | series1 | 0.0131 | 6 |
| compare/filter_range/lin | series2 | 0.0129 | 6 |
| compare/agg_count_by/lin | series1 | 0.2867 | 6 |
| compare/agg_count_by/lin | series2 | 0.2987 | 6 |

## DuckDB read controls and peer gap

| Lin case | Series | Lin / DuckDB | Wins /6 |
|---|---|---:|---:|
| compare/filter_eq/lin | series1 | 0.0049 | 6 |
| compare/filter_eq/lin | series2 | 0.0049 | 6 |
| compare/filter_range/lin | series1 | 0.0042 | 6 |
| compare/filter_range/lin | series2 | 0.0041 | 6 |
| compare/agg_count_by/lin | series1 | 0.2079 | 6 |
| compare/agg_count_by/lin | series2 | 0.2035 | 6 |

Candidate read DuckDB wins: 36/36.


## Whole-process peak RSS

KiB from GNU time for the complete fixture lifecycle. The delta compares medians; the maximum preserves isolated high-water observations. These are process peaks, not retained-cache measurements or per-operation allocation counts.

| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |
|---|---|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 18804 | 18790 | 18950 | +0.776 | 19156 |
| compare/update_1row_1k/lin | series2 | 19018 | 18768 | 18956 | -0.326 | 19148 |
| compare/update_1row_1k/sqlite | series1 | 12824 | 12682 | 12824 | +0.000 | 12836 |
| compare/update_1row_1k/sqlite | series2 | 12712 | 12770 | 12640 | -0.566 | 12828 |
| compare/delete_1row_1k/lin | series1 | 18894 | 18940 | 19086 | +1.016 | 19164 |
| compare/delete_1row_1k/lin | series2 | 19024 | 18852 | 18924 | -0.526 | 19024 |
| compare/delete_1row_1k/sqlite | series1 | 12780 | 12670 | 12666 | -0.892 | 12828 |
| compare/delete_1row_1k/sqlite | series2 | 12714 | 12698 | 12818 | +0.818 | 12828 |
| compare/update_1row_10k/lin | series1 | 72124 | 72072 | 72044 | -0.111 | 72424 |
| compare/update_1row_10k/lin | series2 | 72110 | 72054 | 72114 | +0.006 | 72156 |
| compare/update_1row_10k/sqlite | series1 | 16454 | 16616 | 16626 | +1.045 | 16676 |
| compare/update_1row_10k/sqlite | series2 | 16592 | 16562 | 16528 | -0.386 | 16808 |
| compare/delete_1row_10k/lin | series1 | 72138 | 72142 | 72198 | +0.083 | 72288 |
| compare/delete_1row_10k/lin | series2 | 72204 | 72122 | 72162 | -0.058 | 72364 |
| compare/delete_1row_10k/sqlite | series1 | 16560 | 16604 | 16584 | +0.145 | 16648 |
| compare/delete_1row_10k/sqlite | series2 | 16608 | 16616 | 16566 | -0.253 | 16664 |
| compare/update_1row_100k/lin | series1 | 590010 | 590196 | 590162 | +0.026 | 590412 |
| compare/update_1row_100k/lin | series2 | 589884 | 590056 | 590058 | +0.029 | 590188 |
| compare/update_1row_100k/sqlite | series1 | 55720 | 55792 | 55798 | +0.140 | 55848 |
| compare/update_1row_100k/sqlite | series2 | 55632 | 55708 | 55742 | +0.198 | 55824 |
| compare/delete_1row_100k/lin | series1 | 590070 | 590054 | 590024 | -0.008 | 590284 |
| compare/delete_1row_100k/lin | series2 | 590050 | 590076 | 589964 | -0.015 | 590100 |
| compare/delete_1row_100k/sqlite | series1 | 55688 | 55758 | 55666 | -0.040 | 55820 |
| compare/delete_1row_100k/sqlite | series2 | 55810 | 55784 | 55736 | -0.133 | 55816 |
| compare/tx_commit_1row_1k/lin | series1 | 18742 | 19020 | 18934 | +1.024 | 19028 |
| compare/tx_commit_1row_1k/lin | series2 | 18922 | 19030 | 18934 | +0.063 | 19024 |
| compare/tx_commit_1row_1k/sqlite | series1 | 12786 | 12752 | 12742 | -0.344 | 12824 |
| compare/tx_commit_1row_1k/sqlite | series2 | 12820 | 12798 | 12662 | -1.232 | 12824 |
| compare/tx_rollback_1row_1k/lin | series1 | 19006 | 18798 | 18960 | -0.242 | 19152 |
| compare/tx_rollback_1row_1k/lin | series2 | 18918 | 19018 | 19018 | +0.529 | 19152 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 12790 | 12736 | 12786 | -0.031 | 12956 |
| compare/tx_rollback_1row_1k/sqlite | series2 | 12760 | 12790 | 12682 | -0.611 | 12844 |
| compare/tx_commit_1row_10k/lin | series1 | 72186 | 72184 | 72242 | +0.078 | 72460 |
| compare/tx_commit_1row_10k/lin | series2 | 72156 | 72184 | 72114 | -0.058 | 72328 |
| compare/tx_commit_1row_10k/sqlite | series1 | 16496 | 16578 | 16596 | +0.606 | 16640 |
| compare/tx_commit_1row_10k/sqlite | series2 | 16542 | 16598 | 16572 | +0.181 | 16768 |
| compare/tx_rollback_1row_10k/lin | series1 | 72190 | 72160 | 72198 | +0.011 | 72208 |
| compare/tx_rollback_1row_10k/lin | series2 | 72210 | 72130 | 72168 | -0.058 | 72328 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 16610 | 16514 | 16566 | -0.265 | 16664 |
| compare/tx_rollback_1row_10k/sqlite | series2 | 16544 | 16538 | 16460 | -0.508 | 16792 |
| compare/insert_bulk_1k/lin | series1 | 19336 | 19166 | 19126 | -1.086 | 19236 |
| compare/insert_bulk_1k/lin | series2 | 19294 | 19106 | 19030 | -1.368 | 19232 |
| compare/insert_bulk_1k/sqlite | series1 | 13246 | 13318 | 13242 | -0.030 | 13532 |
| compare/insert_bulk_1k/sqlite | series2 | 13306 | 13342 | 13354 | +0.361 | 13532 |
| compare/insert_bulk_10k/lin | series1 | 78256 | 78232 | 78338 | +0.105 | 78592 |
| compare/insert_bulk_10k/lin | series2 | 78190 | 78290 | 78376 | +0.238 | 78468 |
| compare/insert_bulk_10k/sqlite | series1 | 21720 | 21792 | 21734 | +0.064 | 22480 |
| compare/insert_bulk_10k/sqlite | series2 | 21694 | 21720 | 21730 | +0.166 | 22448 |
| compare/durable_insert_1k/lin | series1 | 21662 | 20492 | 20764 | -4.146 | 22600 |
| compare/durable_insert_1k/lin | series2 | 21418 | 20840 | 20716 | -3.278 | 22456 |
| compare/durable_insert_1k/sqlite | series1 | 12952 | 13020 | 13040 | +0.679 | 13288 |
| compare/durable_insert_1k/sqlite | series2 | 12992 | 13032 | 13058 | +0.508 | 13212 |
| compare/durable_insert_10k/lin | series1 | 87564 | 87278 | 86980 | -0.667 | 87488 |
| compare/durable_insert_10k/lin | series2 | 87288 | 87666 | 87788 | +0.573 | 89200 |
| compare/durable_insert_10k/sqlite | series1 | 22408 | 22538 | 22606 | +0.884 | 22672 |
| compare/durable_insert_10k/sqlite | series2 | 22670 | 22504 | 22570 | -0.441 | 22676 |
| compare/filter_eq/lin | series1 | 73644 | 73552 | 73768 | +0.168 | 73980 |
| compare/filter_eq/lin | series2 | 73622 | 73616 | 73580 | -0.057 | 73980 |
| compare/filter_eq/sqlite | series1 | 15866 | 15868 | 15868 | +0.013 | 15964 |
| compare/filter_eq/sqlite | series2 | 15956 | 15832 | 15836 | -0.752 | 15900 |
| compare/filter_eq/duckdb | series1 | 46888 | 47004 | 47066 | +0.380 | 47640 |
| compare/filter_eq/duckdb | series2 | 47104 | 47000 | 46906 | -0.420 | 47508 |
| compare/filter_range/lin | series1 | 73676 | 73658 | 73558 | -0.160 | 73628 |
| compare/filter_range/lin | series2 | 73388 | 73490 | 73600 | +0.289 | 73980 |
| compare/filter_range/sqlite | series1 | 15772 | 15866 | 15900 | +0.812 | 15964 |
| compare/filter_range/sqlite | series2 | 15908 | 15932 | 15836 | -0.453 | 15964 |
| compare/filter_range/duckdb | series1 | 47164 | 47298 | 46998 | -0.352 | 47776 |
| compare/filter_range/duckdb | series2 | 47532 | 47116 | 47080 | -0.951 | 47556 |
| compare/agg_count_by/lin | series1 | 73744 | 73724 | 73602 | -0.193 | 73848 |
| compare/agg_count_by/lin | series2 | 73724 | 73660 | 73680 | -0.060 | 73980 |
| compare/agg_count_by/sqlite | series1 | 15900 | 15964 | 15866 | -0.214 | 15964 |
| compare/agg_count_by/sqlite | series2 | 15864 | 15848 | 15742 | -0.769 | 15836 |
| compare/agg_count_by/duckdb | series1 | 47168 | 47580 | 47592 | +0.899 | 48312 |
| compare/agg_count_by/duckdb | series2 | 47690 | 47066 | 47700 | +0.021 | 48236 |

PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas are outside this run. RSS records cover the full process lifecycle and do not establish a bound on retained index memory. BitSet/posting type sizes grow 32 to 40 bytes in the component model; lifecycle RSS must be assessed separately. No production source is accepted by this report alone.
