# WAL preallocation 32-append amortization I/O diagnostic

Source `bd810056a6ed4aeedebb37633c7d2d6dde018103`; production code and defaults unchanged. Local Apple-only prototype using actual installed libc/macOS SDK fstore_t/F_PREALLOCATE/F_ALLOCATEALL/F_PEOFPOSMODE definitions. Compare unchanged raw_A/raw_B controls with raw bytes whose file storage is reserved in 1MiB steps. No codec, logical file content or Full-sync relaxation. Source-copied production durable_sync keeps the same platform API/fallback; actual fallback route is not separately instrumented.

Use the SHA-verified real current 1k codec-3 payload (336968 bytes). Envelope assembly/checksum, write_all and Full sync are timed. All frame bytes, payloads, checksums (amortized case) and logical file lengths verified; directories/files are create-new and removed after successful checks. Preallocation succeeds and reports 1048576 allocated bytes per call; fresh-file st_blocks changes from 0 to 2048 while file length remains 0. This reports API/stat allocation, not physical power-loss or portable filesystem guarantees. No reader or production lifecycle integration is qualified.

Two independent series, six processes each, balanced rotating/reversing variant order; all raw observations retained. No runner builds/tests overlap measurements; unrelated host state uncontrolled. Each process retains three files for 32 interleaved appends each. Creation order rotates across processes. Capacity refill happens inside operation timing, including metadata length checks and reservation cost; 11 reservations per candidate file. Validate full streams after all appends and then remove them. Identical frame contents are reused for an I/O model, not semantic database replay of repeated generation IDs.

| Series | Variant | Raw A ms | Variant ms | Paired gain | Faster processes |
|---|---|---:|---:|---:|---:|
| 1 | raw_B_control | 0.343313 | 0.324031 | +1.065% | 3/6 |
| 1 | preallocated_raw | 0.343313 | 0.292625 | +17.453% | 6/6 |
| 2 | raw_B_control | 1.411719 | 1.425229 | +3.023% | 5/6 |
| 2 | preallocated_raw | 1.411719 | 1.392688 | +3.419% | 6/6 |

Medians can hide capacity-refill outliers. Therefore also compare sums of all 32 timed operations per file (shown as mean/commit for units); all refill costs remain included. Do not double-add the separately logged reservation spans.

| Series | Variant | Raw total/32 ms | Variant total/32 ms | Paired total-time gain | Faster files |
|---|---|---:|---:|---:|---:|
| 1 | raw_B_control | 0.381368 | 0.455018 | -1.601% | 2/6 |
| 1 | preallocated_raw | 0.381368 | 0.320248 | +20.477% | 5/6 |
| 2 | raw_B_control | 1.337319 | 1.441141 | -9.604% | 2/6 |
| 2 | preallocated_raw | 1.337319 | 1.284537 | +10.605% | 5/6 |

## Interpretation

Amortized total-time gains are +20.48% and +10.60%, 5/6 files each. However raw_B A/A total-time drift is -1.60% and -9.60%, and absolute latency shifts strongly between series. The repeat does not isolate the candidate benefit cleanly from variation. This is a promising lifecycle hypothesis, not an accepted production speedup or universal threshold. A better controlled A/A/real-Db experiment is required before adopting a default reservation policy.

Production follow-up would need best-effort unsupported-filesystem/error fallback, bounded physical/log quota policy, empty/read-only/reopen/truncate/checkpoint/crash invariants, cold open/first-commit/space costs and qualified whole-engine native/durable A/A/A-B plus peers. Reserving physical space adds a disk-space/lifecycle tradeoff even though logical bytes do not change. No dependency or feature default changed here; unchanged engine workspace not rerun for standalone diagnostic executables.

Full named-peer objective remains incomplete: durable 1k SQLite, MSSQL/Kusto live qualification and outstanding contracts. A standalone file I/O result is not a database latency or peer win.
