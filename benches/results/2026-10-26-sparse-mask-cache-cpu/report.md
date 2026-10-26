# Sparse mask cache — rejected CPU prototype

2026-10-26, frozen source aa8f307f. No production source changes. This prototype caches one presence byte per eight floats in one bounded flat Vec (512KiB limit), so classification and serialization can share the masks. Eligible diagnostic policy:512..4095 rows; other sizes use current classifier/writer, including current four-way dispatch. It differs from the old rejected cached-count experiment: masks identify positions as well as counts, and the baseline already has the accepted NEON sparse writer.

## Corrected gate

Current source helper block copied byte-for-byte and hashed. A/A serial controls and cache candidate rotate within24 rounds;two six-process series, six cases,5184 timed compositions. Timer includes codec classification, output allocation, serialization, temporary mask allocation/destruction; output destruction excluded. Both codec decision and bytes are checked against current source after timing. Dense little-endian data uses the same bulk byte-copy mechanism as production; big-endian fallback is scalar. No full payload/scalar columns/frame/checksum/I/O/sync/transaction/maintenance is timed.

Finite oracle covers lengths0..1024,four pattern offsets,None/Some(empty),special float bits and deterministic raw bits, exact classification/bytes,64KiB dense-threshold boundary sizes and512KiB mask-budget fallback. Default fixtures use real public Db HashingEmbedder vectors; zero/half/dense controls share a synthetic vector per row. Every process exits0; all times finite/positive and exact bytes/codec assertions pass. No own builds/tests overlap measurement; source/gate/binary hashes and load samples retained. No causal attribution to allocation, cache traffic or compiler behavior is established.

| Case | Series1 gain /positive processes | Series2 gain /positive processes | A/A absolute difference series1 /2 |
|---|---:|---:|---:|
| default1k | −30.911% /0/6 | −30.437% /0/6 |0.564% /0.842% |
| large10k fallback | +0.407% /5/6 | +1.009% /4/6 |0.590% /1.536% |
| small127 fallback | +0.372% /5/6 | −0.049% /3/6 |0.236% /0.190% |
| zero1k | −3.427% /0/6 | −2.485% /0/6 |0.236% /0.427% |
| half1k | +8.159% /6/6 | +8.016% /6/6 |0.404% /0.425% |
| dense1k | −13.781% /0/6 | −13.227% /0/6 |0.248% /0.207% |

Default candidate medians241/245µs versus baseline183/189µs. The one mask buffer has96000 bytes for these768-dimensional1k cases (93.75KiB); no deep float copy. Dense candidate still computes/stores masks before choosing dense. A niche half-density improvement does not justify default/zero/dense regressions. No full-engine build or peer refresh is warranted for this candidate.

## Initial harness, quarantined

initial/ preserves the earlier source,binary hash,all raw observations and both completed series. Its dense writer encoded elements one by one rather than production bulk copying; sparse output also recomputed popcount per block, including zero masks. Corrected code uses production bulk dense copy and increments the sparse count only for emitted entries. The initial dense control is not a valid production-cost reference and is not pooled into the corrected results. Both initial and corrected default1k regressions remain saved; no result was discarded to obtain a win.

## Decision

**Reject; retain production aa8f307f unchanged.** Do not add a masked column format/cache or retry this unchanged implementation. A CPU-only cache does not establish commit improvement; this one already fails the default component gate. Full durable1k SQLite gap, native100k outlier and absent MSSQL/Kusto qualification remain open. Next independent hypothesis is cold/warm maintenance-WAL overlap using the newly accepted NEON writer, with lifecycle cost explicit and full-engine qualification still required.
