# Explicit take all scan prefetch regression

On main205, an unfiltered `docs | take all` returned only 50 of 73 inserted rows. The source scan prefetch used the implicit default limit even when the query explicitly requested all rows. The full-engine regression fails before the fix and passes after it, including full rows, projected rows, skip before/after unlimited take, finite take before/after unlimited take, zero take and unchanged implicit default. Every returned ID is checked in order.

The fix tracks the explicit unlimited take while calculating source prefetch. Finite take keeps its bound; implicit 50 applies only without an explicit unlimited take. No quota is changed by this fix.

Validation: 69 exec tests and 74 query/optimizer/semantics/security control tests passed locally. Before failure and all after logs retained. CI now runs these test targets on Linux, macOS and Windows. This correctness fix makes no throughput claim and is separate from the running COW mutation candidate.
