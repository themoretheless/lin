# Corrected COW: available-peer API refresh, preparation

Use the exact Rust source tree of Linux-qualified candidate625bc7180, including schema/ordered rollback corrections and UPDATE clone elision. The build checks all runtime source hashes against the frozen full-engine candidate manifest. Cargo builds in a dedicated target directory. No runtime optimization is added for this refresh.

Retain the previous available-peer timed helpers byte-for-byte: examples/peer_bench.rs and scripts/bench-peers.py. Only wrapper output paths, owned fixture/container names, host metadata and isolated build target change. Reads use Lin/SQLite/DuckDB/PostgreSQL/MySQL/MongoDB/pandas at1k/10k/100k; native insert uses the same sizes and five peers excluding pandas. Three fresh processes per size,12 samples,rotated engine order,complete generated values/columns/multiplicity checks. Retain every losing ratio, error and process observation; do not repeat unchanged failures.

Use fresh owned Linux x86_64 PostgreSQL16,MySQL8.4,MongoDB8.0 containers, record actual image digests/architecture and durability settings. Only exact recorded IDs with this run's ownership label may be stopped/removed. Preserve sentinels and verify fixture cleanup after each size. Existing containers are never reused. Pin Python package versions with the existing lock file and record the installed package list.

This is a prepared Rust versus Python/driver/network API comparison. PostgreSQL COPY,MySQL executemany,MongoDB ordered insert_many w=1 j=false,SQLite in-memory executemany and DuckDB DataFrame insertion retain their previous contracts; these are not Full durable equivalents. Pandas is read-only. Lin embeddings and FTS remain enabled. No causal comparison with previous ARM/macOS timings is made.

MSSQL is outside this public run and retains private comparative handling. Kusto is not configured and is not counted as a win. Full source acceptance and the eight-peer objective remain separate. No timing has started for this refresh.
