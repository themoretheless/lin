# Alternating embedding experiment

Three independent processes per variant; 12 fresh-input samples per case.
Variant order reversed in the second pair. All compilation finished before each measurement.
Substantial unrelated host CPU load remained; unchanged control cases drifted.
No reliable speedup was demonstrated, so the reciprocal experiment was removed.
No new peer win is claimed from this run. Exact variant sources and hashes are included.

| Process / variant | Full ms | No embedding ms | No embedding/index ms | No embedding/index/FTS ms |
|---|---:|---:|---:|---:|
| process-1-modulo | 18.353 | 12.583 | 10.996 | 9.448 |
| process-1-reciprocal | 34.464 | 12.307 | 10.808 | 7.853 |
| process-2-modulo | 18.986 | 12.072 | 11.031 | 7.601 |
| process-2-reciprocal | 22.874 | 20.174 | 11.143 | 7.900 |
| process-3-modulo | 18.713 | 12.726 | 10.548 | 7.450 |
| process-3-reciprocal | 17.541 | 11.946 | 10.874 | 7.700 |
