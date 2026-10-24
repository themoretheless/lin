# Shared WAL vector storage revisited: retained

Baseline source 63a48e89b211734262a87cda97c6fdae10cbb529. Revisited the rejected October 1 experiment because later row-major packing, parallel size classification/encoding and changed WAL buffering moved the measured bottleneck: ../2026-10-24-durable-profile/report.md. The old result remains a rejection on its own source; this is not a reinterpretation of its samples.

ColData is internal (persist module private). Its existing Vec variant now owns Option<Arc<[f32]>> per row instead of Option<Vec<f32>>. Packing clones Cell's existing vector Arc; unpacking shares the same allocation. No additional enum variant, codec, public Row/Cell/API, model, index, FTS or sync change. Serde's existing Vec wire shape stays the same. Writer/reader retain the original conservative size_of<Option<Vec<f32>>> logical decoded-budget accounting, deliberately not replacing it by the smaller Arc metadata size. Actual peak RSS is not measured; no 64MiB physical-RSS promise or memory-reduction ratio is claimed.

Validation: 73 lib tests pass, 2 ignored; full cargo test --workspace exits zero (workspace-status.json). Tests verify legacy named MessagePack bytes/readback, original scalar sparse byte reference including special f32 bits/empty/None, parallel worker fallback, shared budget rejection before file modification, and Arc pointer identity from source Row to packed column and reconstructed Row. Internal fixture constructors were adapted to the new container without changing expected values; early compile logs are preserved. No optional-feature/hardware coverage beyond the actual test command is claimed.

Two independent six-pair series use the exact same pinned release binaries. Alternating baseline/candidate process order, native and Full durable 1k/10k, 24 fresh samples, one operation/sample, zero warmup, existing exact fixture validation outside timing. No builds/tests initiated by the runner overlap timing; host load/clocks uncontrolled. Positive paired gain means faster; wins are process medians.

| Case | First paired gain / wins | Repeat paired gain / wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | +3.876% / 4/6 | -1.165% / 2/6 |
| compare/insert_bulk_1k/sqlite | +3.389% / 4/6 | +0.758% / 4/6 |
| compare/insert_bulk_10k/lin | -13.281% / 3/6 | -0.583% / 3/6 |
| compare/insert_bulk_10k/sqlite | -57.571% / 1/6 | -0.204% / 3/6 |
| compare/durable_insert_1k/lin | +6.375% / 4/6 | +11.334% / 6/6 |
| compare/durable_insert_1k/sqlite | +2.815% / 6/6 | +6.961% / 6/6 |
| compare/durable_insert_10k/lin | +4.063% / 6/6 | +3.046% / 4/6 |
| compare/durable_insert_10k/sqlite | -0.215% / 2/6 | +1.439% / 4/6 |

Durable gains repeat, though SQLite also improves in several cases and not all measured Lin gains can be assigned to this change. Durable 1k: +6.375% (4/6) then +11.334% (6/6), controls +2.815%/+6.961%. Durable 10k: +4.063% (6/6) then +3.046% (4/6), controls -0.215%/+1.439%. Native results are mixed; first native 10k SQLite control shifts -57.571%, so that round is not evidence of a candidate-caused native regression. Repeat native 1k -1.165% (2/6), native 10k -0.583% (3/6); first native 1k +3.876% (4/6). No native acceleration or absence of every possible small regression is claimed. Retention rests on two repeated durable improvements, compatibility/ownership proof and full workspace validation; native 1k remains a workload to monitor.

The full named-peer objective remains incomplete. Repeat candidate medians: native 1k Lin 0.841521 ms / SQLite 0.796073 ms; durable 1k 2.381083 / 1.527791 ms; durable 10k 15.478865 / 14.123031 ms. SQLite gaps persist. These bulk writes are distinct from official single-row transaction cases. MSSQL/Kusto access, remaining write coverage and current all-peer verification are still required.

Text logs have only trailing blank lines normalized; raw observations and source snapshots unchanged.
