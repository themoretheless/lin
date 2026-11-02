# Package codegen-units=1: local discovery result

216 fresh macOS processes/1728 observations,24 smokes, complete balanced six-round plan. Independent raw/hash/command/order/numerical audit passed. Fixed runtime625bc7180; no FTS candidate combined. This is one local series and not Linux performance qualification.

|Case|Gain %|Absolute A/A %|Positive pairs|
|---|---:|---:|---:|
|compare/update_1row_1k/lin|-3.576|17.365|3/6|
|compare/update_1row_10k/lin|22.443|4.325|6/6|
|compare/update_1row_100k/lin|6.056|7.354|4/6|
|compare/tx_rollback_1row_1k/lin|13.899|11.091|4/6|
|compare/tx_rollback_1row_10k/lin|6.170|5.896|5/6|
|compare/tx_rollback_1row_100k/lin|-5.289|5.580|2/6|
|compare/insert_bulk_10k/lin|6.480|11.413|5/6|
|compare/delete_1row_10k/lin|0.700|8.038|3/6|
|compare/fts_lex_selective/lin|0.879|7.796|3/6|
|compare/agg_count_by/duckdb|8.672|36.124|3/6|
|compare/durable_update_1row_10k/lin|-9.150|44.046|3/6|
|compare/durable_commit_1row_1k/lin|-6.456|5.199|1/6|

UPDATE10k is the only one of the six warm UPDATE/ROLLBACK cases with6/6 positive pairs and median gain above median absolute A/A. Other cases are mixed or below the local noise estimate. FULL COMMIT1k median gain is -6.456%, absolute A/A5.199%,1/6 positive pairs. This is descriptive, not a retroactively introduced acceptance gate.

No production profile change, broad performance claim or Linux promotion is supported by this local series. All cases and observations are retained, including unfavourable outcomes. Full release correctness for this alternate compiler profile was not rerun. No unchanged rerun was launched.
