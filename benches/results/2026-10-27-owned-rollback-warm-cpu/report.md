# Owned rollback CPU diagnostic: controls fail

Release macOS ARM64, main205 vs owned rollback only, six rotated A/A/B rounds, 18 processes. Same fixture, 16 samples of512 alternating UPDATEs plus one transaction UPDATE+rollback per size1k/10k/100k. Exact row, scalar-index and baseline generation behavior checked. All raw preserved; Handle/snapshot/rollback Drop included. Sources restored byte-for-byte.

| Rows | UPDATE gain | UPDATE A/A | Positive | Rollback gain | Rollback A/A | Positive |
|---:|---:|---:|---:|---:|---:|---:|
| 1000 | -0.594% | 2.199% | 3/6 | +23.110% | 2.957% | 5/6 |
| 10000 | -5.447% | 2.135% | 0/6 | +14.602% | 1.982% | 6/6 |
| 100000 | -2.899% | 1.798% | 1/6 | +19.466% | 1.240% | 6/6 |

Rollback gains do not override failed unchanged UPDATE controls. Timing does not prove a unique cause; code layout and environmental effects are unisolated. Candidate unqualified, not shipped. No fresh-fixture Linux/Full durable/read/peer claim. Further work must explain and resolve controls, not drop them or replace broad acceptance with rollback-only wins.
