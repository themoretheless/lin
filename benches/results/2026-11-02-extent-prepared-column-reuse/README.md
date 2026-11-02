# Reuse prepared column payloads in experimental extent WAL

The initial Db extent bridge discarded a ready prepared-column payload and encoded the record again. This branch retains that payload when generation, next_id, collection, field/schema shape, row count and empty-edge conditions match the existing legacy prepared-writer contract. Stale metadata falls back to ordinary native encoding. Failed and panicked workers retain their previous behavior.

The existing frame envelope, maximum record size, codec and CRC construction are unchanged. The extent transport still runs the strict complete frame decoder before touching disk. This optimization removes duplicate encoding; it does not bypass input or recovery validation.

Focused release validation passed: 9 tests, zero failures, one ignored child helper. Valid prepared payloads and stale generation/next_id/collection/field metadata match the legacy prepared writer byte-for-byte. All five SIGKILL checkpoints ran again. A full feature suite is running, including the actual 4096-row Db insertion/shipping/reopen case.

No throughput gain is claimed yet. Linux run 41391257035 remains frozen at 20a408f6c and does not include this later change. Its results must keep that source identity. Default-mode executable behavior is unchanged because all modified runtime paths are gated behind experimental-extent-wal. No adoption or merge into main.

Full feature follow-up passed: 477 tests, zero failures, four ignored child/legacy helpers. Throughput remains unmeasured for this later source change.
