# Disjoint Store wrapper borrowing: native UPDATE source gate fails

Run [40497180805](https://github.com/themoretheless/lin/actions/runs/40497180805), frozen head1d29dcdf78853e80a2efa5a687998ebed7edecc5. All1332 isolated processes and10656 wall observations completed. Independent raw audit passes in CI and locally. Default library/integration tests407 passed,2 ignored. CI fails only at the predeclared source performance gate. Source/binary/benchmark hashes, every losing control and process peak RSS retained.

| UPDATE target | Series1 gain / absolute A/A | Positive | Series2 gain / absolute A/A | Positive |
|---|---:|---:|---:|---:|
|1k|+9.308% /8.698%|6/6|+0.741% /2.423%|5/6|
|10k|+2.422% /3.253%|4/6|+0.953% /1.964%|5/6|
|100k|+0.322% /8.413%|3/6|+4.933% /6.813%|4/6|

Only1k series1 passes; the other five target series fail. Four unchanged SQLite control series also fail: DELETE1k series2 -4.330% versus A/A1.503%; UPDATE10k series1 -6.700% versus4.118%; DELETE10k series2 -3.328% versus3.301%; DELETE100k series2 -4.961% versus4.495%. Small control-bound violations are retained at full precision, not rounded away. These observations do not establish source causality for the external-engine shifts, but prohibit acceptance under the declared gate.

Candidate UPDATE median request latency is approximately6.903/6.918us at1k,19.975/18.498us at10k and38.020/38.193us at100k across the series. Median per-round Lin/SQLite ratios are2.006/2.037,3.706/3.922,1.674/2.071 respectively. Mutation/transaction comparisons remain0/120 Lin wins. All-case SQLite48/204 and read DuckDB36/36 numerical wins do not establish arbitrary-query or all-peer superiority.

Earlier independent allocation evidence remains valid: native wing/title UPDATE saves10 allocations and582 requested bytes at all three sizes, with complete owned result and Handle destruction checked. It does not substitute for reproducible engine latency. DELETE allocation counts were unchanged; the other index/FTS wrappers are not individually timed by the prepared-update target. Whole-process median RSS deltas across74 case-series range-4.562%..+1.226%, not a retained-memory proof.

Do not promote or rerun this exact candidate unchanged. Public Row stays the exact BTreeMap alias; no production source, result/Pack payload, undo ownership, index/FTS/embedding work, durability or runtime tuning changed. The primary write gap remains. The archived qualified COW native prototype has a public API tradeoff and broader unqualified cases; the pending user API question is not treated as approval. Broader prototype investigation can proceed without changing the production contract. Full eight-peer objective remains active and incomplete.
