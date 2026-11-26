# Peer benchmark evidence by engine

## Inline FTS candidate 0add1938e

The candidate stores short full-text terms inline. Hardware, API, durability and network scope vary by workload. Counts below compare Lin and its peer within the specified run.

| Peer | Candidate evidence | Measurements |
|---|---|---|
| DuckDB | In-memory prepared INSERT on Linux. All 700 THP pairs won across two runs. Default first 100k lost every pair on the 7763 and 9V74 hosts in the first run and on the 7763 in the repeat. Six read cases won all 54 process comparisons on local ARM64. | [Linux paired INSERT](2026-11-26-duckdb-paired-insert.md#inline-full-text-terms-up-to-22-bytes), [reads](2026-11-26-inline-fts-reads/report.md) |
| SQLite | Linux WAL/FULL plus verified recovery: 225/240 wins and lower median at 1k and 10k on all five measured hosts. Six local read cases won 54/54 comparisons. | [Durable Linux INSERT](2026-11-26-sqlite-wal-full.md#inline-full-text-keys-on-linux), [reads](2026-11-26-inline-fts-reads/report.md) |
| PostgreSQL | 54/54 reads and 9/9 COPY plus commit comparisons, localhost ARM64 with tmpfs storage. | [Server peers](2026-11-26-inline-fts-server-peers/report.md) |
| MySQL | 54/54 reads and 9/9 multi-row INSERT plus commit comparisons on the same server setup. | [Server peers](2026-11-26-inline-fts-server-peers/report.md) |
| MongoDB | 54/54 reads and 9/9 ordered insert_many comparisons, w=1 without journaling acknowledgement. | [Server peers](2026-11-26-inline-fts-server-peers/report.md) |
| Microsoft SQL Server | Native Linux x64. Six read cases and native INSERT validated at 1k, 10k and 100k. Detailed comparative results are in the private artifact. | [CI and encrypted results](https://github.com/themoretheless/lin/actions/runs/45852615677) |
| Azure Data Explorer (Kusto) | 54/54 reads and 9/9 inline ingestion comparisons. At 100k, median INSERT was 105.837 ms for local Lin and 26,204.949 ms through visibility on Azure. One failed process was repeated. | [Results and retry](2026-11-26-inline-fts-kusto/report.md) |
| pandas | 54/54 DataFrame read comparisons on local ARM64. | [Reads](2026-11-26-inline-fts-reads/report.md) |

## Remaining peer losses on the candidate

- DuckDB, default Linux page policy, first 100k INSERT. On EPYC 7763 and 9V74 Lin lost 75/75 pairs across the two runs. Median paired overhead was 23.622 and 25.212 ms on the two 7763 runs and 26.226 ms on 9V74. First 10k lost 13/175 pairs despite winning each host median. Repeated INSERT won 350/350 default-page pairs; requesting transparent huge pages for both engines won 700/700 first and repeat pairs combined.
- SQLite WAL/FULL on Linux. Lin lost 11/120 pairs at 1k and 4/120 at 10k. Each host median favored Lin at both sizes.

## Earlier revisions

The rows link to independently measured workloads. Dates, Lin revisions, hardware, and APIs vary across reports. The November local ARM64 matrix covers 1k, 10k and 100k rows using the same Lin worker binary as the live Kusto runs. The PostgreSQL, MySQL and MongoDB server peers ran on localhost. November Kusto measurements include the network trip to Azure.

| Peer | Verified workload and scope | Evidence |
|---|---|---|
| DuckDB | In the paired run of candidate 417c1fc8d, the default first 100k INSERT lost all 75 EPYC pairs, while all 75 repeat pairs won. With THP requested for both engines Lin won 399/400 pairs across EPYC and ARM64. The single loss was a first 10k INSERT on EPYC 7763. This candidate was withdrawn after comparison with the Lin control. | [Local ARM64 matrix](2026-11-26-embedded-peer-refresh/report.md), [paired run 45821850933](https://github.com/themoretheless/lin/actions/runs/45821850933) and [paired Linux INSERT and profiles](2026-11-26-duckdb-paired-insert.md) |
| SQLite | WAL/FULL at 1k and 10k on EPYC 7763, EPYC 9V74 and ARM64 N2. Lin won all six medians and 136 of 144 pairs with recovery verified. | [Local ARM64 matrix](2026-11-26-embedded-peer-refresh/report.md) and [WAL/FULL](2026-11-26-sqlite-wal-full.md) |
| PostgreSQL | Six read cases and COPY plus commit, at all three sizes, on the November ARM64 worker. | [Public matrix](2026-11-26-local-peer-refresh/report.md) |
| MySQL | Six read cases and multi-row insert plus commit at all three sizes on the same frozen build. | [Public matrix](2026-11-26-local-peer-refresh/report.md) |
| MongoDB | Six read cases and ordered insert_many at all three sizes. Write concern was w=1 with journaling acknowledgement disabled. | [Public matrix](2026-11-26-local-peer-refresh/report.md) |
| Microsoft SQL Server | The November 26 native Linux x64 rerun on revision 9fdc66564 completed six read cases and native insertion at 1k, 10k and 100k rows. All six workloads preserved the sentinel and removed their fixture tables. Detailed comparative measurements are held in the owner's private artifacts. | [Public qualification details](2026-10-26-mssql-native-ci/report.md) and [November CI run](https://github.com/themoretheless/lin/actions/runs/45812959495) |
| Azure Data Explorer (Kusto) | Six read cases and inline ingestion validated on the live service at all three sizes. Streaming ingestion was disabled on this cluster. | [Read 1k](2026-11-26-kusto-live-reads/1000.md), [10k](2026-11-26-kusto-live-reads/10000.md), [100k](2026-11-26-kusto-live-reads/100000.md), [inline ingestion](2026-11-26-kusto-inline/report.md) |
| pandas | Six DataFrame read cases at all three sizes. The matrix defines no native database INSERT for pandas. | [Public matrix](2026-11-26-local-peer-refresh/report.md) |
