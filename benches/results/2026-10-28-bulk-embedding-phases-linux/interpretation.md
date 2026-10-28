# Embedding phase interpretation

[Run40631973188](https://github.com/themoretheless/lin/actions/runs/40631973188), frozen source fde0cf63b054b79692641682c7755870cf672b4a, profiles restored COW+CAS baseline a7d4b02c3. Independent audit passed for48fresh processes /384wall observations /132trace invocations.449instrumented release tests /2ignored and6Linux full-value/phase-count smokes passed. All318raw artifact files retained and individually SHA256 verified inside the archive.

At10k rows, vector allocation/copy into Arc accounts for15.3035% of instrumented outer INSERT wall, feature hashing7.3426%, row installation5.6967%, text collection3.9060%, normalization3.1980%, scratch reset1.8013%, lowercase1.7813%, references0.0366%. At1k, vector copy8.9571%, feature hashing7.9694%; see audited-report.md for every phase.

Observer calibration is material: traced/disabled1.1548at10k and1.1811at1k. Disabled/original0.9985/1.0040; A/A3.1515%/1.8693% respectively. These are different instrumented processes; do not subtract these overheads from individual phases, multiply into an unrelated baseline or interpret them as exact uninstrumented fractions. Phase and per-process medians need not sum100%.132trace invocations include validation/calibration, not just384measured observations.

Vector allocation/copy is a useful next hypothesis, particularly at10k. This phase includes both allocation and copying; it does not distinguish allocator latency, memory traffic or page faults. No unique cause established for prior regressions. Future candidates must retain every full768-dimensional vector, bit/order correctness, custom embedder behavior, row/error/transaction semantics and unchanged peer controls. No vector omission, reduced dimension, preparation-time relocation or relaxed durability.

The profile adds only four declared reversible source patches plus a temporary module. Every source/benchmark was restored; no production change. It does not measure RSS/allocation counts/WAL, qualify an optimization or establish superiority over any peer. No unchanged diagnostic rerun is needed; next work should test a changed, bounded allocation/copy hypothesis.
