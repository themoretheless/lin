# Rejected per-batch byte-pair slot cache

Three independent process pairs, alternating order; 12 fresh fixtures per case.
The candidate allocated a 256 KiB lazy slot table for batches of at least 1024
texts. All byte pairs and several vector dimensions matched float bits, including
reuse and a large Unicode batch. Workspace tests passed.

| Pair | Original full 10k | Cached full 10k | Original no embed | Cached no embed |
|---|---:|---:|---:|---:|
| 1 | 24.435 ms | 30.080 ms | 14.914 ms | 14.015 ms |
| 2 | 12.257 ms | 15.419 ms | 9.480 ms | 10.705 ms |
| 3 | 14.320 ms | 16.193 ms | 10.639 ms | 10.845 ms |

The candidate lost all full-insertion pairs and was removed. Background CPU
load was substantial/uncontrolled and unchanged controls varied; no causal
regression percentage or peer win is claimed. Additional retained batch memory
is not justified by these observations. Sources, hashes and raw runs are saved.
