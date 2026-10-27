# Single reverse-map write: paired warm index CPU diagnostic

Previous lazy COW vs redundant reverse-write removal, macOS ARM64. Pure insert_at calls; exhaustive forward/reverse membership checks outside timing. No engine/WAL/RSS or peer-superiority claim.

|Case|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---:|---:|---:|---|
|width1/unchanged|series1|0.251|9.136|3/6|True|
|width1/unchanged|series2|1.369|5.043|4/6|True|
|width1/changed|series1|9.807|2.812|6/6|True|
|width1/changed|series2|3.448|3.887|3/6|False|
|width2/unchanged|series1|1.524|6.471|5/6|True|
|width2/unchanged|series2|1.767|3.270|5/6|True|
|width2/changed|series1|7.482|2.680|6/6|True|
|width2/changed|series2|5.034|6.307|6/6|False|
|width3/unchanged|series1|-0.303|2.503|2/6|True|
|width3/unchanged|series2|4.307|2.683|5/6|True|
|width3/changed|series1|11.274|4.551|6/6|True|
|width3/changed|series2|12.913|4.744|5/6|False|
|width8/unchanged|series1|-0.684|13.186|3/6|True|
|width8/unchanged|series2|0.555|2.706|3/6|True|
|width8/changed|series1|26.380|41.471|6/6|False|
|width8/changed|series2|14.211|8.482|5/6|False|

Component gate: False. All 36 processes / 2304 observations retained.

Rejected: 5/8 changed-key target windows failed; all unchanged-key control windows passed. Median gains alone do not qualify this source. Baseline src/index.rs restored byte-for-byte. No unchanged rerun or full-engine/peer claim.
