# Bounded token feature slots: rejected

Run https://github.com/themoretheless/lin/actions/runs/41617316413 at6077447f23b56198d8e23d87b032c1ab693c88e0 completed216 processes/6156 operation samples/108 lifecycle-wall/108 RSS. Independent audit verified runtime/harness/helper hashes, explicit package compilation for both ordinary ELF workers, workspace tests, all exact readbacks/digests and every numeric window. Local workspace415 passed/0failed/3ignored. This is numerical rejection, not infrastructure failure.

| Rows | Series | Median gain % | A/A % | Positive pairs | Pass |
|---|---|---:|---:|---|---|
| 1000 | 1 | -1.497159 | 2.232816 | 3/6 | False |
| 1000 | 2 | -0.386966 | 4.508377 | 1/6 | False |
| 10000 | 1 | -2.605716 | 2.679993 | 2/6 | False |
| 10000 | 2 | -1.473919 | 2.985815 | 1/6 | False |
| 100000 | 1 | 2.974372 | 1.284281 | 5/6 | False |
| 100000 | 2 | -0.625791 | 0.889663 | 3/6 | False |

Zero of six native target windows passed; all twelve read-control windows passed. Eleven of twelve resource windows passed; process wall10k series2 was -2.006811% versus tolerance2%. All RSS windows passed. The100k series1 +2.974372% signal had only5/6 positive pairs and did not repeat in series2 (-0.625791%). No validated benefit for this workload; do not retain cache solely because a different word corpus might benefit.

Reject and restore src/embed.rs to baseline b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype remains at66f8ab874ebb3f1b4897ffb002b0a3f93514af3c. Preserve all binaries/raw artifacts; every archive member was reread against manifest.json. No unchanged numerical retry or main merge. No FULL WAL or fresh peer timings for this source. The complete eight-peer objective remains unfinished.
