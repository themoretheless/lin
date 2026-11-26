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

Reject the Linux x64 two-thread cap from `69c4136f1`. At default allocator settings, the first 100k insert lost all eight same-host pairs on all four EPYC hosts (32 losses in 32 pairs). Median paired slowdowns were 12.05 ms on 9V45 and 28.43 / 29.25 / 30.96 ms on the three 9V74 hosts. The 100k repeat also slowed in all 32 pairs, by median differences of 5.12 to 9.66 ms. Minor faults on the first 100k insert changed by at most 13 per host. Lower kernel CPU consumption did not translate into lower wall time.

[Experiment and paired sample data](https://github.com/themoretheless/lin/actions/runs/45789043619)

## Scheduling chunk size

Increasing each work item from 64 to 256 rows on large inserts slowed the first 100k insertion by a median paired 1.31 ms on EPYC 9V45 and 3.89 ms on EPYC 7763. The candidate won only two of eight pairs per host. Repeated 100k inserts slowed by 0.85 and 0.72 ms respectively. The engine retains the original 64-row work items.

[Chunk-size comparison and raw samples](https://github.com/themoretheless/lin/actions/runs/45791100037)

## Skipping WAL type checks in memory

Candidate `bfb4e00bc` won 16 of 32 same-host pairs for the first 100k insert against the original engine. Median paired changes were −0.44 and −2.85 ms on EPYC 7763, +0.69 ms on EPYC 9V45 and +0.37 ms on Neoverse N2. One 7763 host also showed a 3.09 ms repeat slowdown. The skip was dropped after this experiment.

[Paired samples and CPU provenance](https://github.com/themoretheless/lin/actions/runs/45793773235)

## First 100k transaction by statement

Eight fresh processes per host and mode. Each transaction runs ten prepared statements of 10,000 rows. Values are median milliseconds. The total is calculated per transaction before aggregation.

| Host | Mode | Total | Sixth statement | Commit |
|---|---|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor (amd-1) | default | 167.126 | 23.844 | 0.002 |
| AMD EPYC 9V74 80-Core Processor (amd-1) | thp | 111.790 | 16.071 | 0.003 |
| AMD EPYC 7763 64-Core Processor (amd-2) | default | 189.288 | 25.810 | 0.002 |
| AMD EPYC 7763 64-Core Processor (amd-2) | thp | 131.840 | 17.829 | 0.002 |
| AMD EPYC 7763 64-Core Processor (amd-3) | default | 192.602 | 26.375 | 0.002 |
| AMD EPYC 7763 64-Core Processor (amd-3) | thp | 134.549 | 18.501 | 0.002 |
| AMD EPYC 7763 64-Core Processor (amd-4) | default | 191.125 | 25.752 | 0.002 |
| AMD EPYC 7763 64-Core Processor (amd-4) | thp | 130.201 | 17.542 | 0.002 |
| Neoverse-N2 (arm64) | default | 99.977 | 15.816 | 0.002 |
| Neoverse-N2 (arm64) | thp | 75.764 | 11.885 | 0.002 |

Statement six had the highest median on every host in both modes. The profile measures elapsed time for each whole statement.

[Raw statement profiles](https://github.com/themoretheless/lin/actions/runs/45796253951)

## Cold 100k insertion by component

Median milliseconds across eight fresh processes in each mode. The separate profiling build times row construction and ID-map reservation. Each component is summed across ten statements before taking its median.

| CPU | Mode | Transaction | Build rows | Reserve ID map | Sixth statement | Sixth ID reserve |
|---|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor | default | 163.98 | 101.21 | 6.04 | 23.52 | 3.93 |
| AMD EPYC 9V74 80-Core Processor | thp | 110.54 | 56.25 | 5.24 | 16.07 | 3.11 |
| AMD EPYC 7763 64-Core Processor | default | 197.44 | 126.21 | 6.43 | 26.82 | 4.21 |
| AMD EPYC 7763 64-Core Processor | thp | 137.98 | 77.80 | 6.01 | 18.81 | 3.91 |
| AMD EPYC 9V45 96-Core Processor | default | 76.79 | 42.06 | 3.53 | 11.76 | 2.29 |
| AMD EPYC 9V45 96-Core Processor | thp | 68.27 | 33.00 | 3.65 | 10.68 | 2.34 |
| Neoverse-N2 | default | 102.89 | 62.88 | 4.88 | 16.10 | 3.40 |
| Neoverse-N2 | thp | 72.92 | 40.55 | 3.82 | 11.34 | 2.48 |

[Raw component profiles and separate uninstrumented DuckDB comparisons](https://github.com/themoretheless/lin/actions/runs/45798842879)

## Cold versus repeat component timings

For each fresh process, subtract the median of its two repeat transactions from its first transaction. The table reports the median paired differences across eight processes in milliseconds. Each transaction inserts 100,000 rows.

| CPU | Mode | Transaction difference | Row construction difference | ID-map reservation difference |
|---|---|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor | default | 61.06 | 50.76 | 2.06 |
| AMD EPYC 9V74 80-Core Processor | thp | 14.80 | 8.59 | 1.45 |
| AMD EPYC 7763 64-Core Processor | default | 80.03 | 66.00 | 2.53 |
| AMD EPYC 7763 64-Core Processor | thp | 24.44 | 15.70 | 2.09 |
| AMD EPYC 9V45 96-Core Processor | default | 16.02 | 13.32 | 0.59 |
| AMD EPYC 9V45 96-Core Processor | thp | 10.09 | 5.96 | 0.83 |
| Neoverse-N2 | default | 38.86 | 32.31 | 1.76 |
| Neoverse-N2 | thp | 9.03 | 7.27 | 1.19 |

[Source profiles](https://github.com/themoretheless/lin/actions/runs/45798842879)
