# Linux owned-row copy diagnostic: iterator not selected

GitHub40454702148, headf433792642eca7bacc52bdebb277bd9d9736764b, completed diagnostic success. Diagnostic selection=false, engine_qualified=false, full objective=false. This green workflow verifies complete data, not an accepted optimization. First missing-helper preparation run40454669295 is separately retained with failure log; it produced no timings and was corrected before this run.

Exact failed Map/static-output source, default System allocator, real checked UPDATE fixtures, complete returned BTreeMap retained. One release binary selects std clone/control std or iterator clone+collect at startup. Common call/branch and28RAII probes change cost. Manual prepared timing excludes added full-row readback, framework wall includes it. Inputs/compiled instrumented source hashes,39process IDs/start times/status, CPU/load/toolchain environment and every raw record retained. No production source/API/allocator change.

Independent verify.py passes:39processes,1287phase records,936wall observations; expected roles/modes/cases, all raw phase JSON/log equivalence, all per-process medians and paired series values, staged decomposition and selection flag rederived. Preflight all Cell variants/Unicode/special float/vector bits/MessagePack/isolation and real complete UPDATE row checks passed.

| Rows/metric | Series | Iterator gain % | Absolute A/A % | Positive | Selection gate |
|---|---|---:|---:|---:|---|
| 1000/prepared_ns | series1 | -11.473 | 8.927 | 2/6 | False |
| 1000/prepared_ns | series2 | -7.681 | 17.572 | 2/6 | False |
| 1000/copy_ns | series1 | -6.823 | 1.064 | 0/6 | False |
| 1000/copy_ns | series2 | -9.355 | 3.109 | 0/6 | False |
| 10000/prepared_ns | series1 | -6.391 | 1.177 | 0/6 | False |
| 10000/prepared_ns | series2 | -3.018 | 4.059 | 1/6 | False |
| 10000/copy_ns | series1 | -12.479 | 1.896 | 0/6 | False |
| 10000/copy_ns | series2 | -9.170 | 3.568 | 0/6 | False |
| 100000/prepared_ns | series1 | -3.313 | 3.619 | 2/6 | False |
| 100000/prepared_ns | series2 | -9.291 | 3.522 | 0/6 | False |
| 100000/copy_ns | series1 | -5.011 | 4.828 | 0/6 | False |
| 100000/copy_ns | series2 | -13.276 | 3.687 | 0/6 | False |

## Staged diagnostic, median of three process medians

| Rows | Prepared us | Copy us | Keys us | Values us | Assembly us |
|---|---:|---:|---:|---:|---:|
| 1000 | 17.997 | 6.530 | 0.611 | 0.591 | 4.808 |
| 10000 | 27.411 | 11.988 | 0.802 | 0.931 | 9.875 |
| 100000 | 49.846 | 23.766 | 1.863 | 1.232 | 19.960 |

Iterator clone is slower in all36 copy process-size comparisons; no promotion or unchanged retry. Prepared1k has substantial A/A variability, explicitly retained. No peer speed claim.

Staged assembly dominates its diagnostic path, while key allocation/copy and Cell/Arc clone are comparatively small. Assembly includes collection from owned small vectors, any temporary buffer/sorting/tree allocation and construction; it is not a direct measurement of standard-clone node allocation alone. Extra per-stage timers/traversals mean staged and standard/iterator wall values are not fair candidate alternatives; nested medians do not sum. These figures motivate allocation-event tracing of actual std BTreeMap clone on Linux rather than a global allocator swap or source-incompatible Row change.

No Full durable/native insertion/read/SQLite control, RSS or p99 qualification here. The previous720process comparison still shows0/120SQLite wins for the Map candidate. All eight named peers remain the full unachieved objective; no narrowing to this diagnostic.
