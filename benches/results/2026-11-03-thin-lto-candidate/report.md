# Portable release ThinLTO — unqualified build-profile candidate

Runtime Rust sources unchanged versus b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Root Cargo.toml adds profile.release lto="thin" and codegen-units=1. No native CPU target, RUSTFLAGS, target-specific intrinsic, fast-math, model/features/API/durability change, custom allocator or trained PGO profile. This becomes the ordinary portable workspace release configuration if eventually accepted; it is not a benchmark-only switch.

Hypothesis: cross-crate optimization and one code-generation unit can reduce call/layout overhead across hashing, row construction, indexes and query execution without per-feature source changes. Longer builds/linking and higher compiler memory use are material costs; binary sizes and actual runtime/resources must be measured. No speedup assumed. This is distinct from rejected exact reciprocal modulo and deferred sparse-vector materialization ideas; prior performance failures remain preserved.

Require full release workspace correctness (not only default debug tests), source/binary provenance, frozen baseline release profile, all-size native/read/lifecycle/RSS qualification, and fresh peer/FULL WAL and platform checks before adoption. Baseline and candidate must build separately with their own manifest profiles and clean lin/worker artifacts. Cargo profile change alone is not evidence of any peer victory. Eight-peer objective remains unfinished.

Local cargo test --locked --release --workspace completed:414 passed,0failed,3ignored; log hash recorded in source-status.json. This validates the candidate optimized local build, not Linux timings or universal platform correctness.
