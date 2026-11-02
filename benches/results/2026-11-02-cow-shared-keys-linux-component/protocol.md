# Shared row keys: dedicated Linux component experiment

This is a new source-representation experiment, not full performance acceptance. Original COW 625bc7180 is compared to shared Arc<str> keys at 3fafb9589. The local experiment gave consistent insertion/delete/FTS gains but mixed UPDATE results on a shared host. No candidate is adopted.

Build both exact source/test trees, run full release correctness, and compile the same adapted harness bytes under Rust 1.96.0/default release settings on one Ubuntu 24 runner. The common harness differs from the historical harness only by durable_json_row using k.to_string() at the owned JSON verification boundary. Timed operations stay unchanged. Binary paths use equal-length role suffixes. All DuckDB labels use the exact baseline binary to avoid source-linked external engine differences.

Twelve unchanged cases: UPDATE and rollback at 1k/10k/100k, bulk INSERT 10k, DELETE 10k, selective FTS, DuckDB GROUP BY, FULL UPDATE 10k and FULL COMMIT 1k. Each of two series has six balanced permutations, baseline/control/candidate, eight observations per fresh process: 432 measured processes, 3456 observations, 48 smoke processes total. Keep every observation, failure and control difference. Verify exact commands, hashes, schedule and independent numerical recomputation.

Discovery criterion for follow-up only: improvements must exceed baseline/control absolute variation and remain positive in all six pairs in both series; regressions remain explicit. This is not an adoption gate or proof of superiority over all peers. Full matrix regression acceptance, SQLite FULL WAL, MSSQL and real Kusto requirements remain open.
