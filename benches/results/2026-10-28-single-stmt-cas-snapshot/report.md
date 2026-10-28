# Single-statement CAS snapshot component comparison

macOS ARM64, same frozen compare harness; previous COW baseline versus single-statement snapshot-elision candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/wal_scalar_full_update/lin|series1|31.932|13.723|6/6|True|
|compare/wal_scalar_full_update/lin|series2|35.768|3.419|6/6|True|
|compare/wal_scalar_normal_update/lin|series1|91.371|1.839|6/6|True|
|compare/wal_scalar_normal_update/lin|series2|91.663|1.738|6/6|True|
|compare/update_1row_1k/lin|series1|6.569|22.486|4/6|True|
|compare/update_1row_1k/lin|series2|12.424|13.472|5/6|True|
|compare/update_1row_100k/lin|series1|-6.428|8.589|1/6|True|
|compare/update_1row_100k/lin|series2|-3.945|5.967|3/6|True|

Component gate: True. All 144 processes / 1152 wall observations retained.
