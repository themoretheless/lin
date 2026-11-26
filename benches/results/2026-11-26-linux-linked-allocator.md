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

## Targeted EPYC 7763 qualification

Two of eight allocated runners matched the target CPU. Both used the same linked binary build procedure as above. The table uses default pages and reports paired Lin minus peer milliseconds and Lin wins.

| Runner | DuckDB first 10k | DuckDB repeat 10k | DuckDB first 100k | DuckDB repeat 100k | SQLite FULL 1k | SQLite FULL 10k |
|---|---:|---:|---:|---:|---:|---:|
| EPYC 7763 amd-5 | −3.615 (25/25) | −12.263 (25/25) | −7.692 (24/25) | −96.742 (25/25) | −0.833 (23/24) | −14.473 (24/24) |
| EPYC 7763 amd-6 | −3.687 (25/25) | −12.104 (25/25) | −2.968 (20/25) | −92.293 (25/25) | −0.771 (24/24) | −13.138 (24/24) |

[CPU provenance and measurements](https://github.com/themoretheless/lin/actions/runs/45870525222)

## Linked worker read and server matrix

On a separate Linux x64 runner, the linked worker won 54/54 read comparisons per peer against SQLite, DuckDB, pandas, PostgreSQL, MySQL and MongoDB. The six read workloads ran at 1k, 10k and 100k in three independent processes. Server-native insertion won 9/9 comparisons per peer against PostgreSQL, MySQL and MongoDB. SQLite and DuckDB INSERT are qualified separately above.

[Read and native INSERT measurements](https://github.com/themoretheless/lin/actions/runs/45870700571)

## Native SQL Server qualification

The linked x64 worker completed six read queries and native INSERT at 1k, 10k and 100k against SQL Server on Linux. All workloads preserved the sentinel and removed their temporary tables. The source and binary hashes remained stable. Comparative timings are in the encrypted artifact. [CI and qualification evidence](https://github.com/themoretheless/lin/actions/runs/45872370569)

## Linux x64 versus Azure observations from separate runs

Lin ran on the native Linux x64 peer-matrix runner. The Azure client ran on the M4 Max in the live Kusto qualification, including network transfer and visibility polling for ingestion. The benchmark adapter was identical at both commits. These sample ranges cover different hosts and times.

| Rows | Read cases with disjoint ranges | Smallest read separation | Lin INSERT ms | Kusto ingestion ms | Smallest insert separation |
|---:|---:|---:|---:|---:|---:|
| 1000 | 6/6 | 2974.24× | 0.301 | 913.238 | 844.21× |
| 10000 | 6/6 | 938.69× | 3.892 | 1760.421 | 133.37× |
| 100000 | 6/6 | 1402.96× | 48.712 | 26204.949 | 60.24× |

Separation divides the shortest Kusto sample by the longest Lin sample for the same size and operation. INSERT times are medians of three process medians. [Linux x64 samples](https://github.com/themoretheless/lin/actions/runs/45870700571) and [Azure samples](2026-11-26-inline-fts-kusto/samples.json).
