# COW next API port

The user explicitly approved COW with a distinct public Row for the next API version. Work is isolated on codex/cow-row-vnext based on6812cc5ca. Four archived prototype files were three-way merged against04749994 with zero conflicts; source includes current transaction abort/generation/atomicity and schema recovery fixes. This retains lazy storage promotion, in-place UPDATE, field-level undo and bounded bitmap-prefix behavior. The separate WAL encoder candidate is not combined.

Default debug library/integration suite:443passed,0failed,2ignored. Retained ownership/model/isolation/bitmap oracles plus current transaction/crash regressions passed. Exact source hashes and merge outputs retained. cargo check --all-targets is still running at this checkpoint; do not claim it passed.

No fresh speedup is established for this port. Previous eager COW had failed controls; previous lazy Linux matrix was incomplete. Required next evidence: finish all-target API compilation, release validation, paired native/durable/read controls, memory/quota lifecycle and current named-peer matrix. The eight-peer objective remains unachieved.
