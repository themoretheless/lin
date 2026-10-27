# Naive bitmap empty-check incremental warm diagnostic

Six rotated A/A/B rounds,18processes;16samples of512prepared alternating updates plus one rollback per size, exact readback. Both variants use integrated COW on main047; only scalar-index emptiness differs. UPDATE -1.278/+1.976/+11.977% at1k/10k/100k, A/A2.871/1.962/0.497%. Large warm benefit does not override sparse/empty kernel regressions retained separately. Not accepted, no unchanged retry and no peer/fresh-fixture claim.
