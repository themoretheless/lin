# Borrow stored rows in derived-maintenance wrappers: correctness/allocation prototype

Production047 Rust sources, exact BTreeMap Row alias retained. Change only four Store wrappers: index_insert_at, fts_insert_at, fts_remove_at, fts_sync_at. Borrow collections immutably while mutating disjoint scalar-index/FTS fields; avoid a complete stored Row clone. The scalar wrapper keeps the original BTreeMap index iteration and first-error boundary; FTS helpers call the same insert/remove/sync implementation. Missing-row/no-index/no-FTS behavior remains. This isolates wrapper borrowing and broadens it to index/FTS insertion/removal; it does not repeat the old combined borrowed-original-UPDATE+FTS candidate unchanged. UPDATE row construction, undo ownership, native Pack payloads, full output, embedding, public signatures and durability remain original.

Default library/integration suite407 passed,2 ignored. Exact source restored. No new clock/phase probes or allocator setting is added to production. Separate allocation example uses a process-wide counting System allocator, not a latency comparison or replacement backend. Six fresh processes(three per binary),nine cases each,54 observations:wing UPDATE,title UPDATE,DELETE at1k/10k/100k. Fixture setup/expected-row creation/readback are excluded from counting. Mutation execution and Handle destruction are counted; full owned result is validated against an independently constructed expected complete row between those count intervals. Affected count,generation,store state and scalar membership are checked too.

| Case(all sizes) | Baseline allocations | Candidate allocations | Requested-byte difference | Deallocation difference |
|---|---:|---:|---:|---:|
| Wing UPDATE |71|61|-582|-10|
| Title UPDATE |85|75|-582|-10|
| DELETE |71|71|0|0|

All three process replicates agree; independent audit reconstructs raw coordinates and medians. Results attribute one full row clone removed in the prepared UPDATE path. The other three wrappers are not independently isolated by this allocation fixture; their latency/lifecycle benefits are unproven. Requested bytes are cumulative allocation requests, not retained memory or RSS. No full-engine speedup or peer victory is inferred from the allocation decrease.

Next: matched normal release binaries and unchanged case-isolated native UPDATE/DELETE/transaction/native-insert/Full-durable/read controls, A/A, two six-round series, complete readback and RSS. No prototype rollout or gate relaxation. COW API decision remains optional and pending; the production Row alias stays untouched. All-eight-peer objective remains incomplete.
