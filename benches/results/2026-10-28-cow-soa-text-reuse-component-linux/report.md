# SoA unchanged-text reuse component comparison

Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus SoA text-reuse candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/update_1row_1k/lin|series1|4.824|3.889|5/6|False|
|compare/update_1row_1k/lin|series2|2.704|2.110|4/6|False|
|compare/update_1row_10k/lin|series1|-0.424|3.572|2/6|False|
|compare/update_1row_10k/lin|series2|3.343|1.040|5/6|False|
|compare/update_1row_100k/lin|series1|11.524|8.917|6/6|True|
|compare/update_1row_100k/lin|series2|9.432|9.111|4/6|False|
|compare/wal_scalar_full_update/lin|series1|1.598|1.826|4/6|True|
|compare/wal_scalar_full_update/lin|series2|-0.832|1.289|1/6|True|
|compare/wal_scalar_normal_update/lin|series1|-1.009|1.994|0/6|True|
|compare/wal_scalar_normal_update/lin|series2|-0.223|2.024|2/6|True|
|compare/delete_1row_1k/lin|series1|-3.663|2.700|2/6|False|
|compare/delete_1row_1k/lin|series2|-6.051|2.487|0/6|False|
|compare/tx_rollback_1row_1k/lin|series1|-1.105|2.885|3/6|True|
|compare/tx_rollback_1row_1k/lin|series2|2.444|3.631|5/6|True|
|compare/tx_rollback_1row_100k/lin|series1|-0.700|5.710|3/6|True|
|compare/tx_rollback_1row_100k/lin|series2|4.368|5.487|5/6|True|
|compare/update_1row_1k/sqlite|series1|-2.064|4.343|2/6|True|
|compare/update_1row_1k/sqlite|series2|-1.431|2.785|3/6|True|

Component gate: False. All 324 processes / 2592 wall observations retained.
