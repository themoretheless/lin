# Owned COW undo: rejected warm experiment

Exact705dbd035 candidate vs prior COWa9d50c155.36fresh processes/3456observations. Independent raw/source recomputation passed.

|Rows|Metric|Series|Gain %|A/A %|Positive|Pass|
|---|---|---|---:|---:|---:|---|
|1000|rollback_ns|series1|-11.012|9.613|0/6|False|
|1000|rollback_ns|series2|-3.646|11.442|2/6|False|
|1000|ns_per_update|series1|0.710|2.381|5/6|True|
|1000|ns_per_update|series2|-0.107|4.225|3/6|True|
|10000|rollback_ns|series1|-7.336|2.944|1/6|False|
|10000|rollback_ns|series2|-9.264|8.953|1/6|False|
|10000|ns_per_update|series1|-0.611|1.564|1/6|True|
|10000|ns_per_update|series2|-1.447|1.650|1/6|True|
|100000|rollback_ns|series1|-0.538|7.506|2/6|False|
|100000|rollback_ns|series2|-9.760|16.883|0/6|False|
|100000|ns_per_update|series1|-1.920|1.472|1/6|True|
|100000|ns_per_update|series2|-1.345|3.683|2/6|True|

All6rollback target windows failed, all6UPDATE control windows passed. No accepted acceleration. Source optimization is reverted; its tests and exact source remain in705dbd035 and all raw records retained here.

Source inspection found that Transaction rollback/Drop clears uncommitted packs after undo; these packs can retain updated Rows during undo. This is a plausible ownership mechanism, not an established cause of the measured slowdown. Future hypotheses must separately verify ownership/counters and complete lifecycle timing. No unchanged rerun, no dropped values or relaxed sync. Full eight-peer objective remains open.
