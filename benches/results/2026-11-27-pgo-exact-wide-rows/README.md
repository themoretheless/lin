# PGO и точный размер буферов широких строк

Кандидат 4523c531f, контрольный движок fe2717127. Обе сборки используют один и тот же код бенч-утилиты и работают на том же раннере, что и встроенный движок сравнения. Linux x64 использует статический TCMalloc, ARM64 — штатный аллокатор. Локальные тесты пакета lin: 645 прошло, 2 пропущено.

Каждая разница — медиана парных разниц в миллисекундах. SQLite: 24 пары на размер, WAL/FULL, сверка строк после восстановления. DuckDB: 25 пар на размер, первый INSERT в новом процессе и медиана четырёх последующих INSERT. Указан стандартный режим страниц. На двух дополнительных Xeon замеры DuckDB пропущены фильтром CPU.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | -0.065 | +0.064 | +0.102 | 6/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | -0.831 | -9.206 | -8.244 | 24/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | -0.028 | -0.255 | -0.223 | 24/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | +0.581 | +0.459 | -0.177 | 11/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | -0.029 | +0.168 | +0.263 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | -1.035 | -8.585 | -7.115 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.117 | +0.151 | +0.237 | 5/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | -1.191 | -8.805 | -7.671 | 24/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.005 | +1.018 | +0.958 | 1/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | +0.207 | -1.462 | -1.527 | 16/24 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | первый | -1.324 | -0.857 | +0.775 | 19/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | повторные | -2.623 | -9.411 | -6.927 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | первый | -22.559 | +67.916 | +89.474 | 0/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | повторные | -24.381 | -30.262 | -6.815 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-3 | 10000 | первый | -0.538 | -7.767 | -6.989 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-3 | 10000 | повторные | -0.440 | -9.997 | -9.531 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-3 | 100000 | первый | -5.717 | -49.180 | -42.381 | 25/25 |
| duckdb | AMD EPYC 9V45 96-Core Processor | paired-duckdb-insert-amd-3 | 100000 | повторные | -5.540 | -73.663 | -68.068 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | первый | -0.842 | -12.283 | -11.308 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 10000 | повторные | -0.824 | -18.629 | -17.771 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | первый | -8.811 | -93.594 | -85.825 | 25/25 |
| duckdb | Neoverse-N2 | paired-duckdb-insert-arm64 | 100000 | повторные | -8.543 | -157.025 | -148.879 | 25/25 |

Для первого INSERT 100k с подсказкой glibc о больших страницах разницы Lin минус DuckDB составили paired-duckdb-insert-amd-2: +66.887 мс, paired-duckdb-insert-amd-3: -48.784 мс, paired-duckdb-insert-arm64: -98.715 мс. Фактический размер страниц в этом прогоне не измерялся.

[SQLite WAL/FULL](https://github.com/themoretheless/lin/actions/runs/46012313025) · [DuckDB](https://github.com/themoretheless/lin/actions/runs/46012312886)

## Повтор на трёх EPYC 7763

На первом INSERT 100k прежний разрыв 67,916 мс не воспроизвёлся. Размеры и порядок нагрузки сохранены. Каждый раннер выполнил по 25 пар на размер. Отрицательная разница означает выигрыш первого участника.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | PGO − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 10000 | первый | -0.949 | -3.933 | -2.999 | 24/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 10000 | повторные | -0.599 | -12.302 | -11.697 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 100000 | первый | -10.183 | +4.020 | +14.566 | 2/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-1 | 100000 | повторные | -9.006 | -84.270 | -75.264 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | первый | -0.932 | -4.100 | -3.158 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 10000 | повторные | -0.556 | -12.589 | -12.023 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | первый | -11.148 | +4.095 | +15.321 | 1/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-2 | 100000 | повторные | -6.869 | -86.206 | -78.285 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 10000 | первый | -0.965 | -4.056 | -3.036 | 24/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 10000 | повторные | -0.869 | -12.515 | -11.812 | 25/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 100000 | первый | -10.666 | -0.182 | +9.393 | 13/25 |
| duckdb | AMD EPYC 7763 64-Core Processor | paired-duckdb-insert-amd-4 | 100000 | повторные | -6.281 | -90.403 | -84.196 | 25/25 |

[Повтор и сырые данные](https://github.com/themoretheless/lin/actions/runs/46015865238)
