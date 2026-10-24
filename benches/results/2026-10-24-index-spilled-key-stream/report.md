# Spilled-only index key streaming — qualification

2026-10-24 baseline12713b2, same accepted sample-once core. Unlike rejected global streaming, candidate dispatches only indexes with >2 fields to a private streaming implementation, matching current IndexKey's two-part inline storage boundary. One/two-field indexes retain original key precomputation algorithm, plus one entry dispatch branch. Original filtered-position vector, captured-now, sample-once bitmap policy, insertion order, errors and formats unchanged. Complete source snapshots retained; no public API, WAL/sync or bitmap default changes.

Additional oracle compares three/five-field keys against the original algorithm across0/1/2/99/100/101/128/256 rows, unique/nonunique, partial predicates, sampling on/off, missing indexed field and a boolean outside sample. Errors, reverse map, forward keys, posting representation and positions match.256-row case includes duplicate conflict after an inserted prefix. Existing threshold-boundary tests and full workspace passed. Initial focused log precedes adding256; full workspace includes the final fixture. Production restored after measurements.

Two independent series ×six alternating process pairs, twelve samples/case/process, same six-case worker as previous global-streaming experiment. Fresh empty index clone/setup before timer; actual insertion timed; exact source-row posting reconstruction outside timing and destruction excluded. No runner build/test overlap; unrelated host activity uncontrolled. Source/binary/compiler metadata, build/runner and raw observations retained. No full-Db/RSS/p99/peer claim.

| Series | Fixture | Baseline ms | Scoped stream ms | Median paired gain | Positive pairs |
|---:|---|---:|---:|---:|---:|
| 1 | bitmap_1k | 0.067698 | 0.067177 | -0.692% | 2/6 |
| 1 | bitmap_10k | 0.604282 | 0.625323 | -3.826% | 2/6 |
| 1 | standard_card100 | 0.093365 | 0.090323 | +1.765% | 5/6 |
| 1 | disabled_1k | 0.057594 | 0.059062 | -1.709% | 1/6 |
| 1 | disabled_10k | 0.544469 | 0.553011 | -1.569% | 1/6 |
| 1 | spilled_3fields | 0.123563 | 0.120427 | +3.010% | 6/6 |
| 2 | bitmap_1k | 0.064833 | 0.066333 | -1.386% | 2/6 |
| 2 | bitmap_10k | 0.609041 | 0.604854 | +0.176% | 3/6 |
| 2 | standard_card100 | 0.092177 | 0.091885 | -0.103% | 2/6 |
| 2 | disabled_1k | 0.058094 | 0.058052 | -0.709% | 2/6 |
| 2 | disabled_10k | 0.537083 | 0.551667 | -3.938% | 1/6 |
| 2 | spilled_3fields | 0.124177 | 0.119864 | +3.587% | 5/6 |

## Decision: reject scoped dispatch

Spilled3-field case repeats +3.01% /+3.59%,6/6 and5/6 positive pairs. However default disabled2-field control loses -1.57% /-3.94% at10k (1/6 each) and -1.71% /-0.71% at1k. Source algorithm retained there does not imply identical compiled timing. No assembly attribution/A/A calibration was performed, so the decline is an empirical guard failure, not proof of a specific branch/code-layout mechanism. Local spilled-key gain is insufficient to accept this global code change.

No unchanged-candidate full-engine/peer refresh justified after the component control failure. No measured allocation/RSS saving claim. Further unchanged dispatch retries are unsupported; a different mechanism or stronger hotspot evidence is needed.

Candidate including oracle fixtures remains as artifact; production index.rs restored byte-for-byte, verified against every baseline core source hash. Accepted bitmap sample-once optimization retained. Full eight-peer objective remains open, including durable SQLite gap and missing live MSSQL/Kusto comparisons.
