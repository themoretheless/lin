# Incremental owned rollback: warm diagnostic gate passed

Private prototype built on the clone-free owned-row helper, replacing remove-all/reinsert-all scalar/FTS maintenance with existing scalar insert_at reverse-key replacement and FTS sync_row(current,restored). Unchanged FTS fields short-circuit; unchanged scalar keys retain membership. Complete owned Rows, exact public BTreeMap alias, public restore_rows, journal accounting, generation and persistence semantics remain unchanged. Identity/stale-map/short-column fallback still installs the incoming Row before rebuilding maps. Production core is restored after every test/build.

Full default-feature release library/integration suite:434passed,2ignored. Dedicated tests cover changed id/uri and row-map restoration, exact repeated-update rollback rows, FTS membership after successful title/uri update and after a rejected unique-index update. The first fixture incorrectly attempted immutable docs.body; its failed source/log/status are retained. The command chain incorrectly continued timing after that fixture failure. The fixture was corrected without changing candidate engine/gate sources, and the complete functional suite passed on the exact measured source hashes. Build/run/analyzer now explicitly refuse a failed functional status. No timings were regenerated or removed to obtain a pass.

36fresh processes, two6-round series,3456metric observations over1k/10k/100k rows. Baseline/control use one exact executable; candidate is separately built.16samples per process-size:512alternating prepared UPDATE operations plus one explicit transaction UPDATE+rollback. Full canonical target-row comparison and indexed counts/generation checks occur outside timers; ordinary Handle/undo/Pack/drop costs remain inside. Same ordinary System allocator, no counting wrapper. Source/binary hashes, rotated role order, PIDs, loads and all samples retained. This is repeated-row local macOS diagnostic evidence, not fresh-fixture native Linux/Full sync/read/peer acceptance.

| Rows | Metric | Series | Gain % | Absolute A/A % | Positive | Pass |
|---:|---|---|---:|---:|---:|---|
| 1000 | rollback_ns | series1 | 43.949 | 3.638 | 6/6 | True |
| 1000 | rollback_ns | series2 | 42.740 | 0.977 | 6/6 | True |
| 1000 | ns_per_update | series1 | 1.120 | 2.931 | 4/6 | True |
| 1000 | ns_per_update | series2 | 0.581 | 1.518 | 5/6 | True |
| 10000 | rollback_ns | series1 | 45.675 | 4.273 | 6/6 | True |
| 10000 | rollback_ns | series2 | 46.507 | 4.632 | 6/6 | True |
| 10000 | ns_per_update | series1 | 1.967 | 1.738 | 6/6 | True |
| 10000 | ns_per_update | series2 | 3.097 | 1.507 | 6/6 | True |
| 100000 | rollback_ns | series1 | 42.334 | 3.799 | 6/6 | True |
| 100000 | rollback_ns | series2 | 43.192 | 1.559 | 6/6 | True |
| 100000 | ns_per_update | series1 | 0.002 | 1.501 | 3/6 | True |
| 100000 | ns_per_update | series2 | 1.957 | 0.795 | 6/6 | True |

Diagnostic gate=true: all6rollback target windows win6/6 and exceed A/A; all6unchanged UPDATE control windows stay within the original -max(2%,A/A) tolerance. Different prior prototype/harness revisions are not a causal comparison; only these own baseline/control/candidate pairs establish these gains. No production adoption or full eight-peer achievement follows. Next is a frozen-source fresh-fixture Linux whole-engine matrix including native/read/durable/SQLite/DuckDB controls and unchanged correctness, RSS and raw-data audits.
