# Known-tail FTS pop: rejected Linux qualification

Run 40412446615; frozen head afe57e706c17149d96b7465c3c103c248dc6a1fa. All 36 measurement processes completed, two six-round A/A/B series; acceptance failed. Independently recomputed all 5760 raw observations and 240 case comparisons using verify.py.

| DELETE size | Series 1 gain | Series 2 gain | Positive pairs |
|---|---:|---:|---|
| 1k | -1.119% | +0.826% | 2/6, 4/6 |
| 10k | -2.122% | -3.555% | 2/6, 1/6 |
| 100k | -8.854% | -2.960% | 1/6, 2/6 |

Candidate beats SQLite in 0/120 Lin scenarios. All unchanged update/transaction and SQLite controls retained in paired-report.md; several also regress beyond A/A limits, so timing does not prove a unique causal attribution. Nonetheless no target size passes the prespecified acceptance rule. Seven fewer allocation requests are not a latency win.

Do not enable this candidate in production or repeat it unchanged. Keep stateful oracle and diagnostic evidence for future distinct designs. Production Rust source remains main205. No Full durable/read/all-eight-peer qualification is established.
