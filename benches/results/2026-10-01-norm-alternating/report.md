# Sparse normalization comparison

Three independent process pairs; 12 fresh-fixture samples per case.
The second pair reverses variant order. All compilation finishes before measurement.
Exact variant sources and their SHA-256 hashes are included. Host CPU frequency and other load remain uncontrolled.
Full insertion improved in all three pairs; this is evidence for this fixture, not a general peer win.

| Process / variant | Full ms | No embedding ms | No embedding/index ms | No embedding/index/FTS ms |
|---|---:|---:|---:|---:|
| process-1-exact_sum | 13.064 | 9.372 | 8.228 | 5.865 |
| process-1-sorted | 14.393 | 9.694 | 8.627 | 6.150 |
| process-2-exact_sum | 13.131 | 9.463 | 8.319 | 6.007 |
| process-2-sorted | 14.131 | 9.513 | 8.376 | 5.960 |
| process-3-exact_sum | 13.362 | 9.449 | 8.329 | 6.048 |
| process-3-sorted | 14.102 | 9.750 | 8.338 | 5.954 |
