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

## Paired INSERT on the component-profile hosts

The ordinary release binary ran before profiling instrumentation was applied. Cells show median Lin / DuckDB milliseconds and Lin wins out of 25 pairs. Repeat uses the median of four subsequent insertions per process.

| Processor | Allocator mode | 10k first | 10k repeat | 100k first | 100k repeat |
|---|---|---|---|---|---|
| AMD EPYC 9V74 80-Core Processor | Default | 15.52 / 16.04 (22/25) | 7.09 / 15.86 (25/25) | 162.99 / 138.12 (0/25) | 95.06 / 137.68 (25/25) |
| AMD EPYC 9V74 80-Core Processor | THP | 10.75 / 15.87 (25/25) | 7.17 / 15.83 (25/25) | 110.16 / 138.02 (24/25) | 89.47 / 137.75 (25/25) |
| AMD EPYC 7763 64-Core Processor | Default | 18.52 / 19.53 (20/25) | 7.92 / 19.20 (25/25) | 197.96 / 168.95 (1/25) | 108.20 / 168.11 (25/25) |
| AMD EPYC 7763 64-Core Processor | THP | 13.29 / 19.41 (25/25) | 8.60 / 19.10 (25/25) | 137.91 / 168.20 (25/25) | 105.82 / 168.15 (25/25) |
| AMD EPYC 9V45 96-Core Processor | Default | 6.82 / 13.86 (25/25) | 3.95 / 13.65 (25/25) | 76.12 / 129.50 (25/25) | 60.53 / 131.54 (25/25) |
| AMD EPYC 9V45 96-Core Processor | THP | 6.11 / 13.95 (25/25) | 4.09 / 13.57 (25/25) | 68.13 / 129.70 (25/25) | 57.36 / 129.59 (25/25) |
| Neoverse-N2 | Default | 9.91 / 23.25 (25/25) | 4.57 / 23.13 (25/25) | 101.07 / 216.61 (25/25) | 61.18 / 216.35 (25/25) |
| Neoverse-N2 | THP | 7.82 / 23.17 (25/25) | 4.79 / 23.19 (25/25) | 74.58 / 216.42 (25/25) | 60.06 / 216.60 (25/25) |

