# Shared row keys: unqualified vnext prototype

Parent experiment: inline row keys, commit 4d3c104c7. That candidate reduced clone allocations but produced mixed local timing with regressions; it was not adopted.

This candidate uses a single Arc<str> representation, eliminating variant dispatch and reducing native 64-bit key size from 24 to 16 bytes. Both short and long names share ownership when cloned. This may improve B-tree density and lookup costs but adds atomic reference counting for short names and an initial allocation for every distinct short key. No performance benefit is assumed.

Ordering, hashing, wire bytes, borrowed str lookup, owned String conversion, quota charges and COW isolation must retain the existing tests. Public API migration remains vnext-only. No change to main, WAL format or durability contracts.

Full release validation completed successfully: 462 passed, 0 failed, 2 ignored, including the allocation mechanism test. The timing harness will compare against the same original COW baseline; the candidate benchmark build is pending. No performance qualification.
