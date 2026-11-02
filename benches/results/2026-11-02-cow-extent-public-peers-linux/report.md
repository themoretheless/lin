# COW Arc with extent feature: Ubuntu22 x86 available-peer API comparison

Checkout `8445e1e198e6020ec85c49d991650713dce8e251`, runtime60e64559e, default plus experimental-extent-wal. Linux x86_64; complete values/columns/multiplicity; three fresh processes at each of1k/10k/100k;12 calibrated samples per case/engine/process. No previous-host causal speedup inference.

Prepared Rust Lin versus Python APIs and server drivers/network is the declared contract. Server tmpfs/native insertion contracts differ from Full durable writes. Embeddings and FTS stay enabled. All losing comparisons remain. MSSQL private comparisons and real Kusto are outside this run; the full eight-peer objective remains open.

lin: 0.4.0, sqlite: 3.37.2, duckdb: 1.5.6, postgres: 160015, mysql: 8.4.13, mongo: 8.0.34, pandas: 3.0.6.

## read

Aggregate numerical wins: 108/108; positive process comparisons: 324/324.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | point_get | duckdb | 0.000235 | 0.235637 | 1000.977 | 3/3 |
| 1000 | point_get | mongo | 0.000235 | 0.463002 | 1966.811 | 3/3 |
| 1000 | point_get | mysql | 0.000235 | 0.251828 | 1069.754 | 3/3 |
| 1000 | point_get | pandas | 0.000235 | 0.017555 | 74.574 | 3/3 |
| 1000 | point_get | postgres | 0.000235 | 0.201103 | 854.277 | 3/3 |
| 1000 | point_get | sqlite | 0.000235 | 0.003531 | 14.999 | 3/3 |
| 1000 | filter_eq | duckdb | 0.000964 | 0.487249 | 505.331 | 3/3 |
| 1000 | filter_eq | mongo | 0.000964 | 0.647302 | 671.324 | 3/3 |
| 1000 | filter_eq | mysql | 0.000964 | 0.343793 | 356.551 | 3/3 |
| 1000 | filter_eq | pandas | 0.000964 | 0.202976 | 210.509 | 3/3 |
| 1000 | filter_eq | postgres | 0.000964 | 0.269177 | 279.166 | 3/3 |
| 1000 | filter_eq | sqlite | 0.000964 | 0.023386 | 24.254 | 3/3 |
| 1000 | text_substr | duckdb | 0.003895 | 0.324016 | 83.190 | 3/3 |
| 1000 | text_substr | mongo | 0.003895 | 1.219596 | 313.129 | 3/3 |
| 1000 | text_substr | mysql | 0.003895 | 0.432807 | 111.122 | 3/3 |
| 1000 | text_substr | pandas | 0.003895 | 0.267355 | 68.643 | 3/3 |
| 1000 | text_substr | postgres | 0.003895 | 0.279043 | 71.644 | 3/3 |
| 1000 | text_substr | sqlite | 0.003895 | 0.076342 | 19.601 | 3/3 |
| 1000 | materialize | duckdb | 0.111003 | 0.814432 | 7.337 | 3/3 |
| 1000 | materialize | mongo | 0.111003 | 1.871012 | 16.856 | 3/3 |
| 1000 | materialize | mysql | 0.111003 | 2.616607 | 23.572 | 3/3 |
| 1000 | materialize | pandas | 0.111003 | 1.246044 | 11.225 | 3/3 |
| 1000 | materialize | postgres | 0.111003 | 0.544421 | 4.905 | 3/3 |
| 1000 | materialize | sqlite | 0.111003 | 0.439964 | 3.964 | 3/3 |
| 1000 | join_inner | duckdb | 0.388658 | 1.307255 | 3.364 | 3/3 |
| 1000 | join_inner | mongo | 0.388658 | 38.908879 | 100.111 | 3/3 |
| 1000 | join_inner | mysql | 0.388658 | 5.501784 | 14.156 | 3/3 |
| 1000 | join_inner | pandas | 0.388658 | 2.456237 | 6.320 | 3/3 |
| 1000 | join_inner | postgres | 0.388658 | 1.086101 | 2.794 | 3/3 |
| 1000 | join_inner | sqlite | 0.388658 | 1.033577 | 2.659 | 3/3 |
| 1000 | join_filter | duckdb | 0.192486 | 1.021080 | 5.305 | 3/3 |
| 1000 | join_filter | mongo | 0.192486 | 20.127755 | 104.568 | 3/3 |
| 1000 | join_filter | mysql | 0.192486 | 2.975093 | 15.456 | 3/3 |
| 1000 | join_filter | pandas | 0.192486 | 2.277432 | 11.832 | 3/3 |
| 1000 | join_filter | postgres | 0.192486 | 0.713686 | 3.708 | 3/3 |
| 1000 | join_filter | sqlite | 0.192486 | 0.541336 | 2.812 | 3/3 |
| 10000 | point_get | duckdb | 0.000237 | 0.239976 | 1011.227 | 3/3 |
| 10000 | point_get | mongo | 0.000237 | 0.463836 | 1954.541 | 3/3 |
| 10000 | point_get | mysql | 0.000237 | 0.257466 | 1084.926 | 3/3 |
| 10000 | point_get | pandas | 0.000237 | 0.017280 | 72.814 | 3/3 |
| 10000 | point_get | postgres | 0.000237 | 0.205060 | 864.096 | 3/3 |
| 10000 | point_get | sqlite | 0.000237 | 0.003562 | 15.008 | 3/3 |
| 10000 | filter_eq | duckdb | 0.001088 | 0.661094 | 607.706 | 3/3 |
| 10000 | filter_eq | mongo | 0.001088 | 1.459355 | 1341.502 | 3/3 |
| 10000 | filter_eq | mysql | 0.001088 | 1.029483 | 946.345 | 3/3 |
| 10000 | filter_eq | pandas | 0.001088 | 0.655622 | 602.676 | 3/3 |
| 10000 | filter_eq | postgres | 0.001088 | 0.630253 | 579.355 | 3/3 |
| 10000 | filter_eq | sqlite | 0.001088 | 0.208298 | 191.476 | 3/3 |
| 10000 | text_substr | duckdb | 0.047705 | 0.434996 | 9.118 | 3/3 |
| 10000 | text_substr | mongo | 0.047705 | 6.933003 | 145.331 | 3/3 |
| 10000 | text_substr | mysql | 0.047705 | 2.167232 | 45.430 | 3/3 |
| 10000 | text_substr | pandas | 0.047705 | 1.368917 | 28.696 | 3/3 |
| 10000 | text_substr | postgres | 0.047705 | 1.077635 | 22.590 | 3/3 |
| 10000 | text_substr | sqlite | 0.047705 | 0.738205 | 15.474 | 3/3 |
| 10000 | materialize | duckdb | 1.165010 | 3.468568 | 2.977 | 3/3 |
| 10000 | materialize | mongo | 1.165010 | 11.337758 | 9.732 | 3/3 |
| 10000 | materialize | mysql | 1.165010 | 24.813680 | 21.299 | 3/3 |
| 10000 | materialize | pandas | 1.165010 | 5.238166 | 4.496 | 3/3 |
| 10000 | materialize | postgres | 1.165010 | 4.036401 | 3.465 | 3/3 |
| 10000 | materialize | sqlite | 1.165010 | 4.890048 | 4.197 | 3/3 |
| 10000 | join_inner | duckdb | 4.078724 | 7.814358 | 1.916 | 3/3 |
| 10000 | join_inner | mongo | 4.078724 | 385.155252 | 94.430 | 3/3 |
| 10000 | join_inner | mysql | 4.078724 | 55.852106 | 13.694 | 3/3 |
| 10000 | join_inner | pandas | 4.078724 | 11.486961 | 2.816 | 3/3 |
| 10000 | join_inner | postgres | 4.078724 | 10.542409 | 2.585 | 3/3 |
| 10000 | join_inner | sqlite | 4.078724 | 12.243906 | 3.002 | 3/3 |
| 10000 | join_filter | duckdb | 2.032951 | 4.527172 | 2.227 | 3/3 |
| 10000 | join_filter | mongo | 2.032951 | 196.491904 | 96.654 | 3/3 |
| 10000 | join_filter | mysql | 2.032951 | 29.256646 | 14.391 | 3/3 |
| 10000 | join_filter | pandas | 2.032951 | 6.927142 | 3.407 | 3/3 |
| 10000 | join_filter | postgres | 2.032951 | 5.694655 | 2.801 | 3/3 |
| 10000 | join_filter | sqlite | 2.032951 | 6.302640 | 3.100 | 3/3 |
| 100000 | point_get | duckdb | 0.000241 | 0.238841 | 992.758 | 3/3 |
| 100000 | point_get | mongo | 0.000241 | 0.473121 | 1966.564 | 3/3 |
| 100000 | point_get | mysql | 0.000241 | 0.246236 | 1023.499 | 3/3 |
| 100000 | point_get | pandas | 0.000241 | 0.017218 | 71.570 | 3/3 |
| 100000 | point_get | postgres | 0.000241 | 0.221164 | 919.283 | 3/3 |
| 100000 | point_get | sqlite | 0.000241 | 0.003628 | 15.080 | 3/3 |
| 100000 | filter_eq | duckdb | 0.002155 | 1.734106 | 804.532 | 3/3 |
| 100000 | filter_eq | mongo | 0.002155 | 8.902719 | 4130.385 | 3/3 |
| 100000 | filter_eq | mysql | 0.002155 | 7.791227 | 3614.712 | 3/3 |
| 100000 | filter_eq | pandas | 0.002155 | 5.065539 | 2350.139 | 3/3 |
| 100000 | filter_eq | postgres | 0.002155 | 4.389697 | 2036.584 | 3/3 |
| 100000 | filter_eq | sqlite | 0.002155 | 2.113310 | 980.462 | 3/3 |
| 100000 | text_substr | duckdb | 0.554046 | 1.318513 | 2.380 | 3/3 |
| 100000 | text_substr | mongo | 0.554046 | 65.275426 | 117.816 | 3/3 |
| 100000 | text_substr | mysql | 0.554046 | 19.677522 | 35.516 | 3/3 |
| 100000 | text_substr | pandas | 0.554046 | 12.285062 | 22.173 | 3/3 |
| 100000 | text_substr | postgres | 0.554046 | 9.003108 | 16.250 | 3/3 |
| 100000 | text_substr | sqlite | 0.554046 | 7.456779 | 13.459 | 3/3 |
| 100000 | materialize | duckdb | 22.004818 | 33.360019 | 1.516 | 3/3 |
| 100000 | materialize | mongo | 22.004818 | 108.556046 | 4.933 | 3/3 |
| 100000 | materialize | mysql | 22.004818 | 230.836148 | 10.490 | 3/3 |
| 100000 | materialize | pandas | 22.004818 | 46.484038 | 2.112 | 3/3 |
| 100000 | materialize | postgres | 22.004818 | 41.285239 | 1.876 | 3/3 |
| 100000 | materialize | sqlite | 22.004818 | 55.667237 | 2.530 | 3/3 |
| 100000 | join_inner | duckdb | 57.190674 | 91.310486 | 1.597 | 3/3 |
| 100000 | join_inner | mongo | 57.190674 | 3910.782074 | 68.381 | 3/3 |
| 100000 | join_inner | mysql | 57.190674 | 493.024666 | 8.621 | 3/3 |
| 100000 | join_inner | pandas | 57.190674 | 112.283434 | 1.963 | 3/3 |
| 100000 | join_inner | postgres | 57.190674 | 122.763751 | 2.147 | 3/3 |
| 100000 | join_inner | sqlite | 57.190674 | 144.324270 | 2.524 | 3/3 |
| 100000 | join_filter | duckdb | 26.844774 | 42.338726 | 1.577 | 3/3 |
| 100000 | join_filter | mongo | 26.844774 | 1965.395859 | 73.213 | 3/3 |
| 100000 | join_filter | mysql | 26.844774 | 267.527296 | 9.966 | 3/3 |
| 100000 | join_filter | pandas | 26.844774 | 55.002827 | 2.049 | 3/3 |
| 100000 | join_filter | postgres | 26.844774 | 59.754973 | 2.226 | 3/3 |
| 100000 | join_filter | sqlite | 26.844774 | 67.784187 | 2.525 | 3/3 |

