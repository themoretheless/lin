# Positive-zero WAL full-engine result: rejected

Run [40549331116](https://github.com/themoretheless/lin/actions/runs/40549331116), frozen source `c464574c5ef43afeb8afd858235078ca3329390a`. All 2268 isolated processes /18144 observations completed. Functional tests, smoke matrix, summary and independent raw audit passed; the final performance gate failed. Local frozen-source audit also passed without resetting current core.

9/12 target windows and 12/114 control windows fail. No target passes both series. Positive median improvements do not override failed pair consistency or A/A thresholds. CPU component gate remains false. The candidate is not adopted; no unchanged rerun is justified by this result.

| Target | Series | Gain % | Absolute A/A % | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | 4.355 | 12.489 | 3/6 | False |
| compare/durable_update_1row_1k/lin | series2 | 17.686 | 9.266 | 4/6 | False |
| compare/durable_delete_1row_1k/lin | series1 | 21.593 | 15.562 | 4/6 | False |
| compare/durable_delete_1row_1k/lin | series2 | 9.436 | 7.156 | 5/6 | False |
| compare/durable_commit_1row_1k/lin | series1 | 30.161 | 17.187 | 6/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | 8.942 | 12.515 | 5/6 | False |
| compare/durable_update_1row_10k/lin | series1 | 17.764 | 13.841 | 5/6 | False |
| compare/durable_update_1row_10k/lin | series2 | 19.371 | 11.445 | 6/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | 7.474 | 10.818 | 5/6 | False |
| compare/durable_delete_1row_10k/lin | series2 | 7.568 | 13.480 | 5/6 | False |
| compare/durable_commit_1row_10k/lin | series1 | 13.688 | 8.839 | 4/6 | False |
| compare/durable_commit_1row_10k/lin | series2 | 10.661 | 6.468 | 6/6 | True |

Controls include Lin native UPDATE1k (series1), equality COUNT and range COUNT (both series), and scalar Normal UPDATE (series2); external unchanged SQLite controls also fail. These results do not uniquely identify a source-level cause. All failed controls and original process pairs are retained in `independent-audit.json` and `summary.json`.

Candidate peer comparisons: SQLite FULL DELETE95/96, SQLite FULL WAL18/96, SQLite native/read59/204, DuckDB read36/36. These are this Rust matrix only; they do not establish fresh PostgreSQL/MySQL/MongoDB/pandas/MSSQL/Kusto wins. Raw primitive, Full-sync and API results must not be combined into universal superiority.

Full IEEE/legacy-reader compatibility and the separately tested schema-rejection correction remain functional evidence, not performance acceptance. This benchmark predates `065427b25`; it does not automatically qualify newer compiled source.

`raw-artifact.tar.gz` preserves the complete downloaded artifact; `raw-manifest.json` hashes every raw file. `audited-report.md` contains the complete table with a corrected display heading; the original CI report is retained unmodified inside the archive. Reproduce the independent audit after extraction with `python3 scripts/audit-wal-zero-engine.py benches/results/2026-10-27-wal-zero-full-engine-linux`.
