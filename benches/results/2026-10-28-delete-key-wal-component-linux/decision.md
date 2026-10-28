# Full DELETE key projection: rejected component

Run40619276900,frozen e83aa54a3. Independent frozen-source/raw audit passed:288fresh processes/2304observations,453Linux release tests/2ignored,16fresh correctness smokes,8exact production golden cases andnumeric canonical-limit bypass mutant rejection. Original size guard andallsource restoration verified.

The frozen gate failed2of4Full DELETE target windows and1rollback control window. No production adoption or unchanged rerun.

|Case|Series|Gain %|Absolute A/A %|Positive pairs|Pass|
|---|---|---:|---:|---:|---:|---|
|compare/durable_delete_1row_1k/lin|series1|11.6717|4.7485|5/6|False|
|compare/durable_delete_1row_1k/lin|series2|20.5198|3.4583|6/6|True|
|compare/durable_delete_1row_10k/lin|series1|9.4010|3.0435|5/6|False|
|compare/durable_delete_1row_10k/lin|series2|17.3426|4.6679|6/6|True|
|compare/durable_update_1row_1k/lin|series1|-0.5694|5.2138|3/6|True|
|compare/durable_update_1row_1k/lin|series2|4.2966|4.0773|6/6|True|
|compare/durable_commit_1row_10k/lin|series1|0.6883|2.9869|4/6|True|
|compare/durable_commit_1row_10k/lin|series2|5.6623|6.2941|5/6|True|
|compare/durable_rollback_1row_1k/lin|series1|-1.4532|3.2493|2/6|True|
|compare/durable_rollback_1row_1k/lin|series2|-2.3582|1.6884|2/6|False|
|compare/wal_scalar_full_update/lin|series1|3.9566|2.9626|5/6|True|
|compare/wal_scalar_full_update/lin|series2|1.0274|4.3608|5/6|True|
|compare/wal_scalar_normal_update/lin|series1|-0.8155|1.8091|0/6|True|
|compare/wal_scalar_normal_update/lin|series2|-0.3746|0.8063|2/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series1|1.6645|5.0416|4/6|True|
|compare/durable_delete_1row_1k/sqlite_wal|series2|1.1217|2.2722|4/6|True|

DELETE1k gains11.6717%/20.5198% had5/6and6/6positive pairs;DELETE10k9.4010%/17.3426% had5/6and6/6. The failing target medians exceed A/A,butnot every pair improved. Rollback1k controlseries2failed-2.3582%againstA/A1.6884%. No unique cause is established;failed pairs/startup observations remain intact.

Canonical-size,normal-byte preservation,full result/undo/index/FTS/reopen tests andbounded old-reader compatibility remain correctness evidence. Smaller physical frames andpositive medians do not override the source acceptance gate. The current allocation-free-index contender already excludes this DELETE core change;exactcandidate remains recoverable in Git/raw assets.

The Linux filesystem/mount report is retained. API Full synchronization was not relaxed;no hardware power-loss durability qualification is claimed. Full-source/eight-peer acceptance remains false. Every artifact member andarchive hash was verified.
