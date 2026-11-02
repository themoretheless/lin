# Corrected COW: Ubuntu22 x86 available-peer API comparison

Checkout `10e189548399c7ee0a260436997585826cf0ab01`, runtime625bc7180. Linux x86_64; complete values/columns/multiplicity; three fresh processes at each of1k/10k/100k;12 calibrated samples per case/engine/process. No previous-host causal speedup inference.

Prepared Rust Lin versus Python APIs and server drivers/network is the declared contract. Server tmpfs/native insertion contracts differ from Full durable writes. Embeddings and FTS stay enabled. All losing comparisons remain. MSSQL private comparisons and real Kusto are outside this run; the full eight-peer objective remains open.

lin: 0.4.0, sqlite: 3.37.2, duckdb: 1.5.6, postgres: 160015, mysql: 8.4.13, mongo: 8.0.34, pandas: 3.0.6.

## read

Aggregate numerical wins: 108/108; positive process comparisons: 324/324.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | point_get | duckdb | 0.000215 | 0.234800 | 1092.453 | 3/3 |
| 1000 | point_get | mongo | 0.000215 | 0.461720 | 2148.238 | 3/3 |
| 1000 | point_get | mysql | 0.000215 | 0.248885 | 1157.984 | 3/3 |
| 1000 | point_get | pandas | 0.000215 | 0.017426 | 81.076 | 3/3 |
| 1000 | point_get | postgres | 0.000215 | 0.206204 | 959.405 | 3/3 |
| 1000 | point_get | sqlite | 0.000215 | 0.003503 | 16.297 | 3/3 |
| 1000 | filter_eq | duckdb | 0.000923 | 0.488201 | 529.193 | 3/3 |
| 1000 | filter_eq | mongo | 0.000923 | 0.659615 | 715.000 | 3/3 |
| 1000 | filter_eq | mysql | 0.000923 | 0.341555 | 370.234 | 3/3 |
| 1000 | filter_eq | pandas | 0.000923 | 0.200649 | 217.497 | 3/3 |
| 1000 | filter_eq | postgres | 0.000923 | 0.264954 | 287.201 | 3/3 |
| 1000 | filter_eq | sqlite | 0.000923 | 0.023619 | 25.602 | 3/3 |
| 1000 | text_substr | duckdb | 0.003924 | 0.320295 | 81.629 | 3/3 |
| 1000 | text_substr | mongo | 0.003924 | 1.238170 | 315.554 | 3/3 |
| 1000 | text_substr | mysql | 0.003924 | 0.446208 | 113.718 | 3/3 |
| 1000 | text_substr | pandas | 0.003924 | 0.266811 | 67.998 | 3/3 |
| 1000 | text_substr | postgres | 0.003924 | 0.266596 | 67.943 | 3/3 |
| 1000 | text_substr | sqlite | 0.003924 | 0.074828 | 19.070 | 3/3 |
| 1000 | materialize | duckdb | 0.094293 | 0.823500 | 8.733 | 3/3 |
| 1000 | materialize | mongo | 0.094293 | 1.877821 | 19.915 | 3/3 |
| 1000 | materialize | mysql | 0.094293 | 2.615285 | 27.736 | 3/3 |
| 1000 | materialize | pandas | 0.094293 | 1.251579 | 13.273 | 3/3 |
| 1000 | materialize | postgres | 0.094293 | 0.555578 | 5.892 | 3/3 |
| 1000 | materialize | sqlite | 0.094293 | 0.449429 | 4.766 | 3/3 |
| 1000 | join_inner | duckdb | 0.351598 | 1.322899 | 3.763 | 3/3 |
| 1000 | join_inner | mongo | 0.351598 | 38.244904 | 108.774 | 3/3 |
| 1000 | join_inner | mysql | 0.351598 | 5.421889 | 15.421 | 3/3 |
| 1000 | join_inner | pandas | 0.351598 | 2.418618 | 6.879 | 3/3 |
| 1000 | join_inner | postgres | 0.351598 | 1.092510 | 3.107 | 3/3 |
| 1000 | join_inner | sqlite | 0.351598 | 1.037485 | 2.951 | 3/3 |
| 1000 | join_filter | duckdb | 0.175524 | 1.005103 | 5.726 | 3/3 |
| 1000 | join_filter | mongo | 0.175524 | 19.832936 | 112.993 | 3/3 |
| 1000 | join_filter | mysql | 0.175524 | 2.965310 | 16.894 | 3/3 |
| 1000 | join_filter | pandas | 0.175524 | 2.307167 | 13.144 | 3/3 |
| 1000 | join_filter | postgres | 0.175524 | 0.713994 | 4.068 | 3/3 |
| 1000 | join_filter | sqlite | 0.175524 | 0.538736 | 3.069 | 3/3 |
| 10000 | point_get | duckdb | 0.000216 | 0.231392 | 1073.066 | 3/3 |
| 10000 | point_get | mongo | 0.000216 | 0.470981 | 2184.142 | 3/3 |
| 10000 | point_get | mysql | 0.000216 | 0.257300 | 1193.211 | 3/3 |
| 10000 | point_get | pandas | 0.000216 | 0.017484 | 81.082 | 3/3 |
| 10000 | point_get | postgres | 0.000216 | 0.205637 | 953.627 | 3/3 |
| 10000 | point_get | sqlite | 0.000216 | 0.003610 | 16.739 | 3/3 |
| 10000 | filter_eq | duckdb | 0.001051 | 0.653518 | 622.046 | 3/3 |
| 10000 | filter_eq | mongo | 0.001051 | 1.450541 | 1380.686 | 3/3 |
| 10000 | filter_eq | mysql | 0.001051 | 1.026116 | 976.701 | 3/3 |
| 10000 | filter_eq | pandas | 0.001051 | 0.660116 | 628.327 | 3/3 |
| 10000 | filter_eq | postgres | 0.001051 | 0.627173 | 596.970 | 3/3 |
| 10000 | filter_eq | sqlite | 0.001051 | 0.207013 | 197.044 | 3/3 |
| 10000 | text_substr | duckdb | 0.047803 | 0.438459 | 9.172 | 3/3 |
| 10000 | text_substr | mongo | 0.047803 | 6.845525 | 143.202 | 3/3 |
| 10000 | text_substr | mysql | 0.047803 | 2.164079 | 45.271 | 3/3 |
| 10000 | text_substr | pandas | 0.047803 | 1.375198 | 28.768 | 3/3 |
| 10000 | text_substr | postgres | 0.047803 | 1.069303 | 22.369 | 3/3 |
| 10000 | text_substr | sqlite | 0.047803 | 0.713253 | 14.921 | 3/3 |
| 10000 | materialize | duckdb | 1.010379 | 3.507619 | 3.472 | 3/3 |
| 10000 | materialize | mongo | 1.010379 | 11.306828 | 11.191 | 3/3 |
| 10000 | materialize | mysql | 1.010379 | 25.165938 | 24.907 | 3/3 |
| 10000 | materialize | pandas | 1.010379 | 5.290813 | 5.236 | 3/3 |
| 10000 | materialize | postgres | 1.010379 | 4.046065 | 4.005 | 3/3 |
| 10000 | materialize | sqlite | 1.010379 | 5.118148 | 5.066 | 3/3 |
| 10000 | join_inner | duckdb | 3.638985 | 7.945216 | 2.183 | 3/3 |
| 10000 | join_inner | mongo | 3.638985 | 375.473620 | 103.181 | 3/3 |
| 10000 | join_inner | mysql | 3.638985 | 55.852184 | 15.348 | 3/3 |
| 10000 | join_inner | pandas | 3.638985 | 11.358054 | 3.121 | 3/3 |
| 10000 | join_inner | postgres | 3.638985 | 9.973817 | 2.741 | 3/3 |
| 10000 | join_inner | sqlite | 3.638985 | 12.209627 | 3.355 | 3/3 |
| 10000 | join_filter | duckdb | 1.825889 | 4.524626 | 2.478 | 3/3 |
| 10000 | join_filter | mongo | 1.825889 | 190.104429 | 104.116 | 3/3 |
| 10000 | join_filter | mysql | 1.825889 | 29.108289 | 15.942 | 3/3 |
| 10000 | join_filter | pandas | 1.825889 | 6.781260 | 3.714 | 3/3 |
| 10000 | join_filter | postgres | 1.825889 | 5.792862 | 3.173 | 3/3 |
| 10000 | join_filter | sqlite | 1.825889 | 6.363684 | 3.485 | 3/3 |
| 100000 | point_get | duckdb | 0.000211 | 0.232829 | 1101.479 | 3/3 |
| 100000 | point_get | mongo | 0.000211 | 0.468701 | 2217.350 | 3/3 |
| 100000 | point_get | mysql | 0.000211 | 0.255795 | 1210.125 | 3/3 |
| 100000 | point_get | pandas | 0.000211 | 0.017335 | 82.010 | 3/3 |
| 100000 | point_get | postgres | 0.000211 | 0.218252 | 1032.516 | 3/3 |
| 100000 | point_get | sqlite | 0.000211 | 0.003632 | 17.182 | 3/3 |
| 100000 | filter_eq | duckdb | 0.002083 | 1.751671 | 840.977 | 3/3 |
| 100000 | filter_eq | mongo | 0.002083 | 8.927171 | 4285.932 | 3/3 |
| 100000 | filter_eq | mysql | 0.002083 | 7.767593 | 3729.219 | 3/3 |
| 100000 | filter_eq | pandas | 0.002083 | 5.033725 | 2416.690 | 3/3 |
| 100000 | filter_eq | postgres | 0.002083 | 4.410972 | 2117.706 | 3/3 |
| 100000 | filter_eq | sqlite | 0.002083 | 2.057900 | 987.997 | 3/3 |
| 100000 | text_substr | duckdb | 0.555821 | 1.308829 | 2.355 | 3/3 |
| 100000 | text_substr | mongo | 0.555821 | 66.305001 | 119.292 | 3/3 |
| 100000 | text_substr | mysql | 0.555821 | 19.681831 | 35.410 | 3/3 |
| 100000 | text_substr | pandas | 0.555821 | 11.491572 | 20.675 | 3/3 |
| 100000 | text_substr | postgres | 0.555821 | 9.088597 | 16.352 | 3/3 |
| 100000 | text_substr | sqlite | 0.555821 | 7.240221 | 13.026 | 3/3 |
| 100000 | materialize | duckdb | 25.204069 | 32.756409 | 1.300 | 3/3 |
| 100000 | materialize | mongo | 25.204069 | 108.393805 | 4.301 | 3/3 |
| 100000 | materialize | mysql | 25.204069 | 233.177741 | 9.252 | 3/3 |
| 100000 | materialize | pandas | 25.204069 | 44.966718 | 1.784 | 3/3 |
| 100000 | materialize | postgres | 25.204069 | 41.296318 | 1.638 | 3/3 |
| 100000 | materialize | sqlite | 25.204069 | 56.635699 | 2.247 | 3/3 |
| 100000 | join_inner | duckdb | 52.458310 | 90.673655 | 1.728 | 3/3 |
| 100000 | join_inner | mongo | 52.458310 | 3824.407543 | 72.904 | 3/3 |
| 100000 | join_inner | mysql | 52.458310 | 492.135436 | 9.381 | 3/3 |
| 100000 | join_inner | pandas | 52.458310 | 109.760343 | 2.092 | 3/3 |
| 100000 | join_inner | postgres | 52.458310 | 122.559941 | 2.336 | 3/3 |
| 100000 | join_inner | sqlite | 52.458310 | 141.682508 | 2.701 | 3/3 |
| 100000 | join_filter | duckdb | 25.641870 | 42.279237 | 1.649 | 3/3 |
| 100000 | join_filter | mongo | 25.641870 | 1942.789403 | 75.766 | 3/3 |
| 100000 | join_filter | mysql | 25.641870 | 266.787431 | 10.404 | 3/3 |
| 100000 | join_filter | pandas | 25.641870 | 54.366546 | 2.120 | 3/3 |
| 100000 | join_filter | postgres | 25.641870 | 62.139047 | 2.423 | 3/3 |
| 100000 | join_filter | sqlite | 25.641870 | 67.715401 | 2.641 | 3/3 |

