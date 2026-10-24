# Fused normalization/output construction — CPU gate

2026-10-24 baseline 45ef370. Source-derived baseline/candidate HashingEmbedder modules built in one release executable, identical fill/hash/lowercase code. Candidate computes the existing sum/inverse in the original order, then initializes a fresh Arc slice with normalized values in a single full-dimension pass. Scratch remains unnormalized until existing touched-slot cleanup. Every output slot is initialized before assume_init; allocation ownership/count unchanged. Both complete source snapshots, executable hash, compiler, build/runner and raw samples retained. Production source unchanged.

This differs from the rejected 2026-10-01 sparse-output experiment: that experiment zero-filled an output and copied only touched values after existing normalization. This candidate fuses multiplication and dense output initialization and does not zero-fill then patch. No approximation, model/format change, worker or allocator policy.

Preflight compares every f32 bit against source-derived baseline for dimensions 8/9/768/1536, empty/short/default/Unicode strings and long repeated text to exercise normalization-order fallback. Complete batch output checked for each timed fixture. This is finite valid-input equivalence, not exhaustive arbitrary scratch-state or formal initialization proof. Some fixture strings contain literal backslash-u sequences; no actual vertical-tab coverage claimed for those strings. Existing independent production workspace tests were not rerun because no production patch is proposed.

Two series × six processes, 24 rotating samples/variant/case/process. Warm model/scratch and input preparation outside timing; output allocation/destruction included. Baseline and candidate own separate TLS scratch/table instances, so this is standalone component qualification, not DB lifecycle or cold-start equivalence. No runner builds/tests overlap timing; unrelated host activity uncontrolled. Percentage is median of paired process changes, not aggregate median ratio. All outliers remain.

| Series | Fixture | Baseline us | Candidate us | Median paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 226.227 | 216.481 | +2.503% | 5/6 |
| 1 | default_10k | 2651.552 | 2590.490 | +1.738% | 4/6 |
| 1 | unicode_1k | 2051.412 | 2428.539 | -4.506% | 2/6 |
| 1 | small_dim | 185.476 | 187.558 | -0.452% | 2/6 |
| 2 | default_1k | 191.764 | 185.969 | +3.669% | 6/6 |
| 2 | default_10k | 2161.130 | 2065.172 | +5.028% | 6/6 |
| 2 | unicode_1k | 1621.742 | 1636.185 | -1.187% | 0/6 |
| 2 | small_dim | 165.178 | 161.464 | +1.678% | 6/6 |

## Decision: reject global replacement

Default 1k gain is small (+2.50% /+3.67%), while Unicode regresses in both series (-4.51% /-1.19%, repeat 0/6 wins). Small-dimension result is mixed. Large first/second-series absolute differences also show host/protocol variability; no isolated causal slowdown mechanism inferred. No full-engine gate is justified for the unchanged global policy with this guard failure. Do not claim the component gain closes durable SQLite gap or accelerates public Db insertion.

No source change accepted, no new unsafe production path. Future output policy would need an independently justified dispatch/crossover and resource/lifecycle qualification before engine testing; this report does not establish one. Full SQLite/DuckDB/PostgreSQL/MySQL/MSSQL/MongoDB/Kusto/pandas objective stays open, with durable SQLite and live MSSQL/Kusto unresolved.
