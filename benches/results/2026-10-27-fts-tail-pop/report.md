# Known-tail FTS base pop prototype

Independent hypothesis on main205; does not include the rejected fused token merge. Only Store deletion's known tail-move caller uses move_tail_row. General move_row is unchanged. Cancel pending additions first; if base.last() proves the old tail position, pop it directly, remove empty base key and cancel its pending deletion. Otherwise retain the existing tombstone/folding fallback. Destination addition uses existing semantics.

290 tests passed, 2 existing ignored. Stateful rebuild oracle covers update/delete/reinsert/folds, Unicode, large token fixtures and draining to zero. Intentional missing base.pop mutant fails the oracle, proving sensitivity to stale tail retention. Production source restored byte-for-byte after each run.

No latency or allocation result yet. Candidate is saved for qualification and is not enabled in production. Full paired native/Full durable/read/peer controls remain required; this does not establish the eight-peer goal.
