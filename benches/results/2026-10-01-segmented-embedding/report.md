# Segmented hashing embedding experiment — rejected

Candidate avoids allocating the joined title/body/snippet string for HashingEmbedder by processing borrowed segments. Preserves unigram/trigram then whole-string bigram order, including spaces crossing segment boundaries. Default Embedder method joins segments and delegates to existing embed_batch overrides. Candidate was tested in a git-archive copy of commit 8126c0c; conflicted main worktree was untouched. Patch is experimental and NOT integrated.

Bitexact tests pass: 512 field combinations at each of dimensions 8/768/4096, empty/missing fields, whitespace, NUL, Unicode expansions and long repeated inputs; exact joined text and f32 bits checked. Bench candidate compiled successfully.

Six alternating process pairs for native and durable inserts; 24 fresh fixtures per case, one operation. Defaults embedding, FTS and scalar index preserved. Exact inserted common fields checked outside timers. Durable inserts preserve WAL sync_data. No builds/tests overlapped timing; host load uncontrolled. Fixed case order; large outliers prevent causal attribution. Milliseconds are median of process medians, gains are median paired decreases.

| Path | Rows | Lin baseline | Candidate | Paired decrease | Wins | SQLite control decrease |
|---|---:|---:|---:|---:|---:|---:|
| native | 1k | 1.108021 | 1.290219 | -12.70% | 2/6 | -14.84% |
| native | 10k | 12.179334 | 13.471813 | 2.27% | 4/6 | -0.65% |
| durable | 1k | 2.452302 | 2.514563 | -2.30% | 1/6 | -0.12% |
| durable | 10k | 20.494104 | 19.831896 | 3.25% | 5/6 | -1.44% |

Reject: target native 1k loses 4/6 pairs with large outliers; unchanged SQLite control also regresses, so native effects cannot be isolated. Native 10k is inconclusive. Durable 10k improves 5/6 pairs (+3.25%), but durable 1k loses 5/6 (-2.30%) while its unchanged SQLite control is nearly flat (-0.12%). This does not solve the target small-insert deficit. No broad peer victory or production speedup claimed. Original snapshot sources restored after preserving the candidate patch. Main merge state left untouched.
