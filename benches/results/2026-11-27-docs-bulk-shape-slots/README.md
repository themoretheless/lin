# Проверка расположения полей при массовой вставке

Кандидат f6bc33b3b, контроль fe2717127, измерительная ветка f8c06f2fc. Сборки используют одинаковую утилиту, статический TCMalloc на Linux x64 и штатный аллокатор на ARM64. Клиент и встроенный движок сравнения работают на том же раннере. В тесте корректности чередуются две разные схемы одинаковой ширины. Бенч измеряет обычный пакет однородных строк.

Медианы парных разниц в миллисекундах. Отрицательные значения означают выигрыш первого участника. SQLite WAL/FULL — 24 пары с проверкой восстановленных значений. DuckDB 2.0.0 — 25 пар на размер, первый INSERT и медиана последующих четырёх.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | -0.011 | -0.263 | -0.251 | 24/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | -0.053 | -1.367 | -1.504 | 15/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | -0.026 | +0.233 | +0.215 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | -0.006 | -7.537 | -7.762 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | -0.021 | +0.227 | +0.205 | 2/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | +0.157 | -7.318 | -7.175 | 24/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.019 | -0.175 | -0.152 | 24/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | +1.055 | +1.443 | +0.195 | 8/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.030 | +0.514 | +0.479 | 0/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | +1.430 | -6.097 | -7.176 | 23/24 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 10000 | первый | +0.176 | -2.756 | -2.843 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 10000 | повторные | -0.078 | -11.841 | -11.792 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 100000 | первый | -2.738 | +12.108 | +12.184 | 0/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 100000 | повторные | -0.960 | -80.192 | -79.374 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-2 | 10000 | первый | -0.209 | -3.133 | -2.924 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-2 | 10000 | повторные | +0.005 | -10.357 | -10.424 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-2 | 100000 | первый | -0.405 | +7.319 | +7.834 | 0/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-2 | 100000 | повторные | -0.374 | -65.123 | -63.945 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-4 | 10000 | первый | -0.028 | -2.842 | -2.811 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-4 | 10000 | повторные | -0.230 | -9.980 | -9.706 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-4 | 100000 | первый | -0.868 | +17.132 | +17.364 | 1/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-4 | 100000 | повторные | -0.174 | -54.110 | -53.793 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | первый | -0.025 | -11.264 | -11.171 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | повторные | -0.201 | -17.878 | -17.664 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | первый | -0.681 | -82.451 | -82.294 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | повторные | -1.562 | -150.330 | -148.851 | 25/25 |

На Neoverse-N2 в WAL/FULL 10k кандидат уступил main на 1,430 мс при сохранении выигрыша у SQLite. На x64 отставание от DuckDB сохраняется на первом INSERT 100k.

[SQLite WAL/FULL](https://github.com/themoretheless/lin/actions/runs/46017578899) · [DuckDB](https://github.com/themoretheless/lin/actions/runs/46017578813)
