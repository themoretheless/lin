# Corrected core: available-peer API refresh

Frozen source `bab12daae6aa5150b2ce1746b688144cb89ab959`; fresh release peer_bench. Includes transaction atomic WAL/generation,abort guard and stale-schema HEAD recovery corrections. No performance prototype is enabled. Core/helper/binary hashes are unchanged across measurement.

Warm prepared/API reads at1k/10k/100k,three independent processes per size,rotated engine order and12 samples. Complete independently generated values/columns/multiplicity checked; output order is not compared. Native insertion uses fresh fixtures and six-field readback outside timing; default Lin embeddings and FTS remain enabled. Setup,oracle,teardown and Lin JSON IPC remain outside timing,following the previous frozen contract. These are API comparisons,not pure engine CPU comparisons.

## All outcomes

Read aggregate wins:108/108;positive process comparisons:324/324. Native aggregate wins:15/15;positive process comparisons:45/45. No loss or outlier is removed.

## Environment and scope

lin:0.4.0, sqlite:3.54.1, duckdb:1.5.6, postgres:160015, mysql:8.4.11, mongo:8.0.34, pandas:3.0.6.

Fresh owned ARM containers use dedicated localhost ports55434/53308/27029 and owned tmpfs storage. Image digests,architecture,server durability options and exact container IDs are in environment.json. Sentinel objects survive every size;no fixture objects remain;owned sentinels are removed. Only container IDs created by this runner are stopped/removed,with ownership-label checks and verified zero exits. Existing containers are not reused.

Lin uses a prepared Rust API; SQLite/DuckDB/pandas use Python APIs; PostgreSQL/MySQL/MongoDB include driver/network/result-transfer costs. Native APIs retain SQLite executemany+commit in memory,DuckDB INSERT SELECT from registered DataFrame,PostgreSQL COPY+commit,MySQL multi-row executemany+commit,and MongoDB ordered insert_many with w=1,j=false. Server tmpfs and these acknowledged API contracts are not identical Full durable/failure-atomicity contracts. Pandas participates only in in-memory reads.

Read4536 values are calibrated batch averages,not independent single-operation latency samples;native648 values use fresh insertion fixtures. Replication is three fresh processes,not the timed inner iterations. There is no baseline/candidate causal optimization attribution. This refresh does not qualify the separate frozen Linux KEEP_SIZE candidate or close Full durable SQLite mutation gaps.

MSSQL and Kusto are not included in this matrix and do not count as wins. MSSQL comparative evidence remains private and source-scoped;real Kusto endpoint qualification remains open. The complete eight-peer objective is not achieved.

