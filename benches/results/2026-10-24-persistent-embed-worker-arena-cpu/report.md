# Persistent embedding worker arena: rejected for short 1k target

Source `281255389e56e7034c3645704e96743dcbd14190`; no production source change. A standalone wrapper around public HashingEmbedder methods compares current serial batch with one process-lifetime worker, unlike the earlier rejected per-call scoped-thread prototype. Split >=512 texts with dimension >=128 into two halves; main thread computes the first half, a named worker computes the second, outputs merged in original order. Bounded one-job channel, try_send serial fallback when full/disconnected, serial fallback on response-channel failure. Worker creation failure also falls back to serial. These failure branches are design/code paths, not injected OS failure evidence.

Transfer representation: copy right-half texts into one String with a vector of byte-range spans; valid UTF-8 string boundaries are retained. The timer includes copy/allocation, queueing, worker execution, join/merge and output destruction. Text generation/model construction/exact checks are outside. Warm worker throughput timing, six independent release processes, 12 alternating rounds per case, equal 16/2/4/16 loops for default1k/default10k/Unicode1k/small-dimension cases. No runner build/test overlap; unrelated host activity uncontrolled.

Output bit checks compare against current serial embedding at dimensions 8/9/128/768/1536 and batches 0/1/511/512/513/1000, including Unicode, vertical tab and Unicode separators, then every timed fixture is checked before timing. No quantization/model/normalization changes. Current implementation is the oracle, not an independent semantic reference. First proposed batch call, including worker startup and cold allocation, is separately retained: median 0.405 ms; no baseline cold-start causal comparison. A single persistent worker adds thread/stack/TLS resources until process exit; peak RSS, multiple simultaneous caller throughput, fork/shutdown and injected failure behavior are unqualified. This is not an acceptable production pool qualification by itself.

| Case | Serial ms | Worker ms | Paired gain | Faster processes |
|---|---:|---:|---:|---:|
| default_10k | 1.994271 | 1.816422 | +9.789% | 6/6 |
| default_1k | 0.174882 | 0.175512 | -1.004% | 3/6 |
| small_dim_fallback | 0.151803 | 0.151781 | -0.071% | 2/6 |
| unicode_1k | 1.542430 | 0.852576 | +44.865% | 6/6 |

## Decision

Reject this global >=512 worker policy for the targeted short-text 1k improvement: it is effectively flat/slightly negative (-1.00%, 3/6 processes). The large/long-text gains are component scope and cannot establish actual database gains or justify hiding the failed short-text target. No end-to-end/native/durable/workspace run warranted for this unshipped diagnostic after the failed target gate. A different workload-based policy would require separate evidence and full resource/concurrent/fallback integration gates; it is not enabled here.

Both variants and all raw measurements are retained. No production thread pool, dependency/default change or public API addition. The full peer objective remains open: durable 1k SQLite, missing MSSQL/Kusto and other outstanding contracts.
