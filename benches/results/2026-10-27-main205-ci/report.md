# Main20591dd4: full Linux CI artifact, including losses

[Benchmark run40391600456](https://github.com/themoretheless/lin/actions/runs/40391600456) completed successfully on frozen20591dd47991aa939eab3f31d066e2e6d80c53d1. Full original read/write and mutation JSON/HTML/Markdown retained with SHA256 manifest. Eight samples, one process; this is current-state evidence, not a paired causal comparison against e05ac74b or qualification of the partial-index fix as an optimization.

Across all same-case Lin/SQLite/DuckDB measurements in ci/:26wins,2losses. The declared point_get/filter_eq/join_inner gate passes; remaining cases are not silently promoted to gate requirements. Lin native bulk insert1k is1.106x slower than SQLite,10k1.671x slower. In ci-mutations/:0wins,10losses against SQLite, including transactions.

| Scenario | Lin/SQLite latency ratio (lower is better) |
|---|---:|
| update1k |1.832|
| delete1k |2.252|
| update10k |3.034|
| delete10k |3.990|
| update100k |2.881|
| delete100k |3.350|
| transaction commit1k |1.964|
| transaction rollback1k |2.297|
| transaction commit10k |3.494|
| transaction rollback10k |3.636|

Materialize happens to win against DuckDB in this process (981.328us/1210.169us); the previous frozene05ac CI process lost (798.322us/767.982us). Different runners/sessions are not alternating controls, so neither the change of sign nor the absolute differences establish a causal performance change or close that scenario universally.

Next: await frozen borrowed-row A/A/B Linux candidate run40390146614; preserve the allocation savings separately from latency. Reproduce native insertion losses on the Linux contract before claiming the older ARM peer matrix applies here. Full durable SQLite1k and real Kusto qualification remain open; this artifact measures neither. MSSQL comparative data remains private and is not in this artifact.

[Compute run40391600501](https://github.com/themoretheless/lin/actions/runs/40391600501) completed successfully on the same main20591dd4: Linux/macOS/Windows jobs all passed. Full step statuses retained in compute-status.json. Compile/WGSL validation does not prove physical GPU execution or all-eight-peer performance.
