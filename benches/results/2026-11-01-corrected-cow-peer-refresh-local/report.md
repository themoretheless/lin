# Corrected COW: available-peer API comparison

Source `8e16076334e4f27c53ccac0a0534d5660a63be5a`, runtime625bc7180. macOS ARM64 with native ARM server containers on Linux6.8; complete values/columns/multiplicity; three fresh processes at each of1k/10k/100k;12 calibrated samples per case/engine/process. No previous-host causal speedup inference.

Prepared Rust Lin versus Python APIs and server drivers/network is the declared contract. Server tmpfs/native insertion contracts differ from Full durable writes. Embeddings and FTS stay enabled. All losing comparisons remain. MSSQL private comparisons and real Kusto are outside this run; the full eight-peer objective remains open.

lin: 0.4.0, sqlite: 3.54.1, duckdb: 1.5.6, postgres: 160015, mysql: 8.4.11, mongo: 8.0.34, pandas: 3.0.6.

## read

Aggregate numerical wins: 108/108; positive process comparisons: 324/324.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | point_get | duckdb | 0.000118 | 0.037735 | 320.515 | 3/3 |
| 1000 | point_get | mongo | 0.000118 | 0.381747 | 3242.467 | 3/3 |
| 1000 | point_get | mysql | 0.000118 | 0.263403 | 2237.283 | 3/3 |
| 1000 | point_get | pandas | 0.000118 | 0.004325 | 36.738 | 3/3 |
| 1000 | point_get | postgres | 0.000118 | 0.287815 | 2444.637 | 3/3 |
| 1000 | point_get | sqlite | 0.000118 | 0.000910 | 7.726 | 3/3 |
| 1000 | filter_eq | duckdb | 0.000663 | 0.138973 | 209.596 | 3/3 |
| 1000 | filter_eq | mongo | 0.000663 | 0.473039 | 713.430 | 3/3 |
| 1000 | filter_eq | mysql | 0.000663 | 0.242507 | 365.744 | 3/3 |
| 1000 | filter_eq | pandas | 0.000663 | 0.058722 | 88.563 | 3/3 |
| 1000 | filter_eq | postgres | 0.000663 | 0.323689 | 488.182 | 3/3 |
| 1000 | filter_eq | sqlite | 0.000663 | 0.007424 | 11.196 | 3/3 |
| 1000 | text_substr | duckdb | 0.001853 | 0.061849 | 33.380 | 3/3 |
| 1000 | text_substr | mongo | 0.001853 | 0.729344 | 393.631 | 3/3 |
| 1000 | text_substr | mysql | 0.001853 | 0.330542 | 178.395 | 3/3 |
| 1000 | text_substr | pandas | 0.001853 | 0.075322 | 40.651 | 3/3 |
| 1000 | text_substr | postgres | 0.001853 | 0.332523 | 179.464 | 3/3 |
| 1000 | text_substr | sqlite | 0.001853 | 0.032147 | 17.350 | 3/3 |
| 1000 | materialize | duckdb | 0.040086 | 0.294753 | 7.353 | 3/3 |
| 1000 | materialize | mongo | 0.040086 | 1.261653 | 31.474 | 3/3 |
| 1000 | materialize | mysql | 0.040086 | 1.212687 | 30.253 | 3/3 |
| 1000 | materialize | pandas | 0.040086 | 0.376767 | 9.399 | 3/3 |
| 1000 | materialize | postgres | 0.040086 | 0.421093 | 10.505 | 3/3 |
| 1000 | materialize | sqlite | 0.040086 | 0.165180 | 4.121 | 3/3 |
| 1000 | join_inner | duckdb | 0.134196 | 0.354654 | 2.643 | 3/3 |
| 1000 | join_inner | mongo | 0.134196 | 12.877083 | 95.957 | 3/3 |
| 1000 | join_inner | mysql | 0.134196 | 2.227531 | 16.599 | 3/3 |
| 1000 | join_inner | pandas | 0.134196 | 0.732437 | 5.458 | 3/3 |
| 1000 | join_inner | postgres | 0.134196 | 0.601865 | 4.485 | 3/3 |
| 1000 | join_inner | sqlite | 0.134196 | 0.391564 | 2.918 | 3/3 |
| 1000 | join_filter | duckdb | 0.067586 | 0.246017 | 3.640 | 3/3 |
| 1000 | join_filter | mongo | 0.067586 | 7.268292 | 107.542 | 3/3 |
| 1000 | join_filter | mysql | 0.067586 | 1.309604 | 19.377 | 3/3 |
| 1000 | join_filter | pandas | 0.067586 | 0.637768 | 9.436 | 3/3 |
| 1000 | join_filter | postgres | 0.067586 | 0.453171 | 6.705 | 3/3 |
| 1000 | join_filter | sqlite | 0.067586 | 0.202176 | 2.991 | 3/3 |
| 10000 | point_get | duckdb | 0.000117 | 0.038055 | 325.357 | 3/3 |
| 10000 | point_get | mongo | 0.000117 | 0.440192 | 3763.534 | 3/3 |
| 10000 | point_get | mysql | 0.000117 | 0.322786 | 2759.738 | 3/3 |
| 10000 | point_get | pandas | 0.000117 | 0.004323 | 36.958 | 3/3 |
| 10000 | point_get | postgres | 0.000117 | 0.200630 | 1715.334 | 3/3 |
| 10000 | point_get | sqlite | 0.000117 | 0.000923 | 7.895 | 3/3 |
| 10000 | filter_eq | duckdb | 0.000680 | 0.183603 | 270.060 | 3/3 |
| 10000 | filter_eq | mongo | 0.000680 | 0.971021 | 1428.261 | 3/3 |
| 10000 | filter_eq | mysql | 0.000680 | 0.643919 | 947.132 | 3/3 |
| 10000 | filter_eq | pandas | 0.000680 | 0.258366 | 380.027 | 3/3 |
| 10000 | filter_eq | postgres | 0.000680 | 0.441965 | 650.081 | 3/3 |
| 10000 | filter_eq | sqlite | 0.000680 | 0.068208 | 100.327 | 3/3 |
| 10000 | text_substr | duckdb | 0.015764 | 0.099869 | 6.335 | 3/3 |
| 10000 | text_substr | mongo | 0.015764 | 3.238729 | 205.452 | 3/3 |
| 10000 | text_substr | mysql | 0.015764 | 1.250021 | 79.296 | 3/3 |
| 10000 | text_substr | pandas | 0.015764 | 0.476052 | 30.199 | 3/3 |
| 10000 | text_substr | postgres | 0.015764 | 0.715233 | 45.372 | 3/3 |
| 10000 | text_substr | sqlite | 0.015764 | 0.329831 | 20.923 | 3/3 |
| 10000 | materialize | duckdb | 0.380632 | 1.250291 | 3.285 | 3/3 |
| 10000 | materialize | mongo | 0.380632 | 5.386500 | 14.151 | 3/3 |
| 10000 | materialize | mysql | 0.380632 | 9.706229 | 25.500 | 3/3 |
| 10000 | materialize | pandas | 0.380632 | 1.882771 | 4.946 | 3/3 |
| 10000 | materialize | postgres | 0.380632 | 1.581736 | 4.156 | 3/3 |
| 10000 | materialize | sqlite | 0.380632 | 1.845177 | 4.848 | 3/3 |
| 10000 | join_inner | duckdb | 1.341958 | 2.719791 | 2.027 | 3/3 |
| 10000 | join_inner | mongo | 1.341958 | 112.382000 | 83.745 | 3/3 |
| 10000 | join_inner | mysql | 1.341958 | 20.190917 | 15.046 | 3/3 |
| 10000 | join_inner | pandas | 1.341958 | 4.033271 | 3.006 | 3/3 |
| 10000 | join_inner | postgres | 1.341958 | 4.407479 | 3.284 | 3/3 |
| 10000 | join_inner | sqlite | 1.341958 | 4.486896 | 3.344 | 3/3 |
| 10000 | join_filter | duckdb | 0.671000 | 1.442646 | 2.150 | 3/3 |
| 10000 | join_filter | mongo | 0.671000 | 58.722209 | 87.514 | 3/3 |
| 10000 | join_filter | mysql | 0.671000 | 10.712583 | 15.965 | 3/3 |
| 10000 | join_filter | pandas | 0.671000 | 2.290375 | 3.413 | 3/3 |
| 10000 | join_filter | postgres | 0.671000 | 2.500323 | 3.726 | 3/3 |
| 10000 | join_filter | sqlite | 0.671000 | 2.309260 | 3.442 | 3/3 |
| 100000 | point_get | duckdb | 0.000114 | 0.039662 | 347.363 | 3/3 |
| 100000 | point_get | mongo | 0.000114 | 0.441326 | 3865.141 | 3/3 |
| 100000 | point_get | mysql | 0.000114 | 0.341411 | 2990.081 | 3/3 |
| 100000 | point_get | pandas | 0.000114 | 0.004229 | 37.038 | 3/3 |
| 100000 | point_get | postgres | 0.000114 | 0.247828 | 2170.476 | 3/3 |
| 100000 | point_get | sqlite | 0.000114 | 0.000953 | 8.343 | 3/3 |
| 100000 | filter_eq | duckdb | 0.000854 | 0.446286 | 522.701 | 3/3 |
| 100000 | filter_eq | mongo | 0.000854 | 5.088604 | 5959.896 | 3/3 |
| 100000 | filter_eq | mysql | 0.000854 | 3.342604 | 3914.939 | 3/3 |
| 100000 | filter_eq | pandas | 0.000854 | 2.226979 | 2608.292 | 3/3 |
| 100000 | filter_eq | postgres | 0.000854 | 2.582167 | 3024.296 | 3/3 |
| 100000 | filter_eq | sqlite | 0.000854 | 0.660226 | 773.273 | 3/3 |
| 100000 | text_substr | duckdb | 0.206853 | 0.493718 | 2.387 | 3/3 |
| 100000 | text_substr | mongo | 0.206853 | 27.246500 | 131.719 | 3/3 |
| 100000 | text_substr | mysql | 0.206853 | 9.672875 | 46.762 | 3/3 |
| 100000 | text_substr | pandas | 0.206853 | 4.474958 | 21.633 | 3/3 |
| 100000 | text_substr | postgres | 0.206853 | 5.330209 | 25.768 | 3/3 |
| 100000 | text_substr | sqlite | 0.206853 | 3.267583 | 15.797 | 3/3 |
| 100000 | materialize | duckdb | 5.604437 | 13.067375 | 2.332 | 3/3 |
| 100000 | materialize | mongo | 5.604437 | 43.408312 | 7.745 | 3/3 |
| 100000 | materialize | mysql | 5.604437 | 83.551146 | 14.908 | 3/3 |
| 100000 | materialize | pandas | 5.604437 | 17.121729 | 3.055 | 3/3 |
| 100000 | materialize | postgres | 5.604437 | 17.510041 | 3.124 | 3/3 |
| 100000 | materialize | sqlite | 5.604437 | 20.114480 | 3.589 | 3/3 |
| 100000 | join_inner | duckdb | 16.559042 | 29.182896 | 1.762 | 3/3 |
| 100000 | join_inner | mongo | 16.559042 | 1127.863083 | 68.112 | 3/3 |
| 100000 | join_inner | mysql | 16.559042 | 167.125812 | 10.093 | 3/3 |
| 100000 | join_inner | pandas | 16.559042 | 36.235292 | 2.188 | 3/3 |
| 100000 | join_inner | postgres | 16.559042 | 43.776520 | 2.644 | 3/3 |
| 100000 | join_inner | sqlite | 16.559042 | 50.706750 | 3.062 | 3/3 |
| 100000 | join_filter | duckdb | 7.854395 | 14.427209 | 1.837 | 3/3 |
| 100000 | join_filter | mongo | 7.854395 | 569.591938 | 72.519 | 3/3 |
| 100000 | join_filter | mysql | 7.854395 | 89.297750 | 11.369 | 3/3 |
| 100000 | join_filter | pandas | 7.854395 | 18.700750 | 2.381 | 3/3 |
| 100000 | join_filter | postgres | 7.854395 | 25.293708 | 3.220 | 3/3 |
| 100000 | join_filter | sqlite | 7.854395 | 25.466938 | 3.242 | 3/3 |

