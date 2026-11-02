# ThinLTO: mixed local discovery, no adoption

Fixed625 runtime and exact original harness; global release ThinLTO only, no package-codegen override or rejected FTS changes.216 fresh macOS processes/1728 observations,24 smokes, one balanced six-round series. All18 external DuckDB labels use the same default-profile binary; source, profile compiler flag, binary hashes, exact commands, ordering and independent raw numerical audit passed.

|Case|Gain %|Absolute A/A %|Positive pairs|
|---|---:|---:|---:|
|compare/update_1row_1k/lin|-7.601|42.496|2/6|
|compare/update_1row_10k/lin|-10.209|11.657|2/6|
|compare/update_1row_100k/lin|10.143|8.766|5/6|
|compare/tx_rollback_1row_1k/lin|15.084|43.481|5/6|
|compare/tx_rollback_1row_10k/lin|6.421|10.320|4/6|
|compare/tx_rollback_1row_100k/lin|5.025|8.712|4/6|
|compare/insert_bulk_10k/lin|3.782|10.707|4/6|
|compare/delete_1row_10k/lin|-7.729|3.886|3/6|
|compare/fts_lex_selective/lin|0.921|6.120|3/6|
|compare/agg_count_by/duckdb|-8.954|21.387|2/6|
|compare/durable_update_1row_10k/lin|-9.434|11.013|1/6|
|compare/durable_commit_1row_1k/lin|3.117|11.945|4/6|

None of the six warm UPDATE/ROLLBACK cases has6/6 positive pairs above absolute A/A. Several local A/A medians are large (up to43.481%). UPDATE100k +10.143% is5/6 positive; UPDATE1k/10k medians are negative. DELETE10k is -7.729% with A/A3.886%. Even the invariant DuckDB binary has a -8.954% role contrast with A/A21.387%, so local role noise is retained explicitly. This does not retrospectively explain or waive any previous linked-binary Linux failure.

No broad local benefit or production profile change is supported. Linux behaviour of this alternate profile remains unqualified; local ARM observations cannot prove an x86 Linux regression. No fresh full release correctness qualification of ThinLTO was run. No unchanged acceptance retry. Every case and unfavourable outcome is retained.
