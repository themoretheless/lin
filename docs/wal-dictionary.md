# Dictionary WAL reader capability

Status: **reader supported; automatic production writing disabled**. Default writers still use existing codecs. This is preparation for the measured vector dictionary experiment, not a durable-write speedup claim.

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

Still required before automatic writing: integrate the measured fused selector/encoder, preserve legacy fallbacks and limits, qualify actual frame byte costs, process-crash/acknowledged-write behavior, two independent six-pair end-to-end native/durable series with SQLite control and resource/dense/high-cardinality guards. Encoder CPU measurements alone do not establish database speed. The durable 1k gap and the full named-peer benchmark objective remain open.
