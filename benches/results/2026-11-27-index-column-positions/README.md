# Прямое построение ключа составного индекса

Кандидат c0a5a965c, контроль c0613afcf содержит исправление смешанных схем строк. Единственное различие кода движка — построение ключа вторичного индекса. Оба движка сравнения встроенные и выполнялись на том же Linux-раннере. Lin использует TCMalloc на x64 и штатный аллокатор на ARM64.

Медианы парных разниц в миллисекундах. Отрицательные значения означают преимущество первого участника. SQLite WAL/FULL: 24 пары на размер с проверкой после восстановления. DuckDB: 25 пар на размер, первый INSERT и медиана следующих четырёх.

| Движок | CPU | Раннер | Строк | INSERT | Кандидат − контроль, мс | Кандидат − движок, мс | контроль − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | +0.010 | +0.191 | +0.184 | 3/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | -0.090 | -8.357 | -8.257 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | +0.002 | +0.157 | +0.169 | 4/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | +0.227 | -7.876 | -7.987 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | -0.065 | +0.136 | +0.179 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | +0.135 | -8.535 | -8.902 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.069 | +0.388 | +0.436 | 0/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | +0.274 | -6.443 | -6.562 | 23/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.010 | +0.254 | +0.287 | 1/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | -0.470 | -8.181 | -7.840 | 24/24 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 10000 | первый | -0.089 | -2.751 | -2.695 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 10000 | повторные | -0.017 | -10.160 | -10.056 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 100000 | первый | -0.906 | +13.363 | +15.224 | 0/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 100000 | повторные | +0.399 | -57.600 | -59.155 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | первый | +0.349 | -2.423 | -2.828 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | повторные | -0.089 | -11.523 | -11.466 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | первый | -0.208 | +15.175 | +15.345 | 0/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | повторные | +0.178 | -75.822 | -76.567 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 10000 | первый | -0.117 | -2.913 | -2.829 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 10000 | повторные | -0.137 | -10.237 | -10.140 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 100000 | первый | -0.760 | +11.319 | +11.799 | 0/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 100000 | повторные | -0.794 | -61.054 | -59.953 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-4 | 10000 | первый | -0.209 | -7.739 | -7.626 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-4 | 10000 | повторные | -0.132 | -10.214 | -10.193 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-4 | 100000 | первый | -1.684 | -45.645 | -45.290 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-4 | 100000 | повторные | -0.409 | -71.930 | -70.985 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | первый | +0.030 | -11.327 | -11.070 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | повторные | -0.041 | -18.032 | -17.948 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | первый | -0.194 | -87.862 | -88.801 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | повторные | -0.031 | -152.909 | -152.968 | 25/25 |

На первом INSERT 100k кандидат опередил контроль по медиане на каждом раннере. На EPYC 7763 и двух EPYC 9V74 все 75 пар первого INSERT 100k выиграл DuckDB. Эффект на WAL/FULL 1k различался по раннерам.

[SQLite WAL/FULL](https://github.com/themoretheless/lin/actions/runs/46019819632) · [DuckDB](https://github.com/themoretheless/lin/actions/runs/46019819552)