## read

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | point_get | duckdb | 0.000117 | 0.039923 | 340.030× | 3/3 |
| 1000 | point_get | mongo | 0.000117 | 0.439904 | 3746.718× | 3/3 |
| 1000 | point_get | mysql | 0.000117 | 0.329547 | 2806.792× | 3/3 |
| 1000 | point_get | pandas | 0.000117 | 0.004524 | 38.536× | 3/3 |
| 1000 | point_get | postgres | 0.000117 | 0.314042 | 2674.735× | 3/3 |
| 1000 | point_get | sqlite | 0.000117 | 0.000919 | 7.823× | 3/3 |
| 1000 | filter_eq | duckdb | 0.000653 | 0.138239 | 211.680× | 3/3 |
| 1000 | filter_eq | mongo | 0.000653 | 0.499285 | 764.534× | 3/3 |
| 1000 | filter_eq | mysql | 0.000653 | 0.356779 | 546.321× | 3/3 |
| 1000 | filter_eq | pandas | 0.000653 | 0.057441 | 87.957× | 3/3 |
| 1000 | filter_eq | postgres | 0.000653 | 0.331835 | 508.125× | 3/3 |
| 1000 | filter_eq | sqlite | 0.000653 | 0.007572 | 11.595× | 3/3 |
| 1000 | text_substr | duckdb | 0.002139 | 0.061891 | 28.941× | 3/3 |
| 1000 | text_substr | mongo | 0.002139 | 0.742256 | 347.089× | 3/3 |
| 1000 | text_substr | mysql | 0.002139 | 0.401797 | 187.886× | 3/3 |
| 1000 | text_substr | pandas | 0.002139 | 0.074884 | 35.017× | 3/3 |
| 1000 | text_substr | postgres | 0.002139 | 0.354500 | 165.769× | 3/3 |
| 1000 | text_substr | sqlite | 0.002139 | 0.032345 | 15.125× | 3/3 |
| 1000 | materialize | duckdb | 0.039446 | 0.298533 | 7.568× | 3/3 |
| 1000 | materialize | mongo | 0.039446 | 1.652583 | 41.895× | 3/3 |
| 1000 | materialize | mysql | 0.039446 | 1.239901 | 31.433× | 3/3 |
| 1000 | materialize | pandas | 0.039446 | 0.372283 | 9.438× | 3/3 |
| 1000 | materialize | postgres | 0.039446 | 0.433849 | 10.999× | 3/3 |
| 1000 | materialize | sqlite | 0.039446 | 0.169163 | 4.288× | 3/3 |
| 1000 | join_inner | duckdb | 0.129639 | 0.369397 | 2.849× | 3/3 |
| 1000 | join_inner | mongo | 0.129639 | 13.405021 | 103.403× | 3/3 |
| 1000 | join_inner | mysql | 0.129639 | 2.317729 | 17.878× | 3/3 |
| 1000 | join_inner | pandas | 0.129639 | 0.718441 | 5.542× | 3/3 |
| 1000 | join_inner | postgres | 0.129639 | 0.661488 | 5.103× | 3/3 |
| 1000 | join_inner | sqlite | 0.129639 | 0.392236 | 3.026× | 3/3 |
| 1000 | join_filter | duckdb | 0.064833 | 0.255363 | 3.939× | 3/3 |
| 1000 | join_filter | mongo | 0.064833 | 8.313688 | 128.232× | 3/3 |
| 1000 | join_filter | mysql | 0.064833 | 1.440174 | 22.213× | 3/3 |
| 1000 | join_filter | pandas | 0.064833 | 0.626708 | 9.666× | 3/3 |
| 1000 | join_filter | postgres | 0.064833 | 0.497056 | 7.667× | 3/3 |
| 1000 | join_filter | sqlite | 0.064833 | 0.202809 | 3.128× | 3/3 |
| 10000 | point_get | duckdb | 0.000115 | 0.040549 | 352.245× | 3/3 |
| 10000 | point_get | mongo | 0.000115 | 0.420079 | 3649.155× | 3/3 |
| 10000 | point_get | mysql | 0.000115 | 0.272333 | 2365.715× | 3/3 |
| 10000 | point_get | pandas | 0.000115 | 0.004442 | 38.590× | 3/3 |
| 10000 | point_get | postgres | 0.000115 | 0.244930 | 2127.668× | 3/3 |
| 10000 | point_get | sqlite | 0.000115 | 0.000929 | 8.067× | 3/3 |
| 10000 | filter_eq | duckdb | 0.000667 | 0.186141 | 279.048× | 3/3 |
| 10000 | filter_eq | mongo | 0.000667 | 0.906029 | 1358.245× | 3/3 |
| 10000 | filter_eq | mysql | 0.000667 | 0.591286 | 886.408× | 3/3 |
| 10000 | filter_eq | pandas | 0.000667 | 0.267843 | 401.529× | 3/3 |
| 10000 | filter_eq | postgres | 0.000667 | 0.465146 | 697.309× | 3/3 |
| 10000 | filter_eq | sqlite | 0.000667 | 0.067662 | 101.433× | 3/3 |
| 10000 | text_substr | duckdb | 0.018541 | 0.099181 | 5.349× | 3/3 |
| 10000 | text_substr | mongo | 0.018541 | 3.102459 | 167.329× | 3/3 |
| 10000 | text_substr | mysql | 0.018541 | 1.260125 | 67.964× | 3/3 |
| 10000 | text_substr | pandas | 0.018541 | 0.482873 | 26.043× | 3/3 |
| 10000 | text_substr | postgres | 0.018541 | 0.733035 | 39.536× | 3/3 |
| 10000 | text_substr | sqlite | 0.018541 | 0.324044 | 17.477× | 3/3 |
| 10000 | materialize | duckdb | 0.377835 | 1.244708 | 3.294× | 3/3 |
| 10000 | materialize | mongo | 0.377835 | 5.511209 | 14.586× | 3/3 |
| 10000 | materialize | mysql | 0.377835 | 10.255917 | 27.144× | 3/3 |
| 10000 | materialize | pandas | 0.377835 | 1.976542 | 5.231× | 3/3 |
| 10000 | materialize | postgres | 0.377835 | 1.858646 | 4.919× | 3/3 |
| 10000 | materialize | sqlite | 0.377835 | 1.846146 | 4.886× | 3/3 |
| 10000 | join_inner | duckdb | 1.338472 | 2.627125 | 1.963× | 3/3 |
| 10000 | join_inner | mongo | 1.338472 | 121.065313 | 90.450× | 3/3 |
| 10000 | join_inner | mysql | 1.338472 | 20.618875 | 15.405× | 3/3 |
| 10000 | join_inner | pandas | 1.338472 | 3.892562 | 2.908× | 3/3 |
| 10000 | join_inner | postgres | 1.338472 | 3.925521 | 2.933× | 3/3 |
| 10000 | join_inner | sqlite | 1.338472 | 4.451333 | 3.326× | 3/3 |
| 10000 | join_filter | duckdb | 0.654351 | 1.405604 | 2.148× | 3/3 |
| 10000 | join_filter | mongo | 0.654351 | 63.193292 | 96.574× | 3/3 |
| 10000 | join_filter | mysql | 0.654351 | 11.159125 | 17.054× | 3/3 |
| 10000 | join_filter | pandas | 0.654351 | 2.242917 | 3.428× | 3/3 |
| 10000 | join_filter | postgres | 0.654351 | 2.270812 | 3.470× | 3/3 |
| 10000 | join_filter | sqlite | 0.654351 | 2.343635 | 3.582× | 3/3 |
| 100000 | point_get | duckdb | 0.000116 | 0.040326 | 347.742× | 3/3 |
| 100000 | point_get | mongo | 0.000116 | 0.485000 | 4182.313× | 3/3 |
| 100000 | point_get | mysql | 0.000116 | 0.346361 | 2986.780× | 3/3 |
| 100000 | point_get | pandas | 0.000116 | 0.004441 | 38.293× | 3/3 |
| 100000 | point_get | postgres | 0.000116 | 0.249565 | 2152.084× | 3/3 |
| 100000 | point_get | sqlite | 0.000116 | 0.000972 | 8.381× | 3/3 |
| 100000 | filter_eq | duckdb | 0.000864 | 0.428421 | 495.960× | 3/3 |
| 100000 | filter_eq | mongo | 0.000864 | 5.168812 | 5983.659× | 3/3 |
| 100000 | filter_eq | mysql | 0.000864 | 3.468605 | 4015.419× | 3/3 |
| 100000 | filter_eq | pandas | 0.000864 | 2.264365 | 2621.335× | 3/3 |
| 100000 | filter_eq | postgres | 0.000864 | 2.405750 | 2785.008× | 3/3 |
| 100000 | filter_eq | sqlite | 0.000864 | 0.672902 | 778.983× | 3/3 |
| 100000 | text_substr | duckdb | 0.204750 | 0.475908 | 2.324× | 3/3 |
| 100000 | text_substr | mongo | 0.204750 | 27.114834 | 132.429× | 3/3 |
| 100000 | text_substr | mysql | 0.204750 | 9.835208 | 48.035× | 3/3 |
| 100000 | text_substr | pandas | 0.204750 | 4.847084 | 23.673× | 3/3 |
| 100000 | text_substr | postgres | 0.204750 | 5.174605 | 25.273× | 3/3 |
| 100000 | text_substr | sqlite | 0.204750 | 3.489270 | 17.042× | 3/3 |
| 100000 | materialize | duckdb | 5.701562 | 12.589209 | 2.208× | 3/3 |
| 100000 | materialize | mongo | 5.701562 | 43.341709 | 7.602× | 3/3 |
| 100000 | materialize | mysql | 5.701562 | 84.143833 | 14.758× | 3/3 |
| 100000 | materialize | pandas | 5.701562 | 17.892958 | 3.138× | 3/3 |
| 100000 | materialize | postgres | 5.701562 | 17.802250 | 3.122× | 3/3 |
| 100000 | materialize | sqlite | 5.701562 | 20.152166 | 3.534× | 3/3 |
| 100000 | join_inner | duckdb | 16.577001 | 29.772125 | 1.796× | 3/3 |
| 100000 | join_inner | mongo | 16.577001 | 1126.935937 | 67.982× | 3/3 |
| 100000 | join_inner | mysql | 16.577001 | 167.935188 | 10.131× | 3/3 |
| 100000 | join_inner | pandas | 16.577001 | 37.379125 | 2.255× | 3/3 |
| 100000 | join_inner | postgres | 16.577001 | 47.305979 | 2.854× | 3/3 |
| 100000 | join_inner | sqlite | 16.577001 | 52.311042 | 3.156× | 3/3 |
| 100000 | join_filter | duckdb | 8.219083 | 14.471750 | 1.761× | 3/3 |
| 100000 | join_filter | mongo | 8.219083 | 573.060000 | 69.723× | 3/3 |
| 100000 | join_filter | mysql | 8.219083 | 90.482604 | 11.009× | 3/3 |
| 100000 | join_filter | pandas | 8.219083 | 19.782042 | 2.407× | 3/3 |
| 100000 | join_filter | postgres | 8.219083 | 22.125979 | 2.692× | 3/3 |
| 100000 | join_filter | sqlite | 8.219083 | 26.343521 | 3.205× | 3/3 |

