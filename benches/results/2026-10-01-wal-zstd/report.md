# Zstd sparse WAL compression experiment — rejected

Candidate introduces checksummed internal codec 4 containing original sparse codec, bounded decoded length and Zstd level-1 data. Only sparse payloads >=256KiB are candidates; use compressed frame only when smaller. Original 16MiB decoded frame budget and 64MiB sparse-vector budget preserved. Full fsync unchanged. Old codecs readable, but older readers would not understand new codec 4. This candidate was rejected and is not shipped.

10k default embedded document frame shrinks from 4,624,412 B to 427,472 B (10.82x). Exact local WAL replay, replica apply and checkpoint/reopen test passes. Full workspace offline tests pass before final decoder restriction; final targeted tests verify only sparse codec allowed, declared decoded lengths, truncation and outer CRC, plus exact 10k replay under final decoder.

Six alternating process pairs, 24 fresh fixtures/case, one operation each. Exact field validation outside timer. Pinned optimized binaries; builds/tests did not overlap measurements. Host load uncontrolled; fixed case order per process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.454730 | 2.896187 | 19.591916 | 23.767834 |
| 2 | 2.634812 | 2.839354 | 21.952708 | 23.864604 |
| 3 | 2.444438 | 2.960625 | 20.310520 | 22.962229 |
| 4 | 2.442562 | 2.957167 | 20.515563 | 23.332625 |
| 5 | 2.741313 | 2.973687 | 21.039271 | 23.108708 |
| 6 | 2.496688 | 2.885187 | 20.392916 | 23.480021 |

Candidate loses 6/6 pairs at both sizes. Median paired decreases -16.77% (1k), -13.39% (10k), meaning slower. SQLite controls median +0.35% and -1.06%. Size savings did not translate to speed on this host/workload; no isolated kernel or causality claim. Candidate codec/dependency/integration-test change reverted. Prior sparse codec 3 and WAL/document-map improvements retained. All-eight-peer speed goal remains unproven. Raw observations, candidate sources, test logs and hashes retained.
