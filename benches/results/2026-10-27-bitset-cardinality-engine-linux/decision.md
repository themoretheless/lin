# Exact-cardinality source gate: not accepted

Run [40465435654](https://github.com/themoretheless/lin/actions/runs/40465435654), head613dd7c823e49d2e9ef0537cc674665a79aeb09e. All1332 isolated processes and10656 wall observations completed; independent audit passed in CI and locally. Functional tests408 passed,2 ignored. CI failed only at the frozen source performance gate. Raw data, source snapshots, binaries' hashes, per-process peak RSS and controls are retained.

| Target | Series1 gain / absolute A/A | Positive | Series2 gain / absolute A/A | Positive |
|---|---:|---:|---:|---:|
| Equality COUNT10k | +11.452% /0.393% |6/6| +8.746% /5.572% |6/6|
| Range COUNT10k | +8.100% /1.684% |6/6| +11.479% /6.861% |5/6|

Equality COUNT clears the predeclared target criterion in both series. Range COUNT fails series2 because round10 loses14.747%; this observation is retained, not removed. Four control series fail: SQLite delete1k series1 -2.948% vsA/A1.670%; SQLite delete10k series2 -2.746% vs2.143%; SQLite rollback1k series1 -4.617% vs2.544%; Lin Full-durable insert1k series2 -9.253% vs3.931%. These data do not identify an intrinsic source cause for every control shift, but they prohibit accepting the candidate under the unchanged gate.

Candidate Lin/SQLite numeric wins45/204:36/36 in the three read cases and9/48 insert comparisons; original mutation/transaction cases remain0/120. Candidate reads win36/36 against DuckDB. Query/source/fixture/host scope is fixed in protocol.md; these are numeric process wins, not a claim about arbitrary queries or all named peers. Native1k, Full-durable10k and prepared mutations still leave substantial SQLite gaps.

The median whole-process RSS changes across74 case-series range from-4.146% to+1.045%. Opaque BitSet and posting enum types grow32→40bytes in the component layout measurements, but these process peaks neither establish a retained-memory bound nor prove that layout caused the durable regression. Count gets faster while global metadata and write bookkeeping costs remain unresolved.

Do not promote or rerun this exact candidate unchanged. A distinct next hypothesis can preserve the original posting layout while retaining cardinality, or confine caching to read aggregation; either requires fresh invariants, memory/lifecycle checks, and full-engine controls. Preserve API/owned output/index/FTS/embedding/rollback/durability semantics. Production source remains unchanged. Full superiority over SQLite, DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas is not achieved.
