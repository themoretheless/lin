# Persistent embedding worker owned-String: rejected for short 1k target

Source `281255389e56e7034c3645704e96743dcbd14190`; no production source change. A standalone wrapper around public HashingEmbedder methods compares current serial batch with one process-lifetime worker, unlike the earlier rejected per-call scoped-thread prototype. Split >=512 texts with dimension >=128 into two halves; main thread computes the first half, a named worker computes the second, outputs merged in original order. Bounded one-job channel, try_send serial fallback when full/disconnected, serial fallback on response-channel failure. Worker creation failure also falls back to serial. These failure branches are design/code paths, not injected OS failure evidence.

Transfer representation: clone each right-half text into Vec<String>. The timer includes copy/allocation, queueing, worker execution, join/merge and output destruction. Text generation/model construction/exact checks are outside. Warm worker throughput timing, six independent release processes, 12 alternating rounds per case, equal 16/2/4/16 loops for default1k/default10k/Unicode1k/small-dimension cases. No runner build/test overlap; unrelated host activity uncontrolled.

Output bit checks compare against current serial embedding at dimensions 8/9/128/768/1536 and batches 0/1/511/512/513/1000, including Unicode, vertical tab and Unicode separators, then every timed fixture is checked before timing. No quantization/model/normalization changes. Current implementation is the oracle, not an independent semantic reference. First proposed batch call, including worker startup and cold allocation, is separately retained: median 0.397 ms; no baseline cold-start causal comparison. A single persistent worker adds thread/stack/TLS resources until process exit; peak RSS, multiple simultaneous caller throughput, fork/shutdown and injected failure behavior are unqualified. This is not an acceptable production pool qualification by itself.

| Case | Serial ms | Worker ms | Paired gain | Faster processes |
|---|---:|---:|---:|---:|
| default_10k | 1.965927 | 1.872240 | +5.467% | 6/6 |
| default_1k | 0.175500 | 0.192864 | -10.607% | 0/6 |
| small_dim_fallback | 0.154101 | 0.154913 | -0.552% | 2/6 |
| unicode_1k | 1.564201 | 0.870057 | +44.325% | 6/6 |

## Decision

Reject this global >=512 worker policy for the targeted short-text 1k improvement: it regresses 10.61% in all six processes. The large/long-text gains are component scope and cannot establish actual database gains or justify hiding the failed short-text target. No end-to-end/native/durable/workspace run warranted for this unshipped diagnostic after the failed target gate. A different workload-based policy would require separate evidence and full resource/concurrent/fallback integration gates; it is not enabled here.

Both variants and all raw measurements are retained. No production thread pool, dependency/default change or public API addition. The full peer objective remains open: durable 1k SQLite, missing MSSQL/Kusto and other outstanding contracts.
