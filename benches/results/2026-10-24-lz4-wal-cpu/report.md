# LZ4 on actual current WAL payloads: CPU/byte feasibility

Source `d8bf63a5f69559284a67e22ce13cdf3679cb5d78`; production source, defaults and lockfile unchanged. Diagnostic built with the existing optional compress-lz4 dependency only; automatic WAL compression and new codecs are not implemented or enabled. This differs from the rejected Zstd level-1 experiment (../2026-10-01-wal-zstd/report.md).

## Actual inputs and checks

A pinned helper opened default Full Db, created the ordinary wing/ts index and inserted standard-shaped 1k/10k docs, then exited without Db drop/checkpoint to retain WAL. Each file has a schema codec-1 frame and an actual production codec-3 data frame. The runner verified every retained frame checksum with an independent Python CRC32C table, then extracted the exact data payload: 336968 / 3558340 bytes. WAL/payload SHA256 recorded, original files retained. This process-exit input capture is not a full recovery or power-loss qualification. A deterministic random 336968-byte input serves as incompressible guard, not a real DB workload.

Six independent release processes; 24 alternating rounds at each acceleration 1/4/8; 16 loops per round. Compare checksum of the original payload with LZ4 block compression (size prefix included), checksum of compressed bytes and output allocation/destruction. Exact decompression byte equality checked outside timing. Current CRC source included mechanically in the diagnostic. Framing/codec metadata, original vector/column serialization, file write/Full sync, read costs, production selection/bounds and RSS are excluded. This trusted-input prototype decoder does not qualify an untrusted production reader. No build/test work initiated by this runner overlaps timings; host activity uncontrolled.

| Input | Accel | Original bytes | Compressed bytes | Raw CRC µs | Compress+CRC µs | Paired extra CPU µs |
|---|---:|---:|---:|---:|---:|---:|
| 1000 | 1 | 336968 | 71708 | 32.003 | 154.575 | 122.320 |
| 1000 | 4 | 336968 | 72104 | 31.898 | 146.985 | 115.458 |
| 1000 | 8 | 336968 | 76646 | 31.989 | 138.279 | 106.292 |
| 10000 | 1 | 3558340 | 769756 | 338.273 | 2617.311 | 2262.665 |
| 10000 | 4 | 3558340 | 773480 | 340.841 | 2583.986 | 2242.867 |
| 10000 | 8 | 3558340 | 809826 | 345.421 | 2490.803 | 2142.528 |
| entropy | 1 | 336968 | 338295 | 32.057 | 44.908 | 12.852 |
| entropy | 4 | 336968 | 338295 | 31.932 | 44.620 | 12.734 |
| entropy | 8 | 336968 | 338295 | 32.064 | 44.971 | 12.593 |

## Decision

No production acceptance or database speed claim. At 1k the payload shrinks about 4.4–4.7x, with additional CPU ~0.106–0.122ms; a write+sync experiment must show whether smaller writes repay that cost before implementing any writer/reader format. At 10k the added ~2.14–2.26ms is substantial, so a global compression policy is not supported. Incompressible data grows and still consumes compression CPU; any eventual selection must preserve raw fallback, actual frame bounds and default features. Do not subtract medians from another campaign to predict an end-to-end gain.

Any production follow-up requires bounded length validation before decompression allocation, old-reader/format negotiation, current shared decoded-vector limits, exact old/new replay/checkpoint/replication, corruption/crash gates, Full workspace and same-campaign A/A plus interleaved A/B/native/large guards. CPU/byte evidence alone cannot justify a new default codec.

All six processes per input completed; all 54 exact compression roundtrips passed, with all raw timings retained. Temporary Cargo example removed, current core hashes and lockfile verified unchanged. Full objective remains open, including durable 1k SQLite and missing MSSQL/Kusto endpoints.
