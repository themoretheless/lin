# Prepared in-memory INSERT on Linux, 26 November 2026

Candidate `c730174f2`. Each size has 25 paired trials per host. First is the first insertion in a fresh process. Repeat is the median of four further insertions into fresh databases in the same process. Times below are medians across trials in milliseconds. THP requests transparent huge pages through glibc for both engines. Lin generates default embeddings. DuckDB inserts the six fixture columns from a registered DataFrame. Both engines validate the stored values.

| CPU and runner | Rows | Mode | First Lin / DuckDB (ms) | Lin wins / 25 | Repeat Lin / DuckDB (ms) | Lin wins / 25 |
|---|---:|---|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core (amd-1) | 10000 | Default | 8.30 / 14.66 | 25 | 4.82 / 14.40 | 25 |
| AMD EPYC 9V45 96-Core (amd-1) | 10000 | THP | 7.21 / 14.62 | 25 | 4.87 / 14.46 | 25 |
| AMD EPYC 9V45 96-Core (amd-1) | 100000 | Default | 84.02 / 133.75 | 25 | 67.16 / 134.30 | 25 |
| AMD EPYC 9V45 96-Core (amd-1) | 100000 | THP | 74.11 / 133.26 | 25 | 62.73 / 133.99 | 25 |
| AMD EPYC 7763 64-Core (amd-3) | 10000 | Default | 18.60 / 19.64 | 25 | 7.92 / 19.11 | 25 |
| AMD EPYC 7763 64-Core (amd-3) | 10000 | THP | 13.13 / 19.40 | 25 | 8.31 / 19.11 | 25 |
| AMD EPYC 7763 64-Core (amd-3) | 100000 | Default | 195.44 / 167.58 | 0 | 105.29 / 168.54 | 25 |
| AMD EPYC 7763 64-Core (amd-3) | 100000 | THP | 135.02 / 168.91 | 25 | 104.33 / 168.14 | 25 |
| AMD EPYC 7763 64-Core (amd-4) | 10000 | Default | 18.78 / 19.85 | 23 | 7.91 / 19.47 | 25 |
| AMD EPYC 7763 64-Core (amd-4) | 10000 | THP | 13.62 / 19.58 | 25 | 8.58 / 19.40 | 25 |
| AMD EPYC 7763 64-Core (amd-4) | 100000 | Default | 192.33 / 167.53 | 0 | 105.03 / 168.06 | 25 |
| AMD EPYC 7763 64-Core (amd-4) | 100000 | THP | 132.64 / 167.36 | 25 | 103.65 / 167.67 | 25 |
| Neoverse-N2 (arm64) | 10000 | Default | 10.27 / 23.77 | 25 | 4.70 / 23.54 | 25 |
| Neoverse-N2 (arm64) | 10000 | THP | 8.16 / 23.73 | 25 | 5.00 / 23.57 | 25 |
| Neoverse-N2 (arm64) | 100000 | Default | 101.14 / 217.64 | 25 | 62.58 / 217.63 | 25 |
| Neoverse-N2 (arm64) | 100000 | THP | 77.40 / 217.37 | 25 | 61.28 / 217.36 | 25 |

The default first 100k insertion lost all 50 pairs on the two EPYC 7763 hosts. THP won all 400 paired comparisons across the four measured hosts. This run did not sample EPYC 9V74.

[CI run and raw artifacts](https://github.com/themoretheless/lin/actions/runs/45784842486)
