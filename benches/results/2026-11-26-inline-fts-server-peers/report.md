# Inline full-text terms against local server peers

Apple M4 Max, candidate engine 0add1938e. PostgreSQL 16, MySQL 8.4 and MongoDB 7 ran in disposable localhost containers with tmpfs data. Three fresh benchmark processes per size, three samples per observation. Server timings include driver and round-trip costs. Values and multiplicity were checked. These comparisons cover the native adapter semantics, including MongoDB w=1 with journaling acknowledgment disabled.

| Rows | Peer | Read wins | Smallest read ratio | Insert wins | Lin insert ms | Peer insert ms |
|---:|---|---:|---:|---:|---:|---:|
| 1000 | postgres | 18/18 | 21.71× | 3/3 | 0.35 | 6.49 |
| 1000 | mysql | 18/18 | 65.55× | 3/3 | 0.35 | 10.30 |
| 1000 | mongo | 18/18 | 326.77× | 3/3 | 0.35 | 6.07 |
| 10000 | postgres | 18/18 | 12.63× | 3/3 | 4.38 | 31.55 |
| 10000 | mysql | 18/18 | 66.09× | 3/3 | 4.38 | 86.47 |
| 10000 | mongo | 18/18 | 209.91× | 3/3 | 4.38 | 48.80 |
| 100000 | postgres | 18/18 | 8.58× | 3/3 | 154.89 | 514.22 |
| 100000 | mysql | 18/18 | 19.07× | 3/3 | 154.89 | 1880.24 |
| 100000 | mongo | 18/18 | 36.38× | 3/3 | 154.89 | 1707.29 |
