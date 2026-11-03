# Mutation-maintained orders numeric safety count

Prototype relative to correctness-aligned f37/b9: maintain the number of numeric-ineligible orders alongside existing column updates, instead of scanning the full columns to compute the cached eligibility fact. The original OnceLock tuple and exact source-order iterator sum remain unchanged. No public API, arithmetic, flags, allocator, or feature change. Original eligibility predicate (numeric cell, finite, absolute value below 2^53) is unchanged.

Both constructors initialize zero; collection rebuild clears and recomputes; both registration methods append or replace; in-place update subtracts the old predicate and adds the new; swap removal subtracts the removed row under the same parallel-column guards. Rebuild fallback remains unchanged. Tests cover all paths, NaN, infinities, +/-2^53, nonnumeric types, eligibility recovery, deletion and empty signed-zero sum. Full release workspace: 415 passed, zero failed, 3 ignored. Local test receipt hashes in source-status.json. A mistakenly cold debug target build was interrupted before tests; authoritative run used the existing isolated release cache.

Performance is unqualified. This moves predicate work to writes, which may regress INSERT/UPDATE/DELETE; the same frozen 54-window source gate must reject any binding loss. Main unchanged. All prior rejected prototypes remain rejected.
