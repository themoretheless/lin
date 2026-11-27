# Bulk INSERT with exact allocation when cloning wide rows

Candidate `606fd1e80`. Both engines ran locally on each GitHub Actions Linux runner. Lin used TCMalloc on GNU x64 and the default allocator on ARM64. Cells below show the median paired Lin minus peer time in milliseconds and the number of Lin wins. Positive differences favor the peer.

## Prepared in-memory INSERT against DuckDB 2.0.0

Each of the 25 pairs starts fresh worker processes. Repeat uses the median of the four subsequent inserts within a process. THP requests `glibc.malloc.hugetlb=1` for both processes. Page allocation was not measured. [Benchmark artifacts](https://github.com/themoretheless/lin/actions/runs/45986536191)

| CPU and replica | Mode | 10k first | 10k repeat | 100k first | 100k repeat |
|---|---|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor / amd-1 | Default | -3.462 (25/25) | -12.487 (25/25) | +1.393 (11/25) | -89.679 (25/25) |
| AMD EPYC 7763 64-Core Processor / amd-1 | THP | -3.530 (25/25) | -12.367 (25/25) | -0.083 (14/25) | -88.903 (25/25) |
| AMD EPYC 9V74 80-Core Processor / amd-2 | Default | -3.267 (25/25) | -10.455 (25/25) | +1.712 (7/25) | -71.065 (25/25) |
| AMD EPYC 9V74 80-Core Processor / amd-2 | THP | -3.089 (23/25) | -10.461 (25/25) | +2.430 (6/25) | -69.911 (25/25) |
| AMD EPYC 7763 64-Core Processor / amd-3 | Default | -3.529 (25/25) | -12.703 (25/25) | +0.257 (12/25) | -92.391 (25/25) |
| AMD EPYC 7763 64-Core Processor / amd-3 | THP | -3.377 (25/25) | -12.631 (25/25) | -0.470 (14/25) | -91.715 (25/25) |
| Neoverse-N2 / arm64 | Default | -12.261 (25/25) | -18.587 (25/25) | -94.416 (25/25) | -158.234 (25/25) |
| Neoverse-N2 / arm64 | THP | -12.159 (25/25) | -18.531 (25/25) | -104.375 (25/25) | -160.221 (25/25) |

## Durable INSERT against SQLite WAL/FULL

Each pair includes the candidate, Lin control `2960abb85`, and SQLite. The harness checks exact values after reopen. [Benchmark artifacts](https://github.com/themoretheless/lin/actions/runs/45986536177)

| CPU and replica | SQLite version | 1k vs SQLite | 1k vs control | 10k vs SQLite | 10k vs control |
|---|---|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / amd-2 | 3.37.2 | +0.008 (10/24) | -0.024 (18/24) | +2.800 (7/24) | -0.090 (13/24) |
| AMD EPYC 9V45 96-Core Processor / amd-3 | 3.37.2 | -0.466 (24/24) | -0.037 (16/24) | -0.404 (15/24) | -1.043 (14/24) |
| AMD EPYC 9V74 80-Core Processor / amd-4 | 3.37.2 | -0.080 (20/24) | -0.025 (16/24) | +1.133 (5/24) | -0.015 (12/24) |
| Neoverse-N2 / arm64 | 3.45.1 | +0.173 (4/24) | -0.048 (16/24) | -2.349 (20/24) | -0.069 (13/24) |
