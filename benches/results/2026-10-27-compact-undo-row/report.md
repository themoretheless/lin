# Compact undo field-name snapshot prototype

Internal RowUndo only, main205 baseline. Public Row remains BTreeMap<String,Cell>; Handle rows, Pack payload, journal accounting, wire format and Store collections are unchanged. Snapshot uses Vec<(lossless InlineStr-or-Arc field name, cloned Cell)>. Success drops this owned snapshot inside normal operation; rollback reconstructs String-key rows before existing restoration. No release or allocation moved beyond returned Handle timing.

307 tests pass, two existing ignored. Coverage includes transaction/failed multirow rollback, unique partial indexes, WAL replay/checkpoint, quota/journal contracts, deletes and snapshot/security suites. Additional exact roundtrip oracle covers all Cell variants including float NaN payload and negative zero, Unicode/empty/NUL names and 22/23-byte limits. Intentional long-name corruption fails that oracle. Production source restored byte-for-byte after test and mutant runs.

Separate full-operation allocation diagnostic: three processes per binary, nine operations per process, 54 exact validations. Wing update 71 -> 62, title update 85 -> 76, delete 71 -> 62 at every 1k/10k/100k size, 150 fewer requested bytes. Counts include Handle drop and rollback snapshot drop, exclude fixture setup/readback. No latency measurement yet. The much larger standalone Row clone win does not carry over to whole-engine latency automatically.

Rollback conversion allocates new String keys and may increase failure/rollback cost; long-name-only capture, explicit rollback, multirow and memory budgets require focused performance controls. Candidate is not production-enabled and does not establish a win over SQLite or any named peer.
