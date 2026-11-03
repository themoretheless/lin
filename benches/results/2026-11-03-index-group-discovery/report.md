# Bounded index grouping: rejected overall, 100k discovery win

Baseline c9a929d2c, runtime equivalent to 3401bc17d. Candidate groups up to 16 precomputed keys for non-unique indexes without bitmap sampling, reducing forward BTreeMap lookups. Higher cardinality falls back before mutation; unique/bitmap paths retain original behavior. Independent reference tests cover existing keys, two slabs, partial/unique constraints and 1/2/16/17/513 key cardinalities. Focused tests: 8 passed, 1 ignored.

Local macOS ARM64, ordinary default worker, 36 processes / 288 samples. Four independent baseline/control/candidate pairs per size, eight samples per process. Input and binary hashes frozen before measurement; exact six-field readback and default embeddings remain enabled. Raw observations, full readbacks and final tests are in raw.tar.gz with per-member SHA256; all medians/A/A reconstructed independently.

1k median gain -4.1181%, A/A 3.6093%, 2/4 positive; 10k -4.2715%, A/A 3.2743%, 0/4 positive. Both failed. 100k +5.4977%, A/A 1.8635%, 4/4 positive: this discovery window passed. Overall candidate fails because every size was required to pass. Local background activity was uncontrolled; neither causal regression nor Linux qualification follows. No unchanged numerical retry is planned.

Source restored byte-for-byte; candidate.patch retains the exact implementation and tests for research. The first evidence-save attempt incorrectly asserted all individual windows must have failed, so that attempt stopped before archival/restoration; the following commit completes preservation and records the actual mixed outcome. No measurement was restarted.

Next work: profile the exact Linux insertion workload, including transaction overhead, before selecting another change. DuckDB INSERT losses and the full eight-peer objective remain unresolved.
