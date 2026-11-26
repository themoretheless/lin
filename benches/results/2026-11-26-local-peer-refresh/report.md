# Local peer matrix on the Kusto-qualified Lin binary

Three fresh processes per size, three samples per case. PostgreSQL, MySQL and MongoDB ran in dedicated localhost Docker containers on tmpfs. Lin used its prepared Rust API. Server timings include driver and round-trip costs. Reads validate returned values and multiplicity. Native inserts validate six fields outside the timer. pandas participates in reads only.

| Mode | Rows | Lin wins / process comparisons | Smallest peer / Lin median ratio |
|---|---:|---:|---:|
| read | 1000 | 72/72 | 17.67× |
| read | 10000 | 72/72 | 12.97× |
| read | 100000 | 72/72 | 7.45× |
| insert | 1000 | 9/9 | 16.60× |
| insert | 10000 | 9/9 | 7.00× |
| insert | 100000 | 9/9 | 6.79× |
