# Sparse block branch / entry packing CPU qualification

2026-10-24, baseline ca9eec4. Production source unchanged. Source-derived serial sparse WAL helper: same 8-element block width, compare short-circuit all-zero detection, packed u64 index/value, and their combination against original OR reduction + two slice copies. This does not repeat the earlier block-width experiment.

Real current default embeddings at 1k/10k, forced dense guard, None/empty/zero/tail/signed-zero/NaN payload/infinity boundary fixture. Every variant matches original bytes including an existing buffer prefix. Little-endian packing uses explicit to_le_bytes, no unsafe code. These finite fixtures are not universal format qualification.

Two series × six processes, twelve interleaved rotating samples per variant/case/process, reused output capacity. Setup and byte validation outside timing; warm CPU component only. No SQLite or end-to-end measurements; no causal DB-latency claim. Hashes/build/runner/raw samples retained.

| Case | Variant | Paired gain first / repeat | Positive processes first / repeat |
|---|---|---:|---:|
| default_1k | early | -0.25% / +0.98% | 3/6 / 3/6 |
| default_1k | packed | +4.08% / +3.75% | 6/6 / 6/6 |
| default_1k | early_packed | +1.18% / +2.82% | 3/6 / 3/6 |
| default_10k | early | +2.81% / +5.62% | 6/6 / 6/6 |
| default_10k | packed | +4.20% / +4.59% | 6/6 / 6/6 |
| default_10k | early_packed | +5.63% / +5.35% | 6/6 / 5/6 |
| dense | early | +2.76% / +1.53% | 6/6 / 5/6 |
| dense | packed | +6.00% / +5.76% | 6/6 / 6/6 |
| dense | early_packed | +21.62% / +21.70% | 6/6 / 6/6 |
| boundaries | early | -8.56% / -7.40% | 0/6 / 0/6 |
| boundaries | packed | +11.37% / +10.94% | 6/6 / 6/6 |
| boundaries | early_packed | -5.21% / -5.98% | 1/6 / 0/6 |

Packed-entry-only is a candidate for a subsequent engine gate; early exit is not justified at target 1k. First-series packed 1k saves roughly 9.7 us CPU per 1000 rows, only a fraction of the remaining durable SQLite gap. This report does not close that gap or prove an end-to-end gain. MSSQL/Kusto live comparisons remain missing. No production change accepted here.
