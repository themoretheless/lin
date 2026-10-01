# Rejected fresh-thread embedding experiment

Three independent process pairs, alternating order, 12 fresh fixtures per case.
Each >=4096-text batch creates up to four scoped workers; worker creation and
join are inside the measured insertion. Smaller/custom embedding paths unchanged.

| Pair | Serial full 10k | Scoped workers | Serial no embed | Worker-run no embed |
|---|---:|---:|---:|---:|
| 1 | 12.990 ms | 14.860 ms | 8.487 ms | 8.235 ms |
| 2 | 12.508 ms | 11.956 ms | 8.557 ms | 8.245 ms |
| 3 | 11.701 ms | 12.095 ms | 8.166 ms | 8.671 ms |

Two full-insertion pairs regressed; one improved. This implementation is
rejected. A separate reusable-worker-pool experiment follows. Workspace tests
and ordered bit-equivalence tests passed. Background load uncontrolled.
Sources/hashes/raw results are saved. No peer win is established here.
