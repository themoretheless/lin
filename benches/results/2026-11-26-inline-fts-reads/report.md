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
