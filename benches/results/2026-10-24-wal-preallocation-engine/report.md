# Apple WAL physical preallocation — real engine qualification

2026-10-24. Candidate: `candidate-persist.rs`; baseline: `before-persist.rs`. Exact core and executable hashes: `baseline-provenance.json`, `candidate-provenance.json`, `helper-provenance.json`, `interleaved-build-provenance.json`, and peer subdirectories. Source commit identifies the baseline; candidate hashes identify the measured uncommitted source. The measured candidate is retained with the limited scope below.

## Implementation and cost

Apple writable WAL opening reserves 1 MiB physically for an empty log. Frames >=64 KiB check allocation and request growth rounded to MiB, with each request capped at incoming bytes +1 MiB. This is best-effort F_PREALLOCATE; ordinary writes and existing error handling continue if reservation fails. Non-Apple builds do nothing. Logical length, frame bytes, codec, checksum, sync policy and logical quota accounting are unchanged. No claim of a physical quota limit or power-loss qualification.

Cold workers observed allocation while open: initial 0 -> 1,048,576 bytes; after 1k commit 339,968 -> 1,048,576 bytes. After process exit the 1k log was 337,083 logical /339,968 allocated bytes for both versions (`closed-file-allocation.json`). Different filesystems may behave differently. Whole-worker maximum RSS medians 19,570,688 /19,644,416 bytes include reference validation and teardown; this is not isolated allocator attribution.

## Conventional engine pairs

Two independent series, each six alternating fresh process pairs, 24 samples/case/process, native/durable 1k/10k, unchanged SQLite control. Raw runs and all statuses are retained. Gain is median of paired percentage changes; it differs from the ratio of aggregate medians.

| Case | Lin paired gain series 1 /2 | SQLite control series 1 /2 |
|---|---:|---:|
| Native 1k | +1.59% /-0.19% | +1.01% /+0.75% |
| Native 10k | +3.06% /-1.86% | +0.58% /-0.01% |
| Durable 1k | +11.58% /+6.37% | +0.23% /-0.03% |
| Durable 10k | -0.99% /-0.29% | -0.36% /+0.04% |

Durable 1k candidate medians 1.933 /2.514 ms still lose to SQLite 1.376 /1.391 ms. Durable 10k candidate 12.887 /12.620 ms versus SQLite 13.212 /13.023 ms. Native results are mixed; no native acceleration claim. All raw outliers remain, including the second series SQLite 1k outlier. The slight conventional 10k losses are retained, not hidden.

## Interleaved calibrated pairs

Fresh baseline/candidate builds used identical compiler and worker source; A/A binaries are byte-identical. Two series each of A/A and A/B, six worker pairs per 1k/10k, 24 alternating samples/worker; exact nine-field and f32-bit validation outside timing. 2,304 measured operations plus 96 preflights. Lifecycle differs from the conventional benchmark; do not pool protocols.

| Rows | A/A median absolute difference series 1 /2 | A/B paired gain series 1 /2 | A/B positive pairs |
|---|---:|---:|---:|
| 1k | 3.47% /1.93% | +3.16% /+7.22% | 5/6 /6/6 |
| 10k | 0.97% /2.10% | +0.09% /+0.70% | 3/6 /5/6 |

First 1k series is inconclusive against its A/A dispersion. Independent repeat supports a limited macOS 1k benefit together with conventional pairs. No universal confidence interval, p99, or broad 10k acceleration claim.

## Cold and correctness guards

Six fresh process pairs, no warm samples: median paired commit improvement 9.17%; open-to-commit 7.86%; open 1.28%; drop/checkpoint -0.98%. Input generation precedes timers; process loader is excluded. These observations do not prove universal startup/teardown performance. Raw time-l/RSS and every operation are retained in cold files.

Focused allocation-invariant test and full default `cargo test --workspace` passed (`checks.json`, logs). Reservation preserves contents, logical length, cursor and read-only open; overflow input is checked. Failed allocation/unsupported filesystem was not dynamically injected.

Separate process exit without Db Drop/checkpoint followed by read-only recovery verified 1/3/4 acknowledged commits (1k/3k/4k rows), all nine projected fields and every embedding f32 bit. Four commits cross the first MiB allocation boundary. Single-commit WAL matches the historical baseline byte-for-byte. No physical power-cut guarantee follows from these tests.

## Peer refresh and open objective

See `peer-audit.json` and each peer directory for current candidate measurements, exact hashes, server settings, cleanup and sentinel checks. Read contracts and native insertion contracts remain distinct; native insertion is not cross-engine disk-durability equivalence. pandas participates in read scenarios only. MSSQL and Kusto live endpoints are still missing; no wins against them are claimed. Durable 1k SQLite gap remains. The full eight-peer objective is not complete.

Current candidate refresh completed: **108/108 read comparisons** against six peers; **15/15 native insertion comparisons** against five peers. All samples complete and positive; exact validation, source stability, sentinel preservation, cleanup and stopped owned servers checked. Minimum aggregate read speedup 1.870x; native 1.113x. These are finite scenario results, not universal engine superiority.
