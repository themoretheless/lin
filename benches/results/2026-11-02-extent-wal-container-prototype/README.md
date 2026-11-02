# Experimental fixed-extent WAL container

Motivation: Linux run 41386055044 measured 69.545% lower point-write + fdatasync latency for fully prewritten fixed EOF, versus -1.714% for sparse fixed EOF. This is I/O evidence, not database speed or format acceptance.

The new `experimental-extent-wal` feature exposes an isolated container; Db/OpenOpts, normal WAL writers/readers, snapshots, checkpoints and replication still use the existing format. There is no automatic conversion, migration or adoption.

`LINEXT01` header: physical file size, committed frame-byte length, append sequence, CRC32C, reserved zero bytes. Frames begin at offset 4096, retain the existing LIN06 framing/codec/CRC bytes, and are validated by the existing strict frame decoder. The whole fixed extent is initially zero-written and synced. Append validates input before touching disk, writes the frame bytes and checksummed logical boundary, then performs one Full durability sync. I/O failure poisons the handle; an exclusive file lock rejects a second cooperating writer. Capacity exhaustion refuses the append; extent growth is not implemented.

Recovery requires a valid header CRC, exact physical size, bounded logical length and valid complete checksummed frames inside the logical region. An invalid boundary never falls back to an older value or scans for a plausible end. Bytes outside a valid committed boundary are explicitly unpublished extent space, not legacy WAL records; interrupted/torn data inside the boundary is an error. This distinction needs crash qualification before any runtime adoption.

Initial tests cover exact frame-byte roundtrip and repeated append; every header/frame byte corruption; physical truncation/extension and a logical boundary inside a frame; invalid input/full extent preserving the published bytes; existing-file preservation; payload-before-boundary interruption; and every interrupted header prefix. These are simulated byte-level interruption tests, not power-loss qualification.

Still required: fault-injected sync-error qualification; real process crash tests; directory creation durability; lock lifecycle crash qualification; bounded streaming recovery; extent growth, checkpoint/truncate and snapshot offsets; replication/export semantics; old-reader refusal and migration behavior; full database integration and regression tests; matched Linux database benchmarks including SQLite FULL WAL. No speed or all-peer superiority claim follows from the container implementation alone.

Initial focused run: four tests passed, one failed from a parallel temporary-directory naming collision. The original failure log is retained. The fixture now combines process ID, an atomic sequence and time. The follow-up also checks competing-writer refusal, real read-only-descriptor write failure poisoning, and refusal by the legacy raw-frame decoder.

Focused release follow-up: 6 passed, 0 failed (extent_wal tests and ownership test). Full release with experimental-extent-wal passed: 469 tests, zero failures, two ignored; production integration and performance qualification remain absent.

Process-crash harness is being added. Test-only hooks stop a child at mid-payload, before boundary publication, mid-header, after-header and after-sync. The parent sends SIGKILL, waits for actual signal 9 termination, then checks strict reopen outcomes. This tests process death on a running OS; it does not simulate power loss or a disk cache losing unsynced data. The hooks compile out of non-test builds.

The first actual SIGKILL run passed all five interruption stages: 7 focused tests passed and the child helper was intentionally ignored by the parent harness. Readiness polling was then strengthened to wait for the complete stage marker rather than file existence, avoiding a test-only create/write race. A final full feature suite is running after that change.
