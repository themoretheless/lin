# COW vNext paired warm diagnostic

Source6147b9fc5 vs corrected BTreeMap baseline6812cc5ca; release macOS ARM64. Two six-pair series,36fresh processes/3456observations; complete canonical row,indexed count,committed generation checked; Drop inside timing. Independent raw/source recomputation passed.

|Rows|Metric|Series|Gain %|Absolute A/A %|Positive|Pass|
|---|---|---|---:|---:|---:|---|
|1000|rollback_ns|series1|55.491|16.573|6/6|True|
|1000|rollback_ns|series2|59.472|3.714|6/6|True|
|1000|ns_per_update|series1|59.734|3.008|6/6|True|
|1000|ns_per_update|series2|59.923|1.300|6/6|True|
|10000|rollback_ns|series1|62.937|21.065|5/6|False|
|10000|rollback_ns|series2|60.689|13.986|6/6|True|
|10000|ns_per_update|series1|57.624|5.782|6/6|True|
|10000|ns_per_update|series2|60.001|1.549|6/6|True|
|100000|rollback_ns|series1|62.255|9.967|6/6|True|
|100000|rollback_ns|series2|61.662|7.665|6/6|True|
|100000|ns_per_update|series1|60.899|2.247|6/6|True|
|100000|ns_per_update|series2|61.046|1.529|6/6|True|

Overall diagnostic gate:false. All UPDATE windows passed with57.624–61.046%median gains; rollback10k/series1 failed5/6. Repeat2 medians baseline12000ns/control12229ns/candidate25562.5ns give−113.021%gain. Other pairs and all16raw samples are retained; no unique cause is established. No unchanged rerun to obtain pass.

This probe measures repeated prepared mutations on warm fixtures; it does not establish cold/fresh-process latency, complete read/bulk/durable/memory controls or peer superiority. COW remains a next-API development candidate, not a qualified production speedup. Full eight-peer objective remains open.
