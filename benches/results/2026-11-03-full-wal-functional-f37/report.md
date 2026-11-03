# FULL WAL smoke after restoring f37 runtime

The rejected caller-participation implementation was removed. The runtime closure now matches b9f10bc776b05eb5235237e9a900c2ef6aaed1fd (f37 equivalent). The new FULL WAL benchmark source remains unchanged from 140ae4941. An ordinary release functional check executed all four 1k/10k Lin/SQLite cases again under this restored runtime, preserving explicit sync/pragma checks and exact commit/reopen readback. All four cases completed. This verifies the harness with the restored runtime, not a repeated qualification of the rejected optimization.

One sample per case is functional evidence only. Raw data, diagnostic log and hashes are archived; no performance superiority or speed ratio is asserted. Separate repeated FULL WAL qualification remains open, as does the full eight-peer objective.
