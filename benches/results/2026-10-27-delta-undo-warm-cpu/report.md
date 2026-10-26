# Full-row-diff undo CPU controls reject rollout

Release macOS ARM64, six rotated A/A/B rounds,18processes; full baseline/candidate hashes and raw retained. Same repeated-row UPDATE512 operations/sample plus one explicit rollback,16samples at1k/10k/100k. Every sample checks exact row/index and existing generation behavior. Handle/snapshot destruction and rollback restore inside timing; fixtures outside. Production sources restored exactly.

| Case | Gain | Absolute A/A | Positive |
|---|---:|---:|---:|
| 1000/ns_per_update | -10.659868% | 3.176014% | 0/6 |
| 1000/rollback_ns | 11.307141% | 4.706163% | 5/6 |
| 10000/ns_per_update | -8.117662% | 0.610458% | 0/6 |
| 10000/rollback_ns | 13.686127% | 1.042005% | 6/6 |
| 100000/ns_per_update | -6.500882% | 0.438258% | 0/6 |
| 100000/rollback_ns | 12.867792% | 1.406250% | 6/6 |

All UPDATE targets regress beyond A/A, zero positive pairs at every size. Rollback gains do not establish acceptable whole-workload behavior. Candidate rejected, not shipped. This diagnostic is not fresh-fixture Linux, Full durable or any peer qualification. Keep losses and do not repeat unchanged.
