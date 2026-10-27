# Case-isolated COW qualification protocol

Same frozen production baseline and COW-inplace source as run40428404395, including the same owner-PID lock fix. No selective-index prototype. The prior run is retained and remains unqualified because four unchanged SQLite controls exceeded A/A.

Experimental-design change: each case runs in its own process, so previous cases cannot leave allocator/cache state in the next case. Twenty cases, twelve rounds, three binary roles = 720 real processes, eight fresh-fixture operations per case/process = 5760 observations. Baseline/control use the identical binary; candidate uses the prior COW binary source. Rotate case ordering by round and A/A/B ordering by round and case. This does not prove earlier control differences were caused by preceding-case state.

status.json has 36 completed *groups*, each made of 20 isolated processes; process-status.json retains all 720 actual process exits and load telemetry. The summary validates every process and all raw observations. Peer comparisons now use nearby separate processes in the same round, explicitly not within-process measurements. Same target and unchanged-control acceptance thresholds; no omitted cases or losers, no reduced durability/index/FTS work. Full eight-peer and durable qualification remain separate.
