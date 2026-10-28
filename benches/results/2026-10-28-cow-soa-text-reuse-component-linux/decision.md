# SoA text reuse: rejected component

Run40615737232,frozen e23c6d6a2. Independent frozen-source/raw audit passed for all324fresh processes/2592observations.453Linux release tests/2ignored and18fresh correctness smokes passed. The frozen gate failed5of6target windows and2control windows. No production adoption or unchanged rerun.

|Case|Series|Gain %|Absolute A/A %|Positive pairs|Pass|
|---|---|---:|---:|---:|---|
|compare/update_1row_1k/lin|series1|4.8239|3.8891|5/6|False|
|compare/update_1row_1k/lin|series2|2.7044|2.1102|4/6|False|
|compare/update_1row_10k/lin|series1|-0.4240|3.5722|2/6|False|
|compare/update_1row_10k/lin|series2|3.3426|1.0404|5/6|False|
|compare/update_1row_100k/lin|series1|11.5244|8.9174|6/6|True|
|compare/update_1row_100k/lin|series2|9.4317|9.1113|4/6|False|
|compare/wal_scalar_full_update/lin|series1|1.5984|1.8261|4/6|True|
|compare/wal_scalar_full_update/lin|series2|-0.8316|1.2888|1/6|True|
|compare/wal_scalar_normal_update/lin|series1|-1.0093|1.9939|0/6|True|
|compare/wal_scalar_normal_update/lin|series2|-0.2229|2.0242|2/6|True|
|compare/delete_1row_1k/lin|series1|-3.6627|2.7002|2/6|False|
|compare/delete_1row_1k/lin|series2|-6.0513|2.4868|0/6|False|
|compare/tx_rollback_1row_1k/lin|series1|-1.1048|2.8847|3/6|True|
|compare/tx_rollback_1row_1k/lin|series2|2.4436|3.6308|5/6|True|
|compare/tx_rollback_1row_100k/lin|series1|-0.7002|5.7103|3/6|True|
|compare/tx_rollback_1row_100k/lin|series2|4.3682|5.4867|5/6|True|
|compare/update_1row_1k/sqlite|series1|-2.0640|4.3431|2/6|True|
|compare/update_1row_1k/sqlite|series2|-1.4306|2.7847|3/6|True|

Native UPDATE1k gains4.8239%/2.7044% had5/6and4/6positive pairs;UPDATE10k-0.4240%/3.3426% had2/6and5/6. UPDATE100k11.5244% passed series1;series2gain9.4317% withA/A9.1113% failed4/6positive pairs. Native DELETE1k controls failed-3.6627%/-6.0513%,withA/A2.7002%/2.4868%. No unique cause is established; failed pairs and startup observations remain intact.

The separate System allocator diagnostic showed unchanged-text synchronization dropping from3000to0allocations per1000calls. That result remains valid for the isolated method, but does not qualify DB throughput. Runtime mutant checks also remain correctness evidence only.

This current branch excludes the SoA core change and retains the prior text synchronization implementation while the separate single-Cmp candidate is under measurement. The experimental SoA branch and exact candidate sources remain recoverable in Git/raw assets. Do not combine this rejected candidate into a full-source contender based on allocator counts alone.

All raw observations,source/build/test/smoke assets and protocol are in raw-artifact.tar.gz;every member hash and archive digest were verified. The eight-peer objective remains incomplete.
