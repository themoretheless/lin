# Strict benchmark completion and win gate

This changes Python benchmark finalization, not the Rust engine or measured timings. A tied median previously passed --require-wins because only strictly faster peers failed the gate. Now Lin must be strictly faster than every requested peer; ties return exit 3 while still preserving complete measurement status.

Completion now recomputes medians from observations and requires exactly the expected distinct process repetition IDs plus nonempty, finite, positive timing samples. Duplicate/missing process IDs, NaN/Inf, zero/negative and empty samples cannot produce a successful complete report; invalid complete-size groups record an error and return exit 2. Cached medians are cleared rather than trusted. All raw observations are retained. A strict numeric win alone still does not establish significance or causality; the separate A/A calibration requirements remain.

Regression evidence: the new tie, duplicated-process and invalid-sample tests failed on the previous implementation. After correction all 14 adapter tests pass, including empty-sample, actual SQLite/DuckDB fixtures and independent process/incomplete checks. Logs retained. This is benchmark tooling only; unchanged Rust workspace/performance measurements were not rerun.

The saved current-source read and native matrices were re-finalized with the new gate into separate artifact directories, without rerunning timing or modifying original results: all 108 read comparisons and 15 native insertion comparisons still pass, with identical medians. This verifies their existing wins are strict and observations satisfy this gate; it is not a new performance campaign, a current adapter live qualification, or proof of MSSQL/Kusto/durable coverage.

Full objective remains open: durable 1k SQLite gap, MSSQL/Kusto live validation and applicable missing contracts. No peer win manufactured by accepting ties or invalid samples.