[Uninstrumented paired measurements and runner identities](https://github.com/themoretheless/lin/actions/runs/45798842879)

## Byte-pair content hash formatting

Candidate `026ac1b11` still lost all 50 default first-100k comparisons against DuckDB on the two EPYC 7763 hosts. Lin / DuckDB medians were 193.50 / 165.33 ms and 194.42 / 167.83 ms. Against the baseline Lin build, median paired first-100k differences were −3.63 and −1.78 ms on those hosts, −0.63 ms on EPYC 9V45, and +1.08 ms on ARM64. The candidate won 19 of 32 first-100k pairs against baseline. The original formatter was retained because the full-operation gain did not reproduce across architectures.

[Paired benchmark and raw measurements](https://github.com/themoretheless/lin/actions/runs/45801207174)

## First-touch faults during row construction

Release timings below are Lin / DuckDB median milliseconds, with Lin wins in parentheses. Each pair uses the same allocator setting. Fault counts and row-build milliseconds come from a separate instrumented build, summed across ten batches for the first 100k insert and summarized by the median of eight processes. Faults are process-wide minor faults during the build intervals.

| Runner | Mode | 10k first | 10k repeat | 100k first | 100k repeat | Build faults | Build ms |
|---|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor (amd-1) | default | 17.85 / 19.21 (25/25) | 7.55 / 18.84 (25/25) | 187.94 / 167.07 (1/25) | 99.77 / 165.54 (25/25) | 80,358.5 | 123.52 |
| AMD EPYC 7763 64-Core Processor (amd-1) | thp | 12.68 / 19.10 (25/25) | 8.16 / 18.85 (25/25) | 129.79 / 164.28 (25/25) | 99.48 / 166.62 (25/25) | 30,269.0 | 75.18 |
| AMD EPYC 9V74 80-Core Processor (amd-2) | default | 15.23 / 15.96 (22/25) | 6.61 / 15.77 (25/25) | 163.87 / 138.39 (0/25) | 95.52 / 137.98 (25/25) | 80,358.0 | 101.22 |
| AMD EPYC 9V74 80-Core Processor (amd-2) | thp | 10.38 / 16.01 (25/25) | 6.94 / 15.83 (25/25) | 110.79 / 138.14 (25/25) | 90.75 / 137.95 (25/25) | 28,200.0 | 56.91 |
| AMD EPYC 9V74 80-Core Processor (amd-3) | default | 15.28 / 16.05 (21/25) | 6.54 / 15.86 (25/25) | 165.63 / 138.80 (0/25) | 95.15 / 138.76 (25/25) | 80,360.0 | 100.18 |
| AMD EPYC 9V74 80-Core Processor (amd-3) | thp | 10.23 / 15.97 (25/25) | 6.75 / 15.87 (25/25) | 110.61 / 138.78 (25/25) | 90.80 / 138.64 (25/25) | 28,224.5 | 55.76 |
| AMD EPYC 7763 64-Core Processor (amd-4) | default | 19.05 / 19.40 (19/25) | 7.85 / 19.20 (25/25) | 196.47 / 168.46 (0/25) | 106.00 / 167.93 (25/25) | 80,358.5 | 137.17 |
| AMD EPYC 7763 64-Core Processor (amd-4) | thp | 13.64 / 19.34 (25/25) | 8.43 / 19.13 (25/25) | 136.55 / 167.82 (25/25) | 104.89 / 167.58 (25/25) | 30,019.5 | 83.81 |
| Neoverse-N2 (arm64) | default | 9.62 / 23.10 (25/25) | 4.36 / 23.03 (25/25) | 99.17 / 216.67 (25/25) | 61.25 / 216.79 (25/25) | 80,357.0 | 61.01 |
| Neoverse-N2 (arm64) | thp | 7.58 / 23.03 (25/25) | 4.57 / 22.96 (25/25) | 74.05 / 217.16 (25/25) | 60.08 / 216.34 (25/25) | 34,317.5 | 39.78 |

[Paired release measurements and instrumented profiles](https://github.com/themoretheless/lin/actions/runs/45803896427)

## Coverage of the five-to-eight-cell candidate

The Lin fixture in `tools/peer-worker/src/main.rs` supplies seven fields (id, uri, wing, title, body, layer, ts). Row construction adds hash and embedding, yielding nine slots. Candidate `f2f14bcf2` changes the storage path only for widths five through eight. Both builds already allocate the nine-slot row directly in its final vector. Consequently differences from this paired run cannot qualify that change for this workload. The candidate was withdrawn.

## Exact-size iterator for wide rows

Candidate `7bbab9732` won 20 of 32 first-100k pairs against the previous worker. Median paired changes were −0.55 and −3.38 ms on two EPYC 7763 hosts, −0.52 ms on EPYC 9V74 and −1.06 ms on ARM N2. Repeat 100k slowed by 1.04 ms on the first 7763. With default settings, DuckDB won all 75 first-100k EPYC pairs. Lin won all 400 comparisons with THP enabled for both engines. The candidate was withdrawn and the wide-row WAL regression test retained.

[Raw paired measurements](https://github.com/themoretheless/lin/actions/runs/45806695900)

## Scope of a huge-page hint on the output vector

A local arm64 macOS layout probe on the restored constructor measured 112 bytes per Row and 24 per Cell. The public worker splits 100k records into ten 10k statements, so each row-builder output vector requests 1,120,000 bytes at this layout. Such a vector cannot contain a complete 2 MiB page. Its storage also excludes separately allocated row cells and vectors. Across 100k records the ten output requests total 11.2 MB, while the EPYC 7763 heap profile above attributed 308.8 MB to embedding construction. These figures concern requested storage rather than elapsed time. No engine change or Linux speedup is claimed from the layout probe.

## First-transaction premium outside row building

Using the instrumented fault-profile run under default settings, subtract the sum of ten row-build intervals from each 100k transaction. Within each of eight processes, subtract the median residual of its two repeats from the first residual. Times below are medians of those eight differences. Total premium compares the transaction times in the same way.

| CPU | Runner | Total premium ms | Premium outside row building ms |
|---|---|---:|---:|
| EPYC 7763 | amd-1 | 75.58 | 11.51 |
| EPYC 7763 | amd-4 | 86.49 | 12.82 |
| EPYC 9V74 | amd-2 | 63.67 | 10.99 |
| EPYC 9V74 | amd-3 | 60.57 | 10.46 |
| Neoverse N2 | arm64 | 36.20 | 6.79 |

The residual includes every stage outside the row builder. These within-Lin first/repeat differences do not measure the duration of an individual stage or a Lin/DuckDB gap.

[Per-process transaction and row-build intervals](https://github.com/themoretheless/lin/actions/runs/45803896427)

## Reuse uniform source layout for ID presence

Candidate `154c986e4` versus baseline `c226a2334`, eight alternating same-host pairs per size. First 10k improved in 28/32 pairs and first 100k in 22/32. At 100k the paired median improvement was 2.60 and 1.09 ms on the two EPYC 7763s, 1.27 ms on EPYC 9V74 and 1.50 ms on ARM N2. Repeat 100k was mixed, with changes of −1.26, +0.58, +0.32 and −0.60 ms respectively. Retained for further qualification on the experimental branch.

Against DuckDB on standard pages, all three EPYC hosts still lost all 25 first-100k pairs each. The gaps between medians were 25.56, 28.04 and 24.28 ms. With THP enabled symmetrically, Lin won all 400 pairs across sizes and hosts.

[DuckDB samples and same-host candidate comparisons](https://github.com/themoretheless/lin/actions/runs/45815495705)

## Linux kernel CPU during the first 100k INSERT

Six fresh processes per setting and host from the allocator experiment at 16cc0d25a. Each cell shows default / glibc THP medians. CPU time sums all threads, while wall time measures the full transaction. Peak RSS is the process high-water mark.

| Host | Wall ms | Kernel CPU ms | User CPU ms | Minor faults | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor amd-1 | 156.0 / 104.4 | 281.1 / 141.5 | 210.7 / 159.7 | 91268.5 / 33628.5 | 546.9 / 553.9 |
| AMD EPYC 7763 64-Core Processor amd-2 | 188.8 / 129.9 | 355.6 / 184.7 | 253.9 / 209.7 | 91293.5 / 35780.5 | 546.9 / 554.6 |
| AMD EPYC 7763 64-Core Processor amd-3 | 192.5 / 140.9 | 336.9 / 191.8 | 274.1 / 217.4 | 91291.5 / 36615.5 | 546.8 / 554.9 |
| AMD EPYC 7763 64-Core Processor amd-4 | 189.5 / 130.8 | 344.4 / 192.5 | 262.4 / 203.8 | 91295.0 / 36190.5 | 546.8 / 554.2 |
| Neoverse-N2 arm64 | 102.0 / 73.5 | 194.4 / 111.9 | 120.1 / 106.7 | 91285.5 / 38576.0 | 546.1 / 552.9 |

[Raw resource samples and host details](https://github.com/themoretheless/lin/actions/runs/45767474001)

## Inline temporary shape buffers

Candidate 417c1fc8d won 17 of 32 first-100k pairs against d922ce2a8. On the second EPYC 7763 it slowed first 100k by a paired median 2.29 ms and repeat 100k by 3.28 ms. ARM64 repeat 100k slowed by 1.34 ms. The candidate was withdrawn. Against DuckDB, EPYC default first-100k margins were losses of 24.85, 27.74 and 34.59 ms. Lin won 399 of 400 THP pairs, with a single first-10k loss on the first 7763.

[Raw samples and CPU provenance](https://github.com/themoretheless/lin/actions/runs/45821850933)

## Shape setup in the 100k row builder

Eight fresh processes per mode. Each process inserts 100k rows three times. Shape setup sums elapsed intervals across concurrent workers. Build time counts wall time across the ten statements. For repeats, average the second and third transaction per process before taking the median. Units are milliseconds.

| Host | Mode | First shape sum | Repeat shape sum | First build wall | Repeat build wall |
|---|---|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor amd-1 | default | 1.86 | 1.85 | 101.90 | 50.75 |
| AMD EPYC 9V74 80-Core Processor amd-1 | thp | 1.51 | 1.63 | 56.56 | 47.37 |
| AMD EPYC 7763 64-Core Processor amd-3 | default | 2.29 | 2.18 | 126.26 | 60.25 |
| AMD EPYC 7763 64-Core Processor amd-3 | thp | 2.00 | 2.09 | 76.42 | 60.76 |
| AMD EPYC 7763 64-Core Processor amd-4 | default | 2.39 | 2.33 | 125.09 | 60.60 |
| AMD EPYC 7763 64-Core Processor amd-4 | thp | 2.09 | 2.18 | 76.97 | 61.08 |
| Neoverse-N2 arm64 | default | 2.71 | 2.73 | 61.74 | 31.20 |
| Neoverse-N2 arm64 | thp | 2.36 | 2.53 | 41.10 | 33.48 |

[Per-process stage samples and CPU provenance](https://github.com/themoretheless/lin/actions/runs/45824512626)

## Sampled vector and cell construction on the first 100k INSERT

Each fresh process samples the eighth row in every group of 16, giving 6,250 rows per transaction. The columns are median per-process mean microseconds per sampled row across eight processes per mode. Repeat pools the two repeat transactions within each process. Vector includes input selection and embedding output construction. Cells includes field evaluation and populating the output row. Worker intervals overlap. The two phases do not cover the entire transaction.

| Host | Mode | First vector | Repeat vector | First cells | Repeat cells |
|---|---|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor amd-2 | default | 4.694 | 2.152 | 0.243 | 0.272 |
| AMD EPYC 9V74 80-Core Processor amd-2 | thp | 2.559 | 2.045 | 0.228 | 0.255 |
| AMD EPYC 7763 64-Core Processor amd-3 | default | 5.471 | 2.315 | 0.323 | 0.327 |
| AMD EPYC 7763 64-Core Processor amd-3 | thp | 3.265 | 2.366 | 0.284 | 0.319 |
| Neoverse-N2 arm64 | default | 2.542 | 1.109 | 0.227 | 0.263 |
| Neoverse-N2 arm64 | thp | 1.620 | 1.198 | 0.223 | 0.263 |

[Raw stage timings and CPU identities](https://github.com/themoretheless/lin/actions/runs/45825381818)

## Hashing features versus allocating and copying the output vector

Matched sampling within each transaction. Columns show median per-process mean microseconds per sampled row (6,250 samples per transaction, eight fresh processes per mode). Fill is the ASCII fixture feature-hashing call. Copy is the Arc allocation and slice copy combined. Repeat pools two later transactions. Neither component measures the entire transaction.

| Host | Mode | First fill | Repeat fill | First allocation + copy | Repeat allocation + copy |
|---|---|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor amd-1 | default | 0.292 | 0.286 | 5.014 | 1.845 |
| AMD EPYC 7763 64-Core Processor amd-1 | thp | 0.272 | 0.281 | 2.637 | 1.912 |
| AMD EPYC 9V45 96-Core Processor amd-2 | default | 0.180 | 0.174 | 1.526 | 0.749 |
| AMD EPYC 9V45 96-Core Processor amd-2 | thp | 0.177 | 0.175 | 1.063 | 0.748 |
| AMD EPYC 9V74 80-Core Processor amd-3 | default | 0.216 | 0.208 | 4.012 | 1.593 |
| AMD EPYC 9V74 80-Core Processor amd-3 | thp | 0.201 | 0.204 | 1.673 | 1.304 |
| AMD EPYC 7763 64-Core Processor amd-4 | default | 0.289 | 0.283 | 4.982 | 1.642 |
| AMD EPYC 7763 64-Core Processor amd-4 | thp | 0.273 | 0.278 | 2.512 | 1.883 |
| Neoverse-N2 arm64 | default | 0.161 | 0.160 | 2.164 | 0.850 |
| Neoverse-N2 arm64 | thp | 0.160 | 0.160 | 1.341 | 0.924 |

[Raw sampled intervals and host identities](https://github.com/themoretheless/lin/actions/runs/45826400045)

## Output buffer allocation and write on 100k inserts

Diagnostic build 79eeb10c6 sampled 625 rows in each 10k statement. Each cell is microseconds per sampled row, aggregated within a process before taking the median across eight processes. Repeats pool the next two transactions in the same process. Allocation includes Arc header initialization and any first-touch work during that interval. Write includes acquiring unique access and copying the payload. Worker timings overlap across threads.

| CPU / runner | First allocation default / THP | First write default / THP | Repeat allocation default / THP | Repeat write default / THP |
|---|---:|---:|---:|---:|
| EPYC 7763 amd-1 | 4.757 / 2.595 | 0.148 / 0.135 | 1.680 / 1.280 | 0.209 / 0.188 |
| EPYC 9V74 amd-2 | 3.906 / 1.844 | 0.112 / 0.100 | 1.489 / 1.212 | 0.186 / 0.167 |
| EPYC 7763 amd-3 | 4.875 / 2.538 | 0.146 / 0.131 | 1.687 / 1.780 | 0.193 / 0.182 |
| EPYC 7763 amd-4 | 4.899 / 2.493 | 0.148 / 0.130 | 1.655 / 1.762 | 0.207 / 0.187 |
| Neoverse-N2 arm64 | 2.215 / 1.305 | 0.087 / 0.099 | 0.802 / 0.893 | 0.123 / 0.115 |

[Run and raw samples](https://github.com/themoretheless/lin/actions/runs/45827839411)

## Huge pages and repeat latency within Lin

In two independent runs, all eight host/run medians showed slower 10k repeats and faster 100k repeats when glibc requested huge pages for Lin. For each trial, subtract the default Lin median of four repeats from the corresponding THP median. Positive differences mean THP took longer. The ranges span the four host medians in each run.

| Run | Repeat 10k difference (ms) | THP slower, 10k | Repeat 100k difference (ms) | THP slower, 100k |
|---|---:|---:|---:|---:|
| [45786610712](https://github.com/themoretheless/lin/actions/runs/45786610712) | +0.272 to +0.541 | 74/100 | −4.906 to −0.860 | 25/100 |
| [45821850933](https://github.com/themoretheless/lin/actions/runs/45821850933) | +0.242 to +0.473 | 73/100 | −4.356 to −1.693 | 20/100 |

## glibc top_pad at 2 MiB

Both peers received the same top_pad setting in their treatment processes. No explicit huge-page hint was set. Values are median paired differences in milliseconds (Lin treatment minus the comparison). Each host used 25 pairs per size. Repeats use the median of four subsequent inserts in fresh databases.

| CPU / runner | First 10k vs DuckDB | Repeat 10k vs DuckDB | First 100k vs DuckDB | Repeat 100k vs DuckDB | First 100k vs default Lin | Repeat 100k vs default Lin |
|---|---:|---:|---:|---:|---:|---:|
| EPYC 7763 amd-1 | −1.866 | −11.650 | +23.716 | −53.453 | −7.038 | +7.812 |
| EPYC 9V74 amd-3 | −1.770 | −9.402 | +19.689 | −37.150 | −4.857 | +7.892 |
| EPYC 7763 amd-4 | −1.941 | −11.808 | +28.895 | −45.674 | −8.177 | +11.152 |
| Neoverse-N2 arm64 | −14.161 | −18.632 | −117.613 | −150.244 | −1.393 | +4.379 |

Reject 2 MiB top_pad as a default. On EPYC it lost all 75 cold 100k comparisons against DuckDB and increased Lin's repeat 100k median on all four hosts.

[Raw samples and CPU provenance](https://github.com/themoretheless/lin/actions/runs/45828769837)

## Transferring the first row buffer

Candidate 8892009af reused the first batch's Vec. With default settings it won only 1/100 first 100k pairs against DuckDB on four EPYC 7763 runners. Its first 100k paired median versus the Lin baseline ranged from −3.314 to +5.987 ms across those hosts. With THP, two first 100k pairs on amd-2 still lost (498/500 wins overall).

Candidate 8950abf0a also removed the destination reserve on an empty collection. Values below are median paired Lin minus DuckDB differences in milliseconds at default settings.

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 9V74 amd-1 | −1.710 | −9.130 | +20.327 | −46.687 |
| EPYC 9V74 amd-2 | −1.135 | −9.125 | +25.672 | −42.207 |
| EPYC 9V45 | −6.666 | −9.445 | −46.448 | −65.453 |
| ARM64 Neoverse-N2 | −13.928 | −18.714 | −118.592 | −155.544 |

The revised candidate lost all 50 default first 100k pairs on the two 9V74 hosts. It won all 400 THP pairs. Against the Lin baseline the first 10k improved on all four hosts (25/32 wins), but first 100k on ARM64 slowed by a paired median of 1.747 ms (3/8 wins). Both changes were withdrawn.

[First candidate](https://github.com/themoretheless/lin/actions/runs/45830890614) and [revised candidate](https://github.com/themoretheless/lin/actions/runs/45831180568).

## Eight builders on four Linux x64 vCPUs

Withdraw candidate 2aea0f92b. On two EPYC 7763 hosts it lost all 16 first 100k pairs against the four-builder baseline. Median paired slowdowns were 21.69 and 19.17 ms. System CPU increased by 56.22 and 48.67 ms summed across threads, and involuntary context switches increased by 126 and 119.5. Minor faults increased by a median of only 24.5 per host. The ARM64 configuration remained at four builders.

Median paired Lin minus DuckDB latency in milliseconds, default / transparent huge pages:

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 7763 amd-1 | +1.30 / −5.26 | −8.40 / −8.96 | +45.33 / −10.47 | −67.93 / −73.16 |
| EPYC 7763 amd-4 | +1.15 / −5.33 | −8.85 / −9.04 | +43.94 / −8.08 | −68.91 / −75.60 |
| Neoverse-N2 | −13.46 / −15.65 | −18.84 / −18.58 | −114.06 / −142.19 | −154.86 / −156.67 |

All eight THP losses in 300 comparisons occurred on first inserts on EPYC: one at 10k and seven at 100k.

[Paired timings and process counters](https://github.com/themoretheless/lin/actions/runs/45833579837)

## Observed transparent huge pages around 100k INSERT

Eight fresh processes per host and mode. Each process measured three transactions on fresh databases. Snapshots of /proc/self/smaps_rollup follow preparation and precede the timer, then follow commit and the timer. The repeat column averages the two repeat snapshots within each process before taking the median across processes. Units are MiB. Percentages summarize per-snapshot AnonHugePages / Anonymous.

| Host | THP before first | THP after first | Share after first | THP before repeats | THP after repeats |
|---|---:|---:|---:|---:|---:|
| EPYC 7763 amd-2 | 159 | 371 | 68.0% | 421 | 426 |
| EPYC 9V45 amd-3 | 139 | 306 | 56.2% | 365.5 | 377.5 |
| EPYC 7763 amd-4 | 179 | 389 | 71.1% | 432 | 432 |
| ARM64 Neoverse-N2 | 138 | 335 | 61.5% | 390.5 | 395 |

In default mode every host had a zero median for AnonHugePages before and after both first and repeat inserts. With the glibc hint, part of the process was already on huge pages after preparation, and repeat transactions started with markedly greater huge-page residency. The process-wide counter covers allocator arenas, prepared input and other anonymous mappings.

[Snapshots and CPU provenance](https://github.com/themoretheless/lin/actions/runs/45835991931)

In the same profile, pair THP and default processes by trial number and subtract THP from default. Each cell is the median of eight paired differences. Row-build time and process minor faults cover the ten non-overlapping build intervals of the first 100k transaction. The residual is transaction time minus build time within each process. For repeats, average the two repeat transactions per process before pairing.

| Host | First transaction saving ms | Row-build saving ms | Residual saving ms | Fewer row-build minor faults, first | Repeat transaction saving ms |
|---|---:|---:|---:|---:|---:|
| EPYC 7763 amd-2 | 58.37 | 49.37 | 8.63 | 50028.5 | 1.06 |
| EPYC 9V45 amd-3 | 9.52 | 10.37 | −0.70 | 38222 | 2.13 |
| EPYC 7763 amd-4 | 60.32 | 49.82 | 9.68 | 49681 | −1.05 |
| ARM64 Neoverse-N2 | 24.01 | 18.72 | 5.24 | 45609 | 0.21 |

All 32 first transactions were faster with THP. Only 7 of 16 repeat pairs improved across the two 7763 hosts. These are diagnostic-build timings. The separate peer runs above measure release performance against DuckDB.

## Post-build time on 100k inserts

Medians in milliseconds from eight processes per host and mode. The validation interval includes checks for duplicate IDs and URIs. Maintenance includes row-map reservation and updates to store structures and indexes. Append moves completed rows into the collection. Sum each stage over ten statements per transaction and average the two repeat transactions within each process. EPYC ranges span three independent 7763 hosts.

| Host | Mode | Transaction | Build | Validation | Maintenance | Append |
|---|---|---|---:|---:|---:|---:|
| EPYC 7763 | default | first | 126.13–128.65 | 8.13–8.69 | 46.84–49.21 | 7.31–7.37 |
| EPYC 7763 | default | repeat | 54.88–60.62 | 7.55–8.24 | 41.18–42.34 | 1.55–1.66 |
| EPYC 7763 | THP | first | 77.85–78.96 | 7.53–7.58 | 41.32–43.12 | 4.41–4.97 |
| EPYC 7763 | THP | repeat | 61.59–61.99 | 7.28–7.53 | 39.35–39.71 | 0.73–0.87 |
| Neoverse-N2 | default | first | 59.56 | 4.62 | 26.97 | 3.58 |
| Neoverse-N2 | default | repeat | 31.91 | 4.57 | 24.65 | 0.73 |
| Neoverse-N2 | THP | first | 40.62 | 4.13 | 22.82 | 2.13 |
| Neoverse-N2 | THP | repeat | 33.51 | 4.04 | 22.30 | 0.39 |

On the first default EPYC transaction, ID-map reservation accounted for 6.06–6.34 ms within maintenance. Identifying the remaining cost requires timings inside the maintenance routine.

[Linux run and per-statement samples](https://github.com/themoretheless/lin/actions/runs/45837167074)

## Maintenance critical path on 100k inserts

All 1,920 measured statements entered the parallel maintenance branch. Setup precedes three concurrent operations. The table gives elapsed milliseconds per 100k transaction at default settings, as first / repeat. Operation medians overlap in time. Each mode had eight processes per host.

| Host | Maintenance total | Setup | Secondary index | Full-text index | Row maps and HNSW branch |
|---|---:|---:|---:|---:|---:|
| EPYC 7763 amd-2 | 46.14 / 40.85 | 9.97 / 8.51 | 11.34 / 10.77 | 35.14 / 31.62 | 20.64 / 19.77 |
| EPYC 9V74 | 47.62 / 42.47 | 9.31 / 7.86 | 8.99 / 8.51 | 37.79 / 34.11 | 21.94 / 18.86 |
| EPYC 7763 amd-4 | 51.14 / 45.10 | 10.47 / 9.46 | 11.51 / 11.36 | 39.76 / 34.89 | 23.22 / 21.59 |
| Neoverse-N2 | 27.17 / 24.48 | 6.48 / 5.31 | 8.74 / 8.54 | 19.90 / 18.74 | 12.50 / 10.22 |

The full-text index had the longest aggregate lane time on every host, for first and repeat inserts and with both page settings. With THP the first full-text pass still took 34.31–36.65 ms on EPYC and 17.24 ms on ARM64.

[Linux run and per-statement samples](https://github.com/themoretheless/lin/actions/runs/45838227208)

## Full-text partition build and merge

On every measured 10k statement the 5k-row tail produced 5,005 distinct terms. Exactly five already existed in the destination at merge time. The other 5,000 became new keys. This is the vocabulary distribution of this fixture.

Median milliseconds per 100k transaction, default allocator, first / repeat. Left and tail build concurrently. Merge follows their completion.

| Host | Full-text total | Destination half | Temporary half | Merge |
|---|---:|---:|---:|---:|
| EPYC 9V45 | 23.57 / 23.78 | 14.31 / 15.33 | 12.00 / 12.88 | 7.66 / 6.50 |
| EPYC 7763 amd-2 | 36.27 / 31.26 | 21.93 / 20.29 | 17.46 / 16.70 | 12.08 / 8.53 |
| EPYC 7763 amd-3 | 39.55 / 35.68 | 22.64 / 22.36 | 20.75 / 19.25 | 13.72 / 10.41 |
| EPYC 7763 amd-4 | 39.25 / 33.94 | 24.08 / 22.13 | 20.98 / 18.26 | 12.46 / 9.08 |
| ARM64 Neoverse-N2 | 23.04 / 21.24 | 11.12 / 12.52 | 8.67 / 9.59 | 9.10 / 6.24 |

[Per-statement counters and timing samples](https://github.com/themoretheless/lin/actions/runs/45839343845)

## Preparing hashes before the FTS merge

Withdraw 2d408555d. Against baseline 9f518bd41 the candidate had a slower first 100k median on all five hosts (+0.155 to +1.096 ms), and won only 16/40 paired first inserts. Repeat 100k medians on all four EPYC hosts also regressed (+0.131 to +0.646 ms).

Median paired Lin minus DuckDB milliseconds, default / THP, 25 pairs per size per host:

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 7763 amd-1 | −0.64 / −5.72 | −11.45 / −10.88 | +28.17 / −31.16 | −62.40 / −64.57 |
| EPYC 9V74 | −0.57 / −5.28 | −9.14 / −8.83 | +25.56 / −26.67 | −43.27 / −47.83 |
| EPYC 9V45 | −6.58 / −7.41 | −9.57 / −9.69 | −46.53 / −56.69 | −63.29 / −68.15 |
| EPYC 7763 amd-4 | −0.60 / −5.75 | −10.69 / −10.36 | +26.06 / −32.40 | −64.89 / −64.83 |
| ARM64 Neoverse-N2 | −13.20 / −15.55 | −18.55 / −18.39 | −116.11 / −142.87 | −155.82 / −156.91 |

Lin won all 500 THP comparisons. At default settings first 100k lost 75/75 pairs across the 7763 and 9V74 hosts. [Run and raw paired samples](https://github.com/themoretheless/lin/actions/runs/45840431320)

## Inline full-text terms up to 22 bytes

Candidate 0add1938e against baseline 9f518bd41. In the first qualification, repeat 100k improved by a paired median of 5.23–7.99 ms on all five hosts (37/40 wins). First 100k on ARM64 regressed by a median of 0.97 ms (3/8 wins). On EPYC 7763 repeat 10k slowed by 0.49 ms (1/8 wins). Keep the candidate experimental pending independent replication.

Median paired Lin minus DuckDB milliseconds, default / THP. Each cell summarizes 25 pairs per mode.

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 9V45 amd-1 | −6.85 / −7.65 | −9.29 / −9.26 | −52.27 / −62.25 | −70.31 / −74.76 |
| EPYC 9V45 amd-2 | −6.87 / −7.86 | −9.31 / −9.32 | −50.94 / −64.83 | −75.23 / −77.50 |
| EPYC 7763 | −1.05 / −6.44 | −10.88 / −10.74 | +23.62 / −35.62 | −67.02 / −66.92 |
| EPYC 9V74 | −0.28 / −4.79 | −8.73 / −8.43 | +26.23 / −29.05 | −46.00 / −50.52 |
| Neoverse-N2 | −13.31 / −15.50 | −18.66 / −18.40 | −121.78 / −145.28 | −160.61 / −160.38 |

The default first 100k lost all 50 pairs across EPYC 7763 and 9V74. THP won 500/500 pairs. [First qualification and raw samples](https://github.com/themoretheless/lin/actions/runs/45843502735/attempts/1)

In attempt 1, process peak RSS sampled immediately after the first 100k transaction and minor faults during that transaction decreased in all 40 same-host pairs. Peak RSS is a process high-water mark, including preparation. The table shows medians of candidate minus baseline for eight pairs on each host.

| Host | Peak RSS change MiB | Minor fault change | Baseline first-insert minor faults |
|---|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor (amd-1) | -0.86 | -68.5 | 91292.0 |
| AMD EPYC 9V45 96-Core Processor (amd-2) | -1.05 | -70.0 | 91292.0 |
| AMD EPYC 7763 64-Core Processor (amd-3) | -1.10 | -66.0 | 91290.0 |
| AMD EPYC 9V74 80-Core Processor (amd-4) | -0.96 | -69.5 | 91292.0 |
| Neoverse-N2 (arm64) | -1.57 | -67.0 | 91281.5 |

Attempt 2 on the identical revision reproduced the repeat 100k improvement against baseline in all 16 pairs: 7.267 ms on EPYC 7763 and 5.671 ms on ARM64. The earlier ARM64 first 100k slowdown did not repeat (4.054 ms faster, 8/8 wins). ARM64 first 10k was 0.164 ms slower, 1/8 wins. EPYC first 100k improved by 2.738 ms, 6/8 wins. Three other x64 jobs ran on non-target CPUs and returned no timings.

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 7763 | −0.42 / −5.93 | −11.19 / −10.45 | +25.21 / −33.88 | −63.20 / −67.11 |
| Neoverse-N2 | −13.48 / −15.77 | −18.71 / −18.50 | −121.85 / −145.90 | −160.72 / −161.50 |

Values are paired Lin minus DuckDB milliseconds, default / THP. The default first 100k lost all 25 EPYC pairs. Lin won all 200 THP pairs. [Independent repeat](https://github.com/themoretheless/lin/actions/runs/45843502735/attempts/2).

## Cold-start CPU and page faults of the inline FTS candidate

For each fresh Lin process, subtract the mean of its two repeat 100k transactions from its first 100k transaction. The table shows medians of eight within-process differences under default Linux page settings. System and user CPU accumulate across threads and cannot be added to elapsed time. The benchmark recorded zero major faults across all 168 transactions.

| Attempt / runner | CPU | Extra wall ms | Extra system CPU ms | Extra user CPU ms | Extra minor faults |
|---|---|---:|---:|---:|---:|
| 1 / amd-1 | EPYC 9V45 | 21.11 | 68.08 | −6.35 | 39,715 |
| 1 / amd-2 | EPYC 9V45 | 17.49 | 73.63 | −20.01 | 39,985 |
| 1 / amd-3 | EPYC 7763 | 84.45 | 249.49 | 36.11 | 44,683 |
| 1 / amd-4 | EPYC 9V74 | 70.78 | 199.63 | 31.47 | 44,819 |
| 1 / arm64 | Neoverse-N2 | 40.23 | 137.34 | −6.00 | 40,961 |
| 2 / amd-1 | EPYC 7763 | 85.44 | 234.79 | 54.56 | 44,348 |
| 2 / arm64 | Neoverse-N2 | 36.94 | 130.43 | −4.97 | 41,297 |

The two 7763 measurements reproduce an 84–85 ms first-transaction penalty within Lin. Relative to the two 9V45 hosts, the 7763 incurred about 11–13% more extra minor faults but over three times as much extra system CPU. These process counters cannot distinguish time servicing faults from other kernel work. A kernel CPU profile during row construction on the 7763 would distinguish first-touch page allocation from allocator synchronization and scheduling costs.

[Attempt 1 samples](https://github.com/themoretheless/lin/actions/runs/45843502735/attempts/1) and [attempt 2 samples](https://github.com/themoretheless/lin/actions/runs/45843502735/attempts/2), file `build-workers.json` in each matched runner artifact.

## glibc arena growth padding with default pages

The diagnostic worker compared glibc defaults with `glibc.malloc.top_pad=2097152`. Each host ran eight alternating-order pairs at each size. Every process validated its rows against the other side of the pair. Times below are median paired differences in milliseconds (padded minus default). For repeats, the two later transactions were averaged within each process before pairing.

| CPU / runner | First 10k | Repeat 10k | First 100k | Repeat 100k | First 100k peak RSS delta MiB |
|---|---:|---:|---:|---:|---:|
| EPYC 9V74 / 1 | −1.064 | +0.080 | −3.242 | +9.986 | −0.561 |
| EPYC 7763 / 2 | −1.410 | −0.783 | −5.614 | +10.564 | −0.479 |
| EPYC 9V74 / 3 | −0.704 | +0.069 | −1.896 | +11.096 | −0.576 |
| EPYC 7763 / 4 | −1.384 | −0.307 | −5.420 | +12.409 | −0.586 |

Reject the 2 MiB padding setting. First 100k improved in 30/32 pairs, but repeat 100k regressed in 31/32. First-transaction minor faults rose on every host (median paired increases of 920–1,467). This workload compared two Lin processes with the same diagnostic binary.

[Run, host details and raw process measurements](https://github.com/themoretheless/lin/actions/runs/45863266830)

## Symmetric TCMalloc on Linux

Release engine 0add1938e. With TCMalloc preloaded into both engines, Lin won each host median. The table shows median paired Lin minus DuckDB milliseconds, with Lin wins out of 25 in parentheses. Repeated INSERT uses the median of four later transactions within each process. Each engine verified the six projected fields after insertion.

| Host | First 10k | Repeat 10k | First 100k | Repeat 100k |
|---|---:|---:|---:|---:|
| EPYC 7763 amd-1 | −3.353 (25) | −11.643 (25) | −1.892 (18) | −93.702 (25) |
| EPYC 9V74 amd-2 | −3.210 (24) | −10.050 (25) | −3.163 (23) | −74.908 (25) |
| EPYC 7763 amd-3 | −3.442 (25) | −11.732 (25) | −3.816 (20) | −94.993 (25) |
| Neoverse-N2 | −12.355 (25) | −18.997 (25) | −104.201 (25) | −167.092 (25) |

With default allocation the same three EPYC hosts lost the first 100k by 27.269, 18.886 and 26.771 ms respectively, winning 1/75 pairs in total. TCMalloc improved Lin's own first 100k by 23.462–30.289 ms on EPYC. On N2 it slowed Lin's first 100k by 9.722 ms (all 25 pairs) and first 10k by 0.942 ms (22/25 losses), although Lin still beat DuckDB. The experiment changes the allocator through the process environment. Default Linux behavior remains an open performance gap on EPYC.

[Paired samples, CPU and allocator provenance](https://github.com/themoretheless/lin/actions/runs/45864248068)

Comparing the TCMalloc Lin process with the default DuckDB process in those same randomized blocks, first 100k wins were 20/25, 24/25 and 22/25 on amd-1, amd-2 and amd-3 respectively. Median paired Lin advantages were 3.388, 4.343 and 3.587 ms. First 10k won 74/75 and repeat inserts won 150/150 across these EPYC hosts. The separate [WAL/FULL qualification](2026-11-26-sqlite-wal-full.md#linux-x64-tcmalloc-candidate) used the same x64-only allocator setting.
