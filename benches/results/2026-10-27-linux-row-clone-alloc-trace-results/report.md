# Linux standard Row clone: allocation service dominates

GitHub40456308165, heada4a335f5a921805946d8e6002b90200b06d3bbc0, completed diagnostic success. Frozen Map/static-output source, actual checked UPDATE fixtures/default payload/index/FTS/embeddings, public Row unchanged. Benchmark-root GlobalAlloc wrapper delegates all operations to System. Only clone owner thread is traced;64event fixed atomic buffer, no logger heap allocation. Overflow/failed allocation refused. Full output readback occurs after manual prepared timer. All production sources restored. No backend swap or optimization shipped.

Independent verify.py passes18fresh processes,594phase records,432wall observations,1980actual traced events. Recomputes all process/paired/layout medians with independent median routine, validates complete role matrix, exact per-record distributions, raw log/JSON equivalence and source/helper/instrumented hashes. All198traced records have9fields,10successful allocation events:1request544bytes align8 and9key-sized requests2/3/4/5/9bytes align1. Off/control records have no events. No overflow/realloc/free/zeroed event in measured clones.

| Rows | Traced clone us | System service sum us | 544B service us | Remainder us |
|---|---:|---:|---:|---:|
| 1000 | 7.725 | 7.009 | 6.557 | 0.772 |
| 10000 | 17.562 | 16.322 | 15.835 | 1.282 |
| 100000 | 35.672 | 34.039 | 33.267 | 1.677 |

| Metric | Trace overhead % | Absolute A/A % |
|---|---:|---:|
| 1000/prepared_ns | 10.231 | 1.247 |
| 1000/copy_ns | 19.269 | 3.325 |
| 10000/prepared_ns | 15.346 | 2.336 |
| 10000/copy_ns | 23.067 | 3.854 |
| 100000/prepared_ns | 10.912 | 3.921 |
| 100000/copy_ns | 21.166 | 2.787 |

Strong diagnostic finding: time inside System for the544B request dominates the traced clone; key allocation and Arc/value copying are not the dominant measured component. The request is consistent with the single BTreeMap row-node allocation for this9field shape; exact glibc internal cause/fragmentation/cache behavior has not been proven. Do not infer a causal allocator mechanism solely from layout/service timing. Independent repeats/grouped medians are retained; medians do not add.

The tracer perturbs execution: copied-row overhead approximately19–23%, prepared overhead11–15%; time intervals include end clock-read overhead and System delegation, while atomic logging/TLS checks mostly lie outside per-event service intervals. Shared wrapper inactive branch also changes fixture execution. No causal cross-run latency ratio, unchanged performance, CPU-only percentile or memory/RSS claim. No peer acceptance follows.

Next: controlled intervention on the observed medium-sized System allocation path, preserving the full output and all engine work, plus heap-lifecycle/RSS and same-runtime peer controls. Existing rejected global MiMalloc insertion study remains rejected; this trace does not authorize adopting it unchanged. All eight named peers remain unqualified as a complete objective.
