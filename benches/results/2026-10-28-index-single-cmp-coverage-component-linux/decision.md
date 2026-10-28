# Single-Cmp DNF bypass: rejected component

Run40617087906,frozen81d75c242. Independent frozen-source/raw audit passed:288fresh processes/2304observations,451Linux release tests/2ignored and16fresh correctness smokes. One target window andoneunchanged DuckDBcontrol window failed. No production adoption or unchanged rerun.

|Case|Series|Gain %|Absolute A/A %|Positive pairs|Pass|
|---|---|---:|---:|---:|---|
|compare/filter_eq/lin|series1|6.2789|1.9311|6/6|True|
|compare/filter_eq/lin|series2|6.6213|2.5986|5/6|False|
|compare/filter_range/lin|series1|2.4983|2.2728|6/6|True|
|compare/filter_range/lin|series2|1.8379|1.6150|5/6|True|
|compare/point_get/lin|series1|-2.0730|3.2709|2/6|True|
|compare/point_get/lin|series2|-0.1251|1.8165|3/6|True|
|compare/text_substr/lin|series1|0.7035|0.2348|5/6|True|
|compare/text_substr/lin|series2|1.1297|0.1697|6/6|True|
|compare/update_1row_1k/lin|series1|3.3629|3.2717|5/6|True|
|compare/update_1row_1k/lin|series2|-0.2677|2.8259|3/6|True|
|compare/wal_scalar_full_update/lin|series1|-1.0457|9.3042|2/6|True|
|compare/wal_scalar_full_update/lin|series2|2.1819|3.7621|4/6|True|
|compare/filter_eq/sqlite|series1|-0.6549|1.3771|2/6|True|
|compare/filter_eq/sqlite|series2|0.8612|1.5436|4/6|True|
|compare/filter_eq/duckdb|series1|-4.1071|2.9475|2/6|False|
|compare/filter_eq/duckdb|series2|0.5242|4.7934|4/6|True|

filter_eq gained6.2789%/6.6213% against A/A1.9311%/2.5986%. Series1passed6/6,butseries2failed5/6. DuckDB filter_eq controlseries1failed-4.1071%againstA/A2.9475%. No unique cause is established;retain every pair/startup sample. These median gains do not override the frozen gate.

The current DELETE-WAL contender excludes this index optimization andretains baseline index coverage. The experimental source/reference tests/mutant evidence remain recoverable. A future distinct candidate would need its own correctness andpaired qualification;this run must not be repeated unchanged for a pass. Full-source/eight-peer acceptance remains false.

Every raw artifact member andarchive hash was verified;source/build/test/smoke data remain in raw-artifact.tar.gz.
