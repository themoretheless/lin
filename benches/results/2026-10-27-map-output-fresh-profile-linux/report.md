# Linux profile of actual failed Map/static-output candidate

GitHub40452502025, head12dc18a4ac1435770f5bb2955c29da2de756f61c, completed success. This is diagnostic success, not performance acceptance. Four input source files exactly match candidate from failed isolated mutation run40448137140. Frozen baseline source matches main047. Full source/candidate/compare hashes checked before build, source restored after build and checked after measurement. Existing native checked callbacks, default fixture/index/FTS/embeddings/full output preserved. No new optimization in this profile.

Six fresh Linux release processes (three per kind),3sizes each,8 measured samples/case =144wall observations.198phase records include framework calibration/validation; retain all11records per size/process. Independent verify.py parses PHASE log lines, compares stored JSON, requires expected cases/operations/records, and recomputes every25-stage/prepared/drop process median and median-of-process-medians; passed. All raw data and instrumented sources retained.

| UPDATE rows | Prepared us | Full updated-row clone us | Scalar maintenance us | insert_key us | Row maps us | Positions us |
|---|---:|---:|---:|---:|---:|---:|
| 1000 | 10.626 | 5.108 | 0.971 | 0.640 | 0.611 | 0.590 |
| 10000 | 22.063 | 10.596 | 2.203 | 1.482 | 1.242 | 1.543 |
| 100000 | 45.367 | 21.532 | 5.027 | 3.655 | 1.813 | 3.205 |

| DELETE rows | Prepared us | Store delete us | Tail move us | FTS move us |
|---|---:|---:|---:|---:|
| 1000 | 14.692 | 6.529 | 3.325 | 2.353 |
| 10000 | 26.940 | 10.906 | 4.877 | 2.864 |
| 100000 | 55.933 | 22.263 | 11.497 | 6.850 |

Output clone dominates roughly half of observed Linux UPDATE and is much larger in absolute time than the earlier macOS diagnostic. Do not infer a causal cross-platform ratio: host, architecture, compiler, allocator/cache state and probe effects differ. No explicit CPU-model/frequency capture exists in this run; platform string Linux-6.17.0-1022-azure-x86_64-with-glibc2.39 and load snapshots are retained. Nested medians do not sum, timers change cost, and three cases share each process, unlike the720case-isolated acceptance protocol. This is not peer qualification or p99/RSS evidence.

Source audit: Cell::Text and Cell::Vec hold Arc; clone does not deep-copy embedding floats. The remaining clone owns standard String keys and BTreeMap nodes and increments relevant Arc references. Next diagnosis should separate those costs on Linux using exact owned output, rather than optimize small key-construction cost or change API/output payload. Old global MiMalloc insertion prototype was rejected; no allocator switch is accepted from this profile. COW newtype remains a separately measured, source-incompatible prototype and is not silently substituted for frozen Row=BTreeMap. Full-eight-peer objective remains false.
