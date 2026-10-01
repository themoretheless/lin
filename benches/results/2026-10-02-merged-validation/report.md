# Applied merge validation

User authorized resolving the pending merge on 2026-10-02. Applied the tested resolution: preserve current embedding/FTS/WAL/slab implementations, retain Embedder.embed_batch_owned default API, hybrid top-k selection and large-bulk reopen regression. Source formatting completed.

Final main-tree checks: cargo test --offline --workspace PASS; cargo bench --offline --bench compare --no-run PASS; git diff --cached --check for src/tests PASS. Test outputs preserved verbatim. Hybrid bounded/full-rank test and large-bulk reopen regression both pass. Earlier isolated performance evidence is in ../2026-10-01-merge-proposal/report.md; no new all-peer or GPU performance claim. MSSQL/Kusto comparison remains missing.
