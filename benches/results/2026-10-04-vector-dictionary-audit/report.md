# Bit-exact vector dictionary size audit

Source: 172703c. Diagnostic prototype uses actual default embeddings produced by Db insert with the Rust compare fixture's title/body/ID/URI/wing/layer shape, 768 dimensions and scalar wing/ts index. Fixed timestamp 1700000000000 avoids time drift; it does not enter embedding input. Three independent release processes produced identical statistics for 1000 and 10000 rows. This is byte-size/roundtrip evidence, not performance timing or a changed WAL format.

| Documents | Current sparse vector bytes | Prototype dictionary vector bytes | Vector-section reduction |
|---|---:|---:|---:|
| 1000 | 238,992 | 112,542 | 52.91% |
| 10000 | 2,539,664 | 1,180,574 | 53.51% |

Every measured vector has 4–6 distinct nonzero f32 bit patterns. At 1k: dictionary sizes 4/5/6 occur 773/224/3 times, total 28874 nonzero entries. At 10k: 8022/1906/72 times, total 307458 entries. These counts and sizes were identical across three processes.

Accounting: current sparse row = u32 dimension code + u32 count + (u32 index,u32 f32 bits) per nonzero. Proposed row = same eight header bytes + u8 dictionary length + four bytes per distinct value + (u16 index,u8 value-code) for dimensions <=65536, otherwise u32 index. Values are dictionary entries of exact bits, not quantized numbers. -0.0 and NaN payloads remain distinct from +0.0. The trusted-data prototype encodes/decodes entries and asserts all f32 bits match, for every measured vector plus synthetic empty/all-positive-zero/negative-zero/NaN/Inf/repeated values. It is NOT a production decoder and has no untrusted-input safety validation or reader compatibility gate.

These totals cover vector-column rows only. They exclude other columns, frame headers/checksums, column metadata and write/fsync overhead. They do not establish a 53% reduction of full WAL, CPU benefit or end-to-end latency. Per-row dictionary search/encoding costs could offset byte savings. A dictionary-overflow fallback (>255 distinct values) is sketched in the prototype but was not triggered by the measured workload or validated for roundtrip.

Next justified candidate: versioned dictionary codec with raw escape for poorly compressible vectors, preserving None/Some(empty), every float bit and arbitrary dimensions; bounded decoded allocations and ordered-index/value-code checks; reading all existing frame/codec versions; checksum/truncation/malformed-input tests; durable fixture reopen and timing against current source/SQLite. Existing reader rejection of new codecs must be deliberate and documented. No speedup may be claimed until those checks and timings pass.

The temporary existing profile_durable example was saved and restored byte-for-byte to HEAD BEFORE running the pinned audit binary. Build passed. Original/instrumented example and source/binary SHA256 hashes are saved. Production source/WAL format are unchanged; no full workspace tests were needed for the reverted diagnostic-only source. `git diff --check` passed after log trailing whitespace normalization. The full nine-engine goal remains incomplete, including SQLite small/durable write gaps and unavailable MSSQL/Kusto endpoint evidence.
