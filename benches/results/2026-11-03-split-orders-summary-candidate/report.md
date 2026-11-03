# Independently lazy orders SUM and numeric safety: candidate

Baseline runtime b9f10bc776b05eb5235237e9a900c2ef6aaed1fd had a shared OnceLock<(sum,safe)>: an unfiltered scalar SUM computed numeric filter-safety facts it did not use, while grouped SUM requested safety and computed a scalar SUM it did not use. Isolated gate 41653785707 failed all 18 first-summary windows and four additional write controls, despite repeat-SUM wins. This prior failure remains binding.

Change only exec.rs/store.rs: separate independently lazy sum and numeric-safety OnceLocks. Scalar SUM uses only the exact original source-order iterator sum. Grouped/filter path requests only original finite/numeric/below-2^53 safety predicate. All five original invalidation locations clear both caches; all constructors initialize both empty. Test-only combined accessor preserves existing mutation/tag test coverage. New test proves either request leaves the unused cache uninitialized. No incremental floating subtraction/reassociation, relaxed integer precision/quota, API/feature/model/WAL change or work shifted outside operation timers.

This avoids the identified unnecessary scan but does not guarantee a benchmark win: an extra cache field and invalidation may affect Store layout and writes, and cold queries still scan their required values. Correctness tests, full workspace and new frozen cold/warm/write/read/resource qualification are required. No production merge or eight-peer success claimed.

Correctness passed: four grouped SUM regressions; full release workspace 415 passed / zero failed / 3 ignored, including independent cache laziness and existing mutation/tag/cache rollback checks. No new performance result yet.
