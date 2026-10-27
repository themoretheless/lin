# Frozen COW candidate: expanded full-engine source qualification

Source gate: **FAIL**. 1332 processes, 10656 wall observations. Candidate SQLite wins: 122/204.

Source commit: `fdc4a0ee2bb5820b417cd9661af70f2315e776cb`. Baseline SHA256: `47983101d0c75c6cb40d0817ae1040626ca8e72c1eaca1336d6430e36e2c6051`. Candidate SHA256: `5a9c33567a2ee6443fe0af4e5fb42501e6416142817840279bfdb973882828bc`.

Baseline/control share production047 Rust source. Candidate is the exact saved COW/in-place/Eq-Hash/bounded-bitmap-prefix prototype134417eb, previously qualified only for native mutations. Its public Row newtype is source-incompatible; this expanded benchmark is not approval to change the production API. Retain full value output, index/FTS/embedding/undo/Pack/durability work and unchanged allocator/runtime.

Gain is positive when candidate source is faster than baseline source. A/A is the median absolute baseline/control difference. Each series contains six process pairs; every control remains in the table.

| Case | Series | Gain % | A/A % | Positive /6 | Gate |
|---|---|---:|---:|---:|---|
| compare/update_1row_1k/lin | series1 | +60.610 | 2.281 | 6 | PASS |
| compare/update_1row_1k/lin | series2 | +60.377 | 0.899 | 6 | PASS |
| compare/update_1row_1k/sqlite | series1 | +2.122 | 1.053 | 6 | PASS |
| compare/update_1row_1k/sqlite | series2 | +1.133 | 0.770 | 3 | PASS |
| compare/delete_1row_1k/lin | series1 | +9.829 | 0.885 | 6 | PASS |
| compare/delete_1row_1k/lin | series2 | +9.069 | 1.623 | 6 | PASS |
| compare/delete_1row_1k/sqlite | series1 | -0.444 | 1.380 | 1 | PASS |
| compare/delete_1row_1k/sqlite | series2 | -2.039 | 2.073 | 1 | PASS |
| compare/update_1row_10k/lin | series1 | +59.560 | 1.849 | 6 | PASS |
| compare/update_1row_10k/lin | series2 | +60.056 | 1.549 | 6 | PASS |
| compare/update_1row_10k/sqlite | series1 | +1.327 | 1.808 | 4 | PASS |
| compare/update_1row_10k/sqlite | series2 | +0.183 | 2.953 | 3 | PASS |
| compare/delete_1row_10k/lin | series1 | +4.270 | 2.857 | 5 | PASS |
| compare/delete_1row_10k/lin | series2 | +5.965 | 1.411 | 5 | PASS |
| compare/delete_1row_10k/sqlite | series1 | +0.964 | 4.080 | 4 | PASS |
| compare/delete_1row_10k/sqlite | series2 | -0.539 | 3.185 | 1 | PASS |
| compare/update_1row_100k/lin | series1 | +63.227 | 2.543 | 6 | PASS |
| compare/update_1row_100k/lin | series2 | +63.097 | 5.732 | 6 | PASS |
| compare/update_1row_100k/sqlite | series1 | +3.308 | 6.795 | 3 | PASS |
| compare/update_1row_100k/sqlite | series2 | -0.181 | 7.627 | 3 | PASS |
| compare/delete_1row_100k/lin | series1 | +6.466 | 4.648 | 4 | PASS |
| compare/delete_1row_100k/lin | series2 | +10.351 | 7.726 | 4 | PASS |
| compare/delete_1row_100k/sqlite | series1 | +8.076 | 12.275 | 5 | PASS |
| compare/delete_1row_100k/sqlite | series2 | -0.744 | 6.650 | 2 | PASS |
| compare/tx_commit_1row_1k/lin | series1 | +59.672 | 1.762 | 6 | PASS |
| compare/tx_commit_1row_1k/lin | series2 | +60.129 | 1.928 | 6 | PASS |
| compare/tx_commit_1row_1k/sqlite | series1 | -0.630 | 2.563 | 3 | PASS |
| compare/tx_commit_1row_1k/sqlite | series2 | -0.150 | 2.174 | 3 | PASS |
| compare/tx_rollback_1row_1k/lin | series1 | +46.718 | 0.632 | 6 | PASS |
| compare/tx_rollback_1row_1k/lin | series2 | +45.454 | 1.636 | 6 | PASS |
| compare/tx_rollback_1row_1k/sqlite | series1 | -2.052 | 1.587 | 1 | FAIL |
| compare/tx_rollback_1row_1k/sqlite | series2 | -1.918 | 1.204 | 1 | PASS |
| compare/tx_commit_1row_10k/lin | series1 | +57.278 | 1.134 | 6 | PASS |
| compare/tx_commit_1row_10k/lin | series2 | +58.599 | 2.578 | 6 | PASS |
| compare/tx_commit_1row_10k/sqlite | series1 | +1.131 | 1.063 | 4 | PASS |
| compare/tx_commit_1row_10k/sqlite | series2 | -0.288 | 3.107 | 1 | PASS |
| compare/tx_rollback_1row_10k/lin | series1 | +43.447 | 4.031 | 6 | PASS |
| compare/tx_rollback_1row_10k/lin | series2 | +44.661 | 0.331 | 6 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series1 | +0.500 | 2.662 | 4 | PASS |
| compare/tx_rollback_1row_10k/sqlite | series2 | -2.005 | 0.921 | 1 | FAIL |
| compare/insert_bulk_1k/lin | series1 | -0.003 | 0.728 | 3 | PASS |
| compare/insert_bulk_1k/lin | series2 | +2.489 | 0.787 | 6 | PASS |
| compare/insert_bulk_1k/sqlite | series1 | +1.359 | 0.730 | 4 | PASS |
| compare/insert_bulk_1k/sqlite | series2 | +1.684 | 1.173 | 6 | PASS |
| compare/insert_bulk_10k/lin | series1 | -16.332 | 76.151 | 0 | PASS |
| compare/insert_bulk_10k/lin | series2 | -13.177 | 80.856 | 0 | PASS |
| compare/insert_bulk_10k/sqlite | series1 | +0.646 | 0.246 | 5 | PASS |
| compare/insert_bulk_10k/sqlite | series2 | +0.189 | 0.840 | 3 | PASS |
| compare/durable_insert_1k/lin | series1 | -0.928 | 2.041 | 3 | PASS |
| compare/durable_insert_1k/lin | series2 | -5.093 | 1.605 | 0 | FAIL |
| compare/durable_insert_1k/sqlite | series1 | -1.275 | 1.010 | 2 | PASS |
| compare/durable_insert_1k/sqlite | series2 | +1.781 | 2.069 | 5 | PASS |
| compare/durable_insert_10k/lin | series1 | -0.516 | 0.848 | 2 | PASS |
| compare/durable_insert_10k/lin | series2 | -1.775 | 1.023 | 1 | PASS |
| compare/durable_insert_10k/sqlite | series1 | -0.766 | 0.635 | 0 | PASS |
| compare/durable_insert_10k/sqlite | series2 | +0.265 | 1.008 | 3 | PASS |
| compare/filter_eq/lin | series1 | -0.932 | 0.904 | 2 | PASS |
| compare/filter_eq/lin | series2 | -4.675 | 2.282 | 2 | FAIL |
| compare/filter_eq/sqlite | series1 | +0.084 | 0.527 | 3 | PASS |
| compare/filter_eq/sqlite | series2 | +0.546 | 0.243 | 5 | PASS |
| compare/filter_eq/duckdb | series1 | -9.700 | 4.377 | 1 | FAIL |
| compare/filter_eq/duckdb | series2 | -6.348 | 4.173 | 1 | FAIL |
| compare/filter_range/lin | series1 | -1.614 | 1.668 | 2 | PASS |
| compare/filter_range/lin | series2 | +1.412 | 2.565 | 4 | PASS |
| compare/filter_range/sqlite | series1 | +0.238 | 0.590 | 3 | PASS |
| compare/filter_range/sqlite | series2 | +0.436 | 0.575 | 4 | PASS |
| compare/filter_range/duckdb | series1 | +2.024 | 1.493 | 4 | PASS |
| compare/filter_range/duckdb | series2 | +0.632 | 4.670 | 4 | PASS |
| compare/agg_count_by/lin | series1 | -0.688 | 1.069 | 3 | PASS |
| compare/agg_count_by/lin | series2 | -0.212 | 1.915 | 2 | PASS |
| compare/agg_count_by/sqlite | series1 | +0.106 | 0.328 | 4 | PASS |
| compare/agg_count_by/sqlite | series2 | -0.549 | 1.137 | 3 | PASS |
| compare/agg_count_by/duckdb | series1 | +0.425 | 6.326 | 3 | PASS |
| compare/agg_count_by/duckdb | series2 | +5.133 | 5.964 | 4 | PASS |

