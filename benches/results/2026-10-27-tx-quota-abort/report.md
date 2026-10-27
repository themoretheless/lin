# Failed interactive transaction cannot commit

Baseline source: 6d518a8a74618b9af0d0c4d88095daca807d7dee. Both regressions fail at runtime with unchanged core sources (baseline.log and baseline-status.json).

## Reproduction

Set max_tx_mutations=0 or max_journal_bytes=0 after a durable seed. Execute one UPDATE inside an interactive transaction. run returns a quota error, but commit previously returned success; wing remained sys while generation stayed 1 and exported WAL after the rejected write was empty. This allowed memory state not represented in the committed log.

## Correction

Record an aborted state on failed run preparation, execution or quota checking. Refuse subsequent run/run_prepared/prepare/commit. Explicit rollback and Drop continue to restore the full transaction, including any earlier accepted packs. A standalone successful transaction is unchanged apart from the state guard. No sync mode, quota value or error threshold is relaxed.

## Validation

Full local release library/integration suite: 420 passed, 2 ignored, zero failed (full-tests.log). Five dedicated regressions cover both rejected quotas, failure after an accepted pack, stale-CAS execution failure, subsequent prepared execution rejection, explicit rollback and Drop after parse failure, no new WAL and generation continuity for the next normal write. Existing eight WAL/subprocess recovery tests also pass.

The Linux reserve experiment is frozen at an earlier source and continues independently. Its result cannot qualify this newer execution source automatically. This is a correctness repair, not a claimed performance improvement. Peer superiority and real Linux validation of this follow-up remain open.
