# Two-cell inline UPDATE component comparison

macOS ARM64, same frozen compare harness; previous COW baseline versus two-cell inline candidate. No RSS claim. Full engine and eight-peer goal remain unqualified.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|compare/wal_scalar_full_update/lin|series1|-19.888|31.278|1/6|False|
|compare/wal_scalar_full_update/lin|series2|12.705|9.280|4/6|False|
|compare/wal_scalar_normal_update/lin|series1|6.673|10.801|4/6|False|
|compare/wal_scalar_normal_update/lin|series2|2.723|17.163|4/6|False|
|compare/update_1row_1k/lin|series1|8.150|6.901|4/6|True|
|compare/update_1row_1k/lin|series2|15.528|16.919|5/6|True|
|compare/update_1row_100k/lin|series1|9.170|14.369|4/6|True|
|compare/update_1row_100k/lin|series2|-0.005|7.502|3/6|True|

Component gate: False. All 144 processes / 1152 wall observations retained.

Rejected and baseline src/exec.rs restored byte-for-byte. All four scalar WAL target windows failed; controls passed. A/A variability is large, so no unique causal explanation or reliable speedup is established. No unchanged rerun or production adoption.
