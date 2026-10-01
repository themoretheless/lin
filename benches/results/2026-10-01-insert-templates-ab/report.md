# Rejected prepared insertion templates

Three independent process pairs, alternating variant order; 12 fresh fixtures
per case. Both sources and SHA-256 hashes are saved under variants/.

| Pair | Original full 10k | Templates full 10k |
|---|---:|---:|
| 1 | 12.996 ms | 12.765 ms |
| 2 | 13.459 ms | 13.371 ms |
| 3 | 13.320 ms | 13.802 ms |

Process-median aggregates: original 13.320 ms, templates 13.371 ms.
The candidate did not consistently improve insertion, added retained row-template
memory to Prepared, and was removed. Full workspace tests passed the candidate;
a targeted repeated-insert/time test also passed. No peer win was established.
Background load is uncontrolled; these observations do not prove universal behavior.
