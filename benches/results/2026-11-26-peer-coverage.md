# Peer benchmark evidence by engine

The rows link to independently measured workloads. Dates, Lin revisions, hardware, and APIs vary across reports. The October matrix includes 1k, 10k and 100k rows. Its server peers ran on localhost. November Kusto measurements include the network trip to Azure.

| Peer | Verified workload and scope | Evidence |
|---|---|---|
| DuckDB | On three EPYC hosts the default first 100k INSERT lost all 75 pairs in the wide-row candidate run. With THP enabled for both engines Lin won all 400 pairs across EPYC and ARM64. The candidate was withdrawn. | [Paired INSERT and profiles](2026-11-26-duckdb-paired-insert.md) |
| SQLite | WAL/FULL at 1k and 10k on EPYC 7763, EPYC 9V74 and ARM64 N2. Lin won all six medians and 136 of 144 pairs with recovery verified. | [WAL/FULL](2026-11-26-sqlite-wal-full.md) |
| PostgreSQL | Six read cases and COPY plus commit, at all three sizes, on frozen Lin b7195e2. | [Public matrix](2026-10-26-persistent-peer-refresh/report.md) |
| MySQL | Six read cases and multi-row insert plus commit at all three sizes on the same frozen build. | [Public matrix](2026-10-26-persistent-peer-refresh/report.md) |
| MongoDB | Six read cases and ordered insert_many at all three sizes. Write concern was w=1 with journaling acknowledgement disabled. | [Public matrix](2026-10-26-persistent-peer-refresh/report.md) |
| Microsoft SQL Server | Native Linux x64 qualification covers six read cases and native insertion. Detailed comparative measurements are held in the owner's private artifacts. | [Public qualification details](2026-10-26-mssql-native-ci/report.md) and [November CI run](https://github.com/themoretheless/lin/actions/runs/45763396102) |
| Azure Data Explorer (Kusto) | Six read cases and inline ingestion validated on the live service at all three sizes. Streaming ingestion was disabled on this cluster. | [Read 1k](2026-11-26-kusto-live-reads/1000.md), [10k](2026-11-26-kusto-live-reads/10000.md), [100k](2026-11-26-kusto-live-reads/100000.md), [inline ingestion](2026-11-26-kusto-inline/report.md) |
| pandas | Six DataFrame read cases at all three sizes. The matrix defines no native database INSERT for pandas. | [Public matrix](2026-10-26-persistent-peer-refresh/report.md) |
