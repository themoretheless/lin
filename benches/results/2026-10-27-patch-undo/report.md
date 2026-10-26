# Patch-driven undo snapshot prototype

Main205 baseline, public Row/API/Pack/WAL/journal accounting unchanged. UPDATE undo stores prior values only for actual patch fields plus implicit hash when body recomputes it. Missing prior field represented as None; explicit hash deduplicated. SmallVec inline capacity1 keeps single-field event smaller. DELETE undo unchanged. Rollback applies inverse patch to current-row clone then passes restored ownership to Store helper; no full-row diff scan, compact-key conversion or cold attributes.

291 tests pass, 2 existing ignored. Oracle covers added fields, body/hash implicit and explicit, absent hash, signed zero, NaN payload, long Unicode names, repeated reverse order and spilled multi-field patches. Mutant treating absent as Null fails. Sources restored after tests/builds.

First six-round CPU diagnostic passes all six UPDATE/rollback targets with6/6 positive pairs and gains above A/A. UPDATE6.51..7.61%,rollback17.44..19.66%. This is not fresh-fixture Linux, Full durable/read/peer qualification. New paired Linux matrix retains DELETE,transaction and SQLite controls unchanged. No production rollout yet.
