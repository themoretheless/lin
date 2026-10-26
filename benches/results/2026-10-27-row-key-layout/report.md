# Shared/inline row-key clone CPU prototype

Production Row remains BTreeMap<String,Cell>. Standalone candidate is BTreeMap<Key,Cell>, where short names use existing lossless InlineStr (22 bytes max), long names use Arc<str>. Content-based Ord/Borrow<str> preserves exact lookup and iteration order; names are not truncated. No public API, persistence format or production source changes.

Six independent processes; five shapes (short9, short32, long9, mixed32, Unicode9). 24 interleaved rotated A/A/B samples per shape and process, 1024 clone+drop operations per variant/sample. Total 2211840 clone/drop operations. Setup outside timing, drop inside; allocation counters disabled during timing and measured separately for one clone/drop. All 720 sample records retained, exact name/value lookup verified before measurement. Initial missing whitespace in Rust match guard failed compilation; corrected, failure log retained.

| Shape | Median clone gain | Absolute A/A | Positive processes | Allocation requests |
|---|---:|---:|---:|---:|
| short9 | 81.00% | 0.47% | 6/6 | 10 -> 1 |
| short32 | 77.66% | 0.92% | 6/6 | 37 -> 5 |
| long9 | 81.87% | 0.99% | 6/6 | 10 -> 1 |
| mixed32 | 78.91% | 0.31% | 6/6 | 37 -> 5 |
| Unicode9 | 79.99% | 0.56% | 6/6 | 10 -> 1 |

Key enum is 24 bytes, same size as String on this target. These are owned BTreeMap clone CPU results only. Construction/interning, map mutation, serialization, foreign public Rust API types, full-engine throughput/latency, cold caches, RSS, parallel Arc refcounts, WAL/durable and peer comparisons remain unqualified. Long names move allocation to initial Arc creation; that setup cost is excluded here and must be measured separately before adoption. No serialization compatibility or broad semantic equivalence claim follows from lookup checks.

This supports a distinct row-storage design investigation, not accepting any earlier rejected borrowed-row/FTS/index candidate and not claiming a database speedup. Existing String-key public Row API requires an explicit compatibility strategy before any production change.
