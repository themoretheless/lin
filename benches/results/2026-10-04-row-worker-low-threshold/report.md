# Lower row-worker threshold: rejected

Baseline: d70fed1. Candidate changes only the parallel row-construction threshold from 4096 to 512. Thread creation/join stays inside operation timing; timestamp, validation order, worker-creation fallback and all persisted representations stay unchanged. Sources and binary SHA256 hashes are saved.

Six independent alternating baseline/candidate process pairs per workload; 24 fresh fixtures per process, one operation/sample, no warmup. Setup/schema/preparation excluded. Exact fixture/count/value/readback validation passed. No builds/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -0.51% | 2/6 |
| compare/insert_bulk_1k/sqlite | 0.29% | 4/6 |
| compare/insert_bulk_10k/lin | 0.28% | 4/6 |
| compare/insert_bulk_10k/sqlite | 0.68% | 4/6 |
| compare/durable_insert_1k/lin | -1.64% | 2/6 |
| compare/durable_insert_1k/sqlite | -2.57% | 2/6 |
| compare/durable_insert_10k/lin | -0.88% | 0/6 |
| compare/durable_insert_10k/sqlite | 0.38% | 4/6 |

Speedup is median(100*(baseline_i-candidate_i)/baseline_i). Neither small workload improved consistently, so starting a second worker at 1k does not close the SQLite gap in this evidence. Durable 1k SQLite control also slowed, limiting causal inference about its Lin slowdown. The 10k path uses two execution contexts in both variants; its timing changes do not directly measure benefit/cost of the lower threshold, and may reflect noise or compilation changes.

Decision: reject. src/exec.rs restored byte-for-byte to baseline, retaining threshold 4096 and the earlier OS thread-creation fallback. Two focused parallel-bulk tests passed; both benchmark binaries built. Full workspace tests were not run for this rejected one-line candidate. `git diff --check` passed after restoring source and normalizing log trailing blank lines.

The full nine-engine objective remains incomplete. This is evidence against lowering the threshold, not a performance improvement or full peer matrix refresh.
