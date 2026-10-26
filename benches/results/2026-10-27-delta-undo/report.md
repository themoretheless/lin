# Full-row-difference delta undo prototype: rejected

Main205 baseline. Internal Update undo payload SmallVec<(String,Option<Cell>)> records old values of changed/removed fields and absence of added fields. Deletion undo remains full Row. Rollback clones current row, applies inverse delta, then moves restored row through owned restoration helper. Public API, Pack/journal accounting and wire format unchanged. Pointer identity fast path avoids scanning unchanged Arc payloads (including embeddings).

290 tests pass, 2 existing ignored. Dedicated oracle restores changed/added/removed keys, more than four edits (spill path), body/hash, signed zero, Unicode names and two edits reversed in order. Deliberately omitting added-field undo fails oracle. Initial pre-pointer-shortcut test logs preserved. Sources restored exactly after all runs.

Rejected after CPU diagnostic: UPDATE regresses10.66/8.12/6.50% with0/6 positive at every size, despite rollback improvements11.31/13.69/12.87%. Whole-row diff scans and larger undo payload make this design unsuitable as measured. Do not hide successful-update losses behind rollback gains. No full-engine/peer acceptance. A distinct design could derive undo only from actual patch fields and implicit body/hash edits; it must retain exact missing-field/repeated-edit semantics and all controls.
