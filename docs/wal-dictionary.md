# Dictionary WAL codec

Status: **reader supported; writer available behind an explicit switch, off by default**. Default writers still use existing codecs. No durable-write speedup is claimed: the writer has correctness tests, not a benchmark acceptance.

## Writer switch

`lin::persist::set_wal_dictionary_writer(true)` lets this process write codec 5; it returns the previous setting, and `wal_dictionary_writer()` reads it. With the switch on, a vector column that the encoder would write in the sparse layout (tag 7) is also encoded in the dictionary layout (tag 8) and replaced only when the dictionary bytes are strictly fewer; the frame is then written under codec 5. Dense columns, rows with more than 16 distinct values (where the raw escape costs one byte more than sparse) and columns below the sparse threshold keep their existing codec byte for byte. Both encoders (typed columns and rows) apply the same rule. Decoded sizes are identical in both layouts, so the 64 MiB decoded budget is accounted exactly as for the sparse column.

The column is currently encoded twice when the switch is on (sparse, then dictionary); the fused selector from the CPU gate is not integrated. Turn the switch on only when every reader of the log — followers, backup tooling and the previous release kept for rollback — can read codec 5: an older reader rejects such frames as corrupt.

## Framing and compatibility

The existing `LIN\x06` checksum envelope now recognizes internal codec byte `5`. Its length includes codec byte, column-record payload and four checksum bytes. Existing CRC32C/IEEE CRC32 acceptance is unchanged. Bare `LIN\x05` is not an on-disk format. Codecs 1/2/3 and optional feature-gated 4 retain their existing readers.

Codec 5 carries an insert-column record and must contain at least one dictionary vector column (tag 8). It may mix existing scalar, dense-vector (6) and sparse-vector (7) columns. Tag 8 is rejected under old codecs. Codec 5 without a tag-8 column, other record kinds, trailing data and malformed fields are rejected.

A frozen reader built before this capability was verified to open the baseline fixture, reject a complete valid codec-5 fixture and leave **every fixture file byte unchanged**, including FTS files. A capable read-only reader validates its values without changing the fixture. These are source-specific compatibility results; the package version string alone does not negotiate codec support. Do not enable new writes until all consumers can read codec 5 and the writer gates below pass.

## Vector rows (tag 8)

All integers are little endian.

- `u32 dimension_code = 0`: None; row ends immediately.
- Positive code: dimension is code minus one, followed by `u32 nonzero_count` and `u8 dictionary_length`.
- Length 0: raw escape entries `(u32 index, u32 f32_bits)`.
- Length 1..16: that many distinct nonzero `u32 f32_bits`, then entries `(index, u8 ordinal)`. Index width is u16 for dimension <=65536, otherwise u32.
- Indexes must be strictly increasing and within dimension; ordinals must be valid and every dictionary value must be used. Count cannot exceed dimension. Positive-zero entries and duplicate dictionary values are invalid. -0, distinct NaN payloads and infinities retain their exact bits.
- Some(empty) is dimension_code 1, count 0, dictionary_length 0. Some(all-positive-zero) uses a positive dimension and the same empty raw entry list; it remains distinct from None.

## Allocation and validation

Aggregate decoded-vector budget remains 64 MiB, with the existing conservative `Option<Vec<f32>>` metadata cost. Codec 5 accounts for dictionary, sparse and dense vector columns together. Checked counts, frame availability and metadata bounds precede metadata allocation. Dictionary tables, all index/ordinal/value entries and decoded-size budget are validated before allocating each decoded value array; a second bounded pass materializes the values. This is a logical decoded-vector budget, **not** a whole-process peak-RSS guarantee. Frame/scalar metadata retain existing bounded-frame rules.

## Empty-vector materialization fix

Column materialization previously converted `Some(empty)` to Null despite sparse decoding preserving its presence. It now clones every Some vector, including empty, and leaves None as Null. This fixes legacy sparse and MessagePack column replay too. Reopen/checkpoint tests cover the old sparse representation separately. Existing historical Null values are not reinterpreted as empty vectors; information already lost in older snapshots is not reconstructed.

## Verified and remaining work

The evidence bundle is [dictionary-wal-reader](../benches/results/2026-10-24-dictionary-wal-reader/report.md): bit-pattern and width-boundary roundtrips; malformed tables/counts/indexes/ordinals; checksums; truncation/corruption replay; shared budgets; empty/null materialization; frozen old-reader rejection; read-only replay; writable reopen/checkpoint; unchanged default writer; default workspace tests.

Writer coverage: `dictionary_writer_roundtrips_bits_and_is_smaller`, `dictionary_writer_keeps_existing_codecs_when_it_does_not_help`, `dictionary_writer_row_encoder_matches_column_encoder` (unit) and `tests/wal_dictionary.rs` (durable insert, crash recovery with the switch on and off, follower apply, mixed-codec log, checkpoint and reopen, smaller log than the default codecs on the default embedder's vectors).

Still required before writing by default: the fused selector/encoder (one pass instead of two), process-crash/acknowledged-write behaviour under fault injection, two independent six-pair end-to-end native/durable series with SQLite control and resource/dense/high-cardinality guards. Encoder CPU measurements alone do not establish database speed. The durable 1k gap and the full named-peer benchmark objective remain open.