## native

Aggregate numerical wins: 14/15; positive process comparisons: 42/45.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | insert_native | duckdb | 1.647260 | 5.767894 | 3.502 | 3/3 |
| 1000 | insert_native | mongo | 1.647260 | 11.935075 | 7.245 | 3/3 |
| 1000 | insert_native | mysql | 1.647260 | 18.384414 | 11.161 | 3/3 |
| 1000 | insert_native | postgres | 1.647260 | 7.741443 | 4.700 | 3/3 |
| 1000 | insert_native | sqlite | 1.647260 | 2.910762 | 1.767 | 3/3 |
| 10000 | insert_native | duckdb | 21.550813 | 30.549996 | 1.418 | 3/3 |
| 10000 | insert_native | mongo | 21.550813 | 119.618712 | 5.551 | 3/3 |
| 10000 | insert_native | mysql | 21.550813 | 204.753724 | 9.501 | 3/3 |
| 10000 | insert_native | postgres | 21.550813 | 58.728046 | 2.725 | 3/3 |
| 10000 | insert_native | sqlite | 21.550813 | 33.667420 | 1.562 | 3/3 |
| 100000 | insert_native | duckdb | 259.957038 | 230.341253 | 0.886 | 0/3 |
| 100000 | insert_native | mongo | 259.957038 | 1324.259875 | 5.094 | 3/3 |
| 100000 | insert_native | mysql | 259.957038 | 2090.331526 | 8.041 | 3/3 |
| 100000 | insert_native | postgres | 259.957038 | 648.071045 | 2.493 | 3/3 |
| 100000 | insert_native | sqlite | 259.957038 | 369.785321 | 1.422 | 3/3 |


Independent local collection verified all 88 archive member hashes, frozen Git input blobs, exact ELF x86_64 hash, original 18 process reports and 5184 observations. Read comparisons: 108/108 aggregate wins and 324/324 process wins, minimum peer/Lin ratio 1.299648. Native insertion: 14/15 aggregate wins, 42/45 process wins. The sole losing comparison is DuckDB native INSERT 100000: Lin 259.957038 ms versus DuckDB 230.341254 ms, peer/Lin 0.886074 (all three processes lose). The original win gate stays failed; no unchanged rerun or waiver.

Runtime is the exact original COW source 625bc7180, not the later RowKey/extent prototypes. This matrix covers six available peers at the documented Rust/Python/driver/network API boundaries; server inserts use the protocol tmpfs configuration and do not replace SQLite FULL WAL durability comparisons. MSSQL and real Kusto remain outside this public matrix. No adoption or full-objective completion.
