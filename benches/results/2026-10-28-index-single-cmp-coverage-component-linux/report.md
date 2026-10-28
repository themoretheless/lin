# Single-comparison index coverage component comparison

Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus single-comparison coverage candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/filter_eq/lin|series1|6.279|1.931|6/6|True|
|compare/filter_eq/lin|series2|6.621|2.599|5/6|False|
|compare/filter_range/lin|series1|2.498|2.273|6/6|True|
|compare/filter_range/lin|series2|1.838|1.615|5/6|True|
|compare/point_get/lin|series1|-2.073|3.271|2/6|True|
|compare/point_get/lin|series2|-0.125|1.816|3/6|True|
|compare/text_substr/lin|series1|0.704|0.235|5/6|True|
|compare/text_substr/lin|series2|1.130|0.170|6/6|True|
|compare/update_1row_1k/lin|series1|3.363|3.272|5/6|True|
|compare/update_1row_1k/lin|series2|-0.268|2.826|3/6|True|
|compare/wal_scalar_full_update/lin|series1|-1.046|9.304|2/6|True|
|compare/wal_scalar_full_update/lin|series2|2.182|3.762|4/6|True|
|compare/filter_eq/sqlite|series1|-0.655|1.377|2/6|True|
|compare/filter_eq/sqlite|series2|0.861|1.544|4/6|True|
|compare/filter_eq/duckdb|series1|-4.107|2.947|2/6|False|
|compare/filter_eq/duckdb|series2|0.524|4.793|4/6|True|

Component gate: False. All 288 processes / 2304 wall observations retained.
