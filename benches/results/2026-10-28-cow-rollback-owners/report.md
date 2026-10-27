# Pending Pack Row ownership: runtime diagnostic

Exact55a00a124 COW core with temporary test-only probes, source restored. One runtime test exercises both dropped and held public UPDATE Handles, then clears uncommitted transaction Packs and rolls back with full canonical-row/generation comparison.

With the UPDATE Handle alive, Arc owner count is3(store,Pack,Handle). After dropping Handle it is2; clearing Pack reduces it to1. With Handle retained, clearing Pack reduces3to2 and rollback keeps the held updated result unchanged. These exact counts confirm the additional Pack owner for this fixture. They do not measure allocations, establish the cause of the prior slowdown or prove a latency improvement.

The first fixture omitted body and therefore hash; it failed before ownership observations. Failed probe/log/status are retained under missing-body-*; body was added without changing core or owner assertions.

Next hypothesis must remove both avoidable sharing during undo and the Pack owner before undo, retain complete lifecycle/drop timing and held-result isolation, and pass independent performance gates. No production source change in this diagnostic.
