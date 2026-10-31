# Full WAL positive-zero prototype: correctness qualified locally, timing pending

Baseline runtime21dcd7d81, including the three Linux-confirmed rollback fixes. The rejected FTS optimization is excluded. Only persist.rs and the new private persist/positive_zero.rs change. Public Cell serialization, Normal WAL bytes, embedding dimensions, logical rows and Full durability policy are preserved.

For Full UPDATE/DELETE records (and affected Batch children), positive f32 zero is encoded as MessagePack integer0. Every other f32 retains ordinary encoding, including negative zero and NaN payloads. The serializer tracks the exact extra bytes the canonical encoding would require. It rejects the original canonical payload/envelope limit before any write, even when compact bytes fit. Batch depth16and beyond falls back to canonical encoding. Existing frame magic/codec/CRC and sync remain unchanged.

Local Rust1.101 macOS ARM64: **457release tests passed,2ignored**. New tests cover full Record round trips and byte-size accounting over zero/negative-zero/IEEE patterns, array boundaries and deep Batch; actual Full frames/CRC; exact Normal bytes; and canonical payload/envelope rejection with file bytes/log_bytes unchanged.

Two separately compiled runtime negatives were rejected: normalizing negative zero, and bypassing the canonical-size adjustment. Every source was restored to the tested positive candidate afterward.

A fresh export of production04749994b9ca7a090476b95401f0f7166d8b870a compiled the legacy reader. All8scenarios passed for both canonical and compact streams: UPDATE, DELETE, Batch, mixed Batch, deep fallback, empty vector, negative-zero vector and IEEE values. Tests compare complete decoded records and surviving rows as canonical MessagePack to preserve float bits; they check gen/next_id, duplicate replay no-op, ID lookup/projection and CRC corruption rejection. All32fixture files are preserved in a verified golden archive. This is a bounded reader/replay proof, not universal client or power-loss qualification.

The constructed UPDATE fixture's two-record stream is8716canonical bytes versus5900compact bytes; unchanged seed data remains part of both streams. Deep/empty/negative-zero-only fixtures remain byte-identical. These are fixture byte counts, not measured engine throughput or a peer win.

No timing acceptance or production adoption is claimed. Linux correctness and a fresh paired Full UPDATE/DELETE/commit matrix with native/read/Normal/rollback/peer controls remain required. Existing full-source COW regressions and the eight-peer objective remain open.
