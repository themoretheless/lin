# Large-row worker creation fallback

Baseline: 156e991. Large inserts formerly used scope.spawn, which panics if the OS cannot create a thread. Candidate uses Builder::spawn_scoped and handles its io::Error by constructing the right half serially. Parallel success retains original order and one captured timestamp. Actual worker panics are still propagated; this specifically handles thread creation failure. No public API, persistence format or durability policy changes.

A deterministic regression injects WouldBlock at the real tail-merge helper and verifies 2053 tail rows, original left prefix, ordered IDs, Unicode/last duplicate title fields and the same timestamp. This tests the fallback branch without exhausting system resources; a real OS resource exhaustion event was not induced. Existing parallel boundary/duplicate-ID/URI tests and full `cargo test --offline --workspace` passed. Both benchmark binaries built. `git diff --check` passed.

Six alternating independent baseline/candidate process pairs per workload, 24 fresh fixtures per process, one operation/sample and no warmup. Exact affected-count/value/readback checks passed. Setup/preparation excluded; thread creation/join and synchronous durable operation remain inside measured time. Builds and tests did not overlap timing.

| Workload | Median paired speedup | Faster pairs | Baseline median ms | Candidate median ms |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | -0.03% | 3/6 | 0.927625 | 0.919969 |
| compare/insert_bulk_1k/sqlite | -1.42% | 2/6 | 0.813354 | 0.818125 |
| compare/insert_bulk_10k/lin | 1.68% | 5/6 | 9.669646 | 9.507427 |
| compare/insert_bulk_10k/sqlite | -0.16% | 2/6 | 10.678635 | 10.623844 |
| compare/durable_insert_1k/lin | 0.37% | 3/6 | 2.569562 | 2.549240 |
| compare/durable_insert_1k/sqlite | -0.59% | 2/6 | 1.385448 | 1.396979 |
| compare/durable_insert_10k/lin | 1.20% | 5/6 | 18.931135 | 18.730156 |
| compare/durable_insert_10k/sqlite | -0.16% | 3/6 | 13.791615 | 13.802958 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Small workloads are essentially unchanged/mixed. Larger workloads show modest positive changes; these do not establish that the error-handling branch causes acceleration. The change is retained for availability, with no observed consistent slowdown in these six process pairs. Thread resources and transient right-half row headers remain the tradeoff for successful parallel execution; fallback uses one execution context.

Candidate Lin vs SQLite: native 1k loses all six pairs (aggregate medians 0.919969 vs 0.818125 ms); native 10k wins all six (9.507427 vs 10.623844 ms); durable 1k/10k lose all six (2.549240 vs 1.396979 ms and 18.730156 vs 13.802958 ms). The full nine-engine goal remains incomplete, including the prior absent MSSQL/Kusto endpoints. This is a regression/availability check, not a fresh full peer matrix.
