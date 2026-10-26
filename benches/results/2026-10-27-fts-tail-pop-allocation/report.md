# Known-tail FTS allocation diagnostic

Three independent processes per variant, nine operations per process: 54 operations with exact readback. Setup and readback excluded; Prepared::run and output Handle drop included. Counting System allocator includes allocation/reallocation requests, not RSS or peak memory.

All DELETE sizes 1k/10k/100k: 71 -> 64 allocations; requested bytes saved 488/489/490 respectively. UPDATE wing remains 71, UPDATE title remains 85, with no byte change. These are the same seven allocation savings as rejected fused FTS v1; no latency conclusion follows.

Frozen source and binary hashes in provenance.json. Production sources restored exactly. A full paired engine experiment is required before acceptance.
