# Current durable WAL stage profile

2026-10-24, production source `a989b5ab544943f9df97110fd2a4907be29d4655`. Temporary instrumentation only; exact before/instrumented persist.rs and source/binary hashes retained. Production source restored before running and verified byte-identical to HEAD afterward. No runtime/format/sync change landed. This is diagnostic evidence, not a speedup or peer win.

Three independent pinned release compare processes, Lin durable insertion at 1k/10k, 24 fresh measured samples plus preflight (27 typed events per size per process), one operation, no warmup, max iterations 1. Existing exact fixture readback gates passed; all run.json complete. No builds/tests initiated by this runner overlap measurement; other host activity uncontrolled. Input setup/schema/preparation/validation/drop excluded from benchmark timers; diagnostic logging/timers perturb execution. No new workspace test required for restored profiling-only edits.

Median of three process medians:

| Phase | 1k ms | 10k ms |
|---|---:|---:|
| pack_ns | 0.078291 | 1.435458 |
| classify_ns | 0.093292 | 0.403000 |
| encode_ns | 0.434041 | 1.681500 |
| checksum_ns | 0.032125 | 0.338250 |
| write_ns | 0.039167 | 0.196417 |
| sync_ns | 0.827417 | 1.414250 |

Classify is **inside** encode; do not add twice. Sparse serial span is also nested inside encode/classification-independent. Independent phase medians cannot be summed/subtracted to derive exact end-to-end time or causal improvements. At 1k, sparse serialization median is **0.298625 ms**, vector bytes **238,992** of full payload **336,968** (~70.9%). At 10k payload is **3,558,340** bytes. Parallel sparse worker spans at 10k overlap and must not be summed as elapsed wall time. Raw worker spans retained in sparse-and-sync.json.

All **486** observed Apple durable_sync calls used F_BARRIERFSYNC successfully; **0** fallback sync_data calls. Includes metadata/setup frames and measured data frames; frame metrics associate only immediately preceding typed encodes, avoiding accidental metadata pairing. Thus an unexpected fallback to stronger full sync is not the observed explanation for the small-batch gap. This is API-route evidence, not physical power-loss qualification or an equality claim about SQLite's platform implementation.

Current 1k pack (~0.078ms) is small; encoder (~0.434ms) and sync (~0.827ms) dominate these WAL stages. Do not weaken Full-sync or change peer pragmas to claim a win. Earlier bitmask and sparse reserve candidates were rejected; reports were inspected to avoid repeating unsupported ideas.

The old bit-exact vector dictionary size audit recorded 112,542 vector bytes versus 238,992 sparse bytes on this fixture shape. Current sparse-section byte count matches that historical audit, but the dictionary encoder CPU cost and current full-frame result remain unmeasured. If its old byte count reproduced, projected full payload would be 210,518 bytes (~37.5% smaller), **not** a latency prediction. A versioned dictionary codec with bounded raw fallback is the next justified experiment, gated by the attached plan. No new codec implemented or performance claimed here.

Durable 1k SQLite gap and unavailable MSSQL/Kusto remain open; named-peer objective unchanged. Existing read/native results are separate contracts and source-specific.
