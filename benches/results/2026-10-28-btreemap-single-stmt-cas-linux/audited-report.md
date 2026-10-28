# BTreeMap single-statement CAS snapshot component comparison

Platform: Linux-6.17.0-1022-azure-x86_64-with-glibc2.39; same frozen compare harness; corrected BTreeMap baseline versus single-statement snapshot-elision candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/wal_scalar_full_update/lin|series1|41.383|2.827|5/6|False|
|compare/wal_scalar_full_update/lin|series2|42.047|2.921|6/6|True|
|compare/wal_scalar_normal_update/lin|series1|93.836|2.416|6/6|True|
|compare/wal_scalar_normal_update/lin|series2|93.597|1.640|6/6|True|
|compare/update_1row_1k/lin|series1|3.082|3.368|4/6|True|
|compare/update_1row_1k/lin|series2|-1.775|6.792|2/6|True|
|compare/update_1row_100k/lin|series1|-1.346|5.180|2/6|True|
|compare/update_1row_100k/lin|series2|-0.470|2.690|1/6|True|

Component gate: False. All 144 processes / 1152 wall observations retained.
