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
