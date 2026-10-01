# Proposed merge resolution with measured hybrid top-k

Date: 2026-10-02. Proposal is tested in an isolated archive of 8126c0c, NOT applied to the conflicted main checkout.

Keep the current verified embedding, FTS, slab-registration and bounded sparse WAL implementations. Preserve the branch's default Embedder.embed_batch_owned API without switching current inserts to it. Carry over hybrid bounded ranking and the large-bulk reopen regression test. Native bulk packing/large-batch persistence fixes are already covered by main's actual inserted-slice WAL path; keep its 16MiB record bound and sparse encoding rather than replacing it with the branch's 64MiB dense bound. Existing pending-posting and embedding bit compatibility paths remain.

Full offline workspace tests PASS; hybrid bounded-vs-full ranking checks three query texts and 42 skip/take combinations each, plus default top-50; includes ties, zero take and offsets beyond the last result. Large-bulk reopen regression passes. Initial test attempt used invalid search hybrid syntax and failed; fixed to the actual search syntax, then complete suite passed. Original failure log retained.

Six alternating process pairs, 24 observations each, one operation, warm prepared fts_hybrid_common case on 10k default documents. No tests/builds overlapped measurement. Both binaries preserve embedding, FTS and default behavior. Setup and input drop excluded; output drop included. Host load uncontrolled; no CPU-only, GPU or all-peer superiority claim.

Baseline median of process medians: 13.947979ms. Candidate: 8.289114ms. Median paired decrease: 40.44%; wins 6/6.

| Pair | Baseline ms | Candidate ms |
|---:|---:|---:|
| 1 | 13.030604 | 8.288291 |
| 2 | 13.081750 | 8.234500 |
| 3 | 14.367584 | 8.318812 |
| 4 | 21.752271 | 9.063396 |
| 5 | 15.007979 | 8.289937 |
| 6 | 13.528375 | 8.283313 |

The current merge still needs the user choice required by push-or-clear. No merge, push, branch deletion or worktree deletion performed. All-eight-peer objective remains unproven.
