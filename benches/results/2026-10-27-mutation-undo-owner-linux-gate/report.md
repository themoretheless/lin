# Frozen Linux qualification inputs: undo ownership candidate

Separate experiment from failed borrowed-row6ac970ef. Baseline is current production Rust20591dd4; candidate snapshots come from30ee3dd6 and additionally move the original update Row into undo. Local278tests and54allocation-counter operations pass. This stage contains input scripts/snapshots only; no latency result yet.

Same original acceptance gate, same20native cases,8samples/case,12rotated A/A/B rounds and unchanged SQLite/delete/transaction controls. Copy inputs to a fresh owned CI directory; never overwrite old benchmark measurements. Build paths are separately pinned and hashed. Both baseline and candidate source files are restored after temporary builds/tests. Full durable performance and all-eight-peer qualification remain separate requirements.
