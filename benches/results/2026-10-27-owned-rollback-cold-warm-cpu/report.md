# Owned cold rollback CPU diagnostic

Six rotated A/A/B rounds,18processes, same fixture/sampling/readback contract as owned-rollback-warm-cpu. All successful operations and rollback controls retained. Exact baseline/candidate source and binary hashes in provenance.json. Sources restored exactly. Cold function attributes change compilation, not runtime semantics.

| Case | Median gain | Absolute A/A | Positive pairs |
|---|---:|---:|---:|
| 1000/ns_per_update | 0.376197% | 2.046931% | 3/6 |
| 1000/rollback_ns | 20.156508% | 3.049770% | 6/6 |
| 10000/ns_per_update | -2.018871% | 1.886628% | 2/6 |
| 10000/rollback_ns | 20.548977% | 2.486854% | 6/6 |
| 100000/ns_per_update | 0.329696% | 1.160485% | 4/6 |
| 100000/rollback_ns | 21.008634% | 1.423366% | 6/6 |

Not qualified: UPDATE10k -2.019% falls outside max(2%,1.887%A/A). Rollback wins6/6 at all sizes cannot erase this failed unchanged control. Small positive UPDATE results at other sizes remain below A/A. This repeated-row CPU evidence does not establish fresh-fixture Linux, Full durable, whole-database or peer performance, nor isolate code layout as the cause. Further attribute tuning based only on these rounded figures would risk overfitting; no rollout accepted.
