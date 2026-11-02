# Remaining SQLite FULL WAL gaps

Matched pairs from run 41391257035 only. Ratios are Lin/SQLite; below 1 is faster. No cross-run mixing.

| Rows | Operation | S1 wins | S1 median ratio | S2 wins | S2 median ratio |
|---|---|---:|---:|---:|---:|
| 1k | update | 6/6 | 0.6933 | 6/6 | 0.6804 |
| 1k | delete | 6/6 | 0.7006 | 6/6 | 0.7515 |
| 1k | commit | 6/6 | 0.7068 | 6/6 | 0.7284 |
| 1k | rollback | 4/6 | 0.9805 | 4/6 | 0.9853 |
| 10k | update | 4/6 | 0.9783 | 6/6 | 0.8979 |
| 10k | delete | 5/6 | 0.9181 | 6/6 | 0.9089 |
| 10k | commit | 6/6 | 0.8837 | 6/6 | 0.9256 |
| 10k | rollback | 0/6 | 1.6175 | 0/6 | 1.6191 |

Next optimization must address rollback CPU work as well as any remaining commit/update losses. Rollback performs no WAL write/sync in this fixture, so a faster WAL is insufficient. CPU profile of native insertion is a separate investigation and cannot establish rollback gains.

The component result is not full acceptance: all control variations and losses remain binding; latest runtime changes after frozen experiment are not covered.
