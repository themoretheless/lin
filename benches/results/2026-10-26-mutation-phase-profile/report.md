# Mutation phase profile, frozen production core e05ac74b

Three processes per update/delete scenario at1k/10k/100k. Eight measured fresh-fixture operations per run plus validation/calibration operations; all11 phase events per run retained in phase-summary.json, including preflights. Total18 completed runs. Profiled executable is instrumented: phase clock/TLS overhead and logging are not performance qualification, and these macOS ARM numbers are not a causal comparison to Linux CI. The first cold lookup and all outliers remain present.

Instrumentation accumulates spans and prints them at operation exit. Nested exec_program/total spans overlap; medians cannot be added or subtracted to infer exclusive work. Source restored after build; raw logs, source snapshot and executable hash retained.

First-process medians (ns):

| Scenario | Total | Lookup | Derived/index | FTS | Row maps |
|---|---:|---:|---:|---:|---:|
| update1k |4708|250|417|542|333|
| update10k |8959|834|1083|833|875|
| update100k |11750|1292|2417|1000|1291|
| delete1k |6542|459|4000|included|included|
| delete10k |10208|1083|6042|included|included|
| delete100k |17666|1334|11125|included|included|

`before_clone`, `updated_clone`, `store_clone`, and `pack_result_clone` are separately recorded for updates. FTS sync clones the current Row before checking whether indexed fields changed. Ownership review supports removing that clone through disjoint borrows and retaining the original stored row with mem::replace, while preserving an owned undo snapshot. This is a hypothesis for matched full-engine testing, not an accepted speedup.

Do not elide native Pack rows indiscriminately: explicit transactions account their bytes against max_journal_bytes, and a multi-statement program ending in an empty result can derive Done.n from Pack. The first row-copy prototype was rejected on this contract review before qualification.
