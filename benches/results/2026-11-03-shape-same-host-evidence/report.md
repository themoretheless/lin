# Same-host phase attribution after forced rebuild

Run https://github.com/themoretheless/lin/actions/runs/41613433635 at d3b9a4945aaff3485059d57727476d96dc251031. Baseline b9f10bc776b05eb5235237e9a900c2ef6aaed1fd; rejected candidate d8871eb36b119b9960989bf4a5feea9877168cc5. Both independent audits passed: all source closures, exact reconstruction of patches, four ELF binaries, actual lin/worker compilation logs, no ordinary stderr, full six-field result values and all 36 samples across 12 processes. Four logical CPUs with affinity 0..3 in both profiles. All archived members reread and hashed. Unlike run41612695460, ordinary candidate did not reuse the baseline diagnostic binary.

| Rows | Baseline ordinary ms | Candidate ordinary ms |
|---|---:|---:|
| 1k | 2.283682 | 2.263174 |
| 10k | 23.134916 | 22.345815 |
| 100k | 314.974815 | 227.202882 |

100k diagnostic per-10k-statement medians (microseconds): build 5265 -> 3877; validate 1774.5 -> 1798.5; vectors 15015 -> 7155.5; total embed 17723 -> 10035.5; maintenance 10011.5 -> 9833.5. Detailed values and all losses remain in the two audit JSON files. Phase medians are not additive full operation estimates. This is baseline-first then candidate diagnostic reconnaissance, with compilation between sources and only three operation samples per case; it is not a paired performance qualification.

The large vector-phase difference persists on one runner despite unchanged embed.rs. Allocation placement, cache locality, compiler layout and scheduling remain hypotheses, not established mechanisms. Do not attribute the whole difference to field-name checks. Next independent prototype: construct one bounded shared row shape inside each INSERT execution before spawning row workers. This keeps setup cost inside execution and avoids persistent prepare metadata. It must pass fresh correctness, all-size source controls, lifecycle/RSS, durable and peer gates before adoption.

The earlier 216-process prepared-cache rejection remains binding. No source is adopted or merged. The eight-peer objective is unfinished.
