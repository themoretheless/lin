# Empty-store uniqueness lookup bypass — rejected

Candidate skips existing-row ID/URI map lookups only when the target collection starts empty. Batch duplicate checks remain unconditional. Nonempty collections retain existing-row checks. No integrity or durability checks removed.

Full offline workspace tests pass. New test exercises duplicate ID and URI both within empty batches and against existing rows; checks unchanged rows/generation on failure and a successful retry. Test retained after reverting production candidate.

Six alternating process pairs, 24 fresh fixtures per case, one operation. Full native insert paths; exact field validation outside timer. Pinned optimized binaries; builds/tests did not overlap timings. Fixed case order, uncontrolled host load. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 0.925917 | 0.943958 | 9.825541 | 11.067854 |
| 2 | 0.945312 | 0.938146 | 9.737375 | 10.202000 |
| 3 | 0.920334 | 0.946167 | 10.115688 | 10.557375 |
| 4 | 0.989354 | 0.970459 | 10.962521 | 10.787104 |
| 5 | 0.976438 | 0.982563 | 10.108250 | 10.348437 |
| 6 | 0.974646 | 0.943937 | 10.284292 | 10.408563 |

1k faster 3/6, median paired decrease +0.07%; 10k faster only 1/6, median decrease -3.37% (slower). Unchanged SQLite/DuckDB controls also vary. No stable improvement; production candidate rejected and original exec source restored. Previously retained WAL and document registration improvements remain. All-eight-peer goal remains unproven.
