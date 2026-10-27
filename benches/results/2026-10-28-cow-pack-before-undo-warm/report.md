# Pack-before-undo COW: rejected warm gate

Exactf6c99ccbd vs prior COWea84c2132.36processes/3456observations; independent recomputation passed. All6rollback windows failed;3UPDATE control windows failed. A/A is large in several windows; no unique cause is established. No unchanged rerun and no accepted acceleration.

|Rows|Metric|Series|Gain %|A/A %|Positive|Pass|
|---|---|---|---:|---:|---:|---|
|1000|rollback_ns|series1|-3.922|7.744|3/6|False|
|1000|rollback_ns|series2|-54.238|34.249|2/6|False|
|1000|ns_per_update|series1|-1.355|1.643|2/6|True|
|1000|ns_per_update|series2|-24.494|2.547|2/6|False|
|10000|rollback_ns|series1|-7.309|20.960|3/6|False|
|10000|rollback_ns|series2|-157.168|76.674|2/6|False|
|10000|ns_per_update|series1|-4.022|1.560|1/6|False|
|10000|ns_per_update|series2|-18.860|6.735|1/6|False|
|100000|rollback_ns|series1|-20.359|38.952|2/6|False|
|100000|rollback_ns|series2|4.231|40.720|4/6|False|
|100000|ns_per_update|series1|-4.165|4.652|0/6|True|
|100000|ns_per_update|series2|-2.761|10.498|2/6|True|

The new ownership mechanism was motivated by actual Arc counts, but that evidence does not establish useful latency. Source changes are reverted; complete raw/build/source hashes retained. Client-held rollback/Drop regression remains. Complete eight-peer objective remains open.
