# Persistent owned-text hashing worker retest — rejected for default1k

Frozena5e28e36, public serial HashingEmbedder versus two-half hashing with a
bounded persistent worker. Ownership: copy right-half texts into Vec<String>,
try_send to capacity1 queue, compute left on caller, then merge in original order.
Copied text+String metadata is capped at1MiB before copying. Worker drops inputs
before replying; full/disconnected queues fall back to serial and are injected
in preflight. No real OS spawn-failure/fork/unload/concurrent-throughput proof.

This is the same owned-transfer mechanism rejected in the
[October24 experiment](../2026-10-24-persistent-embed-worker-cpu/report.md), with
new bounded-input, A/A and separate cold measurements. This retest corroborates
that rejection. It does not justify repeating the mechanism unchanged again.
The earlier arena variant was also flat/negative for short1k. Results from these
sources/protocols are not pooled.

Two six-process series,24 rotated A/A/warm-worker rounds,11 cases,9504 timed
compositions. Output destruction excluded; text copying, queue/wait, worker
execution and result merge included. Each case retains a worker for its rounds
and joins it after timing; there is no new production global pool. The first
candidate call includes worker creation and cold TLS, recorded separately once
per case/process, compared here to a warm serial reference (not matched serial
cold-start proof). Every result is raw-bit checked against the public serial
provider. No model, normalization, cache, DB/WAL or production-source change.

| Case | Warm gain s1 / s2 (%) | Positive processes s1 / s2 | A/A absolute s1 / s2 (%) | First candidate us s1 / s2 |
|---|---:|---:|---:|---:|
| default_1k | -8.368 / -6.235 | 0/6 / 0/6 | 1.398 / 0.795 | 185.604 / 180.792 |
| default_10k | +7.599 / +7.952 | 6/6 / 6/6 | 0.573 / 0.565 | 3080.021 / 2899.541 |
| boundary_682 | -0.094 / +0.039 | 2/6 / 3/6 | 0.094 / 0.059 | 108.645 / 100.876 |
| boundary_683 | -15.780 / -17.785 | 0/6 / 0/6 | 0.533 / 0.301 | 149.146 / 144.958 |
| boundary_512_dim1024 | +13.149 / +13.287 | 6/6 / 6/6 | 0.791 / 0.968 | 99.562 / 105.042 |
| guard_dim64 | +0.048 / -0.096 | 5/6 / 2/6 | 0.084 / 0.078 | 156.958 / 164.667 |
| guard_16500 | +0.001 / -0.136 | 3/6 / 2/6 | 0.481 / 0.170 | 2850.209 / 2828.999 |
| short_repeated | +0.014 / -0.173 | 3/6 / 2/6 | 0.316 / 0.429 | 72.667 / 72.041 |
| long_unicode | +48.563 / +48.577 | 6/6 / 6/6 | 0.303 / 0.115 | 3659.834 / 3609.625 |
| skewed_lengths | -81.999 / -83.339 | 0/6 / 0/6 | 0.255 / 0.844 | 123.938 / 115.938 |
| input_budget_guard | +0.112 / +0.006 | 4/6 / 3/6 | 0.148 / 0.092 | 7116.917 / 7034.542 |

Reject the general policy: default1k regresses6–8% in all12 processes and entry683
regresses16–18%. Large/Unicode gains cannot close or hide the default failure.
The skewed public-API case includes empty strings; Db normally skips empty-text
rows before calling its provider, so its≈82% loss is not directly a Db workload
claim. The ordinary nonempty default1k loss is sufficient for rejection.
The input-budget case stays serial. Bounded queue/input design is not a complete
production pool/resource qualification; RSS/shutdown/fork/multi-caller gates
remain unqualified. No full-engine run warranted after the failed target gate.
No observations removed. Future long-text-only routing would be a separate
hypothesis, not acceptance of this failed small-batch policy.
