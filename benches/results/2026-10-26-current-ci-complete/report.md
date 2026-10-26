# Current-head Linux CI: complete, with retained losses

Frozen e05ac74b5ac6276f054b5d29a2215ae655441161, [run40381280504](https://github.com/themoretheless/lin/actions/runs/40381280504), completed successfully. Downloaded full benchmark artifact; raw runs retained under ci/ and ci-mutations/, with SHA256 manifest. Eight samples, one process; these are observations, not alternating multi-process candidate qualification.

The declared point_get/filter_eq/join_inner budget gate passes against same-run SQLite and DuckDB. Its three-case scope is not the full objective. The remaining artifact reveals actionable losses:

| Scenario | Lin us | Peer us | Observation |
|---|---:|---:|---|
| materialize | 798.322 | DuckDB767.982 | Lin slower by3.95% |
| update1row1k | 8.703 | SQLite3.470 | Lin2.51x slower |
| delete1row1k | 10.416 | SQLite4.202 | Lin2.48x slower |
| update1row10k | 24.487 | SQLite10.2305 | Lin2.39x slower |
| delete1row10k | 30.701 | SQLite8.2475 | Lin3.72x slower |
| update1row100k | 48.628 | SQLite27.106 | Lin1.79x slower |
| delete1row100k | 55.734 | SQLite25.0125 | Lin2.23x slower |

Fresh-fixture mutation benchmarks completed; the workflow currently records these measurements without requiring a win. Preserve that distinction: green CI does not prove broad peer superiority. Full scenario contracts, lifecycle timing and validation are in raw JSON and benches/compare.rs. Do not substitute native in-memory mutation observations for Full durable commits.

Next performance action: reproduce/profile mutation losses with matched baseline/candidate controls, measuring predicate lookup, undo capture, derived-index/FTS maintenance and result/drop costs. Existing row journals and bounded planner work are already implemented; verify actual allocation/full-scan causes before changing them. SQLite durable1k and real Kusto qualification remain independently open.
