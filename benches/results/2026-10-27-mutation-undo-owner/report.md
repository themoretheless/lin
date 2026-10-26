# Original update row owned by undo journal: saved prototype

Extends the previous borrowed-row candidate: move the original stored Row into RowUndo::Update, then borrow it from the last journal entry for FTS/HNSW/map maintenance. When no row journal is active, borrow the owned local original instead. Rust enforces the lifetime; no unsafe code or persistent-format/API change. Undo has the complete original row before any fallible index maintenance. Pack payload and accounting remain intact.

276 focused tests passed (2 existing ignored), including partial-index transitions, rollback, WAL replay/reopen, grouping and query semantics. Two additional regressions preserve native max_journal_bytes and affected counts after an empty final query. Production source restored byte-for-byte; manifests and candidate snapshots retained. Allocation experiment is recorded in ../2026-10-27-mutation-undo-owner-allocation/report.md:30 allocations/1746 requested bytes saved per measured update, unchanged delete. This is not a latency qualification and the candidate is not enabled.

Linux run40394312683 currently measures the earlier borrowed-row snapshot, not this added ownership transfer. Await its complete results, then qualify the incremental candidate separately with unchanged controls and full durable/native gates before claiming a performance win.
