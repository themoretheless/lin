# Read comparison with inline full-text keys

Engine 0add1938e, Apple M4 Max, shared workstation with concurrent compiler activity. Six validated prepared/API workloads, three fresh processes per size and five samples per observation. The substring case uses substring matching. Dedicated ranked lexical searches are outside this matrix. Ratios are peer median divided by Lin median within the same process and case.

| Rows | Peer | Lin wins | Smallest ratio | Closest case |
|---:|---|---:|---:|---|
| 1000 | sqlite | 18/18 | 6.08× | join_inner |
| 1000 | duckdb | 18/18 | 17.82× | join_inner |
| 1000 | pandas | 18/18 | 9.21× | join_inner |
| 10000 | sqlite | 18/18 | 7.72× | join_filter |
| 10000 | duckdb | 18/18 | 4.88× | join_inner |
| 10000 | pandas | 18/18 | 6.37× | join_inner |
| 100000 | sqlite | 18/18 | 7.79× | join_inner |
| 100000 | duckdb | 18/18 | 4.86× | join_inner |
| 100000 | pandas | 18/18 | 5.78× | join_inner |

## Prepared ranked full-text reads

Apple M4 Max, eight alternating fresh-process pairs per size. Baseline 28d181d78 has identical engine sources to 9f518bd41. Candidate 0add1938e. Each prepared query used FtsSeek. The probe checked expected matches and both builds returned identical ordered IDs in every pair. Per-process timings average queries over 40 ms. Host load was uncontrolled.

| Rows | Query | Baseline µs | Candidate µs | Median paired change | Candidate wins |
|---|---|---:|---:|---:|---:|
| 10000 | rare | 0.41 | 0.41 | -0.42% | 5/8 |
| 10000 | common | 61.78 | 61.25 | -2.51% | 5/8 |
| 10000 | miss | 0.26 | 0.26 | -0.69% | 4/8 |
| 10000 | prefix | 99.85 | 100.80 | +1.05% | 1/8 |
| 100000 | rare | 0.47 | 0.48 | +3.61% | 2/8 |
| 100000 | common | 1346.38 | 1147.46 | -11.15% | 6/8 |
| 100000 | miss | 0.30 | 0.30 | +0.11% | 4/8 |
| 100000 | prefix | 2637.15 | 1964.38 | -14.75% | 6/8 |
