# Corrected COW + CAS + UPDATE: source gate rejected

Frozen2339a81cb, candidate runtime625bc7180 against correctness-aligned exact BTreeMap e3136c0a4. Run41306613457 completed all2412 fresh processes/19296 observations. CI raw verification and independent archived-data recomputation passed. Both frozen Linux prerequisites passed, with441/458 tests and6/5 separately compiled negative mutations. All134 functional smoke checks passed.

All ten target windows pass: native UPDATE1k/10k/100k gains51.106–56.538%; scalar Full63.096/63.684%; scalar Normal97.623/97.757%. Every target has6/6 positive pairs and exceeds absolute A/A. All Lin control windows meet the predeclared tolerance; this is not proof of zero regression or whole-source adoption.

Eight external control windows fail: SQLite DELETE-mode durable UPDATE1k series1, native DELETE1k series1, native DELETE10k series2, transaction COMMIT10k series1 and ROLLBACK10k series1; DuckDB filter_range series2 and agg_count_by both series. Each peer uses the role-specific compiled harness as declared. Why these external windows differ is not established. No control is waived, result dropped, threshold relaxed or unchanged retry dispatched. The whole source gate remains rejected.

The full 67-case report and every raw artifact are retained. Independent recomputation binds frozen Git sources/scripts, role manifests,134 raw smokes, process order/commands, samples, medians, RSS, peer ratios and gate results. Executable payloads were not included in the artifact; CI verified executable hashes while live, and archived process/binary hashes are checked for consistency.

Separate local ARM full-value API refresh wins do not override this failed Linux source gate. MSSQL remains private; Kusto and Full durable SQLite peer gaps remain open. Next work must explain or remove the measurement confound with a newly predeclared diagnostic, or materially change runtime code, without rewriting this result as accepted.
