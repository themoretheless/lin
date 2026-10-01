# In-place FTS fold: rejected

Baseline includes the sorted pending-edit correctness fix. Candidate reuses the term key, retains surviving positions in the base buffer, and avoids a second copy for an empty additions list.

Both release diagnostic binaries were built before measurement. Three independent process pairs, alternating the second pair, 12 seconds per process, 10000 resident documents. Each process validates row count, every ID/URI lookup, and exact lexical-search IDs/multiplicity after the measured loop. Other source hashes matched at candidate build.

| Pair | Baseline pairs/s | In-place pairs/s | Change |
|---|---:|---:|---:|
| 1 | 145342.4 | 139326.3 | -4.1% |
| 2 | 137655.2 | 136582.5 | -0.8% |
| 3 | 131639.1 | 151516.1 | 15.1% |

Not retained: two pairs lost and one won; background load is uncontrolled. No causal regression percentage or peer win is claimed. This diagnostic churn rate is not fresh-fixture single-delete latency and does not replace native peer benchmarks. Workspace tests passed on the candidate, including random pending edits, folding, codecs, rollback and durable readback.

The candidate was reverted to the correct sorted-pending baseline. The diagnostic lexical-search validation remains in examples/profile_writes.rs. Sources, hashes, build logs and all completed process logs are included.
