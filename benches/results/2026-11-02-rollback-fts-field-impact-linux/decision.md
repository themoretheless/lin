# Rollback FTS field-impact: rejected

Run [41366596325](https://github.com/themoretheless/lin/actions/runs/41366596325), frozen47d88985bc8f2010cc6272835d276ba6ed655056. Runtime baseline625bc7180 versus candidate65bc82590. All720 processes /5760 observations completed. Fresh460 passed/2 ignored, both compiled negatives, restored focused tests,40 smokes, balanced order and independent audit passed. Only the unchanged component acceptance step failed. Local raw recomputation matches; all3849 archived files and both exact Linux executable hashes verified.

All ten target windows pass with6/6 positive pairs above absolute A/A: warm rollback1k +27.311/+26.743%;10k +23.499/+23.771%;100k +24.223/+23.464%; durable rollback1k +26.207/+24.320%;10k +19.862/+20.110%.

Eight control windows fail:

| Case | Series | Gain % | Absolute A/A % | Positive pairs |
| --- | --- | ---: | ---: | ---: |
|compare/update_1row_10k/lin|series1|-2.943|2.757|2/6|
|compare/durable_delete_1row_1k/lin|series2|-4.884|2.228|0/6|
|compare/wal_scalar_full_update/lin|series2|-10.830|5.226|2/6|
|compare/wal_scalar_normal_update/lin|series2|-2.479|2.393|2/6|
|compare/delete_1row_10k/lin|series2|-3.096|2.433|1/6|
|compare/insert_bulk_10k/lin|series1|-4.580|4.380|2/6|
|compare/agg_count_by/duckdb|series1|-14.757|5.880|2/6|
|compare/agg_count_by/duckdb|series2|-7.031|3.715|3/6|

The failed controls include warm UPDATE10k, durable DELETE1k, scalar FULL/NORMAL, warm DELETE10k, bulk INSERT10k and both DuckDB GROUP BY series. Causes are unproven; no noise waiver or unchanged acceptance retry.

In this run only, candidate rollback1k has Lin/SQLite WAL median paired ratio0.699008 and12/12 numeric wins. Rollback10k remains slower: ratio1.370044 and0/12 wins. This is24 matched rollback pairs, not a fresh complete SQLite durability matrix. Rollback itself synchronizes no WAL in either engine. No cross-run latency ratio is used as causal evidence.

Candidate remains rejected and unadopted. Whole engine and eight-peer objective remain open. The separate package codegen-units experiment uses fixed625 source and does not combine this rejected FTS change.