## native

Aggregate numerical wins: 15/15; positive process comparisons: 45/45.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | insert_native | duckdb | 0.765062 | 1.120605 | 1.465 | 3/3 |
| 1000 | insert_native | mongo | 0.765062 | 4.768937 | 6.233 | 3/3 |
| 1000 | insert_native | mysql | 0.765062 | 7.992562 | 10.447 | 3/3 |
| 1000 | insert_native | postgres | 0.765062 | 4.361562 | 5.701 | 3/3 |
| 1000 | insert_native | sqlite | 0.765062 | 1.000875 | 1.308 | 3/3 |
| 10000 | insert_native | duckdb | 7.787708 | 9.524062 | 1.223 | 3/3 |
| 10000 | insert_native | mongo | 7.787708 | 45.582251 | 5.853 | 3/3 |
| 10000 | insert_native | mysql | 7.787708 | 78.374583 | 10.064 | 3/3 |
| 10000 | insert_native | postgres | 7.787708 | 26.826500 | 3.445 | 3/3 |
| 10000 | insert_native | sqlite | 7.787708 | 12.509625 | 1.606 | 3/3 |
| 100000 | insert_native | duckdb | 90.079333 | 99.904250 | 1.109 | 3/3 |
| 100000 | insert_native | mongo | 90.079333 | 516.322000 | 5.732 | 3/3 |
| 100000 | insert_native | mysql | 90.079333 | 820.642042 | 9.110 | 3/3 |
| 100000 | insert_native | postgres | 90.079333 | 272.605458 | 3.026 | 3/3 |
| 100000 | insert_native | sqlite | 90.079333 | 142.462771 | 1.582 | 3/3 |

