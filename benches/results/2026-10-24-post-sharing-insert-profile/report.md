# Insert phase diagnostics after shared WAL vectors

Source 518db365dd6accd80e86377baf18fedbca15429f in the isolated managed worktree. Temporary exec.rs timers built into a pinned release compare executable, then exec.rs restored byte-for-byte before measurements. No production changes ship from this profile. Three independent processes per native/Full durable mode, 24 fresh measured samples at 1k and 10k plus preflight. Existing exact fixture validation outside timing; six reports complete, all runner exits zero. Raw phase observations, binary/source hashes, snapshots and scripts retained. No compilation/test launched by this runner overlaps timing; host activity/clocks uncontrolled.

Median of process medians, milliseconds:

| Phase | Native 1k | Native 10k | Durable 1k | Durable 10k |
|---|---:|---:|---:|---:|
| build_ns | 0.211833 | 1.432917 | 0.203083 | 1.402834 |
| checks_ns | 0.098875 | 2.340000 | 0.104209 | 2.251291 |
| embed_ns | 0.278917 | 3.274459 | 0.281375 | 3.135875 |
| index_ns | 0.095375 | 0.963917 | 0.096041 | 0.956042 |
| fts_ns | 0.136083 | 1.401625 | 0.137958 | 1.394250 |
| hnsw_ns | 0.000041 | 0.000042 | 0.000042 | 0.000041 |
| maps_ns | 0.038584 | 0.516458 | 0.038500 | 0.511500 |

These are partial batch phases, not an additive decomposition of total statement latency: outer transaction setup, some reserve/edge/final-append work and durable WAL stages are outside the measured spans. Logging adds overhead. Independent medians cannot be summed/subtracted as exact costs or used as a peer latency result. This diagnostic contains only Lin, not a new SQLite comparison or claim of acceleration.

Embedding is the largest measured native 1k phase (~0.279 ms), followed by row building (~0.212 ms), FTS (~0.136 ms), checks (~0.099 ms), scalar indexes (~0.095 ms) and maps (~0.039 ms). The HNSW maintenance call is effectively inactive here (~41 ns, close to timer overhead); do not infer HNSW insertion speed from this fixture. At 10k, embedding and validation/defaults/identity checks dominate these measured phases. The next candidate should target these real paths rather than assumed SmallText packing allocations not used by prepared SQL insertion.

Shared-WAL-vector performance evidence remains in ../2026-10-24-shared-wal-vectors-revisit/report.md. This profile itself does not close remaining SQLite native/durable gaps, missing MSSQL/Kusto endpoints or full named-peer coverage.
