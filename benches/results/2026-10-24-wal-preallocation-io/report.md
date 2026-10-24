# WAL preallocation fresh-file I/O diagnostic

Source `bd810056a6ed4aeedebb37633c7d2d6dde018103`; production code and defaults unchanged. Local Apple-only prototype using actual installed libc/macOS SDK fstore_t/F_PREALLOCATE/F_ALLOCATEALL/F_PEOFPOSMODE definitions. Compare unchanged raw_A/raw_B controls with raw bytes whose file storage is reserved in 1MiB steps. No codec, logical file content or Full-sync relaxation. Source-copied production durable_sync keeps the same platform API/fallback; actual fallback route is not separately instrumented.

Use the SHA-verified real current 1k codec-3 payload (336968 bytes). Envelope assembly/checksum, write_all and Full sync are timed. All frame bytes, payloads, checksums (amortized case) and logical file lengths verified; directories/files are create-new and removed after successful checks. Preallocation succeeds and reports 1048576 allocated bytes per call; fresh-file st_blocks changes from 0 to 2048 while file length remains 0. This reports API/stat allocation, not physical power-loss or portable filesystem guarantees. No reader or production lifecycle integration is qualified.

Two independent series, six processes each, balanced rotating/reversing variant order; all raw observations retained. No runner builds/tests overlap measurements; unrelated host state uncontrolled. Each process creates a fresh file per variant/sample, 24 rounds each. File open/create is outside timing. Candidate reserves 1MiB before the operation timer; its allocation cost is separately timed, and a second derived metric adds that per-sample cost to the operation. Fresh preallocation median ~30us in both series. This separation cannot be interpreted as a free database startup optimization.

| Series | Variant | Raw A ms | Variant ms | Paired gain | Faster processes |
|---|---|---:|---:|---:|---:|
| 1 | raw_B_control | 0.685552 | 0.683885 | -5.616% | 2/6 |
| 1 | preallocated_raw | 0.685552 | 0.677312 | +4.040% | 6/6 |
| 1 | preallocated_including_reserve | 0.685552 | 0.711531 | -2.736% | 2/6 |
| 2 | raw_B_control | 0.528292 | 0.546031 | -10.502% | 1/6 |
| 2 | preallocated_raw | 0.528292 | 0.491291 | +9.742% | 6/6 |
| 2 | preallocated_including_reserve | 0.528292 | 0.531428 | +3.550% | 5/6 |

## Interpretation

Operation-only gains are +4.04% / +9.74%, but including reservation is -2.74% / +3.55%, with large raw_B A/A drift. Fresh-file evidence is inconclusive and cannot justify an engine change or hiding setup costs. The separate 32-append experiment explores amortization explicitly; it does not overwrite this result.

Production follow-up would need best-effort unsupported-filesystem/error fallback, bounded physical/log quota policy, empty/read-only/reopen/truncate/checkpoint/crash invariants, cold open/first-commit/space costs and qualified whole-engine native/durable A/A/A-B plus peers. Reserving physical space adds a disk-space/lifecycle tradeoff even though logical bytes do not change. No dependency or feature default changed here; unchanged engine workspace not rerun for standalone diagnostic executables.

Full named-peer objective remains incomplete: durable 1k SQLite, MSSQL/Kusto live qualification and outstanding contracts. A standalone file I/O result is not a database latency or peer win.