## SQLite gap in candidate-source processes

Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.

| Lin case | Series | Lin / SQLite | Wins /6 |
|---|---|---:|---:|
| compare/update_1row_1k/lin | series1 | 0.7040 | 6 |
| compare/update_1row_1k/lin | series2 | 0.7073 | 6 |
| compare/delete_1row_1k/lin | series1 | 1.9419 | 0 |
| compare/delete_1row_1k/lin | series2 | 1.9459 | 0 |
| compare/update_1row_10k/lin | series1 | 0.9663 | 4 |
| compare/update_1row_10k/lin | series2 | 0.9827 | 4 |
| compare/delete_1row_10k/lin | series1 | 2.8810 | 0 |
| compare/delete_1row_10k/lin | series2 | 2.8793 | 0 |
| compare/update_1row_100k/lin | series1 | 0.7910 | 6 |
| compare/update_1row_100k/lin | series2 | 0.7814 | 5 |
| compare/delete_1row_100k/lin | series1 | 2.6633 | 0 |
| compare/delete_1row_100k/lin | series2 | 2.2995 | 0 |
| compare/tx_commit_1row_1k/lin | series1 | 0.5639 | 6 |
| compare/tx_commit_1row_1k/lin | series2 | 0.5682 | 6 |
| compare/tx_rollback_1row_1k/lin | series1 | 1.0041 | 3 |
| compare/tx_rollback_1row_1k/lin | series2 | 0.9903 | 4 |
| compare/tx_commit_1row_10k/lin | series1 | 0.8464 | 6 |
| compare/tx_commit_1row_10k/lin | series2 | 0.8314 | 6 |
| compare/tx_rollback_1row_10k/lin | series1 | 1.3860 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 1.3571 | 0 |
| compare/insert_bulk_1k/lin | series1 | 1.1605 | 0 |
| compare/insert_bulk_1k/lin | series2 | 1.1545 | 0 |
| compare/insert_bulk_10k/lin | series1 | 1.0698 | 0 |
| compare/insert_bulk_10k/lin | series2 | 1.0413 | 0 |
| compare/durable_insert_1k/lin | series1 | 0.8710 | 6 |
| compare/durable_insert_1k/lin | series2 | 0.9048 | 6 |
| compare/durable_insert_10k/lin | series1 | 0.9143 | 6 |
| compare/durable_insert_10k/lin | series2 | 0.9372 | 6 |
| compare/filter_eq/lin | series1 | 0.0105 | 6 |
| compare/filter_eq/lin | series2 | 0.0107 | 6 |
| compare/filter_range/lin | series1 | 0.0155 | 6 |
| compare/filter_range/lin | series2 | 0.0151 | 6 |
| compare/agg_count_by/lin | series1 | 0.3856 | 6 |
| compare/agg_count_by/lin | series2 | 0.3795 | 6 |

