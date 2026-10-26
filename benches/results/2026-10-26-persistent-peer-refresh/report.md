# Accepted persistent-worker core — available-peer refresh

2026-10-26, frozen source `b7195e2`. Fresh release `peer_bench` built from the accepted core in its managed worktree. No production source edits. Full source/helper/binary hashes, compiler, raw process reports, versions, image IDs/digests/architecture and server settings are retained. Source hashes checked before/after; benchmark helpers unchanged. Builds completed before measurement.

## Scope and results

Warm prepared/API read cases at1k/10k/100k, three independently executed processes per size with rotated engine order and12 samples. Values/columns/multiplicity validated against independently generated expectations; order is not compared. Native insertion uses fresh fixtures and exact six-field readback outside timing; Lin includes default embeddings/FTS. Setup, validation, teardown and Lin JSON IPC are outside timers.

**Read:108/108 aggregate case-size-peer wins,324/324 within-process comparisons positive. Native:15/15 aggregate wins,44/45 within-process comparisons positive.** Read minimum1.723× (100k filtered join versus DuckDB); native minimum1.070× (100k versus DuckDB). These are medians of three process medians under the [benchmark contract](../../../docs/benchmark-contract.md), not universal engine superiority.

Critical retained outlier: native100k process2 Lin143.974ms /DuckDB102.028ms, a loss. Processes1/3 Lin95.346/93.612ms versus DuckDB101.045/102.041ms. Aggregate Lin95.346ms wins, but this three-process sample does not prove tail stability. No causal explanation is inferred and no retry removes this observation.

## Versions and ownership

lin:0.4.0, sqlite:3.54.1, duckdb:1.5.6, postgres:160015, mysql:8.4.11, mongo:8.0.34, pandas:3.0.6.

Three newly created ARM server-image containers on dedicated localhost ports55433/53307/27028 used owned tmpfs benchmark storage. Actual server durability options are in environment.json. Unique benchmark fixtures and unrelated sentinel objects checked after every size; all sentinels unchanged, no linbench_ objects left. Sentinels removed after each mode. Only the three recorded owned container IDs were stopped and removed; cleanup exits all0. Existing containers were not reused.

## Interpretation

Lin uses its prepared Rust API. SQLite/DuckDB/pandas use Python APIs; PostgreSQL/MySQL/MongoDB include their drivers, network round trips and result transfer. Read setup and materialization contracts are fixed; no isolated engine CPU comparison is claimed. Pandas is in-memory and appears only in the read matrix.

Native APIs: Lin prepared Db::empty; SQLite executemany+commit in memory; DuckDB INSERT SELECT from a registered DataFrame; PostgreSQL COPY+commit; MySQL multi-row executemany+commit; MongoDB ordered insert_many with acknowledged w=1,j=false. Server storage is tmpfs. These are successful native API comparisons, not identical durability/failure-atomicity contracts. They do not replace the separate native Rust/Full durable SQLite gate.

Read4536 timing values are calibrated batch averages, not individual-operation latency samples; native648 values use fresh insertion fixtures. Three process repetitions, rather than every timed iteration, define replication. No new baseline/candidate performance improvement is attributed to the persistent worker by this refresh; native/read paths do not use it.

MSSQL/Kusto endpoint-presence flags remain false; neither peer was measured and neither counts as a win. The earlier Full durable10k win is separately documented in the [WAL full-engine report](../2026-10-26-wal-persistent-engine/report.md). Full durable1k still loses SQLite. The complete eight-peer objective remains active and unmet.

## read matrix

