# Candidate public peers — 2026-11-03

Run https://github.com/themoretheless/lin/actions/runs/41567995224, head 95be4cc8f87cc01671e93f0f215f1aaf980eb7b2; runtime equivalent to f37a85ff297b3771f6305c3561b9e1b507555999. The job failed its numerical gate; no timing retry is authorized by this result.

Independent local reconstruction verified all 18 original processes and 5184 samples, binary hash, source manifest, result medians and owned-container cleanup. Read: 108/108 aggregate and 324/324 process wins against SQLite, DuckDB, PostgreSQL, MySQL, MongoDB and pandas. Native INSERT: 13/15 aggregate, 39/45 process wins. The six losses are all DuckDB: 10k Lin 21.6062815 ms vs 19.141627 ms, 100k Lin 296.179777 ms vs 168.9221105 ms. All three processes lost at both sizes.

Prepared read and native insertion contracts are frozen in the input protocol; these are not FULL WAL durability-equivalent comparisons. Scalar/grouped aggregate source gates are separate evidence. MSSQL remains separately encrypted; Kusto is not measured. The full eight-peer objective is not achieved. Next runtime work must address insertion losses while preserving correctness and read/control coverage.

`raw.tar.gz` preserves both downloaded artifacts, including the exact Linux ELF. `raw-manifest.json` records every file hash. The frozen original auditors live in the source commit under `benches/results/2026-11-03-current-candidate-public-peers/`; local audit only remapped the downloaded artifact and binary paths, with source files verified unchanged.
