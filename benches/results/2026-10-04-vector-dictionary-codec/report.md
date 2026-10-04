# Dictionary vector WAL codec prototype: rejected

Baseline: f0d424d. Candidate adds internal codec 4/tag 8 in the checksummed LIN\x06 envelope, retaining readers for codecs 1/2/3. Large sparse columns (>=256 rows) use dictionary-eligible rows; other existing paths remain. Each row uses exact f32 bits, u8 dictionary length/codes and u16 positions for dimensions <=65536 (u32 above that). Dictionary overflow (>255 values) or unprofitable encoding uses marker zero plus raw u32 position/bits pairs. None and Some(empty) remain distinct. CRC/fsync policy unchanged. Old binaries reject codec 4; that backward reader limitation was recognized, and this candidate is NOT deployed.

The earlier size audit supports vector-section reduction around 53%, not a full-WAL or latency claim. This prototype scans vectors for counts/dictionary discovery/encoding and searches dictionaries for codes. Those costs are not separately timed here; whole-operation measurements cannot conclusively attribute the regression to a specific loop or cache effect.

Six alternating independent baseline/candidate process pairs, 24 fresh one-operation samples per case/process, no warmup. Setup/schema/preparation excluded; encoding/write/durability sync remain timed. Exact affected-count/value/readback checks passed. No builds/tests overlapped measurement.

| Workload | Median paired speedup | Faster pairs |
|---|---:|---:|
| compare/durable_insert_1k/lin | -20.66% | 0/6 |
| compare/durable_insert_1k/sqlite | -0.24% | 3/6 |
| compare/durable_insert_10k/lin | -32.60% | 0/6 |
| compare/durable_insert_10k/sqlite | -0.37% | 2/6 |

Speedup = median(100*(baseline_i-candidate_i)/baseline_i). Lin regressed in every pair at both sizes while SQLite controls stayed near flat/mixed. Decision: reject this encoder/format tradeoff. Smaller byte estimates do not satisfy the requested latency goal. A future implementation must address measured encoding cost before a new format is justified; this result does not rule out every dictionary design.

Validation: existing sparse integrity tests passed. Two new dictionary tests passed, covering exact negative-zero/NaN/Inf values, empty/zero/None presence, 65536/65537 position-width boundaries, raw escape after dictionary overflow, all payload truncation prefixes, malformed dictionary codes/tables/order/out-of-range indices and decoded-size budget. Full workspace initially failed only the large-frame codec-number assertion (3 vs 4); that assertion was updated while retaining its frame/reopen/replay/value checks, and the complete workspace suite then passed. Initial failure and final successful logs retained. Both benchmark binaries built. Tests cover synthetic trusted/malformed examples; no broad fuzz campaign was run.

Sparse/dictionary expanded vector allocation follows the existing 64MiB budget; encoded frame limit remains 16MiB. The standalone dictionary helper also rejects oversized dimensions before allocating its values. These limits refer to the new expanding vector path, not a claim that every legacy raw/text allocation shares one global 64MiB ceiling.

Production src/persist.rs AND tests/persist.rs restored byte-for-byte to baseline. Saved source/test copies and SHA256 hashes identify the exact prototype. No new codec has been published in production source. Log trailing blank lines normalized, raw JSON retained. `git diff --check` passed. The full nine-engine objective remains incomplete, including SQLite small/durable write gaps and absent MSSQL/Kusto endpoint evidence.
