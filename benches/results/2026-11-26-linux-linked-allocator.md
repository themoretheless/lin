# Linux release binaries with linked TCMalloc

The x86_64 GNU/Linux CLI and peer worker link the static minimal TCMalloc archive. CI checked both binaries for defined malloc and tc_malloc symbols and checked their dynamic dependencies. ARM64 kept the standard allocator. Results use default page settings.

## Attempt 1

Median paired Lin minus peer time in milliseconds, with paired Lin wins in parentheses. DuckDB repeat uses the median of four subsequent transactions in each fresh process. SQLite WAL/FULL uses fresh databases with readback after reopening.

| CPU / runner | DuckDB first 10k | DuckDB repeat 10k | DuckDB first 100k | DuckDB repeat 100k | SQLite 1k | SQLite 10k |
|---|---:|---:|---:|---:|---:|---:|
| AMD EPYC 9V74 80-Core Processor / amd-1 | -3.352 (25/25) | -10.263 (25/25) | -4.274 (24/25) | -74.007 (25/25) | -0.488 (21/24) | -9.649 (22/24) |
| AMD EPYC 9V74 80-Core Processor / amd-2 | -3.406 (25/25) | -10.259 (25/25) | -3.303 (23/25) | -73.793 (25/25) | -0.466 (21/24) | -5.843 (24/24) |
| AMD EPYC 9V74 80-Core Processor / amd-3 | -3.370 (25/25) | -10.287 (25/25) | -5.534 (23/25) | -75.487 (25/25) | -0.508 (20/24) | -7.027 (19/24) |
| AMD EPYC 9V45 96-Core Processor / amd-4 | -7.240 (25/25) | -9.738 (25/25) | -56.154 (25/25) | -81.074 (25/25) | -0.618 (24/24) | -9.007 (24/24) |
| Neoverse-N2 / arm64 | -13.636 (25/25) | -18.813 (25/25) | -119.118 (25/25) | -160.808 (25/25) | -0.324 (22/24) | -6.998 (24/24) |

[CI and raw samples](https://github.com/themoretheless/lin/actions/runs/45867410306/attempts/1)

## Attempt 2

The same revision ran on three EPYC 9V45 hosts and one Neoverse-N2. One Xeon runner skipped timing. With default pages, Lin won all 400 DuckDB pairs (two sizes, first and repeat). On EPYC the first 100k advantage ranged from 53.522 to 59.248 ms. On N2 it was 120.406 ms. Lin won 184 of 192 SQLite comparisons and all eight host-size medians after recovery. No EPYC 7763 timings were collected in either attempt.

[CI and raw samples](https://github.com/themoretheless/lin/actions/runs/45867410306/attempts/2)
