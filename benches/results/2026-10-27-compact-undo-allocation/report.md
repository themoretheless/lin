# Compact undo full-operation allocation diagnostic

Three independent processes per mode, nine fresh-fixture operations each, all 54 exact row/index validations pass. Every size 1k/10k/100k saves nine requests and 150 requested bytes: wing 71 -> 62, title 85 -> 76, delete 71 -> 62. Prepared::run and Handle/snapshot drop included; setup and readback excluded. This measures allocator requests, not RSS, peak memory or latency.

Source/binary hashes and full raw retained. Production source restored exactly. No earlier rejected FTS, borrowed-row, undo-owner or index candidate included. Public API and WAL format unchanged. Rollback conversion costs and full-engine paired latency still require qualification; no production rollout or peer-speed claim.
