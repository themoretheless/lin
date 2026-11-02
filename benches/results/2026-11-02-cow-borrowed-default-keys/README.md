# Borrowed default field names: unqualified component

Parent shared-key source: 3fafb9589, with dedicated Linux experiment pending. The existing Row::insert(String, Cell) converts String into Arc<str>, allocating a temporary String for literal id/hash/embedding defaults. Expose insert_str(&str, Cell) and use it at those known Row call sites, including derived UPDATE hash. Keep the owning API available.

Allocation test compares equal rows inserted through the owned and borrowed entrypoints, for short and long names. Expected reduction is one temporary allocation per insertion, with no change to values, ownership, wire representation, quotas, WAL or sync. Run full release correctness before timing. No adoption or speed claim.

Full release completed: 463 passed, 0 failed, 2 ignored. A separate nocapture allocation test confirms one fewer allocation for each id/hash/embedding/long custom key insertion; equal stored rows are asserted. Query throughput is not yet measured for this component.
