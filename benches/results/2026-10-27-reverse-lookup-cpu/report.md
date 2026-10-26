# Reverse lookup kernel: insufficient absolute scope

Source verification: LiveIndex.reverse already FxHashMap<usize,IndexKey>, not BTreeMap. Production representation unchanged. Standalone diagnostic builds actual Store index with benchmark title/wing/timestamp fixture then materializes dense Vec<Option<IndexKey>> outside timing. Exact mappings verified. Six independent processes,3sizes x2access patterns x24rotated A/A/B samples x4096lookups:10616832lookup operations. Same-slot and deterministic spread patterns retained. Drop/build/mutation costs are outside this lookup-only kernel and are not qualified.

| Size/access | Saved ns/lookup | Gain | Absolute A/A | Positive processes |
|---|---:|---:|---:|---:|
| 1000/same | 0.564575 | 56.346% | 0.494% | 6/6 |
| 1000/spread | 0.653625 | 55.064% | 5.520% | 6/6 |
| 10000/same | 0.572205 | 57.325% | 0.000% | 6/6 |
| 10000/spread | 0.740051 | 54.647% | 2.834% | 6/6 |
| 100000/same | 0.572083 | 57.323% | 0.257% | 6/6 |
| 100000/spread | 1.157043 | 62.583% | 1.101% | 6/6 |

Despite relative gains, only0.565..1.157ns saved per lookup. This is not evidence of a dominant source of multi-microsecond UPDATE/DELETE gaps. Do not promote a dense reverse rewrite as an engine optimization from these figures. Dense storage also needs bounded sparse/high-position fallback, lifecycle/memory tests and public-field API compatibility; none implemented here.

No production change. Source/binary hashes,fullraw and reproducer preserved. Results do not establish full-engine, cold-cache, Full durable or peer superiority. Lookup kernels are pipelined over hot repeated position arrays and cannot be extrapolated as cold random engine access latency.
