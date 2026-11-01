# UPDATE patch clone elision: target gains, overall gate rejected

Frozen source 0ffe738f4, runtime candidate ed3098197, corrected reference21dcd7d81. The first infrastructure attempt41249170021 has zero timing observations and remains archived separately. Retry41251433614 changes checkout history availability; runtime, harness, samples and thresholds remain identical.

All504 processes/4032 observations completed. Independent source/observation audit passed:455 release tests/two ignored,28 fresh Linux correctness smokes, two separately compiled hash-guard runtime negatives, exact role-specific source manifests and original-source restoration. All six native UPDATE target windows pass with six positive pairs and median gain above absolute A/A:32.295–46.902%.

The full component gate fails on DuckDB filter_eq series1: -11.684% gain, absolute A/A8.412%, two positive pairs of six. Its cause is not established. The Lin UPDATE path is not called by this control. This does not justify discarding its measurements, waiving the failure, or retrying unchanged source. All eighteen Lin control windows pass the frozen tolerance; that is not a claim of zero numerical regressions.

|Case|Series|Target|Gain %|Absolute A/A %|Positive|Gate|
|---|---|---|---:|---:|---:|---|
|compare/update_1row_1k/lin|series1|True|46.902|16.579|6/6|PASS|
|compare/update_1row_1k/lin|series2|True|40.511|15.262|6/6|PASS|
|compare/update_1row_10k/lin|series1|True|32.295|1.318|6/6|PASS|
|compare/update_1row_10k/lin|series2|True|34.001|4.899|6/6|PASS|
|compare/update_1row_100k/lin|series1|True|37.177|4.815|6/6|PASS|
|compare/update_1row_100k/lin|series2|True|34.765|4.509|6/6|PASS|
|compare/durable_update_1row_1k/lin|series1|False|7.081|9.777|5/6|PASS|
|compare/durable_update_1row_1k/lin|series2|False|1.525|3.486|5/6|PASS|
|compare/durable_update_1row_10k/lin|series1|False|2.057|6.946|4/6|PASS|
|compare/durable_update_1row_10k/lin|series2|False|5.907|5.474|6/6|PASS|
|compare/durable_delete_1row_10k/lin|series1|False|4.106|2.193|5/6|PASS|
|compare/durable_delete_1row_10k/lin|series2|False|3.130|4.162|5/6|PASS|
|compare/durable_commit_1row_10k/lin|series1|False|1.198|2.694|4/6|PASS|
|compare/durable_commit_1row_10k/lin|series2|False|-3.739|5.135|2/6|PASS|
|compare/durable_rollback_1row_10k/lin|series1|False|1.917|1.410|5/6|PASS|
|compare/durable_rollback_1row_10k/lin|series2|False|1.799|2.669|4/6|PASS|
|compare/wal_scalar_full_update/lin|series1|False|3.068|3.551|5/6|PASS|
|compare/wal_scalar_full_update/lin|series2|False|2.186|4.933|5/6|PASS|
|compare/wal_scalar_normal_update/lin|series1|False|31.681|6.310|6/6|PASS|
|compare/wal_scalar_normal_update/lin|series2|False|33.397|7.629|6/6|PASS|
|compare/insert_bulk_10k/lin|series1|False|-1.245|3.026|2/6|PASS|
|compare/insert_bulk_10k/lin|series2|False|-0.246|1.896|2/6|PASS|
|compare/filter_eq/lin|series1|False|-0.028|2.219|3/6|PASS|
|compare/filter_eq/lin|series2|False|-0.264|3.280|3/6|PASS|
|compare/filter_eq/duckdb|series1|False|-11.684|8.412|2/6|FAIL|
|compare/filter_eq/duckdb|series2|False|0.242|3.236|3/6|PASS|
|compare/update_1row_1k/sqlite|series1|False|1.404|6.628|3/6|PASS|
|compare/update_1row_1k/sqlite|series2|False|-2.161|3.612|2/6|PASS|

Percentages compare each engine case against its own baseline-role executable. The DuckDB control is not a Lin/DuckDB peer ratio. There is no memory measurement, whole-engine acceptance or eight-peer win. All observations, including the losing DuckDB pairs, remain in the SHA256-verified archive.

This runtime predates the separately reproduced schema-rollback defects. Its target gains do not qualify production adoption. The next candidate must include the Linux-qualified checkpoint correction79a4cb837 and receive fresh integration/correctness/performance qualification; no multiplication of separate component gains is valid.
