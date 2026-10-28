# Atomic index coverage without temporary allocations

Candidate884ebeb81 is isolated from the pending DELETE-WAL contender andallprevious rejected optimizations. Baseline isa7d4b02c3COW+CAS. Only index.rs changes runtime behavior. SingleCmp+singleIndexUse checks partial predicates andnumeric precision directly;compound/other predicates keep the unchanged original bounded DNF. Single-segment field names borrow;dotted fields join once. The original conjunct/DNF helper remains unchanged as the regression oracle.

Three focused release tests pass:720atomic combinations,AND/OR branch alignment,anddotted/Unicode/compound partial predicates. The full release library/integration suite passed452tests,2ignored. Paired timings remain pending.

A System-delegating counter measures only1000coverage calls per fresh process.24processes cover4shapes×2source roles×3repeats;all repeats match. Shape construction,counter reads andJSON are outside the measured counter interval.

|Shape|Baseline allocations|Candidate allocations|Baseline requested bytes|Candidate requested bytes|
|---|---:|---:|---:|---:|
|leaf|6000|0|142000|0|
|dotted|7000|1000|179000|10000|
|partial|6000|0|171000|0|
|compound|19000|19000|557000|557000|

This diagnostic uses an identical temporary hidden shim inbothroles toaccess private planner types;theproduction API/library source was restored exactly andthehelper removed. The initial example could not access private modules;the compiler error/source are retained separately. Instrumented diagnostic binaries are not throughput binaries. Allsource/probe/shim/binary hashes are recorded.

No latency,RSS,full-source acceptance orpeer superiority is proved by fewer allocations. The previous DNF-bypass-only candidate failed its timing gate;these are distinct source changes anddo not turn that rejection into acceptance. Fresh uninstrumented paired timings with unchanged thresholds remain mandatory.
