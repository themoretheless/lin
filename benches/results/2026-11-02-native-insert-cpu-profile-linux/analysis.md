# Native INSERT 100k CPU discovery

Run 41396688130, frozen input 15da4231ce152dcf506025cd9e7f6c9e98f3976f, completed successfully. Independent audit verified 24 positive samples, all six fields of 200000 result rows, exact source blobs and the Linux ELF. No peer ran; these timings do not establish superiority or adoption.

The raw callchain stream contains 4272 samples. Filtering for an explicit `Transaction::run_prepared` ancestor selects 1511 samples. Among this selected CPU work, inclusive shares are:

| Callchain | Selected share |
|---|---:|
| Db::maintain_insert_slab | 50.17% |
| HashingEmbedder::embed_batch | 22.37% |
| FtsIndex::append_slab | 20.71% |
| Store::index_insert_slab | 13.96% |
| Store::row_maps_register_slab | 11.05% |
| fts::lowercased | 7.54% |

Shares overlap because callchains are nested. They exclude preparation, output verification, serialization and destruction outside this ancestor, plus begin/commit and spawned-thread stacks without the ancestor. They are not percentages of the entire operation's elapsed time; missing/inlined frames and sampling uncertainty remain. The worker used explicit frame pointers and debug information; profiled/unprofiled median ratio on this instrumented executable was 1.00443.

Next candidate: FTS lowercase fast path currently scans ASCII text once for ASCII membership and again for uppercase. Investigate one pass for already-lowercase ASCII while retaining exact Unicode normalization, posting-list behavior, snippet fallback and all default features. This is a code-inspection hypothesis informed by the profile, not a measured optimization. Keep embedding and index work in scope if that change cannot close the DuckDB gap. Rollback CPU is a separate unresolved SQLite gap.

Raw perf data, callchains, responses and logs are retained in raw.tar.gz with per-member SHA256. The exact ELF is retained in the workflow artifact and the separately downloaded local artifact directory. Reproduce filtered analysis with the committed analyze-callchains.py; no raw files were modified.
