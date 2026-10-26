# Prepared grouped COUNT dispatch: current CI observation

Run40273448710, frozen97f54abd, Linux x86, Rust1.96.0. The real quick benchmark
reported927335ns/op for `compare/filter_eq/lin`, versus152543ns SQLite and299648ns
DuckDB. The gate failed; full failed-step log is retained. It is one CI process,
not independent performance qualification. No raw run.json artifact was uploaded
by the old workflow after failure.

`benches/compare.rs` prepares `docs | wing == "rag" | count by wing`.
`Db::execute_agg_plan` reuses `execute_filter_count` only for `by: None`.
Grouped filtered COUNT therefore reaches generic row evaluation/grouping even
when an index covers the filter and fixes the group key. The ordinary helper
already contains index-covered grouped logic. This confirms a dispatch omission;
its exact share of elapsed CI time has not been profiled.

Any proposed shared dispatch must preserve generic aggregate quota enforcement.
The existing helper does not enforce `max_aggregate_bytes` for all grouped paths;
blindly routing every grouped prepared query through it can introduce a quota
regression. First scope an index-covered fixed-text-group path with exact quota
accounting, empty-set behavior and independent expected results. Keep residual,
numeric/null group and many-group fallbacks intact unless separately qualified.

The checked-in budget uses old macOS ARM measurements as absolute Linux limits.
Do not raise limits merely to hide this failure. Cross-host budget comparability
and the separately failing join case require their own evidence.
