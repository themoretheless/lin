# Sparse-only fused embedding output — CPU qualification

2026-10-24 baseline 2e1f81b. Follow-up changes the rejected global fused-normalization policy: dense finish retains original l2_normalize +Arc copy. Only the existing touched_count*5 <dim sparse branch computes normalization while initializing output. No new density threshold or text-specific switch. Source snapshots and derived baseline/candidate modules retained, no production acceptance inferred.

Two series ×six processes, 24 rotating samples/case/variant/process, warm models and TLS, output allocation/drop inside timing. All preflight vectors compare f32 bits against source-derived baseline at dimensions 8/9/768/1536, including empty/short/Unicode and long repeated strings. Every timed batch also compared before timing. Finite equivalence, not a proof for arbitrary scratch states. Each uninitialized output slot is written before assume_init. Setup/input/validation excluded; no runner builds/tests overlap timing, unrelated host activity uncontrolled.

## Observed branch occupancy

| Fixture | Rows | Dense rows | Min/max touched |
|---|---:|---:|---|
| default_1k | 1000 | 0 | 26/31 |
| default_10k | 10000 | 0 | 26/33 |
| unicode_1k | 1000 | 0 | 46/46 |
| small_dim | 1000 | 1000 | 8/8 |

Occupancy is measured outside timing using the same feature code. It explains which path is selected, not a causal attribution of all timing differences.

| Series | Fixture | Baseline us | Candidate us | Paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 174.558 | 168.920 | +3.404% | 6/6 |
| 1 | default_10k | 1933.843 | 1886.672 | +2.209% | 6/6 |
| 1 | unicode_1k | 1507.964 | 1504.323 | +0.014% | 3/6 |
| 1 | small_dim | 153.112 | 153.426 | +0.238% | 4/6 |
| 2 | default_1k | 174.971 | 169.837 | +3.166% | 6/6 |
| 2 | default_10k | 1954.011 | 1905.349 | +2.843% | 6/6 |
| 2 | unicode_1k | 1512.115 | 1514.505 | -0.267% | 2/6 |
| 2 | small_dim | 154.997 | 153.842 | +0.641% | 6/6 |

Default 1k +3.40% /+3.17%, both 6/6; Unicode +0.014% /-0.267% (flat/no acceleration claim), small dimension positive/mixed. Compared with the rejected global policy, existing dense fallback removes the repeated material Unicode loss in this finite gate. Absolute target saving is only several microseconds/batch. Candidate advances to a separate full-engine gate; CPU result is not a DB/peer win. Two modules have separate TLS/table state; no cold-start/RSS/power-cut qualification. Full eight-peer objective remains open.

Follow-up [engine gate](../2026-10-24-embedding-sparse-fused-engine/report.md) did not reproduce a full-commit gain. Candidate rejected; production source unchanged.
