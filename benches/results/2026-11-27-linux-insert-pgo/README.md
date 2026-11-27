# Linux INSERT с профильной оптимизацией компилятора

Сборка e3d4025b9 сравнивается с релизным кодом fe2717127 на одном раннере. Обе сборки используют TCMalloc на Linux x64 и штатный аллокатор на ARM64. Для обучения использованы другие размеры пакетов, чем для измерений.

## SQLite WAL/FULL

На каждом размере выполнены четыре блока со всеми шестью порядками запуска трёх участников. Таймер охватывает запись и фиксацию транзакции. После переоткрытия сравниваются все строки. Клиент и встроенные движки работают локально на указанном раннере. Каждая разница — медиана парных разниц в миллисекундах. Отрицательное значение означает выигрыш первого участника.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | PGO − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | -0.029 | +0.336 | +0.348 | 1/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | -0.077 | +4.466 | +4.114 | 7/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | +0.021 | -0.201 | -0.212 | 22/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | -0.433 | +1.611 | +1.831 | 5/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | -0.036 | +0.371 | +0.392 | 1/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | +0.105 | +2.563 | +2.527 | 6/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.018 | +0.146 | +0.153 | 5/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | -0.401 | -8.480 | -7.978 | 24/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | -0.043 | +0.389 | +0.440 | 1/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | -0.345 | -2.087 | -1.738 | 22/24 |

Версии SQLite по раннерам: wal-full-amd-1 — 3.37.2, wal-full-amd-2 — 3.37.2, wal-full-amd-3 — 3.37.2, wal-full-amd-4 — 3.37.2, wal-full-arm64 — 3.45.1.

[Исходные замеры SQLite](https://github.com/themoretheless/lin/actions/runs/46006890946)

## DuckDB 2.0.0

Для каждого размера выполнено 25 пар. Первый INSERT измерен в новом процессе. Повторные INSERT представлены медианой следующих четырёх операций того же процесса. Каждый INSERT DuckDB выполняется в новой базе в памяти.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | PGO − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 10000 | первый | -0.284 | -3.012 | -2.718 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 10000 | повторные | -0.213 | -10.215 | -9.974 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 100000 | первый | -3.102 | +6.955 | +10.313 | 0/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-1 | 100000 | повторные | -3.439 | -65.735 | -61.249 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | первый | -0.284 | -3.258 | -3.083 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | повторные | -0.383 | -12.138 | -11.804 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | первый | -4.598 | +7.574 | +12.005 | 3/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | повторные | -5.957 | -84.597 | -78.501 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 10000 | первый | -0.447 | -3.359 | -2.819 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 10000 | повторные | -0.267 | -10.108 | -9.821 | 25/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 100000 | первый | -4.108 | +10.019 | +14.379 | 0/25 |
| duckdb | AMD EPYC 9V74 80-Core Processor | paired-duckdb-insert-amd-3 | 100000 | повторные | -4.483 | -61.954 | -57.496 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 10000 | первый | -0.439 | -3.808 | -3.463 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 10000 | повторные | -0.437 | -12.434 | -11.972 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 100000 | первый | -8.546 | +7.796 | +15.895 | 0/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 100000 | повторные | -7.034 | -84.839 | -76.770 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | первый | -0.595 | -11.650 | -11.121 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | повторные | -0.632 | -18.259 | -17.629 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | первый | -5.959 | -91.346 | -85.193 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | повторные | -6.804 | -154.774 | -147.785 | 25/25 |

## Первый INSERT 100k с подсказкой glibc для больших страниц

Оба процесса получают GLIBC_TUNABLES=glibc.malloc.hugetlb=1. Фактический размер страниц этими замерами не установлен.

| Раннер | PGO − DuckDB, мс |
|---|---:|
| paired-duckdb-insert-amd-1 | +7.475 |
| paired-duckdb-insert-amd-2 | +6.204 |
| paired-duckdb-insert-amd-3 | +11.219 |
| paired-duckdb-insert-amd-4 | +8.030 |
| paired-duckdb-insert-arm64 | -96.240 |

[Исходные замеры DuckDB](https://github.com/themoretheless/lin/actions/runs/46006890921)
