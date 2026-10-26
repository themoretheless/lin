# Mutation stage diagnostic, main205 source

Temporary RAII stage timers, release build, macOS ARM64. Production sources restored byte-for-byte after build. Exact row/index readback passes for nine operations, fixture setup outside active timers. Counting allocator remains active during measured calls. This is diagnostic instrumentation, not a baseline/candidate benchmark or an accepted speed claim.

Stage order: mutation_positions, update_cas, delete_rows, check_row_fks, index_insert_row, fts_sync_at, row_maps_sync_updated, delete_rows_at, move_deleted_tail, index_remove_row. Values are inclusive nanoseconds; nested spans must not be summed. Timer overhead and cold first invocation perturb observations.

First process: wing-update at 10k/100k takes 7625/7666 ns inside update_cas, with point lookup 958/1208 ns; title-update FTS sync 2792/5875 ns. Delete at 10k/100k takes 9458/12333 ns, of which Store deletion 6667/9584 ns and tail maintenance 3042/4583 ns. First 1k invocation has large cold outliers and cannot be used as representative latency.

These observations reject treating point lookup as an established linear scan bottleneck. They do not identify a sole dominant cost and do not establish any SQLite win. Next refinement should separate FTS tail maintenance, column swap/pop and identity-map update, and repeat independent processes before choosing another prototype. Keep the rejected full-engine FTS v1 result as the acceptance reference.
