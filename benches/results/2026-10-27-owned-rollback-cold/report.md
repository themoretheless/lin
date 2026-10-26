# Owned rollback with cold function separation

Separate hypothesis from b9acd6b1: add #[cold] and #[inline(never)] to Db::rollback and private Store::restore_row_owned. Main205 baseline, no compact undo, FTS or index prototypes included. 289 tests pass, two existing ignored. Production source restored after tests and builds.

Repeated-row CPU diagnostic: rollback improves20.16/20.55/21.01% at1k/10k/100k,6/6 positive each. UPDATE control10k still regresses2.019%, exceeding both1.887% A/A and2% tolerance. Therefore candidate remains unqualified. Do not round2.019% down to2.0% to pass a gate. Other sizes show +0.376/+0.330% within A/A. This does not prove machine-code layout as the unique cause of prior regressions.

All raw preserved in ../2026-10-27-owned-rollback-cold-warm-cpu/. No full-engine/Linux/Full durable/read/peer acceptance. No production change.
