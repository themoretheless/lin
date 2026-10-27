# Linux fresh-input phase result

Run40424181803, frozen diagnostic head92399ca707a40fad736845a25f6b6b8a4247e13d,success. Exact existing compare fixture and checked validation,three processes each baseline/COW.198phase records (33/process,11/size) include8timed calls and3framework calibration/validation calls per size; all retained.144wall observations exist but instrumented timing is NOT acceptance. Original source hashes match main205; all instrumented source hashes and parsed logs independently verified.

Median of three within-process phase medians,ns:

| Size | Baseline call | COW call | Baseline first row clone | COW reference clone | Baseline unattributed UPDATE | COW unattributed UPDATE |
|---|---:|---:|---:|---:|---:|---:|
| 1k | 15499 | 13505 | 6132 | 90 | 1844 | 7895 |
| 10k | 26650 | 23976 | 12003 | 211 | 2524 | 14816 |
| 100k | 47840 | 47118 | 21952 | 230 | 4549 | 27612 |

Unattributed UPDATE is computed per record by subtracting disjoint measured child spans (positions,fks,index/FTS/maps,before/updated clones,undo capture,store replace) from update_cas inclusive time,then taking medians. It includes patch application,guards,body/hash work and uninstrumented bookkeeping; it is not a direct measurement of one operation. Nested parent spans must not be summed. Individual median columns do not form an additive decomposition.

Source inference: COW before/updated clones share Arc; first updated.insert invokes DerefMut -> Arc::make_mut and must deep-clone the shared map. Moving the expensive first deep copy from before_clone into patch application is consistent with the phase redistribution and near-equal100kcall times. These data do not isolate cache misses,allocator latency or Arc detach as sole cause. Direct first-write span and row/key memory layout controls would be needed for causal attribution.

At100k scalar index maintenance remains about5.7us,point-position selection2.3us,FTSsync1.8usbaseline/1.2usCOW,output Handle drop0.46us/0.08us. First cold row access/copy is materially larger than isolated reverse-hash lookup kernel; optimizing only subsequent warm clones cannot eliminate it. Do not move fixture reads outside timer to hide this cost.

All source/bench restored after build. No production optimization accepted: COW stability gate40421852372stillfailed,0/120SQLitewins remain recorded,public API compatibility/read/native/durable/RSS/all-peer scope open. Instrumentation overhead/code-layout changes and inclusion of framework validation calls limit quantitative extrapolation. Next implementation hypothesis must address the first map copy or scattered key storage while retaining lifecycle and failure controls.
