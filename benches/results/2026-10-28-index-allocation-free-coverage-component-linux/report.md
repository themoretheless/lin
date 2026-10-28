# Allocation-free atomic index coverage component comparison

Linux x86_64, same frozen compare harness; previous COW+CAS baseline versus allocation-free atomic coverage candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/filter_eq/lin|series1|10.676|2.207|6/6|True|
|compare/filter_eq/lin|series2|9.739|1.251|6/6|True|
|compare/filter_range/lin|series1|2.476|4.818|5/6|True|
|compare/filter_range/lin|series2|-2.104|4.576|0/6|True|
|compare/point_get/lin|series1|-5.130|2.008|0/6|False|
|compare/point_get/lin|series2|-2.851|3.987|3/6|True|
|compare/text_substr/lin|series1|0.966|0.582|5/6|True|
|compare/text_substr/lin|series2|0.503|0.505|5/6|True|
|compare/update_1row_1k/lin|series1|10.665|6.611|5/6|True|
|compare/update_1row_1k/lin|series2|2.049|8.750|3/6|True|
|compare/wal_scalar_full_update/lin|series1|-0.979|10.143|3/6|True|
|compare/wal_scalar_full_update/lin|series2|-4.200|6.859|1/6|True|
|compare/filter_eq/sqlite|series1|0.383|1.862|4/6|True|
|compare/filter_eq/sqlite|series2|-0.269|0.772|3/6|True|
|compare/filter_eq/duckdb|series1|-0.742|14.947|1/6|True|
|compare/filter_eq/duckdb|series2|-2.805|1.435|1/6|False|

Component gate: False. All 288 processes / 2304 wall observations retained.
