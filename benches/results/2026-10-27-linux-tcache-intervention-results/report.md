# Separate glibc interventions: more caching regresses, bounded refill scan promising

GitHub40457654308, head7efd49af257db749b7f7b9bd9138d020d191418b, completed diagnostic success. Same instrumented binary, System backend and actual checked UPDATE source/fixture/full output in all roles. Four startup environments: inherited baseline, identical control, only tcache_count32, only tcache_unsorted_limit16. Both interventions retained, no combined configuration. All roles trace allocations; instrumentation perturbs runtime. These gains are manual prepared UPDATE latency in this diagnostic, not uninstrumented peer acceptance.

Independent verify.py confirms24processes,792phase records,576wall observations,7920allocation events, same10-event layout per clone, actual benchmark versus GNU-time-wrapper PIDs, exact per-role environment edits, every raw/log median and peak RSS, complete input/instrumented source hashes. No overflow or allocation failure. Source restored after build and measurement.

| Rows | Count32 UPDATE gain % | Unsorted16 UPDATE gain % | Absolute A/A % | Unsorted16 positive | Unsorted16 544B service gain % |
|---|---:|---:|---:|---:|---:|
| 1000 | -3.931 | 4.638 | 1.139 | 5/6 | 8.626 |
| 10000 | -13.969 | 17.875 | 4.525 | 6/6 | 32.603 |
| 100000 | -32.241 | 17.770 | 4.553 | 6/6 | 32.205 |

| Role | Whole-child median peak RSS (GNU time kbytes) |
|---|---:|
| baseline | 590248 |
| control | 590356 |
| count32 | 590672 |
| unsorted16 | 590266 |

Count32 is rejected for this workload: every prepared process-size comparison negative;544B service regresses approximately15/30/47%. Unsorted16 is promising: both10k/100k improve6/6above A/A and1k improves5/6, but this single six-round diagnostic is not selection/engine acceptance. No unchanged repeat to erase the losing count32 observations.

GNU glibc2.39 documents tcache_count as per-size cached-chunk count and tcache_unsorted_limit as a bound on refill unsorted-list scanning (default unlimited). Reference: https://sourceware.org/glibc/manual/2.39/html_node/Memory-Allocation-Tunables.html . Latest manual defaults were intentionally not substituted for2.39. The observed opposite intervention effects are consistent with refill work contributing to the slow request, but do not prove the exact internal glibc path/fragmentation mechanism. Runtime option strings are preserved; effective loader-tunable introspection was not collected in this run and remains a qualification check.

Peak RSS includes all3sizes, fixtures, all helper/thread/process lifecycle through child exit. Largest fixture dominates; close RSS medians do not establish per-size retained-cache bounds or arbitrary threaded-workload memory safety. GNU time output/actual PIDs retained, not parent cumulative getrusage. Config edits were process-local; no machine/global setting or source/API/dependency/backend/payload/durability change.

Next evidence must be uninstrumented native/Full-durable/mutation/read with all same-runtime peers and memory controls, after any lower-bound exploration is declared and independently validated. No source promotion or runtime recommendation, no SQLite win, no eight-peer completion claimed.