| Rows | Case | Peer | Lin ms | Peer ms | Aggregate speedup | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | point_get | duckdb | 0.000115 | 0.041091 | 357.412× | 3/3 |
| 1000 | point_get | mongo | 0.000115 | 0.458597 | 3988.886× | 3/3 |
| 1000 | point_get | mysql | 0.000115 | 0.325796 | 2833.777× | 3/3 |
| 1000 | point_get | pandas | 0.000115 | 0.004456 | 38.754× | 3/3 |
| 1000 | point_get | postgres | 0.000115 | 0.305131 | 2654.030× | 3/3 |
| 1000 | point_get | sqlite | 0.000115 | 0.000936 | 8.141× | 3/3 |
| 1000 | filter_eq | duckdb | 0.000656 | 0.142852 | 217.882× | 3/3 |
| 1000 | filter_eq | mongo | 0.000656 | 0.559440 | 853.270× | 3/3 |
| 1000 | filter_eq | mysql | 0.000656 | 0.377119 | 575.190× | 3/3 |
| 1000 | filter_eq | pandas | 0.000656 | 0.057966 | 88.410× | 3/3 |
| 1000 | filter_eq | postgres | 0.000656 | 0.322208 | 491.439× | 3/3 |
| 1000 | filter_eq | sqlite | 0.000656 | 0.007490 | 11.424× | 3/3 |
| 1000 | text_substr | duckdb | 0.001804 | 0.063675 | 35.291× | 3/3 |
| 1000 | text_substr | mongo | 0.001804 | 0.821406 | 455.259× | 3/3 |
| 1000 | text_substr | mysql | 0.001804 | 0.429504 | 238.050× | 3/3 |
| 1000 | text_substr | pandas | 0.001804 | 0.078921 | 43.741× | 3/3 |
| 1000 | text_substr | postgres | 0.001804 | 0.362075 | 200.678× | 3/3 |
| 1000 | text_substr | sqlite | 0.001804 | 0.032594 | 18.065× | 3/3 |
| 1000 | materialize | duckdb | 0.038870 | 0.299436 | 7.704× | 3/3 |
| 1000 | materialize | mongo | 0.038870 | 1.398472 | 35.978× | 3/3 |
| 1000 | materialize | mysql | 0.038870 | 1.262035 | 32.468× | 3/3 |
| 1000 | materialize | pandas | 0.038870 | 0.401701 | 10.334× | 3/3 |
| 1000 | materialize | postgres | 0.038870 | 0.447113 | 11.503× | 3/3 |
| 1000 | materialize | sqlite | 0.038870 | 0.169495 | 4.361× | 3/3 |
| 1000 | join_inner | duckdb | 0.134047 | 0.371827 | 2.774× | 3/3 |
| 1000 | join_inner | mongo | 0.134047 | 13.201980 | 98.488× | 3/3 |
| 1000 | join_inner | mysql | 0.134047 | 2.359437 | 17.602× | 3/3 |
| 1000 | join_inner | pandas | 0.134047 | 0.779212 | 5.813× | 3/3 |
| 1000 | join_inner | postgres | 0.134047 | 0.669399 | 4.994× | 3/3 |
| 1000 | join_inner | sqlite | 0.134047 | 0.398714 | 2.974× | 3/3 |
| 1000 | join_filter | duckdb | 0.067510 | 0.264256 | 3.914× | 3/3 |
| 1000 | join_filter | mongo | 0.067510 | 7.747604 | 114.762× | 3/3 |
| 1000 | join_filter | mysql | 0.067510 | 1.434139 | 21.243× | 3/3 |
| 1000 | join_filter | pandas | 0.067510 | 0.703057 | 10.414× | 3/3 |
| 1000 | join_filter | postgres | 0.067510 | 0.510729 | 7.565× | 3/3 |
| 1000 | join_filter | sqlite | 0.067510 | 0.205783 | 3.048× | 3/3 |
| 10000 | point_get | duckdb | 0.000114 | 0.041178 | 359.828× | 3/3 |
| 10000 | point_get | mongo | 0.000114 | 0.446279 | 3899.761× | 3/3 |
| 10000 | point_get | mysql | 0.000114 | 0.337304 | 2947.493× | 3/3 |
| 10000 | point_get | pandas | 0.000114 | 0.004556 | 39.814× | 3/3 |
| 10000 | point_get | postgres | 0.000114 | 0.263750 | 2304.750× | 3/3 |
| 10000 | point_get | sqlite | 0.000114 | 0.000966 | 8.442× | 3/3 |
| 10000 | filter_eq | duckdb | 0.000695 | 0.194461 | 279.754× | 3/3 |
| 10000 | filter_eq | mongo | 0.000695 | 1.053026 | 1514.899× | 3/3 |
| 10000 | filter_eq | mysql | 0.000695 | 0.673027 | 968.227× | 3/3 |
| 10000 | filter_eq | pandas | 0.000695 | 0.270822 | 389.608× | 3/3 |
| 10000 | filter_eq | postgres | 0.000695 | 0.489144 | 703.690× | 3/3 |
| 10000 | filter_eq | sqlite | 0.000695 | 0.068358 | 98.341× | 3/3 |
| 10000 | text_substr | duckdb | 0.016377 | 0.105375 | 6.434× | 3/3 |
| 10000 | text_substr | mongo | 0.016377 | 3.558771 | 217.309× | 3/3 |
| 10000 | text_substr | mysql | 0.016377 | 1.379618 | 84.243× | 3/3 |
| 10000 | text_substr | pandas | 0.016377 | 0.493273 | 30.121× | 3/3 |
| 10000 | text_substr | postgres | 0.016377 | 0.811316 | 49.541× | 3/3 |
| 10000 | text_substr | sqlite | 0.016377 | 0.332875 | 20.326× | 3/3 |
| 10000 | materialize | duckdb | 0.389200 | 1.288542 | 3.311× | 3/3 |
| 10000 | materialize | mongo | 0.389200 | 5.597542 | 14.382× | 3/3 |
| 10000 | materialize | mysql | 0.389200 | 10.103583 | 25.960× | 3/3 |
| 10000 | materialize | pandas | 0.389200 | 1.989625 | 5.112× | 3/3 |
| 10000 | materialize | postgres | 0.389200 | 1.665625 | 4.280× | 3/3 |
| 10000 | materialize | sqlite | 0.389200 | 1.910386 | 4.908× | 3/3 |
| 10000 | join_inner | duckdb | 1.371854 | 2.764208 | 2.015× | 3/3 |
| 10000 | join_inner | mongo | 1.371854 | 116.944979 | 85.246× | 3/3 |
| 10000 | join_inner | mysql | 1.371854 | 20.937417 | 15.262× | 3/3 |
| 10000 | join_inner | pandas | 1.371854 | 4.210542 | 3.069× | 3/3 |
| 10000 | join_inner | postgres | 1.371854 | 4.019209 | 2.930× | 3/3 |
| 10000 | join_inner | sqlite | 1.371854 | 4.493250 | 3.275× | 3/3 |
| 10000 | join_filter | duckdb | 0.683375 | 1.488424 | 2.178× | 3/3 |
| 10000 | join_filter | mongo | 0.683375 | 60.047041 | 87.868× | 3/3 |
| 10000 | join_filter | mysql | 0.683375 | 11.178230 | 16.357× | 3/3 |
| 10000 | join_filter | pandas | 0.683375 | 2.466552 | 3.609× | 3/3 |
| 10000 | join_filter | postgres | 0.683375 | 2.604916 | 3.812× | 3/3 |
| 10000 | join_filter | sqlite | 0.683375 | 2.361209 | 3.455× | 3/3 |
| 100000 | point_get | duckdb | 0.000113 | 0.041705 | 368.653× | 3/3 |
| 100000 | point_get | mongo | 0.000113 | 0.469795 | 4152.810× | 3/3 |
| 100000 | point_get | mysql | 0.000113 | 0.345332 | 3052.603× | 3/3 |
| 100000 | point_get | pandas | 0.000113 | 0.004444 | 39.285× | 3/3 |
| 100000 | point_get | postgres | 0.000113 | 0.264476 | 2337.864× | 3/3 |
| 100000 | point_get | sqlite | 0.000113 | 0.000992 | 8.771× | 3/3 |
| 100000 | filter_eq | duckdb | 0.000866 | 0.451968 | 521.802× | 3/3 |
| 100000 | filter_eq | mongo | 0.000866 | 5.351208 | 6178.026× | 3/3 |
| 100000 | filter_eq | mysql | 0.000866 | 3.564855 | 4115.662× | 3/3 |
| 100000 | filter_eq | pandas | 0.000866 | 2.344270 | 2706.485× | 3/3 |
| 100000 | filter_eq | postgres | 0.000866 | 2.492375 | 2877.473× | 3/3 |
| 100000 | filter_eq | sqlite | 0.000866 | 0.685586 | 791.516× | 3/3 |
| 100000 | text_substr | duckdb | 0.206425 | 0.495560 | 2.401× | 3/3 |
| 100000 | text_substr | mongo | 0.206425 | 28.304687 | 137.119× | 3/3 |
| 100000 | text_substr | mysql | 0.206425 | 9.929646 | 48.103× | 3/3 |
| 100000 | text_substr | pandas | 0.206425 | 5.111292 | 24.761× | 3/3 |
| 100000 | text_substr | postgres | 0.206425 | 5.667437 | 27.455× | 3/3 |
| 100000 | text_substr | sqlite | 0.206425 | 3.505104 | 16.980× | 3/3 |
| 100000 | materialize | duckdb | 7.008083 | 12.852917 | 1.834× | 3/3 |
| 100000 | materialize | mongo | 7.008083 | 44.275458 | 6.318× | 3/3 |
| 100000 | materialize | mysql | 7.008083 | 84.388104 | 12.042× | 3/3 |
| 100000 | materialize | pandas | 7.008083 | 19.079313 | 2.722× | 3/3 |
| 100000 | materialize | postgres | 7.008083 | 18.374854 | 2.622× | 3/3 |
| 100000 | materialize | sqlite | 7.008083 | 21.005333 | 2.997× | 3/3 |
| 100000 | join_inner | duckdb | 16.661188 | 29.611355 | 1.777× | 3/3 |
| 100000 | join_inner | mongo | 16.661188 | 1169.392188 | 70.187× | 3/3 |
| 100000 | join_inner | mysql | 16.661188 | 172.402729 | 10.348× | 3/3 |
| 100000 | join_inner | pandas | 16.661188 | 38.359229 | 2.302× | 3/3 |
| 100000 | join_inner | postgres | 16.661188 | 48.391354 | 2.904× | 3/3 |
| 100000 | join_inner | sqlite | 16.661188 | 52.486917 | 3.150× | 3/3 |
| 100000 | join_filter | duckdb | 8.432250 | 14.525875 | 1.723× | 3/3 |
| 100000 | join_filter | mongo | 8.432250 | 615.432229 | 72.986× | 3/3 |
| 100000 | join_filter | mysql | 8.432250 | 91.518354 | 10.853× | 3/3 |
| 100000 | join_filter | pandas | 8.432250 | 19.947751 | 2.366× | 3/3 |
| 100000 | join_filter | postgres | 8.432250 | 22.672417 | 2.689× | 3/3 |
| 100000 | join_filter | sqlite | 8.432250 | 26.318562 | 3.121× | 3/3 |

