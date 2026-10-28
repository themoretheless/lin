# SQLite WAL gap: evidence and next requirements

Source: frozen36666994a,run40607797376. Independent raw/source audit passed; full source acceptance failed. This report derives only from the archived peer comparisons. Ratio is Lin time / SQLite WAL time; below1means Lin is faster. It is not an A/A optimization gain.

|Operation|Lin wins|Comparisons|Lowest window median ratio|Highest window median ratio|
|---|---:|---:|---:|---:|
|durable_update|1|24|1.105177|1.676077|
|durable_delete|0|24|1.123855|1.753914|
|durable_commit|2|24|1.173933|1.761741|
|durable_rollback|0|24|1.234909|1.766275|

Rollback lost24/24peer pairs. The separate caller-thread profile on earlier source75f8 reports zero WAL payload/encode/write/sync for rollback, with execution/rollback work dominating. Those phase percentages are not transferred to source366or multiplied by this run’s peer ratios. They establish that a write-barrier-only intervention cannot address the complete rollback contract. See [phase diagnostic](../benches/results/2026-10-28-cow-durable-phase-refresh-linux/audited-report.md).

The synchronized-prefix RWF_DSYNC probe failed all16Full target windows, so replacing write+fdatasync with that primitive is rejected. This result measures the standalone I/O primitive, not the full engine. See [rejected probe](../benches/results/2026-10-28-wal-rwf-dsync-probe-linux/report.md).

The full-source gate also failed Lin bulk INSERT10k in both series and filter_eq in one series. Pending SoA,bulk-template andsingle-Cmp candidates remain separate; none currently proves whole-source acceptance or all-peer superiority.

Next DELETE representation hypothesis (not implemented or qualified): current Pack::Delete carries full returned rows, while replay resolves identity through id,then uri,then collection-specific same_row_key fallback. An identity-only row must therefore be restricted to cases whose original replay identity is provably retained. Arbitrary append/fact/unidentified rows must keep their full fallback semantics. Do not assume id-only is universally valid.

Required before any DELETE-WAL adoption: preserve returned full rows and rollback ownership; original quota/canonical-size rejection rules; exact generation/next_id; existing readers and snapshots; batches/cascades/missing-id/moved-slot/duplicate-key behavior; CRC/torn-tail recovery and append failure atomicity. Compare full reconstructed logical state,lookup/index/FTS/SoA state and IEEE float bits,including a legacy reader. Smaller bytes alone are not a speedup or proof of unchanged durability. A new frozen paired durable matrix remains mandatory.

Kusto evidence is still absent. MSSQL comparative data remains private. This analysis does not complete the eight-peer objective.
