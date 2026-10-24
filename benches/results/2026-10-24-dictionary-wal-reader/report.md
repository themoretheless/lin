# Bounded dictionary WAL reader; automatic writes remain disabled

Baseline source `fb4af9f`. Runtime source/test snapshot before-persist.rs and candidate-persist.rs, old-reader source/binary/source hashes, raw test logs and actual fixture files retained. This milestone adds production read capability for codec 5 and fixes empty-vector materialization; **no new automatic writer, compression setting or database speedup claim**.

## Implementation

LIN06 checksum envelope codec 5 permits dictionary vector column tag 8 only for typed insert-column records. Strict row dimensions, dictionary length<=16, unique nonzero bit patterns, count bounds, ordered/unique indexes, valid ordinals and used-table coverage. All entries checked before decoded value allocation. None/Some(empty), signed zero, NaN payloads and infinities preserved. 65535/65536/65537-dimensional index-width boundary covered.

Shared 64MiB logical decoded-vector budget includes legacy Option<Vec<f32>> metadata, dictionary/sparse/dense columns together; input bounds and checked sizes precede allocations. This is not a physical peak-RSS bound. Existing code paths/frame limits/checksum acceptance remain; no untrusted-input assertions from the CPU prototype are used in production decoding.

## Verified bug and fix

The first reopen regression failed: codec retained Some(empty), but unchanged cols_to_rows mapped it to Cell::Null. Pre-fix log/status preserved in pre-empty-fix-tests.log/json. Materialization now clones every Some Arc, including empty; None stays Null. Separate legacy sparse reopen/checkpoint regression verifies the fix also applies to existing sparse records. Old Null snapshots are not magically recovered.

## Gates and evidence

Ten targeted dictionary reader tests passed. They cover exact float bits, None/empty, raw escape/high cardinality, 16-entry tables, byte-width boundaries, mixed vector modes and budgets; oversized dimensions/counts, bad/duplicate/unused dictionaries, out-of-range/duplicate/descending indexes, invalid ordinals, redundant positive zero, extra bytes and every payload prefix; deterministic one-bit mutation/no-panic checks; CRC32C and IEEE checksums; every torn frame prefix and complete corruption with valid/invalid CRC; writable reopen/checkpoint, read-only replay with no mutation, legacy sparse empty replay and unchanged default codec-3 writer.

Full default cargo test --workspace passed after final runtime decoder/empty fix. A later addition only extended an existing test with IEEE codec-5 verification; the final ten targeted tests passed again. No new performance benchmark needed to claim a speedup because none is claimed; new writer trials remain outstanding.

Old reader was pinned from pre-change source before editing. Compatibility test first confirmed it opens a normal baseline, then supplied a valid fully checksummed codec-5 frame. It rejected with exit 2/corrupt-log error; recursive byte comparison verified all files unchanged including FTS, HEAD, snapshot and log. New reader validates the same values through read-only replay without mutation. Actual codec-5 fixture preserved in compat-fixture-final; compatibility.json records codecs and SHA256s. Older compat-fixture is a checkpointed initial harness fixture; it is not the preserved live codec-5 proof. Harness was corrected to use read-only replay because writable Db::drop checkpoints the log; the initial trace is retained in compatibility-checkpointed.log.

The frozen helper's source commit and binary hash identify the tested old reader; this does not prove every historic binary behaves identically. Default current writer test still emits codec 3, not 5. Bare LIN05 is not a file-format rollout. See docs/wal-dictionary.md for the exact row contract and compatibility scope.

## Remaining goal

Production fused selector/encoder integration, fallback/resource guards, process-crash/acknowledged-write qualification and repeated full durable/native measurements still required before enabling writes. Previous CPU prototype is not end-to-end speed proof. Durable 1k SQLite gap, refreshed server matrices and unavailable MSSQL/Kusto remain open; named-peer objective unchanged.
