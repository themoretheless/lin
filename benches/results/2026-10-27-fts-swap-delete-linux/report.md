# FTS fused swap-delete: Linux qualification rejected

GitHub run 40408513563, frozen head e0b6915312acac4227e20cbdf84cabe16dcf1d43. Two six-round A/A/B series, 36 processes, 20 scenarios. All measurements completed; acceptance gate exited 3.

Deletion 1k improved 7.940% / 3.417%, 6/6 positive pairs in each series. Deletion 10k: -1.079% / +1.809%, 2/6 and 5/6; deletion 100k: -0.755% / +1.323%, 3/6 and 5/6. These sizes fail the prespecified stable-win gate. Controls also exceed regression limits, including update10k series1, tx_commit10k series2, tx_rollback10k series2 and SQLite controls. Timing alone does not establish the cause of these regressions.

Candidate beats SQLite in 0/120 paired Lin scenarios. Seven fewer allocations per deletion do not establish an overall latency improvement. Candidate remains experimental and production source is unchanged. Do not rerun unchanged or promote a 1k-only win as achievement of the broad goal.

See paired-report.md for every control, summary.json for process medians, and per-process directories for raw observations. Full durable, read workloads and all-eight-peer qualification are not established.
