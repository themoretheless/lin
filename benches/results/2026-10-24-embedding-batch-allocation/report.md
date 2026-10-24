# Whole-batch embedding allocation audit

2026-10-24 baseline b729d64. Actual public production HashingEmbedder called by a standalone diagnostic with a System-delegating counting allocator. Production source/API unchanged. Successful alloc/alloc_zeroed, successful realloc old/new requested sizes and dealloc requested layout sizes counted separately. Counts cover explicit enabled windows only; no timing-based performance claim. The observer uses global atomics and must not be used concurrently with unrelated allocating threads; this worker starts none. Counts are allocator requests, not physical RSS, malloc usable size or allocation latency.

Observer sensitivity is checked before each process: retained Vec allocation of exactly 123 bytes, explicit reserve_exact growth, and destruction must record one alloc, one realloc, one free. Optimizer escape via black_box retains heap storage. Counter JSON serialization occurs after disabling observation. Failed allocations are not counted. This observer is intentionally diagnostic, not a production allocator change.

Three fresh processes per each of four fixtures (12 total). Input strings/reference-slice preparation excluded. Constructor measured before the first model construction in each process. Cold batch includes first thread-local scratch initialization, followed by 12 warm batches. The cold output is cloned outside observation to retain reference Arcs: dropping its original outer Vec frees only that container, while final reference release frees cold Arc vectors. Each warm output is unique, fully compared to the retained cold vectors bit-for-bit outside observation, then dropped in a separate counted window. Exact preflight dimensions and row counts checked. Repeated deterministic output is the oracle here, not an independent embedding algorithm proof.

| Fixture | Rows/dim | Constructor allocs/bytes | Cold batch allocs/bytes | Warm batch allocs/bytes | Warm reallocs | Warm drop frees/bytes |
|---|---|---:|---:|---:|---:|---:|
| default_10k | 10000/768 | 2/131087 | 10005/31044864 | 10001/31040000 | 0 | 10001/31040000 |
| default_1k | 1000/768 | 2/131087 | 1005/3108864 | 1001/3104000 | 0 | 1001/3104000 |
| small_dim | 1000/8 | 1/15 | 1007/68904 | 1001/64000 | 0 | 1001/64000 |
| unicode_1k | 1000/768 | 2/131087 | 1005/3108864 | 1001/3104000 | 0 | 1001/3104000 |

Every measured counter object matches exactly across three processes; warm counts match across all 36 observations per fixture. At 768 dimensions: each Arc slice requests 16 +768*4 =3088 bytes; outer Vec stores 16-byte fat pointers per element. At 1k: 1000*3088 +1000*16 =3,104,000 requested bytes, 1001 allocations. At 10k the corresponding total is31,040,000 bytes/10001 allocations. These equations match current observed implementation/layout on this target; they are not cross-platform ABI promises.

Warm batches show zero reallocs and no frees during generation: all successful requested bytes remain in returned outputs until release. Cold 768 batch adds four scratch allocations totalling 4864 bytes; model constructor separately allocates 131087 bytes (131072-byte bigram lookup plus 15-byte model ID). Small-dimension cold behavior includes initially created default scratch replaced for dimension 8, captured separately in raw data. Unicode fixture includes uppercase/non-ASCII and literal backslash-u sequences as written in gate.rs; it does not claim actual vertical-tab or Unicode-space coverage. No such interpretation is needed for allocation accounting.

## Consequences

For these warm fixtures there is no observed transient allocation churn inside feature processing to eliminate: allocations exactly account for the returned Arc vectors and result container. This rules out another scratch-reservation tweak as a route to eliminating the measured allocation count. It does not establish that allocator time dominates or that direct output filling would improve DB latency. Current output copying/initialization, feature computation and attachment to rows remain separate possible costs. Cold small-dimension scratch replacement is a real finite observation, not the target default 768 benchmark bottleneck.

Next candidate should address output construction or feature work with exact f32-bit and lifecycle preservation, then CPU and calibrated full-engine gates. Existing rejected pool/sparse-output/zero-marker/worker prototypes are not justified unchanged by these counts. No output ownership or public model contract changes accepted here.

No workspace tests rerun for unchanged production. No whole-Db CPU, disk/sync, RSS, p99, power-cut or peer superiority claim. Durable SQLite gap and missing MSSQL/Kusto live qualification persist; full eight-peer objective remains open. Source/binary hashes, compiler, sensitivity evidence, scripts and all raw counters retained.