## native matrix

| Rows | Case | Peer | Lin ms | Peer ms | Aggregate speedup | Positive processes |
|---:|---|---|---:|---:|---:|---:|
| 1000 | insert_native | duckdb | 0.785958 | 1.270083 | 1.616× | 3/3 |
| 1000 | insert_native | mongo | 0.785958 | 5.265625 | 6.700× | 3/3 |
| 1000 | insert_native | mysql | 0.785958 | 8.895354 | 11.318× | 3/3 |
| 1000 | insert_native | postgres | 0.785958 | 5.064625 | 6.444× | 3/3 |
| 1000 | insert_native | sqlite | 0.785958 | 1.044250 | 1.329× | 3/3 |
| 10000 | insert_native | duckdb | 8.234709 | 9.952292 | 1.209× | 3/3 |
| 10000 | insert_native | mongo | 8.234709 | 47.142938 | 5.725× | 3/3 |
| 10000 | insert_native | mysql | 8.234709 | 86.566125 | 10.512× | 3/3 |
| 10000 | insert_native | postgres | 8.234709 | 28.986146 | 3.520× | 3/3 |
| 10000 | insert_native | sqlite | 8.234709 | 13.053959 | 1.585× | 3/3 |
| 100000 | insert_native | duckdb | 95.345895 | 102.028000 | 1.070× | 2/3 |
| 100000 | insert_native | mongo | 95.345895 | 598.321292 | 6.275× | 3/3 |
| 100000 | insert_native | mysql | 95.345895 | 894.742479 | 9.384× | 3/3 |
| 100000 | insert_native | postgres | 95.345895 | 332.624167 | 3.489× | 3/3 |
| 100000 | insert_native | sqlite | 95.345895 | 145.946896 | 1.531× | 3/3 |
