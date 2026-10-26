# Fused FTS swap-delete: allocation evidence

Production Rust20591dd4 versus FTS/Store snapshot339290fe only. Three independent processes per binary, nine cases each (wing/title/delete at1k/10k/100k),54validated fresh-fixture operations. Global System allocator counts allocation/reallocation requests, requested bytes and deallocations inside Prepared::run including Handle drop. Setup/readback excluded; no timing/RSS claim.

Fixture uses the actual comparison's title shape: every tenth document has `wal note`, others `plain`; deleted probe d-500 and moved final row differ in those terms. Timestamps are fixed exact Time values rather than wall-clock offsets; this is a counting diagnostic, not a replacement of the benchmark contract. Hash/changed field, affected count/generation, row existence/count and scalar-index count are validated outside counting.

Every delete:71→64 allocations,487fewer requested bytes. Update controls unchanged: wing71allocations, title85allocations. Requested-byte totals vary with the final row's token length; the paired reduction is487at every size/process. Differences versus the earlier all-plain title counting fixture are not an optimization result.

No unaccepted undo ownership or bitmap-empty changes included. Original source restored after builds. Functional/oracle qualification is in ../2026-10-27-fts-swap-delete/report.md (290tests and mutant rejection). Full-engine latency qualification is a separate ongoing Linux run40408513563; these counts do not establish a SQLite win or justify default enablement.
