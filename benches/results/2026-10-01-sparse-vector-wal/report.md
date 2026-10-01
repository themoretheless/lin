# Sparse-vector WAL: 2026-10-01

10,000 default embedded documents now persist as one checksummed atomic WAL frame: **4,624,412 bytes**. Dense vector storage alone would require 30,760,000 bytes, excluding other fields. The frame ceiling remains 16 MiB.

## Durable insert measurements

Milliseconds; median of eight fresh fixtures per process. Three independent processes, same case order; host load uncontrolled. Setup, teardown and exact field validation are outside the timer. Lin Full fsync and SQLite synchronous FULL remain enabled. Lin also generates embeddings, FTS and CAS. These are workload comparisons, not proof of isolated codec speedup.

| Rows | Process | Lin ms | SQLite ms |
|---:|---:|---:|---:|
| 1k | 1 | 3.065062 | 1.413833 |
| 1k | 2 | 2.971750 | 1.649875 |
| 1k | 3 | 2.900729 | 1.403979 |
| 1k | median of process medians | 2.971750 | 1.413833 |
| 10k | 1 | 25.149438 | 13.684708 |
| 10k | 2 | 26.670146 | 13.870417 |
| 10k | 3 | 25.940749 | 13.750146 |
| 10k | median of process medians | 25.940749 | 13.750146 |

Lin remains slower than SQLite in these durable insert measurements. The former 10k run failed the WAL size limit; it cannot serve as a timing baseline. No overall victory over all eight requested peers is established.

## Verification and compatibility

Full workspace offline tests pass, including exact local WAL replay, replica apply, checkpoint/reopen, float bit preservation, corruption and malformed sparse payload bounds. Source checks and hashes are saved alongside this report.

New writers may emit codec 3 inside checksummed LIN06. Every reader/follower must be upgraded before consuming it. Small/dense vectors retain codec 2; edge-bearing inserts can use the existing MessagePack fallback. Decoded sparse vectors are limited to 64 MiB per record. This does not remove all large-batch limits.
