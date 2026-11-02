# Invariant-peer full matrix: rejected

Run [41371921811](https://github.com/themoretheless/lin/actions/runs/41371921811), frozene108fac78f6766470505e19fb169190114659d19. Same e313 correctness-aligned BTreeMap versus625 corrected COW source pair. All67 cases,2412 fresh processes/19296 observations and134 actual assigned smokes completed. Both correctness prerequisites, synthetic corruption controls, balanced order and independent raw/source/command/numerical audit passed. Only original numerical acceptance failed. All15246 raw files rehashed; local numerical reconstruction matches; exact three Linux ELF identities verified.

All1368 external measured assignments use a byte-identical baseline-built executable. This changes the reference assignment prospectively and does not reclassify earlier linked-peer failures.

All ten target windows pass6/6 positive pairs above absolute A/A:

|Target|Series1 gain %|Series2 gain %|
|---|---:|---:|
|compare/update_1row_1k/lin|53.790|70.528|
|compare/update_1row_10k/lin|37.148|40.775|
|compare/update_1row_100k/lin|41.531|40.275|
|compare/wal_scalar_full_update/lin|44.392|44.538|
|compare/wal_scalar_normal_update/lin|96.311|96.211|

Seven controls fail; five are labels of the exact same external executable:

|Case|Series|Gain %|Absolute A/A %|Positive|Same external executable|
|---|---|---:|---:|---:|---|
|compare/durable_delete_1row_1k/lin|series1|-4.811|3.020|2/6|False|
|compare/durable_commit_1row_1k/sqlite_wal|series1|-5.179|3.531|2/6|True|
|compare/durable_commit_1row_1k/sqlite_wal|series2|-2.106|1.090|2/6|True|
|compare/update_1row_100k/sqlite|series1|-9.003|5.525|2/6|True|
|compare/update_1row_100k/sqlite|series2|-12.954|9.999|2/6|True|
|compare/tx_rollback_1row_1k/sqlite|series1|-5.964|2.835|1/6|True|
|compare/insert_bulk_10k/lin|series2|-3.573|2.904|0/6|False|

Identical-binary failures do not establish a source-code regression or identify a physical cause. Timing variation, process/argument/setup effects remain unresolved. They remain failed under the frozen criterion; no noise waiver or threshold change. The two Lin failures (durable DELETE1k and bulk INSERT10k) are also retained without causal reclassification.

Candidate numeric peer wins226/456. SQLite FULL WAL remains0/96 across matched UPDATE/DELETE/COMMIT/ROLLBACK1k/10k windows. Durable UPDATE/DELETE/COMMIT Lin/SQLite ratios:1k1.125–1.269;10k1.696–1.850. Rollback itself performs no WAL synchronization; its ratios are1k1.950–2.332 and10k2.325–2.570. These are this run only, not a cross-host speedup claim.

Source candidate remains rejected/unadopted. No unchanged acceptance retry. Full source acceptance, SQLite gap and eight-peer objective remain open. The separate Ubuntu22 six-peer API refresh41376860938 is ongoing and includes no new runtime optimization.
