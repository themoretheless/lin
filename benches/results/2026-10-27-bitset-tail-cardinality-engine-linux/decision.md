# Tail-word cardinality: COUNT targets pass, full source gate fails

Run [40488220316](https://github.com/themoretheless/lin/actions/runs/40488220316), frozen head236eb8ba9536c8ca8e026354719b6323d7fb4b35. All1332 isolated processes and10656 wall observations completed; independent audit passes in CI and locally. Functional tests408 passed,2 ignored. CI fails only at the predeclared unchanged source performance gate. All raw data, control losses, source/benchmark snapshots, source/binary hashes and process peak RSS are retained.

| COUNT target10k | Series1 gain / absolute A/A | Positive | Series2 gain / absolute A/A | Positive |
|---|---:|---:|---:|---:|
| Equality | +17.906% /1.594% |6/6| +18.347% /1.720% |6/6|
| Range | +15.171% /2.654% |6/6| +14.454% /2.526% |6/6|

Both read targets pass in both series. Median equality baseline/candidate request latency is1.603/1.325us and1.601/1.315us; range2.365/2.003us and2.337/2.006us. These are measured same-run source comparisons, not cross-run comparisons with the previous inline candidate.

Six control series fail. Lin UPDATE1k series2 regresses2.405% versus A/A0.723%; Lin UPDATE10k series2 regresses2.723% versus0.799%. SQLite commit10k series1 loses3.505% versus1.693%; SQLite rollback10k series1 loses4.861% versus2.226% and series2 loses2.753% versus0.583%. DuckDB equality COUNT series1 loses5.036% versus3.496%. Unchanged external-engine control shifts do not establish a cause, but every declared control is part of acceptance. No case is dropped and no bound is weakened.

Candidate numeric peer wins: SQLite48/204, comprising36/36 read cases and12/48 insert cases (all12 Full-durable1k). Mutation/transaction comparisons remain0/120. Read DuckDB36/36. Native insert1k remains about1.14..1.16x SQLite latency, native10k1.69..1.70x, Full-durable10k1.51x. Existing read superiority does not close write or all-eight-peer gaps.

Whole-process median RSS changes across74 case-series range-0.750%..+2.286%. This does not prove retained-memory bounds or explain timing shifts. The tail candidate preserves32byte BitSet/posting model but adds a heap metadata word and changes Vec growth capacities; earlier detailed layout observations remain separate. Same-run Full-durable1k control passes here, but that does not prove why the inline candidate failed on another host/run.

Do not promote or rerun this exact candidate unchanged. The COUNT benefit is now repeatable within this run, while write bookkeeping/storage effects and broad source controls remain unresolved. Any alternative representation or read-local strategy requires independent invariants and fresh unchanged full-engine controls. No production source, API, result payload, index/FTS/embedding work, rollback, allocator setting or durability contract changed. Full superiority over SQLite, DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas remains unproven.
