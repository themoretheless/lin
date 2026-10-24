# LZ4 write + Full-sync prototype: rejected

Source `ed9af08144bf50a1a69b2e162d532f58832a2a3a`; no production change or new accepted codec. Follow-up to the actual-payload [CPU/byte gate](../2026-10-24-lz4-wal-cpu/report.md). That gate found 1k payload 336968 -> 76646 bytes at LZ4 FAST(8), with added compression CPU. This experiment measures whether standalone write/flush savings repay that cost.

## Protocol and scope

Use the exact SHA-verified retained codec-3 1k payload. Three variants in the same process: raw_A, identical raw_B A/A control, and LZ4 FAST(8). Two independent series of six processes; 24 balanced rotating/reversing-order rounds per variant/process. Fresh create-new append file per sample outside timing; reusable per-variant framing buffers preallocated equally. Timer includes LZ4 allocation/compression when applicable, checksum, 13-byte envelope assembly, one write_all and the source-copied production durable_sync. It excludes opening/creating the file, serialization of the original column payload, post-timer buffer destruction/readback and fixture removal. Sync API routing/fallback is copied unchanged but actual fallback use is not traced. No other builds/tests initiated by this runner overlap timing; unrelated host load uncontrolled.

This is standalone file I/O qualification, not an actual database transaction or physical power-loss test. New files and directory metadata are created outside timing; this lifecycle differs from appending after a real schema frame. Raw envelope uses codec 3; compressed scratch files use deliberately unrecognized experimental codec 0 and are never given to a production reader. No format is proposed or enabled by those bytes. Every file is read back exactly against the assembled bytes, then compressed data decompresses exactly to the input; all owned files/directories are removed. Raw and compressed frame sizes are 336981 and 76659 bytes.

864 measured operations completed (288 each variant), all exact checks passed, all 12 process exits zero. Raw samples retained. Production core/lock hashes unchanged and temporary Cargo example removed. No engine workspace/recovery gate rerun warranted for this unshipped prototype.

| Series | Variant | Raw A ms | Variant ms | Paired gain | Faster processes |
|---|---|---:|---:|---:|---:|
| 1 | raw_B_control | 0.661188 | 0.634761 | +4.797% | 5/6 |
| 1 | lz4_fast8 | 0.661188 | 0.736531 | -13.306% | 1/6 |
| 2 | raw_B_control | 0.305885 | 0.311406 | +0.739% | 4/6 |
| 2 | lz4_fast8 | 0.305885 | 0.442146 | -43.921% | 0/6 |

## Decision

Reject this compression hypothesis for the targeted local 1k durable path. LZ4 is slower by paired medians 13.31% and 43.92%, winning only 1/6 then 0/6 processes despite a >4x smaller output. Raw A/A shows +4.80% then +0.74%; absolute raw latency shifts substantially across series. The result is source/host/protocol-specific, not a universal LZ4 regression or an isolated diagnosis of kernel behavior. It provides no reason to implement a new default WAL writer/reader on this evidence. Do not promote byte savings to a database speedup.

The earlier 10k CPU cost was already material; no broad compression policy is supported. Existing accepted format/default Full sync/embedding/FTS/indexes remain unchanged. Full objective remains open: durable 1k SQLite gap and absent MSSQL/Kusto live qualification remain.
