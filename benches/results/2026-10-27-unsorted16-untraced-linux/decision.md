# Decision: reject unsorted16 as a qualified acceleration

Run [40460186460](https://github.com/themoretheless/lin/actions/runs/40460186460), frozen head `083677b02896fa79d7389a1937e1010594a77a50`. All 1008 isolated processes and 8064 wall observations completed. Independent raw audit passed both in CI and locally. Functional tests: 413 passed, 2 ignored. CI failed solely at the unchanged performance gate.

| UPDATE Lin | Series 1 gain / A/A | Positive pairs | Series 2 gain / A/A | Positive pairs |
|---|---:|---:|---:|---:|
| 1k | +2.964% / 4.601% | 4/6 | +2.087% / 2.297% | 4/6 |
| 10k | -1.419% / 0.868% | 1/6 | +2.629% / 1.509% | 5/6 |
| 100k | -0.274% / 4.059% | 2/6 | +0.810% / 2.290% | 5/6 |

None of the six target series qualifies. Three controls also fail: SQLite UPDATE10k series1 -7.205% versus A/A4.424%; Lin rollback1k series2 -2.869% versus A/A1.867%; SQLite Full-durable insert1k series1 -3.272% versus A/A1.079%. No case or losing control is dropped, and the acceptance band is unchanged.

Candidate same-runtime Lin/SQLite numeric wins are 24/168. All are insert controls: native insert10k 12/12 and Full-durable insert1k 12/12. The original twenty mutation/transaction cases contribute 0/120 Lin wins. Lin UPDATE median per-round latency ratios versus SQLite are 1.485/1.562 at1k, 1.573/1.686 at10k and1.427/1.540 at100k across the two series. This leaves the mutation gap open. Full-durable insert10k is also slower, around1.51x. These workload-specific results do not extend to other sources, machines or peers.

Across 56 case-series comparisons, candidate versus baseline median whole-process RSS changes range from -4.800% to +1.190%. This is an observed process high-water comparison; it is not a bound on allocator retained memory or a proof of a memory improvement.

Earlier traced diagnostics reported much larger gains. This uninstrumented, case-isolated qualification does not reproduce those gains. Different instrumentation, process layouts and hosts prevent attributing the discrepancy to one cause from these runs alone. The diagnostic remains useful for locating allocation service costs, but does not establish a shippable runtime acceleration. Do not repeat this same tuning unchanged or enable it globally.

No production allocator, process environment or Row API changed. The Map/static-output source prototype also remains unqualified against main. Next performance work must address the remaining actual mutation work, while retaining full owned output, rollback, durability, indexes, FTS and embedding contracts. Broad superiority over SQLite, DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas remains unproven.