## DuckDB read controls and peer gap

| Lin case | Series | Lin / DuckDB | Wins /6 |
|---|---|---:|---:|
| compare/filter_eq/lin | series1 | 0.0054 | 6 |
| compare/filter_eq/lin | series2 | 0.0054 | 6 |
| compare/filter_range/lin | series1 | 0.0050 | 6 |
| compare/filter_range/lin | series2 | 0.0049 | 6 |
| compare/agg_count_by/lin | series1 | 0.2658 | 6 |
| compare/agg_count_by/lin | series2 | 0.2702 | 6 |

Candidate read DuckDB wins: 36/36.


## Whole-process peak RSS

KiB from GNU time for the complete fixture lifecycle. The delta compares medians; the maximum preserves isolated high-water observations. These are process peaks, not retained-cache measurements or per-operation allocation counts.

| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |
|---|---|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 18700 | 18714 | 18884 | +0.984 | 18908 |
| compare/update_1row_1k/lin | series2 | 18830 | 18646 | 18868 | +0.202 | 18904 |
| compare/update_1row_1k/sqlite | series1 | 12532 | 12624 | 12668 | +1.085 | 12800 |
| compare/update_1row_1k/sqlite | series2 | 12632 | 12614 | 12778 | +1.156 | 12904 |
| compare/delete_1row_1k/lin | series1 | 18766 | 18790 | 18874 | +0.576 | 18900 |
| compare/delete_1row_1k/lin | series2 | 18612 | 18680 | 18804 | +1.032 | 18896 |
| compare/delete_1row_1k/sqlite | series1 | 12660 | 12636 | 12710 | +0.395 | 12940 |
| compare/delete_1row_1k/sqlite | series2 | 12626 | 12656 | 12696 | +0.554 | 12940 |
| compare/update_1row_10k/lin | series1 | 71962 | 71986 | 72242 | +0.389 | 72404 |
| compare/update_1row_10k/lin | series2 | 71922 | 71926 | 72284 | +0.503 | 72416 |
| compare/update_1row_10k/sqlite | series1 | 16454 | 16484 | 16492 | +0.231 | 16740 |
| compare/update_1row_10k/sqlite | series2 | 16450 | 16432 | 16482 | +0.195 | 16584 |
| compare/delete_1row_10k/lin | series1 | 71950 | 71972 | 72118 | +0.233 | 72324 |
| compare/delete_1row_10k/lin | series2 | 71884 | 71930 | 72204 | +0.445 | 72308 |
| compare/delete_1row_10k/sqlite | series1 | 16418 | 16450 | 16508 | +0.548 | 16608 |
| compare/delete_1row_10k/sqlite | series2 | 16452 | 16416 | 16526 | +0.450 | 16644 |
| compare/update_1row_100k/lin | series1 | 589870 | 589896 | 592582 | +0.460 | 592748 |
| compare/update_1row_100k/lin | series2 | 589952 | 590020 | 592606 | +0.450 | 592688 |
| compare/update_1row_100k/sqlite | series1 | 55622 | 55618 | 55698 | +0.137 | 55752 |
| compare/update_1row_100k/sqlite | series2 | 55568 | 55620 | 55700 | +0.238 | 55764 |
| compare/delete_1row_100k/lin | series1 | 589960 | 589980 | 592666 | +0.459 | 592764 |
| compare/delete_1row_100k/lin | series2 | 589932 | 589954 | 592628 | +0.457 | 592768 |
| compare/delete_1row_100k/sqlite | series1 | 55596 | 55594 | 55694 | +0.176 | 55896 |
| compare/delete_1row_100k/sqlite | series2 | 55618 | 55666 | 55752 | +0.241 | 55776 |
| compare/tx_commit_1row_1k/lin | series1 | 18738 | 18832 | 18936 | +1.057 | 19024 |
| compare/tx_commit_1row_1k/lin | series2 | 18748 | 18830 | 18932 | +0.981 | 19028 |
| compare/tx_commit_1row_1k/sqlite | series1 | 12628 | 12620 | 12652 | +0.190 | 12764 |
| compare/tx_commit_1row_1k/sqlite | series2 | 12582 | 12668 | 12660 | +0.620 | 12760 |
| compare/tx_rollback_1row_1k/lin | series1 | 18824 | 18756 | 18968 | +0.765 | 19032 |
| compare/tx_rollback_1row_1k/lin | series2 | 18842 | 18838 | 18968 | +0.669 | 19016 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 12636 | 12634 | 12776 | +1.108 | 12908 |
| compare/tx_rollback_1row_1k/sqlite | series2 | 12648 | 12536 | 12748 | +0.791 | 12792 |
| compare/tx_commit_1row_10k/lin | series1 | 71934 | 71990 | 72422 | +0.678 | 72480 |
| compare/tx_commit_1row_10k/lin | series2 | 71994 | 72088 | 72300 | +0.425 | 72356 |
| compare/tx_commit_1row_10k/sqlite | series1 | 16412 | 16398 | 16522 | +0.670 | 16704 |
| compare/tx_commit_1row_10k/sqlite | series2 | 16418 | 16446 | 16610 | +1.169 | 16720 |
| compare/tx_rollback_1row_10k/lin | series1 | 71994 | 71992 | 72322 | +0.456 | 72416 |
| compare/tx_rollback_1row_10k/lin | series2 | 72124 | 72022 | 72262 | +0.191 | 72456 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 16414 | 16514 | 16608 | +1.182 | 16748 |
| compare/tx_rollback_1row_10k/sqlite | series2 | 16414 | 16454 | 16570 | +0.950 | 16752 |
| compare/insert_bulk_1k/lin | series1 | 18810 | 18752 | 19328 | +2.754 | 19476 |
| compare/insert_bulk_1k/lin | series2 | 18828 | 18860 | 19282 | +2.411 | 19400 |
| compare/insert_bulk_1k/sqlite | series1 | 13154 | 13102 | 13180 | +0.198 | 13368 |
| compare/insert_bulk_1k/sqlite | series2 | 13052 | 13186 | 13124 | +0.552 | 13172 |
| compare/insert_bulk_10k/lin | series1 | 78632 | 78148 | 78628 | -0.005 | 78660 |
| compare/insert_bulk_10k/lin | series2 | 78572 | 78058 | 78492 | -0.102 | 78664 |
| compare/insert_bulk_10k/sqlite | series1 | 21660 | 21606 | 21646 | -0.065 | 22516 |
| compare/insert_bulk_10k/sqlite | series2 | 21636 | 21542 | 21564 | -0.333 | 21628 |
| compare/durable_insert_1k/lin | series1 | 20562 | 20720 | 20638 | +0.370 | 22660 |
| compare/durable_insert_1k/lin | series2 | 21458 | 20540 | 20656 | -3.738 | 22708 |
| compare/durable_insert_1k/sqlite | series1 | 13078 | 13094 | 13214 | +1.040 | 13304 |
| compare/durable_insert_1k/sqlite | series2 | 13026 | 13102 | 13240 | +1.643 | 13356 |
| compare/durable_insert_10k/lin | series1 | 88388 | 89114 | 88938 | +0.622 | 90048 |
| compare/durable_insert_10k/lin | series2 | 88170 | 89058 | 89316 | +1.300 | 90028 |
| compare/durable_insert_10k/sqlite | series1 | 22470 | 22278 | 22452 | -0.080 | 22736 |
| compare/durable_insert_10k/sqlite | series2 | 22412 | 22480 | 22294 | -0.527 | 22508 |
| compare/filter_eq/lin | series1 | 73482 | 73538 | 74120 | +0.868 | 74196 |
| compare/filter_eq/lin | series2 | 73536 | 73600 | 74064 | +0.718 | 74160 |
| compare/filter_eq/sqlite | series1 | 15708 | 15740 | 15892 | +1.171 | 15928 |
| compare/filter_eq/sqlite | series2 | 15802 | 15832 | 15832 | +0.190 | 15928 |
| compare/filter_eq/duckdb | series1 | 47108 | 46982 | 47100 | -0.017 | 47376 |
| compare/filter_eq/duckdb | series2 | 47028 | 47206 | 46892 | -0.289 | 47332 |
| compare/filter_range/lin | series1 | 73566 | 73484 | 74058 | +0.669 | 74188 |
| compare/filter_range/lin | series2 | 73416 | 73666 | 74058 | +0.874 | 74204 |
| compare/filter_range/sqlite | series1 | 15868 | 15740 | 15772 | -0.605 | 15916 |
| compare/filter_range/sqlite | series2 | 15772 | 15676 | 15904 | +0.837 | 15928 |
| compare/filter_range/duckdb | series1 | 46902 | 47404 | 47284 | +0.814 | 47524 |
| compare/filter_range/duckdb | series2 | 47170 | 46910 | 46994 | -0.373 | 48648 |
| compare/agg_count_by/lin | series1 | 73536 | 73596 | 74028 | +0.669 | 74192 |
| compare/agg_count_by/lin | series2 | 73434 | 73600 | 74040 | +0.825 | 74064 |
| compare/agg_count_by/sqlite | series1 | 15708 | 15644 | 15816 | +0.688 | 15928 |
| compare/agg_count_by/sqlite | series2 | 15708 | 15772 | 15808 | +0.637 | 15936 |
| compare/agg_count_by/duckdb | series1 | 47474 | 47594 | 47680 | +0.434 | 47936 |
| compare/agg_count_by/duckdb | series2 | 47132 | 47346 | 47412 | +0.594 | 48156 |

PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas are outside this run. RSS records cover the full process lifecycle and do not establish a bound on retained index memory. COW Row changes an exact BTreeMap alias into an Arc-backed newtype and adds allocation/indirection at initial construction. Process RSS does not establish complete lifecycle memory superiority; separate held-result/alias and resource checks remain required. No production source is accepted by this report alone.
