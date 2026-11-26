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

## Independent repeat and allocation profile

Run `45786610712` used the same engine code. Both engines received the glibc THP hint in the THP trials. Each row summarizes the first 100k insertion across 25 paired trials, in milliseconds.

| Runner | Default Lin / DuckDB | Default Lin wins | THP Lin / DuckDB | THP Lin wins |
|---|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor (amd-2) | 193.25 / 165.51 | 0/25 | 133.84 / 166.74 | 25/25 |
| AMD EPYC 9V74 80-Core Processor (amd-3) | 164.01 / 138.80 | 0/25 | 109.54 / 138.51 | 25/25 |
| AMD EPYC 9V74 80-Core Processor (amd-4) | 164.62 / 135.25 | 0/25 | 109.35 / 137.78 | 24/25 |
| Neoverse-N2 (arm64) | 110.07 / 220.10 | 25/25 | 84.46 / 220.00 | 25/25 |

Across both sizes and first/repeat comparisons, Lin won 399 of 400 THP pairs. The single loss was a first 100k insertion on EPYC 9V74 (amd-4), 147.32 ms versus 136.28 ms.

Heaptrack on EPYC 7763 attributed 308.8 MB across 100,000 allocations to embedding construction. The whole worker process peaked at 596.5 MB. These are allocation measurements. Time attributable to those allocations remains unknown.

[CI run, raw samples and heaptrack reports](https://github.com/themoretheless/lin/actions/runs/45786610712)

## Two-thread builder experiment

Reject the Linux x64 two-thread cap from `69c4136f1`. At default allocator settings, the first 100k insert lost all eight same-host pairs on each of the first three reporting EPYC hosts. Median paired slowdowns were 12.05 ms on 9V45 and 28.43 / 30.96 ms on the two 9V74 hosts. The 100k repeat also slowed on all three. Minor faults on the first 100k insert changed by fewer than 13 per host. Lower kernel CPU consumption did not translate into lower wall time.

[Experiment and paired sample data](https://github.com/themoretheless/lin/actions/runs/45789043619)
