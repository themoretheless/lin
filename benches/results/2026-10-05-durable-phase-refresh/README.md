# Current durable insert phase diagnostics

Production source: a7ff491, including retained four-way sparse WAL encoder and parallel sparse-size classifier. Temporary timers built into a separate pinned release benchmark binary. Production src/exec.rs and src/persist.rs restored byte-for-byte before running diagnostics. This commit ships evidence only.

Three independent processes, each 24 fresh measured fixtures per 1k/10k Full durable insertion plus benchmark preflight (27 diagnostic events each size/process). Existing fixture validation completes and every run status is complete. One operation per sample, zero warmup, no build/test overlap. Raw phase observations, binary/source SHA-256 and timer patch retained. The table gives medians of three process medians in milliseconds. Classifier time is nested within encoding: do not sum it again. Separate phase medians cannot be added/subtracted to derive exact total or causal gain. Logging perturbs execution; these are bottleneck diagnostics, not new peer superiority results.

| Phase | 1k ms | 10k ms |
|---|---:|---:|
| build_ns | 0.3468 | 3.3979 |
| checks_ns | 0.1858 | 2.7064 |
| embed_ns | 0.5376 | 6.6995 |
| index_ns | 0.1670 | 1.6476 |
| fts_ns | 0.2240 | 2.8532 |
| maps_ns | 0.0451 | 0.7131 |
| pack_ns | 0.3450 | 4.2105 |
| classify_ns | 0.1512 | 1.2422 |
| encode_ns | 0.9295 | 6.1535 |
| checksum_ns | 0.0181 | 0.1923 |
| write_ns | 0.0786 | 0.3708 |
| sync_ns | 1.4489 | 1.5452 |

Payloads remain 336968 bytes at 1k and 3558340 at 10k. The current 1k run has sync about 1.449 ms, encoding 0.929 ms, embedding 0.538 ms, packing 0.345 ms and row build 0.347 ms. Next implementation target is the small-batch vector encoding/packing path; normalization-only changes address a fraction of embedding and the prior half-density experiment was flat. Do not weaken Full-sync to close the SQLite gap. Large-batch results and timings from the previous day are not paired causal comparisons: ambient throughput shifts materially, so future candidates need alternating controls and independent repeats.

The full named-peer objective remains active. This diagnostic does not establish MSSQL/Kusto comparisons or close small SQLite insertion gaps; those remain outstanding.
