# Rejected combined FTS normalization scan

Exploratory local same-binary diagnostic: 7 datasets, 6 balanced rounds, 3 roles (baseline, identical baseline control, candidate), 126 fresh processes, 200000 calls each. All cases matched original per-character lowercase mapping before and after timing. Both functions compiled into one local native executable by rustc -O (actual compiler/host and binary SHA are in summary.json). The compiler was free to optimize both extracted functions; no whole-database or Linux peer result is inferred.

The candidate checks non-ASCII or uppercase in one iterator predicate before falling back to original ASCII conversion / Unicode mapping. Long already-lowercase strings regressed by median 33.42% (0/6 positive; baseline/control variation 5.88%). Late-uppercase strings regressed by 38.43% (1/6 positive; variation 13.51%). Short lowercase cases had high 26–27% control variation and do not establish a gain. Raw observations and every negative pair remain in raw.json.

Candidate rejected, runtime restored byte-for-byte to 4047596cd:src/fts.rs. Added candidate regression test and candidate source retained only in candidate-fts.rs; no rejected runtime change or unrun Cargo test is claimed as accepted. One startup TypeError from incorrect scheduling-helper argument order occurred before measurements; retained in startup-failure.txt and fixed before this diagnostic. No benchmark acceptance gate was relaxed and no unchanged retry was made.

This diagnostic cannot prove an x86 Linux regression or explain compiler behavior; its adverse signal is sufficient to avoid adopting this candidate. FTS normalization remains a profiled cost, while DuckDB INSERT 100k and SQLite rollback 10k gaps remain open.
