# Patch-driven undo: Linux full-engine qualification rejected

GitHub run40418391258, frozen head30ef20cef157d9e977b7c550fbdbfc972f3d392b. Two six-round A/A/B series,36 successful measurement processes,20 scenarios,5760 raw observations. Independent verify.py recomputes240 case comparisons from raw wall samples; candidate qualification false, gate exits3. Tests and mutation-sensitivity checks ran before builds.

| UPDATE size | Series1 gain | A/A | Positive | Series2 gain | A/A | Positive |
|---|---:|---:|---:|---:|---:|---:|
| 1k | +1.274% | 2.130% | 5/6 | +3.091% | 1.664% | 6/6 |
| 10k | -0.686% | 2.396% | 3/6 | +0.751% | 2.081% | 4/6 |
| 100k | -1.383% | 1.521% | 2/6 | -1.787% | 0.941% | 2/6 |

Candidate beats SQLite in0/120 Lin scenarios. Rollback1k improves5.985/7.608%,6/6both; rollback10k4.102/1.973%,5/6and6/6, with series2 below2.540%A/A. These narrow results do not rescue failed UPDATE targets. Every DELETE,commit,rollback and SQLite control retained in paired-report.md.

Reject rollout. Do not replace fresh-fixture Linux results with positive warm repeated-row CPU microbench or nine/49 fewer allocations. Earlier DELETE +64requested-byte memory control remains part of the record. Production source remains main205; candidate snapshots are evidence only. Do not rerun unchanged to seek green or narrow acceptance to rollback1k.

Full durable/read/RSS/multirow/all-eight-peer qualification remains open. This report does not establish causal attribution for noisy timings. Next work requires a different hypothesis or an observed bottleneck in the fresh-input engine path.
