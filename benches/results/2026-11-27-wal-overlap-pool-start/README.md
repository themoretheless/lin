# Запуск потоков WAL во время построения строк

Кандидат 0469f6998, контроль fe2717127. Одинаковые утилиты и локальные встроенные движки на каждом раннере. Linux x64 использует TCMalloc, ARM64 — штатный аллокатор. На каждом размере и раннере проверено 24 парных измерения полной транзакции с восстановлением. Приведены медианы парных разниц в мс.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | +0.109 | +0.334 | +0.220 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | +0.635 | -7.298 | -7.656 | 24/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | +0.138 | +0.714 | +0.439 | 2/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | -0.051 | +2.535 | +3.030 | 6/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | +0.147 | +0.413 | +0.234 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | +0.527 | -7.923 | -7.948 | 24/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | +0.226 | +0.658 | +0.385 | 0/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | +0.440 | +2.089 | +1.281 | 1/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.083 | +0.554 | +0.498 | 0/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | +0.998 | -5.771 | -6.730 | 24/24 |

На 1k кандидат медленнее main на всех пяти раннерах, на 10k — на четырёх. Дополнительный тест f8625ba76 проверил восстановление четырёх независимых баз после одновременного первого INSERT по 1000 строк.

[Исходные измерения](https://github.com/themoretheless/lin/actions/runs/46026169513)
