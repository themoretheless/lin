# Lazy COW fresh-row lifecycle: allocator evidence

Exact COW core used by Linux00c8aa9a3 vs corrected BTreeMap6812cc5ca,release macOS ARM64.18fresh processes;90allocator snapshots have independently balanced alloc/free/realloc ledgers. Full9-cell results plus supplied id/uri/body and exact row count/order validated. System-delegating counting allocator; no timing or RSS claim. Same explicit1GiB output quota; default-quota success is not established.

|Rows|Seed live delta B|Held query live delta B|Query allocation delta|Query requested delta B|Result-drop live delta B|Db-drop live delta B|
|---|---:|---:|---:|---:|---:|---:|
|1000|8000|8000|0|8000|0|0|
|10000|128000|80000|0|80000|0|0|
|100000|1024000|800000|0|800000|0|0|

Medians across3fresh processes per role/size; all individual records retained. COW Row descriptor32bytes vs BTreeMap24bytes. Fresh rows are Inline and still copy their map on result cloning: no eager-COW reduction in fresh materialization allocations. Query live/requested-byte overhead equals8bytes per returned Row. Seed overhead depends on collection descriptor capacity.

The zero deltas after result/Db Drop mean no additional COW retention in these windows, not zero absolute global/cache retention or a general no-leak guarantee. Absolute counters and all realloc old/new sizes remain in raw JSON. Corrupted ledger/readback/missing-process inputs are rejected by the analyzer using temporary copies.

pre-ledger-build contains an unmeasured compiled harness revision before realloc-old-size accounting was added. All18observations use the final ledger-capable binary and pinned gate hash. Production source restored; full engine/peer objective remains unproven.
