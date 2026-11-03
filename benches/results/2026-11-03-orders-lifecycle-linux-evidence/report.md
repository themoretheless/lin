# Aggregate source seed/query lifecycle: passed in this scope

Run https://github.com/themoretheless/lin/actions/runs/41651338091 at dd028856d4c9d4546ba90899fcad1bed02e7596f. Runtime candidate exactly b9f10bc776b05eb5235237e9a900c2ef6aaed1fd (f37 equivalent), correctness-aligned baseline a28aa758b774e216f14a6348fb1cf9dca2eb42bc. Native Linux x86_64 ordinary default-feature release. Independent auditor verified 108 fresh processes, 216 query samples, full source/manifests/harness/workflow/helper hashes, actual compilation and both ELF binaries, exact scalar SUM and every orders id/user_id/total value, and reconstructed all windows. Linux workspace: 414 passed / zero failed / 3 ignored. Every artifact member archived and reread by SHA-256.

| Rows | Metric | Series | Median gain % | A/A % | Allowed loss % | Pass |
|---:|---|---:|---:|---:|---:|---|
| 1000 | process_wall_ns | 1 | 0.691757 | 1.942842 | 2.000000 | True |
| 1000 | process_wall_ns | 2 | -0.686817 | 0.935591 | 2.000000 | True |
| 1000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 5.000000 | True |
| 1000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 5.000000 | True |
| 10000 | process_wall_ns | 1 | 0.609129 | 2.709951 | 2.709951 | True |
| 10000 | process_wall_ns | 2 | -0.028325 | 1.774799 | 2.000000 | True |
| 10000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 5.000000 | True |
| 10000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 5.000000 | True |
| 100000 | process_wall_ns | 1 | -0.003104 | 0.644578 | 2.000000 | True |
| 100000 | process_wall_ns | 2 | 1.581219 | 0.908585 | 2.000000 | True |
| 100000 | peak_rss_kib | 1 | -0.008498 | 0.005309 | 5.000000 | True |
| 100000 | peak_rss_kib | 2 | 0.008760 | 0.000531 | 5.000000 | True |

All twelve resource windows passed. This proves no disallowed full-worker wall/RSS regression in this frozen seed/query workload versus a28, not a general speedup. Whole-worker timing includes startup, docs/users/orders inserts with normal embedding and indexes, first SUM including validation/calibration, complete orders projection, serialization and teardown. Per-query reported samples follow the unchanged worker warmup/calibration, so isolated first-query latency is NOT qualified. Isolated orders INSERT/UPDATE/DELETE cost, arbitrary write workloads and FULL WAL durability remain open.

Original production main is not the performance acceptance baseline: its quota/source-order/large-integer counterexamples remain. No production merge is performed from this resource evidence alone. Existing warm aggregate improvements and previous embedding-candidate refusals are unchanged. Fresh eight-peer source/profile qualification and Kusto access remain required before the full objective can be considered achieved.
