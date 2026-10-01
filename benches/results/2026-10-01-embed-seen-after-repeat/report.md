# Rejected accumulator marker experiment

Replacing the separate seen bitmap with a zero accumulator test preserved all
embedding unit/integration tests but did not establish a performance gain.

| Full 10k insertion | Before | Candidate |
|---|---:|---:|
| Initial pair | 18.323 ms | 12.815 ms |
| Repeat | 13.178 ms | 13.285 ms |

12 fresh fixtures per case in separate processes. Initial unchanged no-embedding
controls also sped up strongly (11.984 to 9.601 ms), invalidating a causal 30%
speedup claim. The repeat did not favor the candidate. The experiment was removed;
the retained source still uses the seen bitmap. No peer win was established.
