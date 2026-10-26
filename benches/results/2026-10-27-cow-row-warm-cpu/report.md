# COW Row repeated-operation CPU diagnostic

Release macOS ARM64,main205 vs COW only. Six rotated A/A/B rounds,18processes.16samples per size1k/10k/100k;512alternating prepared UPDATEs plus one explicit rollback each. Exact row/index and baseline generation checks,Drop inside timing,fixtures outside. Full source/binary hashes/raw preserved; production source restored.

| Case | Gain | Absolute A/A | Positive |
|---|---:|---:|---:|
| 1000/ns_per_update | 48.332991% | 2.557663% | 6/6 |
| 1000/rollback_ns | 44.965127% | 3.072668% | 6/6 |
| 10000/ns_per_update | 48.032938% | 2.366167% | 6/6 |
| 10000/rollback_ns | 44.701440% | 1.501374% | 6/6 |
| 100000/ns_per_update | 47.647750% | 3.601453% | 6/6 |
| 100000/rollback_ns | 45.387875% | 3.648208% | 6/6 |

First diagnostic target gains exceed A/A,6/6positive each. Requires two-series fresh-input Linux validation,then read/bulk/native/durable/RSS/multirow/cold controls. Public Row alias changes to a COW newtype in the prototype: API/derive/external direct-map compatibility is unresolved. No production rollout or peer completion established.