## native

Aggregate numerical wins: 15/15; positive process comparisons: 44/45.

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | insert_native | duckdb | 1.500971 | 7.037808 | 4.689 | 3/3 |
| 1000 | insert_native | mongo | 1.500971 | 11.700321 | 7.795 | 3/3 |
| 1000 | insert_native | mysql | 1.500971 | 18.053628 | 12.028 | 3/3 |
| 1000 | insert_native | postgres | 1.500971 | 7.519444 | 5.010 | 3/3 |
| 1000 | insert_native | sqlite | 1.500971 | 2.907557 | 1.937 | 3/3 |
| 10000 | insert_native | duckdb | 17.595957 | 29.957335 | 1.703 | 3/3 |
| 10000 | insert_native | mongo | 17.595957 | 119.141951 | 6.771 | 3/3 |
| 10000 | insert_native | mysql | 17.595957 | 198.604213 | 11.287 | 3/3 |
| 10000 | insert_native | postgres | 17.595957 | 57.665730 | 3.277 | 3/3 |
| 10000 | insert_native | sqlite | 17.595957 | 33.604408 | 1.910 | 3/3 |
| 100000 | insert_native | duckdb | 219.226133 | 231.642704 | 1.057 | 2/3 |
| 100000 | insert_native | mongo | 219.226133 | 1308.200797 | 5.967 | 3/3 |
| 100000 | insert_native | mysql | 219.226133 | 2048.324180 | 9.343 | 3/3 |
| 100000 | insert_native | postgres | 219.226133 | 635.560070 | 2.899 | 3/3 |
| 100000 | insert_native | sqlite | 219.226133 | 368.365209 | 1.680 | 3/3 |

