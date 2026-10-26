# Single reverse write: repeated UPDATE CPU diagnostic rejected

Release macOS ARM64, frozen main205 vs index-only prototype. Six rotated A/A/B rounds, 18 processes. Each process seeds three collections (1k/10k/100k) with existing fixture, prepares two CAS statements and performs 16 samples of 512 alternating wing updates per size. 442368 total updates, 864 sample readback/index validations. Setup and validation outside timing, returned Handle drop inside operation. No warmup samples excluded. Full raw samples, hashes, build logs, status and summarizer retained; production source restored exactly.

| Rows | Median candidate gain | Median absolute A/A | Positive pairs |
|---:|---:|---:|---:|
| 1000 | -2.388% | 3.548% | 2/6 |
| 10000 | -1.559% | 1.991% | 2/6 |
| 100000 | +0.469% | 1.427% | 4/6 |

No repeatable win exceeds A/A. This candidate is rejected for performance rollout; do not launch unchanged full qualification expecting a different conclusion. Earlier 54-operation allocation control also found zero allocation saving. Correctness tests passing do not establish faster execution.

This is a warm repeated-row maintenance diagnostic, not the fresh-fixture Linux mutation contract, not a Full durable test and not peer comparison. It cannot close SQLite or any eight-peer requirement. No production source change accepted.
