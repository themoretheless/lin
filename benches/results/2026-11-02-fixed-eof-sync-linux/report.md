# Fixed-EOF fdatasync diagnostic

One Linux series; diagnostic only, no production WAL change or adoption. Positive gain means lower median latency relative to growing EOF.

| Mode | Gain % | Same-mode AA % | Positive pairs |
|---|---:|---:|---:|
| append | +0.000 | 1.236 | 0/6 |
| fixed_sparse | -1.714 | 2.629 | 1/6 |
| fixed_zeroed | +69.545 | 1.621 | 6/6 |

All 2304 observations, 36 commands, frozen input blobs and exact Linux executable hash were independently checked. Readback is not a crash/recovery or equivalent-WAL proof. No conclusion about full database superiority follows.
