# Schema commit crash recovery with stale HEAD

Baseline906647977 fails four actual subprocess-exit regressions with unchanged production sources: autocommit and interactive schema+row writes,both without a snapshot and following an older checkpoint. The child successfully commits Full-sync WAL then exits without destructors. Writable reopen replays the committed schema but rejects it because HEAD.catalog_hash belongs to an older generation. Full baseline failures and exact core hashes are preserved.

## Correction

Validate HEAD's catalog against the state at HEAD's generation: the snapshot-loaded catalog when generations match,or the corresponding replayed WAL state when HEAD lies inside the retained WAL. Later valid schema records do not invalidate that earlier catalog hash. A newer snapshot may supersede an older HEAD only when snapshot.catalog_hash matches the independently reconstructed snapshot catalog. Unanchored cases retain the existing strict final-catalog comparison. Existing snapshot compatibility checks remain in place.

No checksum,file format,commit sync,checkpoint interval,publication order or directory barrier changes. Hot write operations are unchanged; writable open now computes catalog hashes at the relevant generation. Read-only open is unchanged. This is a correctness repair before considering a publication-barrier optimization; it does not prove such an optimization safe.

## Validation

Ten targeted tests pass. Four reproduce actual process exit after schema commits and verify recovered collection and score7. Two model filesystem publication states: snapshot ahead of HEAD with retained WAL;HEAD generation inside WAL before later schema records. These models use normal durable checkpoint output and restore owned prior metadata/WAL bytes,not hardware power interruption. Three negative controls reject corrupted same-generation HEAD,corrupted reconstructible stale HEAD and an unverified newer snapshot. The child helper is the tenth test.

Full local default release library/integration suite:**430 passed,2 ignored,zero failed**. All previous aborted-transaction and WAL-generation/crash regressions pass. Source/test hashes and complete logs retained. This does not establish a performance improvement or qualify older frozen KEEP_SIZE timings as this newer source. Linux qualification remains separate.
