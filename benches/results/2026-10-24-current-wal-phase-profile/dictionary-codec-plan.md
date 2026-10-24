# Proposed bit-exact vector dictionary WAL experiment

Status: design candidate only; no production format allocation or measured gain.

Evidence: current WAL profile plus ../2026-10-04-vector-dictionary-audit/report.md. Default vectors in the historical audit have 4–6 distinct nonzero f32 bit patterns. Compression may reduce sync/checksum/write bytes; dictionary construction may offset the savings. An end-to-end result is required.

Proposed isolated identity: LIN06 checksum envelope codec 5 (currently unrecognized), typed vector column tag 8 only under that codec. Existing tags/codecs remain readable. New reader must reject malformed/unknown modes; current old reader rejects unknown codec as corrupt instead of treating it as a truncated tail. This source observation is insufficient: prove an old pinned Db::open rejects a new fixture **without changing any bytes** before enabling new writes.

Row candidate: retain current u32 dimension code (0 None, length+1 Some including empty), u32 nonzero count; add u8 dictionary length. Length 0 is raw escape (u32 index,u32 value bits). Positive lengths <=16 store exact u32 bit patterns and entries with u8 value ordinal; index u16 when dimension<=65536, otherwise u32. +0 excluded, -0/NaN payloads/Inf distinct. Strict ascending indexes and valid ordinals; no duplicate dictionary values. Actual final byte cost must beat existing sparse mode before selecting dictionary. Poorly compressible/dense/arbitrary-dimension inputs retain raw/legacy encoding. This is a proposal, not an already fixed wire contract.

Required gates before retention:

1. Deterministic reference roundtrip and byte accounting; None/Some(empty), every float bit class, partial blocks, 65535/65536/65537 dimensions, high cardinality/raw escape, mixed vector columns and no-benefit fallback.
2. Untrusted decoder bounds before allocation: checked frame lengths/count arithmetic, existing 64MiB decoded budget (including legacy Option<Vec<f32>> metadata accounting), n consistency, dictionary length/ordinals/index ordering/duplicates/trailing bytes, malformed/truncated checksummed frames. Preserve all existing limits and formats.
3. Current reader consumes old golden frames/snapshots; old pinned reader safely rejects new complete frames and does not truncate them. Document forward compatibility and new minimum reader; no silent reinterpretation or checksum polynomial change.
4. Exact durable insert/reopen/checkpoint/replay/rollback/crash fixtures; default embedding/FTS/sync untouched. Full workspace and focused corrupt-input tests.
5. Frozen before/after binaries, two independent six-pair native/durable 1k/10k series with SQLite control, fresh fixtures and exact values. Include dense/repeated/high-cardinality controls, resource/RSS evidence and all rejected/noisy runs. Encoder microbench or byte size alone is insufficient.
6. Retain only measured benefit with acceptable tradeoffs; otherwise restore source/format and preserve evidence. Keep full DuckDB/SQLite/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas objective open.