## native

| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | insert_native | duckdb | 0.764104 | 1.201937 | 1.573× | 3/3 |
| 1000 | insert_native | mongo | 0.764104 | 4.940542 | 6.466× | 3/3 |
| 1000 | insert_native | mysql | 0.764104 | 8.465375 | 11.079× | 3/3 |
| 1000 | insert_native | postgres | 0.764104 | 4.758437 | 6.227× | 3/3 |
| 1000 | insert_native | sqlite | 0.764104 | 1.040979 | 1.362× | 3/3 |
| 10000 | insert_native | duckdb | 7.864792 | 9.726000 | 1.237× | 3/3 |
| 10000 | insert_native | mongo | 7.864792 | 47.392917 | 6.026× | 3/3 |
| 10000 | insert_native | mysql | 7.864792 | 81.892938 | 10.413× | 3/3 |
| 10000 | insert_native | postgres | 7.864792 | 27.618625 | 3.512× | 3/3 |
| 10000 | insert_native | sqlite | 7.864792 | 12.751230 | 1.621× | 3/3 |
| 100000 | insert_native | duckdb | 92.759125 | 101.109708 | 1.090× | 3/3 |
| 100000 | insert_native | mongo | 92.759125 | 579.128521 | 6.243× | 3/3 |
| 100000 | insert_native | mysql | 92.759125 | 859.256792 | 9.263× | 3/3 |
| 100000 | insert_native | postgres | 92.759125 | 283.307271 | 3.054× | 3/3 |
| 100000 | insert_native | sqlite | 92.759125 | 144.587229 | 1.559× | 3/3 |

