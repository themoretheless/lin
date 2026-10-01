# Uniform WAL cell-reference cache

Uniform rows now traverse each BTreeMap once to collect cell references. Column inference and construction then access references by position, avoiding repeated tree lookups. Heterogeneous rows keep field-name lookups. Values and wire format unchanged; references live only during pack construction. Temporary memory is row count × field count × pointer size, plus Vec capacity overhead; no cloned cells in this cache.

Six alternating process pairs, 24 fresh fixtures per case, one operation each. Full exact-row validation outside timer; Full fsync unchanged. Pinned optimized binaries. Builds/tests did not overlap timings. Host load uncontrolled; fixed case order within each process. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.887958 | 3.014417 | 26.585959 | 24.342042 |
| 2 | 3.098854 | 2.997395 | 25.060980 | 23.838500 |
| 3 | 3.074667 | 2.745479 | 25.230354 | 23.845000 |
| 4 | 2.889959 | 2.824542 | 24.700958 | 24.001271 |
| 5 | 3.330626 | 2.509792 | 23.607687 | 23.073562 |
| 6 | 2.779813 | 2.673875 | 27.088042 | 23.482687 |

Candidate faster in 5/6 pairs at 1k, median paired decrease 3.54%; at 10k faster in 6/6, median decrease 5.18%. Unchanged SQLite controls have median paired decreases 1.79% and -0.04%, respectively. No significance or isolated causal percentage claim. Change retained.

| Rows | Candidate median of process medians ms | SQLite same-process median ms |
|---|---:|---:|
| 1k | 2.785011 | 1.472865 |
| 10k | 23.841750 | 14.053677 |

Full offline workspace tests pass, including heterogeneous field union and exact sparse-vector local WAL replay/replica apply/checkpoint/reopen. Scoped formatting and diff checks pass. No GPU measurements. Lin still loses durable insert to SQLite; full eight-peer objective unproven. Raw observations, source baseline and hashes retained.
