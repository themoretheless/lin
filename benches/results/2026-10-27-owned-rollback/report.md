# Owned row rollback prototype

Independent main205 experiment, no compact undo or rejected mutation optimizations. RowUndo owns a complete String-key Row. Private Store::restore_row_owned moves it into the slot and takes ownership of current row, retaining one cloned restored view for derived index/FTS/map maintenance. Original public restore_rows remains unchanged. Journal accounting/API/format remain unchanged. Both Rows and Mixed undo paths use the new helper.

289 tests pass, 2 existing ignored, including transactions, mixed rollback, partial unique indexes, WAL/reopen, mutation contracts and delete/FTS/HNSW regression suites. Production source restored exactly after testing and builds.

First repeated-row diagnostic improves rollback but unchanged UPDATE controls regress beyond A/A at 10k and100k. This is not accepted for production; see ../2026-10-27-owned-rollback-warm-cpu/report.md. Do not combine it with compact snapshots and declare the combined result qualified without measurements.
