# Patch-driven undo first CPU qualification

Six rotated A/A/B rounds,18processes; release macOS ARM64. Same repeated-row fixture,16samples of512 alternating prepared UPDATEs and one explicit transaction UPDATE+rollback per size1k/10k/100k. Exact restored row,index count and existing generation behavior checked. Full source/binary hashes,raw and controls retained; all Drop within operation,fixtures excluded. Production source restored exactly.

| Case | Gain | Absolute A/A | Positive |
|---|---:|---:|---:|
| 1000/ns_per_update | 7.610925% | 1.402591% | 6/6 |
| 1000/rollback_ns | 17.444194% | 6.362231% | 6/6 |
| 10000/ns_per_update | 7.510758% | 1.386614% | 6/6 |
| 10000/rollback_ns | 19.662011% | 3.798633% | 6/6 |
| 100000/ns_per_update | 6.510620% | 1.422724% | 6/6 |
| 100000/rollback_ns | 17.444766% | 2.360237% | 6/6 |

Every target wins6/6 and exceeds A/A in this diagnostic. One series and repeated-row behavior do not prove fresh-input engine performance, DELETE/large/multirow/cold/RSS/read/Full durable or peer wins. Candidate remains experimental pending Linux paired matrix and broader controls. No earlier rejected prototypes accepted by association.
