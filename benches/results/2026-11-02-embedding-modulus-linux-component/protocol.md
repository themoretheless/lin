# Constant modulus Linux root worker component

Baseline 60e64559ec0e5a21efd490d6da5a744e3ac8d0e2; candidate differs only in src/embed.rs (constant 768 token/trigram remainder and expanded dense bit oracle). Same dependencies, worker and root example. Ubuntu 24.04, Rust 1.96.0, default+experimental-extent-wal, root `cargo build --release --example peer_bench` for both roles. This deliberately uses the public peer root example rather than the previous FTS standalone package.

Before timing, full candidate default463/2 and feature483/4 suites, including independent dense bit oracle, must pass. Two balanced six-round series, 1k/10k/100k, 108 fresh processes /864 samples /3996000 exact six-field rows. Base/control use one baseline executable. No altered worker timers, batching, FTS, embeddings or process thresholds. Freeze all source/tests/derive/worker/build/workflow/scripts. All failures and samples retained. Independent series audit recomputes every row and sample summary.

Discovery only: no external peer, full source performance gate, compiled-negative audit or power-loss qualification. Original full acceptance gate remains failed and binding. Linux binary source delta and exact ELF archives must be audited before interpreting results. Do not retry unchanged results or adopt on component gains.
