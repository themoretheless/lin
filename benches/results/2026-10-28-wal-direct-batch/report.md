# Direct MessagePack Batch prototype

Candidate directly encodes UPDATE/DELETE and up to16 Batch containers; deeper Batch subtrees and unsupported Pack variants use unchanged serde. Canonical16MiB rejection includes accumulated savings, public values/Row alias and durable sync unchanged.

CPU gate passed12fresh processes,396blocks,1584000encodes: single-UPDATE Batch median gain51.618/51.333%, mixed UPDATE/DELETE Batch50.055/50.904%; every targetwindow6/6positive aboveA/A. All controls passed. Independent CPU recomputation passed. These are serialization measurements only; absolute timings should not be compared with other harness revisions.

Full release suite439passed/0failed/2ignored; negative-zero mutant caught at runtime. Nested oracle covers depths0,1,15,16,17,24 and canonical size rejection on both compacted/fallback paths. Initial off-by-one depth expectation and initial fixture logs are retained. Legacy-reader compatibility passed separately (../2026-10-28-wal-direct-batch-legacy-reader/report.md).

Full-engine latency and whole eight-peer objective remain unproven. This candidate has not been accepted into production.
